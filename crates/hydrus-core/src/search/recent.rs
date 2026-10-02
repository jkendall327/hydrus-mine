//! The system predicates recently added from the system predicate editors,
//! as the reference keeps them (`predicate_types_to_recent_predicates`,
//! `PushRecentPredicates`, `GetRecentPredicates`, `RemoveRecentPredicate`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::predicate::SystemPredicate;

/// How many of each type are kept.
pub const KEPT: usize = 5;

/// By the reference's number for each kind of predicate
/// ([`SystemPredicate::reference_type`]), the newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentPredicates {
    pub by_type: BTreeMap<u8, Vec<SystemPredicate>>,
}

impl RecentPredicates {
    /// Keep `predicates` (what an editor added), each in turn to the front
    /// of its type's, moved there if already kept, five at most.
    pub fn push(&mut self, predicates: &[SystemPredicate]) {
        for predicate in predicates {
            let kept = self.by_type.entry(predicate.reference_type()).or_default();
            kept.retain(|p| p != predicate);
            kept.insert(0, predicate.clone());
            kept.truncate(KEPT);
        }
    }

    /// Those of these types, type by type, each newest first.
    pub fn of_types(&self, types: &[u8]) -> Vec<SystemPredicate> {
        types
            .iter()
            .filter_map(|t| self.by_type.get(t))
            .flatten()
            .cloned()
            .collect()
    }

    /// Forget `predicate`.
    pub fn remove(&mut self, predicate: &SystemPredicate) {
        for kept in self.by_type.values_mut() {
            if let Some(i) = kept.iter().position(|p| p == predicate) {
                kept.remove(i);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::number::{NumberOp, NumberTest};
    use crate::search::predicate::NumericProperty;

    fn width(n: u64) -> SystemPredicate {
        SystemPredicate::Number {
            property: NumericProperty::Width,
            test: NumberTest::new(NumberOp::Equal, n),
        }
    }

    fn height(n: u64) -> SystemPredicate {
        SystemPredicate::Number {
            property: NumericProperty::Height,
            test: NumberTest::new(NumberOp::Equal, n),
        }
    }

    /// Kept by the reference's numbers for their types (the GUI's tests
    /// check them all against the reference's).
    #[test]
    fn widths_and_heights_are_types_13_and_14() {
        assert_eq!(width(1).reference_type(), 13);
        assert_eq!(height(1).reference_type(), 14);
    }

    /// Newest first, five of a type, one pushed again to the front; others
    /// of other types kept apart.
    #[test]
    fn pushed_predicates_are_kept_as_the_reference_keeps_them() {
        let mut recent = RecentPredicates::default();
        recent.push(&[width(1), height(1)]);
        recent.push(&[width(2), width(3)]);
        assert_eq!(recent.by_type[&13], [width(3), width(2), width(1)]);
        assert_eq!(recent.by_type[&14], [height(1)]);
        recent.push(&[width(4), width(5), width(6)]);
        assert_eq!(
            recent.by_type[&13],
            [width(6), width(5), width(4), width(3), width(2)]
        );
        recent.push(&[width(4)]);
        assert_eq!(
            recent.by_type[&13],
            [width(4), width(6), width(5), width(3), width(2)]
        );
        // by type, in the types' order
        assert_eq!(recent.of_types(&[14, 13])[..2], [height(1), width(4)]);
        assert!(recent.of_types(&[15]).is_empty());
        // forgotten
        recent.remove(&width(6));
        recent.remove(&width(99));
        assert_eq!(
            recent.by_type[&13],
            [width(4), width(5), width(3), width(2)]
        );
    }
}
