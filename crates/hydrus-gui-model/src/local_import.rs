//! The "review files to import" window's model (the reference's
//! `ReviewLocalFileImports`): paths given (by file > import files, or
//! dropped on the window) are parsed a few at a time, as the reference's
//! `LocalFileParse` parses them; a folder's files (and, if asked, its
//! subfolders') join the list, and the sidecars beside them are listed as
//! such. Its good files are what "import now" imports.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use hydrus_core::Mime;
use hydrus_core::numbers::{human_bytes, human_int, value_range};
use hydrus_import::paths;
use hydrus_media::MediaTools;

/// What parsing found a path to be (`ClientImportLocalFileParse.RESULT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parse {
    Good(Mime),
    Empty,
    Missing,
    Unimportable,
    /// Held open by another program.
    Occupied,
    Sidecar,
}

impl Parse {
    /// As the list's filetype column shows it (`GetPrettyMime`).
    pub fn pretty(self) -> String {
        match self {
            Parse::Good(mime) => mime.human_name().to_owned(),
            Parse::Empty => "PROBLEM: file empty".into(),
            Parse::Missing => "PROBLEM: file missing".into(),
            Parse::Unimportable => "PROBLEM: filetype unsupported".into(),
            Parse::Occupied => "PROBLEM: file in use by other program".into(),
            Parse::Sidecar => "sidecar".into(),
        }
    }
}

/// A path in the list: its place (from 1), the path, what it is, and its
/// size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub index: usize,
    pub path: String,
    pub result: Parse,
    pub size: u64,
}

impl Parsed {
    /// The list's row (`_ConvertPathToDisplayTuple`): #, path, filetype,
    /// size ("-" for a file missing or in use).
    pub fn row(&self) -> [String; 4] {
        let size = match self.result {
            Parse::Missing | Parse::Occupied => "-".to_owned(),
            _ => human_bytes(self.size),
        };
        [
            human_int(self.index as u64),
            self.path.clone(),
            self.result.pretty(),
            size,
        ]
    }
}

/// The window's list and parsing.
#[derive(Debug)]
pub struct Review {
    tools: MediaTools,
    parsed: Vec<Parsed>,
    /// Paths given and found in folders, not yet parsed (a folder counts as
    /// one until it is).
    unparsed: VecDeque<String>,
    /// What sidecars of the paths seen so far would be named
    /// (`comparable_sidecar_prefixes`).
    prefixes: HashSet<String>,
    /// Search a folder's subfolders too ("search subdirectories", on by
    /// default).
    pub search_subdirectories: bool,
    paused: bool,
    /// "delete original files after successful import".
    pub delete_after_success: bool,
}

impl Default for Review {
    fn default() -> Self {
        Self::new()
    }
}

impl Review {
    pub fn new() -> Self {
        Self {
            tools: MediaTools::new(),
            parsed: Vec::new(),
            unparsed: VecDeque::new(),
            prefixes: HashSet::new(),
            search_subdirectories: true,
            paused: false,
            delete_after_success: false,
        }
    }

    /// The list, in order.
    pub fn parsed(&self) -> &[Parsed] {
        &self.parsed
    }

    /// Paths to parse (`_AddPathsToList`), after those already given.
    pub fn add_paths(&mut self, paths: impl IntoIterator<Item = String>) {
        self.unparsed.extend(paths);
    }

