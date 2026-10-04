//! The "filename tagging" dialog for files about to be imported from disk
//! (the reference's `EditLocalImportFilenameTaggingPanel`): for each tag
//! service, filename tagging options (hydrus-parse's [`FilenameTagging`])
//! and the dialog's own extras, tags entered for selected files and a
//! number per file; each path's row and the tags the import gets.
//! Recorded by `oracle/record_filename_tagging.py`.

use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::numbers::human_int;
use hydrus_core::tag::clean_tag_checked;
use hydrus_parse::folders::FilenameTagging;

/// The paths list's column titles.
pub const COLUMNS: [&str; 3] = ["#", "path", "metadata"];

/// One tag service's tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTagging {
    pub options: FilenameTagging,
    /// Tags for some files alone, by path index ("tags just for selected
    /// files").
    pub single: BTreeMap<usize, BTreeSet<String>>,
    /// The "#" box: a number for each file, from `base` by `step`, as a
    /// tag in `namespace` (none while it is blank).
    pub number_base: i64,
    pub number_step: i64,
    pub number_namespace: String,
}

impl Default for ServiceTagging {
    fn default() -> Self {
        Self {
            options: FilenameTagging::default(),
            single: BTreeMap::new(),
            number_base: 1,
            number_step: 1,
            number_namespace: String::new(),
        }
    }
}

impl ServiceTagging {
    /// The tags the file at `path` (the list's `index`th) gets, sorted.
    pub fn tags(&self, index: usize, path: &str) -> Vec<String> {
        let mut tags = self.options.tags(path);
        let mut extra: Vec<String> = self
            .single
            .get(&index)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        if !self.number_namespace.is_empty() {
            let n = self.number_base + i64::try_from(index).unwrap_or(i64::MAX) * self.number_step;
            extra.push(format!("{}:{n}", self.number_namespace));
        }
        tags.extend(extra.iter().filter_map(|t| clean_tag_checked(t)));
        tags.into_iter().collect()
    }

    /// Tags entered for the selected files.
    pub fn add_single(&mut self, selected: &[usize], tags: &[String]) {
        let tags: Vec<String> = tags.iter().filter_map(|t| clean_tag_checked(t)).collect();
        // SimplePanel.EnterTagsSingle does not touch per-path storage when
        // there are no tags. An unchanged owned child must preserve that too.
        if tags.is_empty() {
            return;
        }
        for &i in selected {
            self.single
                .entry(i)
                .or_default()
                .extend(tags.iter().cloned());
        }
    }

    /// The selected files' single tags, as their box shows them: every
    /// one any of them has.
    pub fn selected_single(&self, selected: &[usize]) -> Vec<String> {
        let tags: BTreeSet<&String> = selected
            .iter()
            .filter_map(|i| self.single.get(i))
            .flatten()
            .collect();
        tags.into_iter().cloned().collect()
    }

    /// Apply an owned additive editor to a frozen file selection. Untouched
    /// union tags remain attached to their original files; explicit entry of
    /// an existing union tag spreads it to every selected file, as Qt does.
    pub fn apply_selected(
        &mut self,
        selected: &[usize],
        before: &[String],
        after: &[String],
        additions: &[String],
    ) {
        let removed: Vec<_> = before
            .iter()
            .filter(|tag| !after.contains(tag))
            .cloned()
            .collect();
        self.remove_single(selected, &removed);
        let added: Vec<_> = additions
            .iter()
            .filter(|tag| after.contains(tag))
            .cloned()
            .collect();
        self.add_single(selected, &added);
    }

    /// Tags taken out of the selected files' box: none of them keeps them.
    pub fn remove_single(&mut self, selected: &[usize], tags: &[String]) {
        for i in selected {
            if let Some(single) = self.single.get_mut(i) {
                for tag in tags {
                    single.remove(tag);
                }
            }
        }
    }
}

/// A path's row: its number, the path, and its tags.
pub fn row(index: usize, path: &str, tags: &[String]) -> [String; 3] {
    [
        human_int(index as u64 + 1),
        path.to_owned(),
        tags.join(", "),
    ]
}

/// A path's tags, by service (key).
pub type PathServiceTags<'a> = (&'a str, Vec<(&'a str, Vec<String>)>);

/// What the dialog gives the import: for each path with any, its tags by
/// service (key), paths and services without tags left out.
pub fn value<'a>(
    services: &'a [(String, ServiceTagging)],
    paths: &'a [String],
) -> Vec<PathServiceTags<'a>> {
    paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| {
            let tags: Vec<(&str, Vec<String>)> = services
                .iter()
                .map(|(key, s)| (key.as_str(), s.tags(i, path)))
                .filter(|(_, t)| !t.is_empty())
                .collect();
            (!tags.is_empty()).then_some((path.as_str(), tags))
        })
        .collect()
}

/// The "misc" box's directory rows: their labels and indexes (from the
/// top, or from the file when negative), in the reference's order.
pub const DIRECTORIES: [(&str, i64); 6] = [
    ("add first directory?", 0),
    ("add second directory?", 1),
    ("add third directory?", 2),
    ("add third last directory?", -3),
    ("add second last directory?", -2),
    ("add last directory?", -1),
];

/// Why a regex can't be used, as the reference says it, if it can't.
pub fn regex_error(regex: &str) -> Option<String> {
    hydrus_core::url::strings::PyRegex::new(regex)
        .regex()
        .err()
        .map(|e| format!("That regex would not compile!\n\n{e}"))
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty())
}

/// Regexes typed a line each: those that compile, and what is wrong with
/// the others.
pub fn parse_regexes(text: &str) -> (Vec<String>, Vec<String>) {
    let mut good = Vec::new();
    let mut errors = Vec::new();
    for line in lines(text) {
        match regex_error(line) {
            None => good.push(line.to_owned()),
            Some(e) => errors.push(e),
        }
    }
    (good, errors)
}

/// Quick namespaces as text: "namespace:regex", a line each.
pub fn quick_namespaces_text(quick: &[(String, String)]) -> String {
    quick
        .iter()
        .map(|(n, r)| format!("{n}:{r}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Quick namespaces typed a line each ("namespace:regex", split at the
/// first colon): those that are good, and what is wrong with the others
/// (`DialogInputNamespaceRegex`'s checks).
pub fn parse_quick_namespaces(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut good = Vec::new();
    let mut errors = Vec::new();
    for line in lines(text) {
        let (namespace, regex) = line.split_once(':').unwrap_or(("", line));
        let namespace = namespace.trim();
        if namespace.is_empty() {
            errors.push("Please enter something for the namespace.".to_owned());
        } else if let Some(e) = regex_error(regex) {
            errors.push(e);
        } else {
            good.push((namespace.to_owned(), regex.to_owned()));
        }
    }
    (good, errors)
}
