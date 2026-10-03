//! What a potential-duplicates search asks for: which pairs, matching
//! which file searches, and in what order. The duplicates page, the
//! duplicate filter, auto-resolution rules and the Client API share these.

use serde::{Deserialize, Serialize};

use crate::search::context::FileSearchContext;

/// How a potential pair must relate to the file searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairSearchKind {
    /// At least one of the pair matches the first search.
    OneFileMatchesOneSearch,
    /// Both match the first search.
    BothFilesMatchOneSearch,
    /// One matches the first search and the other the second.
    BothFilesMatchDifferentSearches,
}

impl PairSearchKind {
    /// From the reference's `DUPE_SEARCH_*` code.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::OneFileMatchesOneSearch,
            1 => Self::BothFilesMatchOneSearch,
            2 => Self::BothFilesMatchDifferentSearches,
            _ => return None,
        })
    }
}

/// Whether pairs whose files have identical pixels are wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelDuplicates {
    /// Only pixel duplicates, whatever their distance.
    Required,
    /// Any pair within the distance.
    Allowed,
    /// Pairs within the distance that are not pixel duplicates.
    Excluded,
}

impl PixelDuplicates {
    /// From the reference's `SIMILAR_FILES_PIXEL_DUPES_*` code.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::Required,
            1 => Self::Allowed,
            2 => Self::Excluded,
            _ => return None,
        })
    }
}

/// How pairs are ordered (`DUPE_PAIR_SORT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairOrder {
    /// By the larger file of the pair, then the smaller.
    MaxFilesize,
    /// By distance, then how different the two files' sizes are.
    Similarity,
    /// By the smaller file of the pair, then the larger.
    MinFilesize,
    Random,
}

impl PairOrder {
    /// From the reference's `DUPE_PAIR_SORT_*` code.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::MaxFilesize,
            1 => Self::Similarity,
            2 => Self::MinFilesize,
            3 => Self::Random,
            _ => return None,
        })
    }
}

/// A potential-duplicates search (the reference's
/// `PotentialDuplicatesSearchContext`): its file domain is the first file
/// search's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatesSearch {
    pub search_1: FileSearchContext,
    pub search_2: FileSearchContext,
    pub kind: PairSearchKind,
    pub pixel_duplicates: PixelDuplicates,
    pub max_hamming_distance: u32,
}