    /// Whether there is parsing left to do.
    pub fn working(&self) -> bool {
        !self.unparsed.is_empty()
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// The pause button: parsing stops (or goes on) where it is.
    pub fn pause_play(&mut self) {
        self.paused = !self.paused;
    }

    /// The stop button: the paths not yet parsed are let go.
    pub fn cancel(&mut self) {
        self.unparsed.clear();
        self.paused = false;
    }

    /// Parse for about `budget` (the reference's 0.1s at a time), unless
    /// paused; whether it did anything.
    pub fn work(&mut self, budget: Duration) -> bool {
        if self.paused || self.unparsed.is_empty() {
            return false;
        }
        let start = Instant::now();
        while let Some(path) = self.unparsed.pop_front() {
            if std::path::Path::new(&path).is_dir() {
                // (a folder's files to parse, after the rest; its sidecars
                // listed now)
                let found = paths::all_file_paths_noting(
                    &path,
                    self.search_subdirectories,
                    &mut self.prefixes,
                );
                if let Ok((files, sidecars)) = found {
                    self.unparsed.extend(files);
                    for sidecar in sidecars {
                        let size = std::fs::metadata(&sidecar).map_or(0, |m| m.len());
                        self.publish(sidecar, Parse::Sidecar, size);
                    }
                }
            } else {
                let (result, size) = self.parse_file(&path);
                self.publish(path, result, size);
            }
            if start.elapsed() >= budget {
                break;
            }
        }
        true
    }

    /// `DoFileParse`.
    fn parse_file(&mut self, path: &str) -> (Parse, u64) {
        paths::add_sidecar_prefixes([path], &mut self.prefixes);
        let Ok(meta) = std::fs::metadata(path) else {
            return (Parse::Missing, 0);
        };
        if paths::looks_like_sidecar(path, &self.prefixes) {
            return (Parse::Sidecar, 0);
        }
        if !paths::path_is_free(path) {
            return (Parse::Occupied, 0);
        }
        let size = meta.len();
        if size == 0 {
            return (Parse::Empty, size);
        }
        let separator = std::path::MAIN_SEPARATOR;
        if path.ends_with(&format!("{separator}Thumbs.db"))
            || path.ends_with(&format!("{separator}thumbs.db"))
        {
            return (Parse::Unimportable, size);
        }
        match self.tools.detect_mime(std::path::Path::new(path)) {
            Ok(mime) if hydrus_media::mimes::is_allowed(mime) => (Parse::Good(mime), size),
            _ => (Parse::Unimportable, size),
        }
    }

    /// Add a parsed path to the list, at its end, unless it is there
    /// already.
    fn publish(&mut self, path: String, result: Parse, size: u64) {
        if self.parsed.iter().any(|p| p.path == path) {
            return;
        }
        let index = self.parsed.len() + 1;
        self.parsed.push(Parsed {
            index,
            path,
            result,
            size,
        });
    }

    /// Take the rows at these places (from 0) out of the list, numbering
    /// the rest again ("remove files").
    pub fn remove(&mut self, rows: &HashSet<usize>) {
        let mut at = 0;
        self.parsed.retain(|_| {
            let keep = !rows.contains(&at);
            at += 1;
            keep
        });
        for (i, parsed) in self.parsed.iter_mut().enumerate() {
            parsed.index = i + 1;
        }
    }

    /// The good files, in order: what "import now" imports
    /// (`_GetGoodPaths`).
    pub fn good_paths(&self) -> Vec<String> {
        self.parsed
            .iter()
            .filter(|p| matches!(p.result, Parse::Good(_)))
            .map(|p| p.path.clone())
            .collect()
    }

    /// Whether "import now" can be pressed: good files, and parsing done
    /// or paused.
    pub fn can_import(&self) -> bool {
        !self.good_paths().is_empty() && (self.unparsed.is_empty() || self.paused)
    }

    /// The text under the list (`_UpdateWidgets`), and its gauge's value
    /// and range.
    pub fn progress(&self) -> (String, usize, usize) {
        let count = |want: fn(Parse) -> bool| self.parsed.iter().filter(|p| want(p.result)).count();
        let unparsed = self.unparsed.len();
        let total = self.parsed.len() + unparsed;
        let done = total - unparsed;
        let good = count(|r| matches!(r, Parse::Good(_)));
        let empty = count(|r| r == Parse::Empty);
        let missing = count(|r| r == Parse::Missing);
        let unimportable = count(|r| r == Parse::Unimportable);
        let occupied = count(|r| r == Parse::Occupied);
        let sidecars = count(|r| r == Parse::Sidecar);
        let bad = empty + missing + unimportable + occupied;
        let mut message = if total == 0 {
            "waiting for paths to parse".to_owned()
        } else if done < total {
            format!("{} files parsed", value_range(done as u64, total as u64))
        } else {
            format!("{} files parsed", human_int(total as u64))
        };
        if bad > 0 {
            message.push_str(" - ");
            if good > 0 {
                message.push_str(&format!(
                    "{} good | {} bad",
                    human_int(good as u64),
                    human_int(bad as u64)
                ));
            } else if bad == done {
                message.push_str("all bad");
            }
            message.push_str(": ");
            let mut comments = Vec::new();
            for (n, what) in [
                (empty, "were empty"),
                (missing, "were missing"),
                (unimportable, "had unsupported file types"),
                (
                    occupied,
                    "were inaccessible (maybe in use by another process)",
                ),
            ] {
                if n > 0 {
                    comments.push(format!("{} {what}", human_int(n as u64)));
                }
            }
            message.push_str(&comments.join(", "));
        }
        if sidecars > 0 {
            message.push_str(&format!(
                " - and looks like {} txt/json/xml sidecars",
                human_int(sidecars as u64)
            ));
        }
        message.push('.');
        (message, done, total)
    }
}
