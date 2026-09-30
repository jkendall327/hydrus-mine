//! Import folders' settings (`ClientImportLocal.ImportFolder`) and the
//! filename tagging they can do (`FilenameTaggingOptions`).
//!
//! An import folder is stored as an import queue: its name, its own import
//! options and whether it is paused are the queue's, its file seed cache is
//! the queue's file seeds, and the rest is an [`ImportFolderSettings`] in
//! the queue's extra JSON.

use std::collections::BTreeSet;

use hydrus_core::url::strings::PyRegex;
use serde::{Deserialize, Serialize};

use crate::sidecar::Router;

/// What to do with a file once the folder has tried it
/// (`IMPORT_FOLDER_DELETE`, `_IGNORE`, `_MOVE`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderAction {
    /// Leave it, and remember it so it isn't tried again.
    #[default]
    Ignore,
    /// Delete it (to the recycle bin where there is one) and its sidecars.
    Delete,
    /// Move it and its sidecars to a directory.
    Move(String),
}

/// The actions for each outcome the reference acts on.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderActions {
    pub successful_and_new: FolderAction,
    pub successful_but_redundant: FolderAction,
    pub deleted: FolderAction,
    pub error: FolderAction,
}

/// `FilenameTaggingOptions`: tags for a file from its path.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FilenameTagging {
    pub tags_for_all: BTreeSet<String>,
    /// The filename (without extension) as a tag, in this namespace
    /// (empty for none).
    pub add_filename: Option<String>,
    /// Directory names as tags: `(index, namespace)` for the enabled ones,
    /// indexes counting from the top (0, 1, 2) or the file (-1, -2, -3).
    pub directories: Vec<(i64, String)>,
    /// Every match of each regex in the path, in a namespace.
    pub quick_namespaces: Vec<(String, String)>,
    /// Every match of each regex in the path.
    pub regexes: Vec<String>,
}

/// Python's `os.path.splitext` on a file name: the extension is from the
/// last dot, unless the dots are all leading.
fn split_ext(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if name[..i].chars().any(|c| c != '.') => &name[..i],
        _ => name,
    }
}

fn is_sep(c: char) -> bool {
    c == std::path::MAIN_SEPARATOR || (cfg!(windows) && c == '/')
}

/// Python's `os.path.split`: `(head, tail)`, the head without trailing
/// separators unless it is all separators.
fn split(path: &str) -> (&str, &str) {
    match path.rfind(is_sep) {
        None => ("", path),
        Some(i) => {
            let head = &path[..=i];
            let trimmed = head.trim_end_matches(is_sep);
            (
                if trimmed.is_empty() { head } else { trimmed },
                &path[i + 1..],
            )
        }
    }
}

/// Python's `os.path.splitdrive`, for the part after the drive.
fn after_drive(path: &str) -> &str {
    if !cfg!(windows) {
        return path;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return &path[2..];
    }
    // a UNC path: \\server\share
    let normal: String = path
        .chars()
        .map(|c| if c == '/' { '\\' } else { c })
        .collect();
    if normal.starts_with("\\\\") && !normal.starts_with("\\\\\\") {
        let rest = &normal[2..];
        if let Some(i) = rest.find('\\') {
            let after_server = &rest[i + 1..];
            let share_end = after_server.find('\\').map_or(after_server.len(), |j| j);
            if share_end > 0 {
                return &path[2 + i + 1 + share_end..];
            }
        }
    }
    path
}

/// Python's `re.findall`: each match, or its one group, or each of its
/// groups (a group that didn't take part gives an empty string).
fn find_all(regex: &str, text: &str) -> Vec<String> {
    let regex = PyRegex::new(regex);
    let Ok(regex) = regex.regex() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for captures in regex.captures_iter(text) {
        let Ok(captures) = captures else {
            break;
        };
        let groups = captures.len() - 1;
        if groups == 0 {
            out.push(captures.get(0).map_or("", |m| m.as_str()).to_owned());
        } else {
            for g in 1..=groups {
                out.push(captures.get(g).map_or("", |m| m.as_str()).to_owned());
            }
        }
    }
    out
}

