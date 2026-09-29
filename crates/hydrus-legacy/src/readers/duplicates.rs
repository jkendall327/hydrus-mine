//! The duplicate files system.
//!
//! Files that are duplicates of each other form a *media group*
//! (`duplicate_file_members`) with one best file, the king
//! (`duplicate_files`). Media groups that are alternates of each other form
//! an *alternates group*. False positives are recorded between alternates
//! groups, and potential pairs (found by similar-file search, not yet
//! decided by the user) between media groups.

use hydrus_core::{DuplicateType, HashId, TimestampMs};

use super::{column, timestamp_column};
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};
use crate::rows::{Paging, Rows};

macro_rules! group_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);

        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let raw = value.as_i64()?;
                u32::try_from(raw)
                    .map($name)
                    .map_err(|_| rusqlite::types::FromSqlError::OutOfRange(raw))
            }
        }
    };
}

group_id!(
    /// A group of files that are duplicates of each other.
    MediaId
);
group_id!(
    /// A group of media groups that are alternates of each other.
    AlternatesGroupId
);

/// A pair of media groups the similar-files search found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotentialDuplicatePair {
    pub smaller_media_id: MediaId,
    pub larger_media_id: MediaId,
    /// Hamming distance of the perceptual hashes that matched.
    pub distance: i64,
}

/// A pair a duplicates auto-resolution rule proposed and the user declined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclinedPair {
    pub smaller_media_id: MediaId,
    pub larger_media_id: MediaId,
    pub declined: Option<TimestampMs>,
}

/// A pair a duplicates auto-resolution rule acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionedPair {
    pub hash_id_a: HashId,
    pub hash_id_b: HashId,
    pub duplicate_type: DuplicateType,
    pub actioned: Option<TimestampMs>,
}

