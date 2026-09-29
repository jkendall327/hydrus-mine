//! Where the reference keeps each file and thumbnail on disk.
//!
//! A file with sha256 `H` (hex) and type `M` lives at
//! `<location>/<subfolders>/<H><ext(M)>`, and its thumbnail at
//! `<location>/<subfolders>/<H>.thumbnail`, where the subfolders come from
//! the prefix `f<first g hex chars>` (or `t...` for thumbnails) at storage
//! granularity `g`, split into two-character directories with the `f`/`t`
//! on the first one (`ClientFilesPhysical.FilesStorageSubfolder`):
//!
//! | granularity | prefix | directories |
//! |---|---|---|
//! | 1 | `fa` | `fa/` |
//! | 2 | `fab` | `fab/` |
//! | 3 | `fabc` | `fab/c/` |
//! | 4 | `fabcd` | `fab/cd/` |
//!
//! Update files of repositories have no extension.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use hydrus_core::{Mime, Sha256};

use crate::error::{LegacyError, Result};
use crate::readers::FileStorageConfig;

/// Files or thumbnails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrefixKind {
    File,
    Thumbnail,
}

impl PrefixKind {
    pub const fn letter(self) -> char {
        match self {
            PrefixKind::File => 'f',
            PrefixKind::Thumbnail => 't',
        }
    }
}

/// The storage prefix of a hash (`HydrusFilesPhysicalStorage.GetPrefix`).
pub fn prefix(hash: &Sha256, kind: PrefixKind, granularity: usize) -> String {
    let hex = hash.to_hex();
    let mut prefix = String::with_capacity(granularity + 1);
    prefix.push(kind.letter());
    prefix.push_str(&hex[..granularity.min(hex.len())]);
    prefix
}

/// The directories a prefix is stored under, relative to its location:
/// `"fbad"` becomes `["fba", "d"]`.
pub fn prefix_directories(prefix: &str) -> Vec<String> {
    let mut chars = prefix.chars();
    let Some(letter) = chars.next() else {
        return Vec::new();
    };
    let hex: Vec<char> = chars.collect();
    let mut directories: Vec<String> = hex.chunks(2).map(|c| c.iter().collect()).collect();
    match directories.first_mut() {
        Some(first) => first.insert(0, letter),
        None => directories.push(letter.to_string()),
    }
    directories
}

/// The file name of a file: `<sha256 hex><extension>`.
pub fn file_name(hash: &Sha256, mime: Mime) -> String {
    format!("{}{}", hash.to_hex(), mime.storage_extension())
}

/// The file name of a thumbnail: `<sha256 hex>.thumbnail`.
pub fn thumbnail_name(hash: &Sha256) -> String {
    format!("{}.thumbnail", hash.to_hex())
}

/// Resolve a stored *portable* path as the reference does
/// (`HydrusPaths.ConvertPortablePathToAbsPath`): relative paths are relative
/// to the database directory, and the result is lexically normalised.
///
/// A path written on Windows may use backslashes; like the reference, if the
/// resolved path does not exist, backslashes are treated as separators.
pub fn resolve_portable_path(db_dir: &Path, portable: &str) -> PathBuf {
    let resolve = |text: &str| {
        let path = Path::new(text);
        let joined = if path.is_absolute() {
            path.to_owned()
        } else {
            db_dir.join(path)
        };
        normalise(&joined)
    };
    let resolved = resolve(portable);
    if cfg!(not(windows)) && portable.contains('\\') && !resolved.exists() {
        return resolve(&portable.replace('\\', "/"));
    }
    resolved
}

/// `os.path.normpath`: drop `.` and empty components and fold `..` lexically.
fn normalise(path: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// Resolved storage directories for every prefix, for computing paths.
#[derive(Debug, Clone)]
pub struct FileLayout {
    granularity: usize,
    directories: HashMap<String, Vec<PathBuf>>,
}

impl FileLayout {
    /// Resolve a storage configuration against its database directory.
    pub fn new(config: &FileStorageConfig, db_dir: &Path) -> Result<FileLayout> {
        let mut directories: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for (prefix, location) in &config.subfolders {
            let base = config.locations.get(location).ok_or_else(|| {
                LegacyError::bad_value(
                    "client_files_subfolders",
                    format!("unknown location id {}", location.0),
                )
            })?;
            let mut path = resolve_portable_path(db_dir, base);
            path.extend(prefix_directories(prefix));
            directories.entry(prefix.clone()).or_default().push(path);
        }
        Ok(FileLayout {
            granularity: config.granularity,
            directories,
        })
    }

    /// Every directory a file of this hash may be in. Normally exactly one;
    /// none if the prefix has no subfolder (a damaged install).
    pub fn file_paths(&self, hash: &Sha256, mime: Mime) -> Vec<PathBuf> {
        self.paths(hash, PrefixKind::File, &file_name(hash, mime))
    }

    /// Every place a thumbnail of this hash may be.
    pub fn thumbnail_paths(&self, hash: &Sha256) -> Vec<PathBuf> {
        self.paths(hash, PrefixKind::Thumbnail, &thumbnail_name(hash))
    }

    /// The path the reference would use for a file (the first candidate).
    pub fn file_path(&self, hash: &Sha256, mime: Mime) -> Option<PathBuf> {
        self.file_paths(hash, mime).into_iter().next()
    }

    /// The path the reference would use for a thumbnail.
    pub fn thumbnail_path(&self, hash: &Sha256) -> Option<PathBuf> {
        self.thumbnail_paths(hash).into_iter().next()
    }

    /// Every prefix subfolder directory, for scanning storage.
    pub fn directories(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.directories
            .iter()
            .flat_map(|(prefix, paths)| paths.iter().map(move |p| (prefix.as_str(), p.as_path())))
    }

    fn paths(&self, hash: &Sha256, kind: PrefixKind, name: &str) -> Vec<PathBuf> {
        self.directories
            .get(&prefix(hash, kind, self.granularity))
            .map(|dirs| dirs.iter().map(|d| d.join(name)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_nesting() {
        assert_eq!(prefix_directories("fa"), vec!["fa"]);
        assert_eq!(prefix_directories("fab"), vec!["fab"]);
        assert_eq!(prefix_directories("fabc"), vec!["fab", "c"]);
        assert_eq!(prefix_directories("tabcd"), vec!["tab", "cd"]);
        assert_eq!(prefix_directories("f"), vec!["f"]);
    }

    #[test]
    fn portable_paths() {
        let db = Path::new("/db");
        assert_eq!(
            resolve_portable_path(db, "client_files"),
            Path::new("/db/client_files")
        );
        assert_eq!(
            resolve_portable_path(db, "../media/./x"),
            Path::new("/media/x")
        );
        assert_eq!(
            resolve_portable_path(db, "/abs//files/"),
            Path::new("/abs/files")
        );
        assert_eq!(resolve_portable_path(db, "/../x"), Path::new("/x"));
    }
}
