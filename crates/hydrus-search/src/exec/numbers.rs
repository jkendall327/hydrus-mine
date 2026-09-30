//! Number tests as the executor applies them.
//!
//! Counts (notes, URLs, tags, views, relationships, inc/dec ratings) are
//! non-negative integers where a missing row means zero, so every test on
//! them is a set of allowed counts: [`Counts`], a union of at most two
//! ranges. The different "about" rules of the reference's predicate
//! generations are converted into exact integer bounds here, once.
//!
//! File properties (width, duration, framerate, ...) are tested in SQL by
//! [`sql_condition`], with the reference's rule that a missing value counts
//! as zero.

use crate::number::{Comparison, NumberOp, NumberTest};

/// A set of allowed non-negative integers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Disjoint inclusive ranges.
    ranges: Vec<(u64, u64)>,
}

impl Counts {
    pub fn none() -> Self {
        Self { ranges: Vec::new() }
    }

    pub fn range(lo: u64, hi: u64) -> Self {
        if lo > hi {
            Self::none()
        } else {
            Self {
                ranges: vec![(lo, hi)],
            }
        }
    }

    pub fn at_least(lo: u64) -> Self {
        Self::range(lo, u64::MAX)
    }

    pub fn exactly(v: u64) -> Self {
        Self::range(v, v)
    }

    pub fn below(v: u64) -> Self {
        match v.checked_sub(1) {
            Some(hi) => Self::range(0, hi),
            None => Self::none(),
        }
    }

    pub fn above(v: u64) -> Self {
        match v.checked_add(1) {
            Some(lo) => Self::at_least(lo),
            None => Self::none(),
        }
    }

    pub fn except(v: u64) -> Self {
        let mut ranges = Vec::with_capacity(2);
        if let Some(hi) = v.checked_sub(1) {
            ranges.push((0, hi));
        }
        if let Some(lo) = v.checked_add(1) {
            ranges.push((lo, u64::MAX));
        }
        Self { ranges }
    }

    pub fn contains(&self, n: u64) -> bool {
        self.ranges.iter().any(|&(lo, hi)| lo <= n && n <= hi)
    }

    pub fn contains_zero(&self) -> bool {
        self.contains(0)
    }

    /// The counts a [`NumberTest`] accepts: the reference's `NumberTest`
    /// lambda applied to integers, "about" being a percentage either side,
    /// inclusive.
    pub fn from_number_test(test: NumberTest) -> Self {
        let v = test.value;
        match test.op {
            NumberOp::Less => Self::below(v),
            NumberOp::LessOrEqual => Self::range(0, v),
            NumberOp::Greater => Self::above(v),
            NumberOp::GreaterOrEqual => Self::at_least(v),
            NumberOp::Equal => Self::exactly(v),
            NumberOp::NotEqual => Self::except(v),
            NumberOp::ApproxPercent { percent } => {
                let fraction = f64::from(percent) / 100.0;
                let lower = v as f64 * (1.0 - fraction);
                let upper = v as f64 * (1.0 + fraction);
                Self::float_range(lower, upper)
            }
            NumberOp::ApproxAbsolute { tolerance } => {
                Self::range(v.saturating_sub(tolerance), v.saturating_add(tolerance))
            }
        }
    }

    /// Integers `n` with `lower <= n <= upper`.
    pub fn float_range(lower: f64, upper: f64) -> Self {
        if upper < 0.0 || upper < lower {
            return Self::none();
        }
        let lo = if lower <= 0.0 { 0 } else { lower.ceil() as u64 };
        let hi = if upper >= u64::MAX as f64 {
            u64::MAX
        } else {
            upper.floor() as u64
        };
        Self::range(lo, hi)
    }

    /// Integers `n` with `lower < n < upper`.
    pub fn open_float_range(lower: f64, upper: f64) -> Self {
        let lo = if lower < 0.0 {
            0
        } else {
            lower.floor() as u64 + 1
        };
        if upper <= 0.0 {
            return Self::none();
        }
        let hi = if upper >= u64::MAX as f64 {
            u64::MAX
        } else {
            upper.ceil() as u64 - 1
        };
        Self::range(lo, hi)
    }

    /// A legacy comparison whose "about" is `approx`.
    pub fn from_comparison(op: Comparison, v: u64, approx: impl FnOnce(u64) -> Counts) -> Self {
        match op {
            Comparison::Less => Self::below(v),
            Comparison::Greater => Self::above(v),
            Comparison::Equal => Self::exactly(v),
            Comparison::NotEqual => Self::except(v),
            Comparison::Approx => approx(v),
        }
    }
}

