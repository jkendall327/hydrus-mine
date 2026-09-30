//! Comparisons of numbers.
//!
//! The reference implementation has two generations of numeric predicate:
//!
//! - **Number tests** ([`NumberTest`]) carry the full operator set, including
//!   `≤`/`≥` and an explicit tolerance for "about" (either a percentage or an
//!   absolute amount). Width, height, duration, framerate, frame count and the
//!   counts of notes, URLs and words use them.
//! - **Legacy comparisons** ([`Comparison`] and the narrower operator enums
//!   below) are plain operator + value pairs whose "about" tolerance is fixed by
//!   each predicate's executor (usually ±15%). Tag counts, file size, pixel
//!   count, ratio, file viewing statistics, duplicate counts, tag-as-number and
//!   ratings use them.
//!
//! Each predicate uses the narrowest operator type that holds every operator
//! the reference accepts for it, so an executor never sees an operator it has
//! no semantics for.

use std::fmt;

/// The tolerance the text parser gives "about" number tests: ±15%.
pub const DEFAULT_APPROX_PERCENT: u32 = 15;

/// A test of a single number, e.g. `width > 1920` or `duration ≈ 60s ±15%`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NumberTest {
    pub op: NumberOp,
    /// The number compared against, in the property's unit (pixels,
    /// milliseconds, frames per second, a count, ...).
    pub value: u64,
}

/// The operator of a [`NumberTest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NumberOp {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
    /// Within `percent`% either side of the value.
    ApproxPercent {
        percent: u32,
    },
    /// Within `tolerance` either side of the value. The text parser never
    /// produces this; it exists for searches built elsewhere.
    ApproxAbsolute {
        tolerance: u64,
    },
}

impl NumberTest {
    pub const fn new(op: NumberOp, value: u64) -> Self {
        Self { op, value }
    }

    /// "Has any": the property is present and non-zero (`> 0`).
    pub const fn nonzero() -> Self {
        Self::new(NumberOp::Greater, 0)
    }

    /// "Has none": the property is absent or zero (`= 0`).
    pub const fn zero() -> Self {
        Self::new(NumberOp::Equal, 0)
    }
}

impl fmt::Display for NumberTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.value;
        match self.op {
            NumberOp::Less => write!(f, "< {value}"),
            NumberOp::LessOrEqual => write!(f, "\u{2264} {value}"),
            NumberOp::Greater => write!(f, "> {value}"),
            NumberOp::GreaterOrEqual => write!(f, "\u{2265} {value}"),
            NumberOp::Equal => write!(f, "= {value}"),
            NumberOp::NotEqual => write!(f, "\u{2260} {value}"),
            NumberOp::ApproxPercent { percent } => write!(f, "\u{2248} {value} \u{b1}{percent}%"),
            NumberOp::ApproxAbsolute { tolerance } => {
                write!(f, "\u{2248} {value} \u{b1}{tolerance}")
            }
        }
    }
}

/// A legacy comparison operator. "About" means within a tolerance fixed by
/// the predicate (±15% for counts and sizes).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Comparison {
    Less,
    Greater,
    Equal,
    NotEqual,
    Approx,
}

impl Comparison {
    /// The symbol the reference implementation stores for this operator.
    pub const fn symbol(self) -> &'static str {
        match self {
            Comparison::Less => "<",
            Comparison::Greater => ">",
            Comparison::Equal => "=",
            Comparison::NotEqual => "\u{2260}",
            Comparison::Approx => "\u{2248}",
        }
    }
}

/// The operator of a rating comparison. Ratings cannot be tested for
/// inequality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RatingOp {
    Equal,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    Approx,
}

impl RatingOp {
    /// The symbol the reference implementation stores for this operator.
    pub const fn symbol(self) -> &'static str {
        match self {
            RatingOp::Equal => "=",
            RatingOp::Less => "<",
            RatingOp::Greater => ">",
            RatingOp::LessOrEqual => "\u{2264}",
            RatingOp::GreaterOrEqual => "\u{2265}",
            RatingOp::Approx => "\u{2248}",
        }
    }
}

/// The operator of `system:tag as number`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TagNumberOp {
    Less,
    Greater,
    /// Within ±15%.
    Approx,
}

impl TagNumberOp {
    /// The symbol the reference implementation stores for this operator.
    pub const fn symbol(self) -> &'static str {
        match self {
            TagNumberOp::Less => "<",
            TagNumberOp::Greater => ">",
            TagNumberOp::Approx => "\u{2248}",
        }
    }
}

/// The operator of `system:ratio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RatioOp {
    Equal,
    /// Width/height ratio greater than the given one.
    WiderThan,
    /// Width/height ratio less than the given one.
    TallerThan,
    /// Within ±15% of the given ratio.
    Approx,
}

impl RatioOp {
    /// The string the reference implementation stores for this operator.
    pub const fn symbol(self) -> &'static str {
        match self {
            RatioOp::Equal => "=",
            RatioOp::WiderThan => "wider than",
            RatioOp::TallerThan => "taller than",
            RatioOp::Approx => "\u{2248}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_tests_render_like_the_reference() {
        assert_eq!(NumberTest::new(NumberOp::Greater, 5).to_string(), "> 5");
        assert_eq!(
            NumberTest::new(NumberOp::ApproxPercent { percent: 15 }, 120).to_string(),
            "\u{2248} 120 \u{b1}15%"
        );
    }
}