impl FilenameTagging {
    /// `GetTags`: the cleaned tags for the file at `path`.
    pub fn tags(&self, path: &str) -> BTreeSet<String> {
        let mut tags: Vec<String> = self.tags_for_all.iter().cloned().collect();
        let (base, filename) = split(path);
        let stem = split_ext(filename);
        if let Some(namespace) = &self.add_filename {
            tags.push(if namespace.is_empty() {
                stem.to_owned()
            } else {
                format!("{namespace}:{stem}")
            });
        }
        let dirs = after_drive(base).trim_start_matches(std::path::MAIN_SEPARATOR);
        let directories: Vec<&str> = dirs.split(std::path::MAIN_SEPARATOR).collect();
        let len = directories.len() as i64;
        for (index, namespace) in &self.directories {
            let i = if *index < 0 { len + index } else { *index };
            let Some(directory) = usize::try_from(i).ok().and_then(|i| directories.get(i)) else {
                continue;
            };
            tags.push(if namespace.is_empty() {
                (*directory).to_owned()
            } else {
                format!("{namespace}:{directory}")
            });
        }
        for regex in &self.regexes {
            tags.extend(find_all(regex, path));
        }
        for (namespace, regex) in &self.quick_namespaces {
            tags.extend(
                find_all(regex, path)
                    .into_iter()
                    .map(|m| format!("{namespace}:{m}")),
            );
        }
        tags.iter()
            .filter_map(|t| hydrus_core::tag::clean_tag_checked(t))
            .collect()
    }
}

/// What an import folder keeps besides its queue's name, options, paused
/// state and file seeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportFolderSettings {
    pub path: String,
    pub search_subdirectories: bool,
    /// Sidecars and other metadata to bring in with each file.
    pub routers: Vec<Router>,
    /// Tags from each file's path, by tag service (hex key).
    pub filename_tagging: Vec<(String, FilenameTagging)>,
    pub actions: FolderActions,
    /// Seconds between checks.
    pub period: i64,
    pub check_regularly: bool,
    pub last_checked: i64,
    pub check_now: bool,
    /// Files modified more recently than this many seconds ago are left
    /// for the next check (they may still be being written).
    pub last_modified_time_skip_period: i64,
    pub show_working_popup: bool,
    pub publish_files_to_popup_button: bool,
    pub publish_files_to_page: bool,
}

impl Default for ImportFolderSettings {
    fn default() -> Self {
        Self {
            path: String::new(),
            search_subdirectories: true,
            routers: Vec::new(),
            filename_tagging: Vec::new(),
            actions: FolderActions::default(),
            period: 3600,
            check_regularly: true,
            last_checked: 0,
            check_now: false,
            last_modified_time_skip_period: 60,
            show_working_popup: true,
            publish_files_to_popup_button: true,
            publish_files_to_page: false,
        }
    }
}