/// A SQL condition on the numeric expression `expr` for a [`NumberTest`],
/// with a missing (NULL) value counting as zero, as the reference does.
/// Returns the condition and its parameters.
pub(crate) fn sql_condition(expr: &str, test: NumberTest) -> (String, Vec<f64>) {
    let v = test.value as f64;
    let (cond, params) = match test.op {
        NumberOp::Less => (format!("{expr} < ?"), vec![v]),
        NumberOp::LessOrEqual => (format!("{expr} <= ?"), vec![v]),
        NumberOp::Greater => (format!("{expr} > ?"), vec![v]),
        NumberOp::GreaterOrEqual => (format!("{expr} >= ?"), vec![v]),
        NumberOp::Equal => (format!("{expr} = ?"), vec![v]),
        NumberOp::NotEqual => (format!("{expr} != ?"), vec![v]),
        NumberOp::ApproxPercent { percent } => {
            let fraction = f64::from(percent) / 100.0;
            (
                format!("({expr} >= ? AND {expr} <= ?)"),
                vec![v * (1.0 - fraction), v * (1.0 + fraction)],
            )
        }
        NumberOp::ApproxAbsolute { tolerance } => {
            let t = tolerance as f64;
            (format!("({expr} >= ? AND {expr} <= ?)"), vec![v - t, v + t])
        }
    };
    if accepts_zero(test) {
        (format!("({expr} IS NULL OR {cond})"), params)
    } else {
        (format!("({cond})"), params)
    }
}

/// Whether a missing value (zero) passes the test.
pub(crate) fn accepts_zero(test: NumberTest) -> bool {
    match test.op {
        NumberOp::Less => test.value > 0,
        NumberOp::LessOrEqual => true,
        NumberOp::GreaterOrEqual | NumberOp::Equal => test.value == 0,
        NumberOp::Greater => false,
        NumberOp::NotEqual => test.value != 0,
        NumberOp::ApproxPercent { percent } => {
            (test.value as f64) * (1.0 - f64::from(percent) / 100.0) <= 0.0
        }
        NumberOp::ApproxAbsolute { tolerance } => test.value <= tolerance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn op_strategy() -> impl Strategy<Value = NumberOp> {
        prop_oneof![
            Just(NumberOp::Less),
            Just(NumberOp::LessOrEqual),
            Just(NumberOp::Greater),
            Just(NumberOp::GreaterOrEqual),
            Just(NumberOp::Equal),
            Just(NumberOp::NotEqual),
            (0u32..60).prop_map(|percent| NumberOp::ApproxPercent { percent }),
            (0u64..20).prop_map(|tolerance| NumberOp::ApproxAbsolute { tolerance }),
        ]
    }

    /// The reference's `NumberTest.GetLambda`, for non-negative integers.
    fn reference(test: NumberTest, x: u64) -> bool {
        let v = test.value as f64;
        let x = x as f64;
        match test.op {
            NumberOp::Less => x < v,
            NumberOp::LessOrEqual => x <= v,
            NumberOp::Greater => x > v,
            NumberOp::GreaterOrEqual => x >= v,
            NumberOp::Equal => (x - v).abs() < f64::EPSILON,
            NumberOp::NotEqual => (x - v).abs() >= f64::EPSILON,
            NumberOp::ApproxPercent { percent } => {
                let e = f64::from(percent) / 100.0;
                let (lower, upper) = (v * (1.0 - e), v * (1.0 + e));
                if lower <= 0.0 {
                    x <= upper
                } else {
                    lower <= x && x <= upper
                }
            }
            NumberOp::ApproxAbsolute { tolerance } => {
                let t = tolerance as f64;
                let (lower, upper) = (v - t, v + t);
                if lower <= 0.0 {
                    x <= upper
                } else {
                    lower <= x && x <= upper
                }
            }
        }
    }

    proptest! {
        #[test]
        fn counts_match_the_reference_lambda(op in op_strategy(), value in 0u64..200, x in 0u64..400) {
            let test = NumberTest::new(op, value);
            prop_assert_eq!(Counts::from_number_test(test).contains(x), reference(test, x));
            prop_assert_eq!(accepts_zero(test), reference(test, 0));
        }

        #[test]
        fn open_ranges_are_strict(lower in -5.0f64..100.0, width in 0.0f64..50.0, x in 0u64..200) {
            let upper = lower + width;
            let xf = x as f64;
            prop_assert_eq!(Counts::open_float_range(lower, upper).contains(x), lower < xf && xf < upper);
            prop_assert_eq!(Counts::float_range(lower, upper).contains(x), lower <= xf && xf <= upper);
        }
    }

    #[test]
    fn exclusions_and_edges() {
        assert!(!Counts::except(0).contains(0));
        assert!(Counts::except(0).contains(1));
        assert!(Counts::except(3).contains(0));
        assert!(!Counts::below(0).contains(0));
        assert!(Counts::above(u64::MAX).ranges.is_empty());
    }
}
