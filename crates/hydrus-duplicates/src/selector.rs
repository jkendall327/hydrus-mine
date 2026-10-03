//! Testing a pair against a rule's comparators to find which file is A
//! (the reference's `PairSelector.GetMatchingAB` and the comparators'
//! `Test`).

use hydrus_core::{HashId, Mime};
use hydrus_media::jpeg::JpegQuality;
use hydrus_media::visual::Verdict;
use hydrus_search::Clock;
use hydrus_search::media::{self, FileFacts};
use hydrus_store::duplicates::auto::{Comparator, LookingAt, OneFileTest, PairTest};

/// What the comparators that read a file's content ask for.
pub trait FileContent {
    /// The jpeg's encoding quality (unreadable: no quality, not
    /// progressive).
    fn jpeg_quality(&mut self, file: HashId) -> JpegQuality;
    /// The quick comparison of two images, and the detailed one if the
    /// quick one passes (and both can be read); none if either can't be
    /// read.
    fn visual_comparison(&mut self, a: HashId, b: HashId) -> Option<(Verdict, Option<Verdict>)>;

    /// How confident the regional comparison is that two images are
    /// visual duplicates (`VISUAL_DUPLICATES_RESULT_*`); none if either
    /// can't be read or they fail the simple comparison first, which the
    /// reference counts as a failed test whatever the threshold.
    fn visual_confidence(&mut self, a: HashId, b: HashId) -> Option<u8> {
        match self.visual_comparison(a, b)? {
            (simple, Some(regional)) if simple.similar => Some(regional.result),
            _ => None,
        }
    }
}

/// One file of a pair.
#[derive(Debug, Clone, Copy)]
pub struct File<'a> {
    pub id: HashId,
    pub facts: &'a FileFacts,
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

/// `Test(A, B)`.
pub fn test(
    c: &Comparator,
    a: File<'_>,
    b: File<'_>,
    clock: &Clock,
    content: &mut dyn FileContent,
) -> bool {
    match c {
        Comparator::OneFileMetadata {
            looking_at,
            predicates,
        } => {
            let matches = |f: &FileFacts| predicates.iter().all(|p| media::test(p, f, clock));
            match looking_at {
                LookingAt::A => matches(a.facts),
                LookingAt::B => matches(b.facts),
                LookingAt::Either => matches(a.facts) || matches(b.facts),
            }
        }
        Comparator::OneFileHardcoded { looking_at, test } => {
            let files: &[File<'_>] = match looking_at {
                LookingAt::A => &[a],
                LookingAt::B => &[b],
                LookingAt::Either => &[a, b],
            };
            let want = *test == OneFileTest::JpegIsProgressive;
            files.iter().any(|f| {
                f.facts.mime == Some(Mime::ImageJpeg)
                    && content.jpeg_quality(f.id).progressive == want
            })
        }
        Comparator::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => {
            let (Some(va), Some(vb)) = (
                media::extract(*property, a.facts),
                media::extract(*property, b.facts),
            ) else {
                return false;
            };
            media::number_test(test.op, vb * multiplier + *delta as f64, Some(va))
        }
        Comparator::Pair(PairTest::AHasClearlyBetterJpegQuality) => {
            if a.facts.mime != Some(Mime::ImageJpeg) || b.facts.mime != Some(Mime::ImageJpeg) {
                return false;
            }
            match (
                content.jpeg_quality(a.id).quality,
                content.jpeg_quality(b.id).quality,
            ) {
                (Some(qa), Some(qb)) if qa > 0.0 && qb > 0.0 => qa / qb < 0.7,
                _ => false,
            }
        }
        Comparator::Pair(test) => pair_test(*test, a.facts, b.facts),
        Comparator::VisualDuplicates { confidence } => {
            let image = |f: File<'_>| f.facts.mime.is_some_and(hydrus_media::mimes::is_image);
            image(a)
                && image(b)
                && content
                    .visual_confidence(a.id, b.id)
                    .is_some_and(|c| c >= *confidence)
        }
        Comparator::Or(members) => members.iter().any(|m| test(m, a, b, clock, content)),
        Comparator::And(members) => {
            !members.is_empty() && members.iter().all(|m| test(m, a, b, clock, content))
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
        PairTest::AHasClearlyBetterJpegQuality => unreachable!("tested with the files' content"),
    }
}

/// `GetMatchingAB`: which way round (if any) the pair passes every
/// comparator. `swap` puts the second file first before testing (the
/// reference shuffles the pair; pass a random bool for that). `Some(true)`
/// means the first given file is A.
pub fn matching_ab(
    comparators: &[Comparator],
    first: File<'_>,
    second: File<'_>,
    swap: bool,
    clock: &Clock,
    content: &mut dyn FileContent,
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
    let mut all = |list: &[Comparator], a: File<'_>, b: File<'_>| {
        list.iter().all(|c| test(c, a, b, clock, content))
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
