//! Duration eligibility uses the clicked media, rather than its preview representative.

/// Qt singletons retain `Some(0)`; collection recalculation keeps only a positive sum.
/// File durations are nonnegative, so checking any positive member avoids overflow.
pub fn has_duration(mut durations: impl Iterator<Item = Option<u64>>, collection: bool) -> bool {
    if collection {
        durations.flatten().any(|duration| duration > 0)
    } else {
        durations.next().flatten().is_some()
    }
}