impl LegacyDb {
    fn pair_rows<A, B>(
        &self,
        table: &'static str,
        a: &'static str,
        b: &'static str,
    ) -> Result<Rows<'_, (A, B)>>
    where
        A: rusqlite::types::FromSql + 'static,
        B: rusqlite::types::FromSql + 'static,
    {
        let qualified = self.table("main", table)?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &qualified,
                keys: &[a, b],
                columns: &[],
                join: "",
            },
            Box::new(move |row| Ok((column(row, 0, table)?, column(row, 1, table)?))),
        ))
    }

    /// Media groups and their king (`duplicate_files`).
    pub fn duplicate_kings(&self) -> Result<Rows<'_, (MediaId, HashId)>> {
        let table = self.table("main", "duplicate_files")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["media_id"],
                columns: &["king_hash_id"],
                join: "",
            },
            Box::new(|row| {
                Ok((
                    column(row, 0, "duplicate_files")?,
                    column(row, 1, "duplicate_files")?,
                ))
            }),
        ))
    }

    /// Which files are in which media group (`duplicate_file_members`).
    pub fn duplicate_members(&self) -> Result<Rows<'_, (MediaId, HashId)>> {
        self.pair_rows("duplicate_file_members", "media_id", "hash_id")
    }

    /// Every alternates group (`alternate_file_groups`).
    pub fn alternates_groups(&self) -> Result<Rows<'_, AlternatesGroupId>> {
        let table = self.table("main", "alternate_file_groups")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["alternates_group_id"],
                columns: &[],
                join: "",
            },
            Box::new(|row| column(row, 0, "alternate_file_groups")),
        ))
    }

    /// Which media groups are in which alternates group
    /// (`alternate_file_group_members`).
    pub fn alternates_members(&self) -> Result<Rows<'_, (AlternatesGroupId, MediaId)>> {
        self.pair_rows(
            "alternate_file_group_members",
            "alternates_group_id",
            "media_id",
        )
    }

    /// Media group pairs the user confirmed as alternates, as opposed to
    /// merely not-duplicates (`confirmed_alternate_pairs`).
    pub fn confirmed_alternate_pairs(&self) -> Result<Rows<'_, (MediaId, MediaId)>> {
        self.pair_rows(
            "confirmed_alternate_pairs",
            "smaller_media_id",
            "larger_media_id",
        )
    }

    /// Alternates group pairs that are not related (`duplicate_false_positives`).
    pub fn false_positive_pairs(&self) -> Result<Rows<'_, (AlternatesGroupId, AlternatesGroupId)>> {
        self.pair_rows(
            "duplicate_false_positives",
            "smaller_alternates_group_id",
            "larger_alternates_group_id",
        )
    }

    /// Undecided potential duplicate pairs (`potential_duplicate_pairs`).
    pub fn potential_duplicate_pairs(&self) -> Result<Rows<'_, PotentialDuplicatePair>> {
        let table = self.table("main", "potential_duplicate_pairs")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["smaller_media_id", "larger_media_id"],
                columns: &["distance"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "potential_duplicate_pairs";
                Ok(PotentialDuplicatePair {
                    smaller_media_id: column(row, 0, T)?,
                    larger_media_id: column(row, 1, T)?,
                    distance: column(row, 2, T)?,
                })
            }),
        ))
    }

    /// How far the similar-files search has searched from each file
    /// (`shape_search_cache`; `None` means not yet searched). Derived, but
    /// expensive to recompute, so importing it avoids re-searching.
    pub fn similar_files_search_progress(&self) -> Result<Rows<'_, (HashId, Option<i64>)>> {
        let table = self.table("main", "shape_search_cache")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &["searched_distance"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "shape_search_cache";
                Ok((column(row, 0, T)?, column(row, 1, T)?))
            }),
        ))
    }

    /// Pixel-for-pixel duplicate hashes: file hash id to the hash id of its
    /// pixel hash (`pixel_hash_map`).
    pub fn pixel_hash_map(&self) -> Result<Rows<'_, (HashId, HashId)>> {
        self.pair_rows("pixel_hash_map", "hash_id", "pixel_hash_id")
    }

    /// Ids of the duplicates auto-resolution rules
    /// (`duplicate_files_auto_resolution_rules`); each rule's settings are a
    /// named object of type 128 in [`LegacyDb::json_dumps_named`].
    pub fn auto_resolution_rule_ids(&self) -> Result<Vec<i64>> {
        let table = self.table("main", "duplicate_files_auto_resolution_rules")?;
        let mut statement = self
            .connection()
            .prepare(&format!("SELECT rule_id FROM {table} ORDER BY rule_id"))?;
        let ids = statement.query_map([], |row| row.get(0))?;
        Ok(ids.collect::<rusqlite::Result<_>>()?)
    }

    /// Pairs the user declined for an auto-resolution rule
    /// (`duplicate_files_auto_resolution_declined_N`). The rule's other
    /// queues are derived from its search and are not exposed.
    pub fn auto_resolution_declined_pairs(&self, rule_id: i64) -> Result<Rows<'_, DeclinedPair>> {
        let table = self.table(
            "main",
            &format!("duplicate_files_auto_resolution_declined_{rule_id}"),
        )?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["smaller_media_id", "larger_media_id"],
                columns: &["timestamp_ms"],
                join: "",
            },
            Box::new(move |row| {
                Ok(DeclinedPair {
                    smaller_media_id: column(row, 0, &name)?,
                    larger_media_id: column(row, 1, &name)?,
                    declined: timestamp_column(row, 2, &name)?,
                })
            }),
        ))
    }

    /// The history of what an auto-resolution rule did
    /// (`duplicate_files_auto_resolution_actioned_N`), in insertion order.
    pub fn auto_resolution_actioned_pairs(&self, rule_id: i64) -> Result<Rows<'_, ActionedPair>> {
        let table = self.table(
            "main",
            &format!("duplicate_files_auto_resolution_actioned_{rule_id}"),
        )?;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["rowid"],
                columns: &["hash_id_a", "hash_id_b", "duplicate_type", "timestamp_ms"],
                join: "",
            },
            Box::new(move |row| {
                let code: i64 = column(row, 3, &name)?;
                let duplicate_type = u8::try_from(code)
                    .ok()
                    .and_then(DuplicateType::from_code)
                    .ok_or_else(|| {
                        LegacyError::bad_value(&name, format!("unknown duplicate type {code}"))
                    })?;
                Ok(ActionedPair {
                    hash_id_a: column(row, 1, &name)?,
                    hash_id_b: column(row, 2, &name)?,
                    duplicate_type,
                    actioned: timestamp_column(row, 4, &name)?,
                })
            }),
        ))
    }
}