impl ImportFolderSettings {
    /// `GetNextWorkTime`: when the folder is next due, if ever.
    pub fn next_work_time(&self, paused: bool, now: i64) -> Option<i64> {
        if paused {
            None
        } else if self.check_now {
            Some(now)
        } else if self.check_regularly {
            Some(self.last_checked + self.period)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_tags_come_from_the_path_like_the_reference() {
        let tagging = FilenameTagging {
            tags_for_all: ["Imported".to_owned()].into(),
            add_filename: Some("title".into()),
            directories: vec![(0, String::new()), (-1, "folder".into()), (5, "x".into())],
            quick_namespaces: vec![("page".into(), r"p(\d+)".into())],
            regexes: vec![r"(\w+)-(\w+)".into(), "[".into()],
        };
        let sep = std::path::MAIN_SEPARATOR;
        let path = format!("{sep}home{sep}art{sep}blue-eyes p12.jpg");
        let tags: Vec<String> = tagging.tags(&path).into_iter().collect();
        assert_eq!(
            tags,
            vec![
                "blue",
                "eyes",
                "folder:art",
                "home",
                "imported",
                "page:12",
                "title:blue-eyes p12"
            ]
        );
        assert_eq!(split_ext(".hidden"), ".hidden");
        assert_eq!(split_ext("a.tar.gz"), "a.tar");
    }
}

/// `HC.EXPORT_FOLDER_TYPE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportType {
    /// Files are added; nothing is removed.
    #[default]
    Regular,
    /// The folder is made to hold exactly the search's files (and their
    /// sidecars): anything else in it is deleted.
    Synchronise,
}

/// An export folder (`ClientExportingFiles.ExportFolder`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportFolder {
    pub name: String,
    pub path: String,
    pub export_type: ExportType,
    /// Delete the files from the client once exported (never when
    /// synchronising).
    pub delete_from_client_after_export: bool,
    /// Link to the files in the client's storage instead of copying them.
    pub export_symlinks: bool,
    pub search: hydrus_core::search::context::FileSearchContext,
    /// Sidecars (or other metadata) to write with each file.
    pub routers: Vec<Router>,
    pub run_regularly: bool,
    /// Seconds between runs.
    pub period: i64,
    /// How files are named (`[namespace]`, `{hash}`, `(tag)`, ...).
    pub phrase: String,
    pub last_checked: i64,
    pub run_now: bool,
    pub last_error: String,
    pub show_working_popup: bool,
    pub overwrite_sidecars_on_next_run: bool,
    pub always_overwrite_sidecars: bool,
}

impl ExportFolder {
    /// `DoWork`'s test: whether a run is due.
    pub fn is_due(&self, now: i64) -> bool {
        (self.run_regularly && now > self.last_checked + self.period) || self.run_now
    }
}

/// A piece of an export phrase (`ParseExportPhrase`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhraseTerm {
    /// Literal text.
    Text(String),
    /// `[namespace]`: the file's tags in the namespace, as subtags.
    Namespace(String),
    /// `{hash}`, `{tags}`, `{nn tags}`, `{file_id}`, `{#}` (anything else
    /// gives nothing).
    Predicate(String),
    /// `(tag)`: the tag's subtag, if the file has that subtag as a tag.
    Tag(String),
}

/// `ParseExportPhrase`: split out `[...]`, then `{...}`, then `(...)`.
/// An opening bracket without its closing one is an error.
pub fn parse_export_phrase(phrase: &str) -> Result<Vec<PhraseTerm>, String> {
    fn split(
        terms: Vec<PhraseTerm>,
        open: char,
        close: char,
        make: fn(String) -> PhraseTerm,
    ) -> Result<Vec<PhraseTerm>, String> {
        let mut out = Vec::new();
        for term in terms {
            let PhraseTerm::Text(mut text) = term else {
                out.push(term);
                continue;
            };
            while let Some((pre, rest)) = text.split_once(open) {
                let Some((inner, rest)) = rest.split_once(close) else {
                    return Err(
                        "Could not parse that phrase: not enough values to unpack (expected 2, got 1)"
                            .to_owned(),
                    );
                };
                out.push(PhraseTerm::Text(pre.to_owned()));
                out.push(make(inner.to_owned()));
                let rest = rest.to_owned();
                text = rest;
            }
            out.push(PhraseTerm::Text(text));
        }
        Ok(out)
    }
    let terms = vec![PhraseTerm::Text(phrase.to_owned())];
    let terms = split(terms, '[', ']', PhraseTerm::Namespace)?;
    let terms = split(terms, '{', '}', PhraseTerm::Predicate)?;
    split(terms, '(', ')', PhraseTerm::Tag)
}
