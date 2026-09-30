//! File properties that can be compared between two files.

/// A file property a relative comparator compares between two files
/// (`PREDICATE_TYPES_WE_CAN_EXTRACT_FROM_MEDIA_RESULTS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Comparable {
    Size,
    Width,
    Height,
    NumPixels,
    Ratio,
    Duration,
    Framerate,
    NumFrames,
    NumTags,
    NumUrls,
    ImportTime,
    ModifiedTime,
    LastViewedTime,
    ArchivedTime,
}

impl Comparable {
    /// The reference's predicate type code.
    pub const fn from_predicate_type(code: i64) -> Option<Self> {
        Some(match code {
            10 => Comparable::Size,
            13 => Comparable::Width,
            14 => Comparable::Height,
            24 => Comparable::NumPixels,
            15 => Comparable::Ratio,
            16 => Comparable::Duration,
            36 => Comparable::Framerate,
            37 => Comparable::NumFrames,
            8 => Comparable::NumTags,
            52 => Comparable::NumUrls,
            11 => Comparable::ImportTime,
            35 => Comparable::ModifiedTime,
            43 => Comparable::LastViewedTime,
            47 => Comparable::ArchivedTime,
            _ => return None,
        })
    }

    pub fn is_time(self) -> bool {
        matches!(
            self,
            Comparable::ImportTime
                | Comparable::ModifiedTime
                | Comparable::LastViewedTime
                | Comparable::ArchivedTime
        )
    }
}
