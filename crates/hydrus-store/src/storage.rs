//! Where media files and thumbnails live on disk.
//!
//! The layout is the reference's, unchanged (ARCHITECTURE.md, ADR-2): files
//! under `f<hex prefix>` subfolders, thumbnails under `t<hex prefix>`, each
//! subfolder assigned to a base location. With granularity 2 a file is at
//! `<base>/f3a/<sha256 hex><ext>`; with granularity 3 at
//! `<base>/f3a/b/<sha256 hex><ext>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use hydrus_core::{Mime, Sha256};

use crate::error::{Result, StoreError};

/// A base directory holding some of the subfolders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLocation {
    pub path: PathBuf,
    pub ideal_weight: Option<i64>,
    pub max_bytes: Option<i64>,
    /// Subfolder prefixes (e.g. `f00`, `t3a`) stored here.
    pub prefixes: Vec<String>,
}

/// The on-disk layout of media and thumbnails.
#[derive(Debug, Clone, Default)]
pub struct FileStorage {
    /// Hex characters in a prefix (not counting the `f`/`t`).
    granularity: usize,
    prefix_to_base: HashMap<String, PathBuf>,
    locations: Vec<StorageLocation>,
}

impl FileStorage {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut stmt = conn.prepare(
            "SELECT l.path, l.ideal_weight, l.max_bytes, s.prefix
             FROM storage_locations l LEFT JOIN storage_subfolders s USING (location_id)
             ORDER BY l.path, s.prefix",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut storage = FileStorage::default();
        for row in rows {
            let (path, ideal_weight, max_bytes, prefix) = row?;
            let path = PathBuf::from(path);
            if storage.locations.last().is_none_or(|l| l.path != path) {
                storage.locations.push(StorageLocation {
                    path: path.clone(),
                    ideal_weight,
                    max_bytes,
                    prefixes: Vec::new(),
                });
            }
            if let Some(prefix) = prefix {
                let granularity = prefix.len().saturating_sub(1);
                if storage.granularity != 0 && storage.granularity != granularity {
                    return Err(StoreError::Corrupt(format!(
                        "mixed storage granularity: {prefix}"
                    )));
                }
                storage.granularity = granularity;
                storage.prefix_to_base.insert(prefix.clone(), path.clone());
                if let Some(l) = storage.locations.last_mut() {
                    l.prefixes.push(prefix);
                }
            }
        }
        Ok(storage)
    }

    pub fn locations(&self) -> &[StorageLocation] {
        &self.locations
    }

    fn dir_for(&self, kind: char, hash: &Sha256) -> Option<PathBuf> {
        let hex = hash.to_hex();
        let prefix = format!("{kind}{}", &hex[..self.granularity.min(hex.len())]);
        let base = self.prefix_to_base.get(&prefix)?;
        // 'f3ab' -> ['f3a', 'b']: the first folder carries the kind letter
        let hex_prefix = &prefix[1..];
        let mut dir = base.join(format!("{kind}{}", &hex_prefix[..hex_prefix.len().min(2)]));
        let mut rest = hex_prefix.get(2..).unwrap_or("");
        while !rest.is_empty() {
            let take = rest.len().min(2);
            dir = dir.join(&rest[..take]);
            rest = &rest[take..];
        }
        Some(dir)
    }

    /// The path of a media file.
    pub fn file_path(&self, hash: &Sha256, mime: Mime) -> Option<PathBuf> {
        Some(self.dir_for('f', hash)?.join(format!(
            "{}{}",
            hash.to_hex(),
            mime.storage_extension()
        )))
    }

    /// The path of a thumbnail.
    pub fn thumbnail_path(&self, hash: &Sha256) -> Option<PathBuf> {
        Some(
            self.dir_for('t', hash)?
                .join(format!("{}.thumbnail", hash.to_hex())),
        )
    }
}

/// Whether `path` exists as a file.
pub fn exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(granularity: usize) -> FileStorage {
        let mut s = FileStorage {
            granularity,
            ..FileStorage::default()
        };
        for kind in ['f', 't'] {
            let hex = "3ab".get(..granularity).unwrap().to_owned();
            s.prefix_to_base
                .insert(format!("{kind}{hex}"), PathBuf::from("/base"));
        }
        s
    }

    #[test]
    fn paths_follow_the_reference_layout() {
        let hash: Sha256 = format!("3ab{}", "0".repeat(61)).parse().unwrap();
        let s = storage(2);
        assert_eq!(
            s.file_path(&hash, Mime::ImageJpeg).unwrap(),
            PathBuf::from(format!("/base/f3a/{hash}.jpg"))
        );
        assert_eq!(
            s.thumbnail_path(&hash).unwrap(),
            PathBuf::from(format!("/base/t3a/{hash}.thumbnail"))
        );
        let s = storage(3);
        assert_eq!(
            s.file_path(&hash, Mime::ImagePng).unwrap(),
            PathBuf::from(format!("/base/f3a/b/{hash}.png"))
        );
        assert_eq!(
            s.file_path(&hash, Mime::ApplicationHydrusUpdateContent)
                .unwrap(),
            PathBuf::from(format!("/base/f3a/b/{hash}"))
        );
    }
}
