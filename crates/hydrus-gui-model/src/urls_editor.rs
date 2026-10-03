//! The "manage urls" dialog (the reference's `EditURLsPanel`): the files'
//! URLs, the box that adds one, copy and paste, and removing them; its
//! value the URLs to add to and delete from which files.

use std::collections::{BTreeMap, BTreeSet};

/// The warning over the list when editing several files.
pub const MULTIPLE_FILES_WARNING: &str = "Warning: you are editing urls for multiple files!\nBe very careful about adding URLs here, as they will apply to everything.\nAdding the same URL to multiple files is only appropriate for Post URLs that are set to expect multiple files, or if you really need to associate a Gallery URL.";

/// What "ok" asks with text left in the box.
pub const TEXT_LEFT_QUESTION: &str = "You have text still in the input! Sure you are ok to apply?";

/// The dialog's title for `count` files.
pub fn title(count: usize) -> String {
    format!(
        "manage urls for {} files",
        hydrus_core::numbers::human_int(count as u64)
    )
}

/// What adding texts that aren't URLs asks first.
pub fn weird_question(weird: &[String]) -> String {
    format!(
        "The URLs:\n\n{}\n\n--did not parse. Normally I would not recommend importing invalid URLs, but do you want to force it anyway?",
        weird.join("\n")
    )
}

/// URLs added to, or deleted from, some of the files (by their place).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlUpdate {
    pub add: bool,
    pub url: String,
    pub files: Vec<usize>,
}

/// The dialog's state over some files.
#[derive(Debug, Clone)]
pub struct UrlsEditor {
    /// Each file's URLs, as the dialog's changes leave them.
    files: Vec<BTreeSet<String>>,
    updates: Vec<UrlUpdate>,
    selected: BTreeSet<String>,
    /// The box's text.
    pub input: String,
}

impl UrlsEditor {
    pub fn new(files: Vec<Vec<String>>) -> Self {
        Self {
            files: files.into_iter().map(|u| u.into_iter().collect()).collect(),
            updates: Vec::new(),
            selected: BTreeSet::new(),
            input: String::new(),
        }
    }

    /// Whether the warning shows: several files.
    pub fn warning(&self) -> bool {
        self.files.len() > 1
    }

    /// The list: each URL (sorted) and its label, with how many of the
    /// files have it when there are several ("https://a.com/1 (2)").
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for urls in &self.files {
            for url in urls {
                *counts.entry(url).or_default() += 1;
            }
        }
        counts
            .into_iter()
            .map(|(url, count)| {
                let label = if self.files.len() == 1 {
                    url.to_owned()
                } else {
                    format!("{url} ({count})")
                };
                (url.to_owned(), label)
            })
            .collect()
    }

    pub fn selected(&self) -> Vec<String> {
        let rows: Vec<String> = self.rows().into_iter().map(|(u, _)| u).collect();
        rows.into_iter()
            .filter(|u| self.selected.contains(u))
            .collect()
    }

    /// The URLs selected now.
    pub fn select(&mut self, urls: &[String]) {
        self.selected = urls.iter().cloned().collect();
    }

    /// The updates so far, in order.
    pub fn updates(&self) -> &[UrlUpdate] {
        &self.updates
    }

    /// `_EnterURLs`: each URL normalised (`normalise` says how, or that it
    /// isn't a URL), those that aren't URLs added only if `force` agrees
    /// to the question it is given; each added to the files that lack it.
    /// Whether they were added.
    pub fn enter(
        &mut self,
        urls: &[String],
        normalise: &dyn Fn(&str) -> Option<String>,
        force: &mut dyn FnMut(&str) -> bool,
    ) -> bool {
        let mut normalised = Vec::new();
        let mut weird = Vec::new();
        for url in urls {
            match normalise(url) {
                Some(n) => normalised.push(n),
                None => weird.push(url.clone()),
            }
        }
        if !weird.is_empty() {
            if !force(&weird_question(&weird)) {
                return false;
            }
            normalised.extend(weird);
        }
        let mut seen = BTreeSet::new();
        normalised.retain(|u| seen.insert(u.clone()));
        for url in normalised {
            let files: Vec<usize> = (0..self.files.len())
                .filter(|&i| !self.files[i].contains(&url))
                .collect();
            if !files.is_empty() {
                for &i in &files {
                    self.files[i].insert(url.clone());
                }
                self.updates.push(UrlUpdate {
                    add: true,
                    url,
                    files,
                });
            }
        }
        true
    }

    /// The box entered (`AddURL`): its URL added, the box cleared; an
    /// empty box is "ok" (`true`).
    pub fn enter_input(
        &mut self,
        normalise: &dyn Fn(&str) -> Option<String>,
        force: &mut dyn FnMut(&str) -> bool,
    ) -> bool {
        if self.input.is_empty() {
            return true;
        }
        let url = std::mem::take(&mut self.input);
        self.enter(&[url], normalise, force);
        false
    }

    /// Paste (`_Paste`): each line, trimmed, the empty ones left out.
    pub fn paste(
        &mut self,
        text: &str,
        normalise: &dyn Fn(&str) -> Option<String>,
        force: &mut dyn FnMut(&str) -> bool,
    ) -> usize {
        let urls: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        self.enter(&urls, normalise, force);
        urls.len()
    }

    /// `_RemoveURL`: deleted from the files that have it.
    pub fn remove(&mut self, url: &str) {
        let files: Vec<usize> = (0..self.files.len())
            .filter(|&i| self.files[i].contains(url))
            .collect();
        if files.is_empty() {
            return;
        }
        for &i in &files {
            self.files[i].remove(url);
        }
        self.selected.remove(url);
        self.updates.push(UrlUpdate {
            add: false,
            url: url.to_owned(),
            files,
        });
    }

    /// The selected removed (delete), in the list's order.
    pub fn delete_selected(&mut self) {
        for url in self.selected() {
            self.remove(&url);
        }
    }

    /// A URL double-clicked: removed, and put in the box to edit.
    pub fn double_click(&mut self, url: &str) {
        self.select(&[url.to_owned()]);
        self.delete_selected();
        url.clone_into(&mut self.input);
    }

    /// What copy copies: the selected URLs, or all of them, a line each.
    pub fn copy_text(&self) -> String {
        let mut urls = self.selected();
        if urls.is_empty() {
            urls = self.rows().into_iter().map(|(u, _)| u).collect();
        }
        urls.join("\n")
    }

    /// What "ok" asks first, if anything.
    pub fn ok_question(&self) -> Option<&'static str> {
        (!self.input.is_empty()).then_some(TEXT_LEFT_QUESTION)
    }
}
