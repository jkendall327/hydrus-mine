//! Testing a pair against a rule's comparators to find which file is A
//! (the reference's `PairSelector.GetMatchingAB` and the comparators'
//! `Test`).

use hydrus_search::Clock;
use hydrus_search::media::{self, FileFacts};
use hydrus_store::duplicates::auto::{Comparator, LookingAt, PairTest};

/// Whether a comparator needs the files' content (image data, jpeg
/// quantisation tables), which isn't supported yet: a rule with one does
/// its search but tests nothing, so its pairs wait rather than fail.
pub fn needs_file_content(c: &Comparator) -> bool {
    match c {
        Comparator::OneFileHardcoded { .. }
        | Comparator::VisualDuplicates { .. }
        | Comparator::Pair(PairTest::AHasClearlyBetterJpegQuality) => true,
        Comparator::Or(members) | Comparator::And(members) => {
            members.iter().any(needs_file_content)
        }
        _ => false,
    }
}

/// `IsFast`: cheap enough to try first.
fn is_fast(c: &Comparator) -> bool {
    match c {
        Comparator::OneFileMetadata { .. } | Comparator::RelativeFileInfo { .. } => true,
        // both one-file hardcoded tests read jpeg data
        Comparator::OneFileHardcoded { .. } | Comparator::VisualDuplicates { .. } => false,
        Comparator::Pair(test) => *test != PairTest::AHasClearlyBetterJpegQuality,
        Comparator::Or(members) | Comparator::And(members) => members.iter().all(is_fast),
    }
}

/// `OrderDoesNotMatter`: the same result with A and B swapped. (The
/// multiplier is compared exactly, as the reference does.)
#[allow(clippy::float_cmp)]
fn order_does_not_matter(c: &Comparator) -> bool {
    use hydrus_core::search::number::NumberOp;
    match c {
        Comparator::OneFileMetadata { looking_at, .. }
        | Comparator::OneFileHardcoded { looking_at, .. } => *looking_at == LookingAt::Either,
        Comparator::RelativeFileInfo {
            test,
            multiplier,
            delta,
            ..
        } => {
            matches!(test.op, NumberOp::Equal | NumberOp::NotEqual)
                && *multiplier == 1.0
                && *delta == 0
        }
        Comparator::Pair(test) => !matches!(
            test,
            PairTest::AHasClearlyBetterJpegQuality
                | PairTest::AHasSameOrBetterMetadataFlags
                | PairTest::AHasIccProfileIfBDoes
        ),
        Comparator::VisualDuplicates { .. } => true,
        Comparator::Or(members) | Comparator::And(members) => {
            members.iter().all(order_does_not_matter)
        }
    }
}

/// `Test(A, B)`. Comparators needing file content ([`needs_file_content`])
/// never pass.
pub fn test(c: &Comparator, a: &FileFacts, b: &FileFacts, clock: &Clock) -> bool {
    match c {
        Comparator::OneFileMetadata {
            looking_at,
            predicates,
        } => {
            let matches = |f: &FileFacts| predicates.iter().all(|p| media::test(p, f, clock));
            match looking_at {
                LookingAt::A => matches(a),
                LookingAt::B => matches(b),
                LookingAt::Either => matches(a) || matches(b),
            }
        }
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => {
            let (Some(va), Some(vb)) = (media::extract(*property, a), media::extract(*property, b))
            else {
                return false;
            };
            media::number_test(test.op, vb * multiplier + *delta as f64, Some(va))
        }
        Comparator::Pair(test) => pair_test(*test, a, b),
        Comparator::OneFileHardcoded { .. } | Comparator::VisualDuplicates { .. } => false,
        Comparator::Or(members) => members.iter().any(|m| test(m, a, b, clock)),
        Comparator::And(members) => {
            !members.is_empty() && members.iter().all(|m| test(m, a, b, clock))
        }
    }
}

fn pair_test(t: PairTest, a: &FileFacts, b: &FileFacts) -> bool {
    match t {
        PairTest::FiletypeSame => a.mime == b.mime,
        PairTest::FiletypeDiffers => a.mime != b.mime,
        PairTest::HasExifSame => a.has_exif == b.has_exif,
        PairTest::HasIccProfileSame => a.has_icc_profile == b.has_icc_profile,
        PairTest::AHasSameOrBetterMetadataFlags => [
            (a.has_exif, b.has_exif),
            (a.has_xmp, b.has_xmp),
            (a.has_iptc, b.has_iptc),
            (a.has_software_source, b.has_software_source),
            (
                a.has_human_readable_embedded_metadata,
                b.has_human_readable_embedded_metadata,
            ),
        ]
        .iter()
        .all(|&(a, b)| a || !b),
        PairTest::AHasIccProfileIfBDoes => !b.has_icc_profile || a.has_icc_profile,
        // needs the jpeg quantisation tables
        PairTest::AHasClearlyBetterJpegQuality => false,
    }
}

/// `GetMatchingAB`: which way round (if any) the pair passes every
/// comparator. `swap` puts the second file first before testing (the
/// reference shuffles the pair; pass a random bool for that). `Some(true)`
/// means the first given file is A.
pub fn matching_ab(
    comparators: &[Comparator],
    first: &FileFacts,
    second: &FileFacts,
    swap: bool,
    clock: &Clock,
) -> Option<bool> {
    let (one, two) = if swap {
        (second, first)
    } else {
        (first, second)
    };
    let one_is_first = !swap;
    if comparators.is_empty() {
        return Some(one_is_first);
    }
    let fast: Vec<&Comparator> = comparators.iter().filter(|c| is_fast(c)).collect();
    let slow: Vec<&Comparator> = comparators.iter().filter(|c| !is_fast(c)).collect();
    let split = |list: &[&Comparator]| -> (Vec<Comparator>, Vec<Comparator>) {
        let (symmetric, directed): (Vec<&Comparator>, Vec<&Comparator>) =
            list.iter().partition(|c| order_does_not_matter(c));
        (
            symmetric.into_iter().cloned().collect(),
            directed.into_iter().cloned().collect(),
        )
    };
    let (fast_symmetric, fast_directed) = split(&fast);
    let (slow_symmetric, slow_directed) = split(&slow);
    let all = |list: &[Comparator], a: &FileFacts, b: &FileFacts| {
        list.iter().all(|c| test(c, a, b, clock))
    };

    let mut one_two = true;
    let mut two_one = true;
    if !all(&fast_symmetric, one, two) {
        one_two = false;
        two_one = false;
    }
    if one_two && !all(&fast_directed, one, two) {
        one_two = false;
    }
    if two_one && !all(&fast_directed, two, one) {
        two_one = false;
    }
    if (one_two || two_one) && !all(&slow_symmetric, one, two) {
        one_two = false;
        two_one = false;
    }
    if one_two && !all(&slow_directed, one, two) {
        one_two = false;
    }
    if one_two {
        return Some(one_is_first);
    }
    if two_one && !all(&slow_directed, two, one) {
        two_one = false;
    }
    two_one.then_some(!one_is_first)
}
