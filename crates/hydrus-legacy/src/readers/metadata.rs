//! Per-file metadata other than tags: URLs, notes, ratings, recent tags.

use hydrus_core::{HashId, LabelId, NoteId, ServiceId, TagId, TimestampMs, UrlId};

use super::{column, timestamp_column};
use crate::db::LegacyDb;
use crate::error::Result;
use crate::rows::{Paging, Rows};

/// A note on a file: its name (a label) and body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileNote {
    pub hash_id: HashId,
    pub name_id: LabelId,
    pub note_id: NoteId,
}

/// A like/dislike or numerical rating (`local_ratings`).
///
/// Like/dislike ratings are 1.0 (like) or 0.0 (dislike). Numerical ratings
/// are fractions in `0.0..=1.0`; convert with
/// [`crate::objects::NumericalRatingConfig::stars_from_rating`]. Unrated
/// files have no row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rating {
    pub service_id: ServiceId,
    pub hash_id: HashId,
    pub value: f64,
}

/// An inc/dec (counter) rating (`local_incdec_ratings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncDecRating {
    pub service_id: ServiceId,
    pub hash_id: HashId,
    pub value: i64,
}

/// A recently used tag, for the "recent tags" suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentTag {
    pub service_id: ServiceId,
    pub tag_id: TagId,
    pub last_used: Option<TimestampMs>,
}

impl LegacyDb {
    /// Which files have which URLs (`url_map`).
    pub fn url_map(&self) -> Result<Rows<'_, (HashId, UrlId)>> {
        let table = self.table("main", "url_map")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "url_id"],
                columns: &[],
                join: "",
            },
            Box::new(|row| Ok((column(row, 0, "url_map")?, column(row, 1, "url_map")?))),
        ))
    }

    /// File notes (`file_notes`); names and bodies are in [`LegacyDb::labels`]
    /// and [`LegacyDb::notes`].
    pub fn file_notes(&self) -> Result<Rows<'_, FileNote>> {
        let table = self.table("main", "file_notes")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "name_id"],
                columns: &["note_id"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "file_notes";
                Ok(FileNote {
                    hash_id: column(row, 0, T)?,
                    name_id: column(row, 1, T)?,
                    note_id: column(row, 2, T)?,
                })
            }),
        ))
    }

    /// Like/dislike and numerical ratings of every local rating service.
    pub fn ratings(&self) -> Result<Rows<'_, Rating>> {
        let table = self.table("main", "local_ratings")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "hash_id"],
                columns: &["rating"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "local_ratings";
                Ok(Rating {
                    service_id: column(row, 0, T)?,
                    hash_id: column(row, 1, T)?,
                    value: column(row, 2, T)?,
                })
            }),
        ))
    }

    /// Inc/dec ratings of every inc/dec rating service.
    pub fn incdec_ratings(&self) -> Result<Rows<'_, IncDecRating>> {
        let table = self.table("main", "local_incdec_ratings")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "hash_id"],
                columns: &["rating"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "local_incdec_ratings";
                Ok(IncDecRating {
                    service_id: column(row, 0, T)?,
                    hash_id: column(row, 1, T)?,
                    value: column(row, 2, T)?,
                })
            }),
        ))
    }

    /// Recently used tags per tag service (`recent_tags`).
    pub fn recent_tags(&self) -> Result<Rows<'_, RecentTag>> {
        let table = self.table("main", "recent_tags")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["service_id", "tag_id"],
                columns: &["timestamp_ms"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "recent_tags";
                Ok(RecentTag {
                    service_id: column(row, 0, T)?,
                    tag_id: column(row, 1, T)?,
                    last_used: timestamp_column(row, 2, T)?,
                })
            }),
        ))
    }
}
