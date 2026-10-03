//! The "manage times" dialog (the reference's `EditFileTimestampsPanel`):
//! the files' modified, archived and last viewed times, their web domains'
//! modified times and their file services' imported, deleted and
//! previously imported times, each over all the files as a
//! [`TimeRange`]; edited (with a cascading step), added, deleted, copied
//! and pasted as the reference's serialised timestamp data, and applied as
//! the times changed.

use std::collections::BTreeSet;

use hydrus_core::ServiceId;
use hydrus_core::pyjson::PyJson;

/// The reference's timestamp types (`HC.TIMESTAMP_TYPE_*`).
pub const DOMAIN_MODIFIED: i64 = 0;
pub const FILE_MODIFIED: i64 = 1;
pub const IMPORTED: i64 = 3;
pub const DELETED: i64 = 4;
pub const ARCHIVED: i64 = 5;
pub const LAST_VIEWED: i64 = 6;
pub const PREVIOUSLY_IMPORTED: i64 = 7;

/// The viewers' canvas types, as last viewed times name them.
pub const MEDIA_VIEWER: i64 = 0;
pub const PREVIEW: i64 = 1;

/// What the reference calls a timestamp type
/// (`HC.timestamp_type_str_lookup`).
pub fn type_name(kind: i64) -> &'static str {
    match kind {
        DOMAIN_MODIFIED => "domain modified time",
        FILE_MODIFIED => "file modified time",
        2 => "aggregate modified time",
        IMPORTED => "imported time",
        DELETED => "deleted time",
        ARCHIVED => "archived time",
        LAST_VIEWED => "last viewed time",
        PREVIOUSLY_IMPORTED => "previous imported time (for undelete)",
        _ => "unknown",
    }
}

/// The dialog's title.
pub const TITLE: &str = "manage times";

/// What "apply" asks before making many changes.
pub const MANY_CHANGES_QUESTION: &str =
    "This dialog is about to make more than 100 changes! Are you sure this is all correct?";

/// What editing or pasting a web domain time asks when some of the files
/// have none.
pub const SOME_FILES_QUESTION: &str = "Not every file this dialog was launched on has a time for this domain. Do you want to apply what you just set to everything, or just the files that started with this domain?";

/// The answers to [`SOME_FILES_QUESTION`].
pub const SOME_FILES_YES: &str = "all files";
pub const SOME_FILES_NO: &str = "only edit existing values";

/// What deleting web domain times asks.
pub const DELETE_QUESTION: &str = "Remove all selected?";

/// What "add" asks for, and says of a domain already listed.
pub const DOMAIN_PROMPT: &str = "Enter domain.";
pub const DOMAIN_EXISTS: &str = "Sorry, that domain already exists!";

/// The file modified time's warnings, once changed.
pub const MODIFIED_WARNING: &str = "This will also change the modified time of the file on disk!";
pub const MODIFIED_TOO_EARLY: &str =
    "File modified time on disk will not be changed--the timestamp is too early.";

/// The file modified time's button when no file has one.
pub const MODIFIED_UNKNOWN: &str = "unknown -- run file maintenance to determine";

/// A time in milliseconds as the reference shows one
/// (`QDateTimeToPrettyString`, with milliseconds): the date and time in
/// `tz`, and how long ago it is from `now` (in seconds).
pub fn pretty_time(ms: i64, now: i64, tz: &jiff::tz::TimeZone) -> String {
    let when = jiff::Timestamp::from_millisecond(ms).map_or_else(
        |_| "unknown time".to_owned(),
        |t| {
            t.to_zoned(tz.clone())
                .strftime("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        },
    );
    let delta = hydrus_core::time::timestamp_to_pretty_time_delta(ms.div_euclid(1000), now, " ago");
    format!("{when} ({delta})")
}

/// A time over some files (`DateTimeWidgetValueRange`): the earliest and
/// latest of those that have one, how many have one and how many don't,
/// and the cascading step an edit gives it.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeRange {
    min: Option<i64>,
    max: Option<i64>,
    set: usize,
    null: usize,
    pub step_ms: i64,
}

impl PartialEq for TimeRange {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl TimeRange {
    /// `count` files at `ms` (or without a time).
    pub fn of(ms: Option<i64>, count: usize) -> Self {
        let mut range = Self::default();
        range.add(ms, count);
        range
    }

    /// What ranges compare and sort by.
    pub fn key(&self) -> (i64, i64, usize, usize, i64) {
        if self.set > 0 {
            (
                self.min.unwrap_or(0),
                self.max.unwrap_or(0),
                self.set,
                self.null,
                self.step_ms,
            )
        } else {
            (0, 0, self.set, self.null, self.step_ms)
        }
    }

    pub fn add(&mut self, ms: Option<i64>, count: usize) {
        if count == 0 {
            return;
        }
        match ms {
            None => self.null += count,
            Some(ms) => {
                self.set += count;
                if let (Some(min), Some(max)) = (self.min, self.max) {
                    if ms < min {
                        self.min = Some(ms);
                    } else if max < ms {
                        self.max = Some(ms);
                    }
                } else {
                    self.min = Some(ms);
                    self.max = Some(ms);
                }
            }
        }
    }

    /// The same files set to `ms` (those without a time too, if
    /// `overwrite_nulls`), with the same step.
    #[must_use]
    pub fn with(&self, ms: Option<i64>, overwrite_nulls: bool) -> Self {
        let mut range = Self::default();
        range.add(ms, self.set);
        range.add(if overwrite_nulls { ms } else { None }, self.null);
        range.step_ms = self.step_ms;
        range
    }

    /// Every file set to the earliest time.
    #[must_use]
    pub fn with_nulls_overwritten(&self) -> Self {
        self.with(self.best(), true)
    }

    /// The time shown: the earliest.
    pub fn best(&self) -> Option<i64> {
        if self.set > 0 { self.min } else { None }
    }

    /// The one time every file with a time has, if they agree.
    pub fn fixed(&self) -> Option<i64> {
        self.min.filter(|&min| Some(min) == self.max)
    }

    pub fn set_count(&self) -> usize {
        self.set
    }

    pub fn null_count(&self) -> usize {
        self.null
    }

    pub fn is_all_null(&self) -> bool {
        self.set == 0
    }

    pub fn is_multiple_files(&self) -> bool {
        self.set > 1
    }

    /// As the reference words it (`ToString`): "2 files set from ... to
    /// ...", "1 file set to ..., 2 files without a time set", "with step
    /// 1 second".
    pub fn text(&self, now: i64, tz: &jiff::tz::TimeZone) -> String {
        if self.set + self.null == 0 {
            return "no time set".into();
        }
        let pretty = |ms: Option<i64>| {
            ms.map_or_else(|| "unknown time".into(), |ms| pretty_time(ms, now, tz))
        };
        let mut s = String::new();
        if self.set == 1 {
            if self.null > 1 {
                s += "1 file set to ";
            }
            s += &pretty(self.min);
        } else if self.set > 1 {
            s += &format!(
                "{} files set",
                hydrus_core::numbers::human_int(self.set as u64)
            );
            if self.min == self.max {
                s += &format!(" to {}", pretty(self.min));
            } else {
                s += &format!(" from {} to {}", pretty(self.min), pretty(self.max));
            }
        }
        if self.null > 0 {
            let prefix = if self.null == 1 {
                "1 file".to_owned()
            } else {
                format!(
                    "{} files",
                    hydrus_core::numbers::human_int(self.null as u64)
                )
            };
            if !s.is_empty() {
                s += ", ";
            }
            s += &format!("{prefix} without a time set");
        }
        if self.step_ms != 0 {
            s += &format!(
                ", with step {}",
                hydrus_core::time::pretty_time_delta_f64(self.step_ms as f64 / 1000.0)
            );
        }
        s
    }
}

/// One file's times, as the dialog reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileTimes {
    pub inbox: bool,
    pub modified: Option<i64>,
    pub archived: Option<i64>,
    /// Last viewed in the media viewer, and in the preview.
    pub viewed: Option<i64>,
    pub preview: Option<i64>,
    /// Each web domain's modified time.
    pub domains: Vec<(String, i64)>,
    /// Each file service's imported, deleted or previously imported time
    /// (the service, the timestamp type, the time).
    pub services: Vec<(ServiceId, i64, i64)>,
}

/// A file service, as the dialog names it and copy writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceName {
    pub id: ServiceId,
    pub name: String,
    /// Its key, as hex.
    pub key: String,
}

/// The four times over the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Main {
    Modified,
    Archived,
    Viewed,
    Preview,
}

impl Main {
    pub const ALL: [Main; 4] = [Main::Modified, Main::Archived, Main::Viewed, Main::Preview];

    pub fn label(self) -> &'static str {
        match self {
            Main::Modified => "file modified time: ",
            Main::Archived => "archived time: ",
            Main::Viewed => "last viewed in media viewer: ",
            Main::Preview => "last viewed in preview viewer: ",
        }
    }

    fn kind(self) -> (i64, Location) {
        match self {
            Main::Modified => (FILE_MODIFIED, Location::None),
            Main::Archived => (ARCHIVED, Location::None),
            Main::Viewed => (LAST_VIEWED, Location::Canvas(MEDIA_VIEWER)),
            Main::Preview => (LAST_VIEWED, Location::Canvas(PREVIEW)),
        }
    }

    fn of(self, file: &FileTimes) -> Option<i64> {
        match self {
            Main::Modified => file.modified,
            Main::Archived => file.archived,
            Main::Viewed => file.viewed,
            Main::Preview => file.preview,
        }
    }
}

/// One of the grid's times: its button's value, whether the user changed
/// it, and whether it shows (an archived or last viewed time no file has
/// doesn't) and can be edited (a file modified time no file has can't).
#[derive(Debug, Clone, PartialEq)]
pub struct MainTime {
    pub which: Main,
    pub value: TimeRange,
    pub changed: bool,
    pub shown: bool,
    pub enabled: bool,
    /// The button's text until its value changes ("unknown -- ...").
    label: Option<&'static str>,
}

/// A web domain's or file service's row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The files (by place) that have this time.
    pub files: Vec<usize>,
    pub value: TimeRange,
    pub edited: bool,
}

/// Where a timestamp is: a web domain, a viewer, a file service, or
/// nowhere in particular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    None,
    Domain(String),
    Canvas(i64),
    Service(ServiceId),
}

/// A timestamp to write (`TimestampData`): its type, where, and the time
/// (`None` deletes it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeData {
    pub kind: i64,
    pub location: Location,
    pub ms: Option<i64>,
}

/// A change "apply" makes: these files' time set (or deleted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeUpdate {
    pub files: Vec<usize>,
    pub time: TimeData,
}

/// What a paste does: what it says, a question it needs answered first
/// (paste again with the answer), or why it couldn't read the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasted {
    Done(String),
    Ask(&'static str),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct TimesEditor {
    files: Vec<FileTimes>,
    services: Vec<ServiceName>,
    /// When the dialog opened, in milliseconds (a new button's or web
    /// domain's starting time).
    now_ms: i64,
    pub main: Vec<MainTime>,
    /// Web domains, in the order they came (deleted ones stay, unlisted,
    /// to be deleted on apply).
    domains: Vec<(String, Row, bool)>,
    original_domains: BTreeSet<String>,
    /// File service rows: the service, the timestamp type.
    file_services: Vec<(ServiceId, i64, Row)>,
    /// Whether file modified times before 1980 can't go on disk (as on
    /// Windows).
    pub dos_epoch: bool,
}

/// `HydrusPaths.FileModifiedTimeIsOk`: not before the epoch (or, on
/// Windows, the DOS epoch of 1980).
fn modified_time_is_ok(ms: i64, dos_epoch: bool) -> bool {
    let seconds = ms as f64 / 1000.0;
    if dos_epoch {
        seconds >= 315_532_800.0
    } else {
        seconds >= 0.0
    }
}

impl TimesEditor {
    pub fn new(files: Vec<FileTimes>, services: Vec<ServiceName>, now_ms: i64) -> Self {
        let n = files.len();
        let main = Main::ALL
            .iter()
            .map(|&which| {
                let mut range = TimeRange::default();
                for file in &files {
                    if which == Main::Archived && file.inbox {
                        continue;
                    }
                    range.add(which.of(file), 1);
                }
                let known = !range.is_all_null();
                MainTime {
                    which,
                    // (a button with no time starts at now)
                    value: if known {
                        range
                    } else {
                        TimeRange::of(Some(now_ms), 1)
                    },
                    changed: false,
                    shown: known || which == Main::Modified,
                    enabled: known,
                    label: (!known && which == Main::Modified).then_some(MODIFIED_UNKNOWN),
                }
            })
            .collect();
        let mut domains: Vec<(String, Row, bool)> = Vec::new();
        for (i, file) in files.iter().enumerate() {
            for (domain, ms) in &file.domains {
                match domains.iter_mut().find(|(d, _, _)| d == domain) {
                    Some((_, row, _)) => {
                        row.files.push(i);
                        row.value.add(Some(*ms), 1);
                    }
                    None => domains.push((
                        domain.clone(),
                        Row {
                            files: vec![i],
                            value: TimeRange::of(Some(*ms), 1),
                            edited: false,
                        },
                        true,
                    )),
                }
            }
        }
        for (_, row, _) in &mut domains {
            row.value.add(None, n - row.files.len());
        }
        let original_domains = domains.iter().map(|(d, _, _)| d.clone()).collect();
        let mut file_services: Vec<(ServiceId, i64, Row)> = Vec::new();
        for (i, file) in files.iter().enumerate() {
            for &(service, kind, ms) in &file.services {
                match file_services
                    .iter_mut()
                    .find(|(s, k, _)| *s == service && *k == kind)
                {
                    Some((_, _, row)) => {
                        row.files.push(i);
                        row.value.add(Some(ms), 1);
                    }
                    None => file_services.push((
                        service,
                        kind,
                        Row {
                            files: vec![i],
                            value: TimeRange::of(Some(ms), 1),
                            edited: false,
                        },
                    )),
                }
            }
        }
        for (_, _, row) in &mut file_services {
            row.value.add(None, n - row.files.len());
        }
        Self {
            files,
            services,
            now_ms,
            main,
            domains,
            original_domains,
            file_services,
            dos_epoch: cfg!(windows),
        }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Whether copy shows: for one file only.
    pub fn can_copy(&self) -> bool {
        self.files.len() == 1
    }

    fn service_name(&self, id: ServiceId) -> &str {
        self.services
            .iter()
            .find(|s| s.id == id)
            .map_or("unknown service", |s| s.name.as_str())
    }

    fn service_key(&self, id: ServiceId) -> Option<&str> {
        self.services
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.key.as_str())
    }

    pub fn main_time(&self, which: Main) -> &MainTime {
        &self.main[Main::ALL.iter().position(|&m| m == which).unwrap_or(0)]
    }

    fn main_time_mut(&mut self, which: Main) -> &mut MainTime {
        let i = Main::ALL.iter().position(|&m| m == which).unwrap_or(0);
        &mut self.main[i]
    }

    /// A grid time's button text.
    pub fn main_text(&self, which: Main, now: i64, tz: &jiff::tz::TimeZone) -> String {
        let time = self.main_time(which);
        time.label
            .map_or_else(|| time.value.text(now, tz), str::to_owned)
    }

    /// A grid time given a new value (`DateTimesButton.SetValue`): one
    /// with no time is refused; a different one is the user's change.
    pub fn set_main(&mut self, which: Main, value: TimeRange) {
        if value.is_all_null() {
            return;
        }
        let time = self.main_time_mut(which);
        if time.value != value {
            time.value = value;
            time.label = None;
            time.changed = true;
        }
    }

    /// The file modified time's warning, once it is changed.
    pub fn warning(&self) -> Option<&'static str> {
        let time = self.main_time(Main::Modified);
        let ms = time.value.fixed().filter(|_| time.changed)?;
        Some(if modified_time_is_ok(ms, self.dos_epoch) {
            MODIFIED_WARNING
        } else {
            MODIFIED_TOO_EARLY
        })
    }

    /// The web domains listed, sorted by domain, each with its row.
    pub fn domain_rows(&self) -> Vec<(&str, &Row)> {
        let mut rows: Vec<(&str, &Row)> = self
            .domains
            .iter()
            .filter(|(_, _, listed)| *listed)
            .map(|(d, r, _)| (d.as_str(), r))
            .collect();
        rows.sort_by(|a, b| {
            (a.0.to_lowercase(), a.0, a.1.value.key()).cmp(&(
                b.0.to_lowercase(),
                b.0,
                b.1.value.key(),
            ))
        });
        rows
    }

    /// The file service rows, sorted by type (then service and time):
    /// each service, type, and row.
    pub fn file_service_rows(&self) -> Vec<(ServiceId, i64, &Row)> {
        let mut rows: Vec<(ServiceId, i64, &Row)> = self
            .file_services
            .iter()
            .map(|(s, k, r)| (*s, *k, r))
            .collect();
        rows.sort_by(|a, b| {
            let key = |(s, k, r): &(ServiceId, i64, &Row)| {
                (
                    type_name(*k).to_lowercase(),
                    self.service_name(*s).to_lowercase(),
                    r.value.key(),
                )
            };
            key(a).cmp(&key(b))
        });
        rows
    }

    /// A file service row's columns: service, type, time.
    pub fn file_service_text(
        &self,
        service: ServiceId,
        kind: i64,
        row: &Row,
        now: i64,
        tz: &jiff::tz::TimeZone,
    ) -> [String; 3] {
        [
            self.service_name(service).to_owned(),
            type_name(kind).to_owned(),
            row.value.text(now, tz),
        ]
    }

    /// Whether `domain` is listed (so "add" refuses it).
    pub fn has_domain(&self, domain: &str) -> bool {
        self.domains
            .iter()
            .any(|(d, _, listed)| *listed && d == domain)
    }

    /// What "add" starts the new domain's time at: now, for every file.
    pub fn new_domain_value(&self) -> TimeRange {
        TimeRange::of(Some(self.now_ms), self.files.len())
    }

    /// "add": a web domain time for every file.
    pub fn add_domain(&mut self, domain: &str, value: TimeRange) {
        self.domains.retain(|(d, _, _)| d != domain);
        self.domains.push((
            domain.to_owned(),
            Row {
                files: (0..self.files.len()).collect(),
                value,
                edited: true,
            },
            true,
        ));
    }

    fn domain_row(&self, domain: &str) -> Option<&Row> {
        self.domains
            .iter()
            .find(|(d, _, listed)| *listed && d == domain)
            .map(|(_, r, _)| r)
    }

    /// The value an edit of these domains starts from: the first's.
    pub fn domain_value(&self, domain: &str) -> Option<TimeRange> {
        self.domain_row(domain).map(|r| r.value)
    }

    /// Whether editing starting from this domain asks
    /// [`SOME_FILES_QUESTION`]: some files lack it.
    pub fn domain_edit_asks(&self, domain: &str) -> bool {
        self.domain_row(domain)
            .is_some_and(|r| r.files.len() < self.files.len())
    }

    /// The domains edited to `edited`'s time and step (every file given
    /// it, if `all_files`).
    pub fn edit_domains(&mut self, domains: &[String], edited: TimeRange, all_files: bool) {
        let n = self.files.len();
        for (domain, row, listed) in &mut self.domains {
            if !*listed || !domains.contains(domain) {
                continue;
            }
            let mut value = row.value.with(edited.fixed(), false);
            value.step_ms = edited.step_ms;
            if all_files {
                row.files = (0..n).collect();
                value = value.with_nulls_overwritten();
            }
            row.value = value;
            row.edited = true;
        }
    }

    /// The domains deleted ("Remove all selected?"): their times deleted
    /// on apply.
    pub fn delete_domains(&mut self, domains: &[String]) {
        for (domain, _, listed) in &mut self.domains {
            if domains.contains(domain) {
                *listed = false;
            }
        }
    }

    /// The value editing these file service rows starts from: the
    /// first's.
    pub fn file_service_value(&self, service: ServiceId, kind: i64) -> Option<TimeRange> {
        self.file_services
            .iter()
            .find(|(s, k, _)| *s == service && *k == kind)
            .map(|(_, _, r)| r.value)
    }

    /// The file service rows edited to `edited`'s time and step.
    pub fn edit_file_services(&mut self, rows: &[(ServiceId, i64)], edited: TimeRange) {
        for (service, kind, row) in &mut self.file_services {
            if !rows.contains(&(*service, *kind)) {
                continue;
            }
            let mut value = row.value.with(edited.fixed(), false);
            value.step_ms = edited.step_ms;
            row.value = value;
            row.edited = true;
        }
    }

    /// The times, each over the files that have it, with its step
    /// (`_GetValidTimestampDatas`): all of them, or only those changed.
    fn time_datas(&self, only_changes: bool) -> Vec<(Vec<usize>, TimeData, i64)> {
        let mut out = Vec::new();
        for time in &self.main {
            if only_changes && !time.changed {
                continue;
            }
            let Some(ms) = time.value.fixed() else {
                continue;
            };
            let which = time.which;
            let files: Vec<usize> = (0..self.files.len())
                .filter(|&i| {
                    let file = &self.files[i];
                    which.of(file).is_some() && !(which == Main::Archived && file.inbox)
                })
                .collect();
            if files.is_empty() && which != Main::Modified {
                continue;
            }
            let (kind, location) = which.kind();
            out.push((
                files,
                TimeData {
                    kind,
                    location,
                    ms: Some(ms),
                },
                time.value.step_ms,
            ));
        }
        for (domain, row) in self.domain_rows() {
            if only_changes && !row.edited {
                continue;
            }
            let Some(ms) = row.value.fixed() else {
                continue;
            };
            out.push((
                row.files.clone(),
                TimeData {
                    kind: DOMAIN_MODIFIED,
                    location: Location::Domain(domain.to_owned()),
                    ms: Some(ms),
                },
                row.value.step_ms,
            ));
        }
        for (domain, row, listed) in &self.domains {
            if *listed || !self.original_domains.contains(domain) {
                continue;
            }
            out.push((
                row.files.clone(),
                TimeData {
                    kind: DOMAIN_MODIFIED,
                    location: Location::Domain(domain.clone()),
                    ms: None,
                },
                row.value.step_ms,
            ));
        }
        for (service, kind, row) in self.file_service_rows() {
            if only_changes && !row.edited {
                continue;
            }
            let Some(ms) = row.value.fixed() else {
                continue;
            };
            out.push((
                row.files.clone(),
                TimeData {
                    kind,
                    location: Location::Service(service),
                    ms: Some(ms),
                },
                row.value.step_ms,
            ));
        }
        out
    }

    /// A timestamp's serialisable tuple, as the reference writes it.
    fn tuple(&self, time: &TimeData) -> String {
        let location = match &time.location {
            Location::None => "null".to_owned(),
            Location::Domain(d) => {
                let mut out = String::new();
                hydrus_core::pyjson::write_python_string(d, &mut out);
                out
            }
            Location::Canvas(c) => c.to_string(),
            Location::Service(s) => self.service_key(*s).map_or_else(
                || "null".to_owned(),
                |key| {
                    let mut out = String::new();
                    hydrus_core::pyjson::write_python_string(key, &mut out);
                    out
                },
            ),
        };
        let ms = time
            .ms
            .map_or_else(|| "null".to_owned(), |ms| ms.to_string());
        format!("[121, 2, [{}, {location}, {ms}]]", time.kind)
    }

    /// Copy (for one file): every time, or those of `kinds`, as the
    /// reference's serialised list, and the notice; None for several
    /// files.
    pub fn copy(&self, kinds: Option<&[i64]>) -> Option<(String, String)> {
        if self.files.len() != 1 {
            return None;
        }
        let items: Vec<String> = self
            .time_datas(false)
            .into_iter()
            .filter(|(_, t, _)| kinds.is_none_or(|k| k.contains(&t.kind)))
            .map(|(_, t, _)| format!("[2, {}]", self.tuple(&t)))
            .collect();
        let notice = format!(
            "Copied {} encoded time objects!",
            hydrus_core::numbers::human_int(items.len() as u64)
        );
        Some((format!("[26, 3, [{}]]", items.join(", ")), notice))
    }

    /// Paste the reference's serialised times: each sets the time it
    /// names, if shown (a web domain time or file service time only if
    /// listed). A web domain time some files lack asks first; `answers`
    /// are the answers so far, in order.
    pub fn paste(&mut self, text: &str, answers: &[bool]) -> Pasted {
        let items = match parse_times(text, &self.services) {
            Ok(items) => items,
            Err(e) => {
                return Pasted::Error(crate::notes_editor::clipboard_parse_error(
                    "A list of JSON-serialised Timestamp Data objects",
                    text,
                    &e,
                ));
            }
        };
        let mut next = self.clone();
        let mut answers = answers.iter();
        let n = self.files.len();
        for item in &items {
            match (item.kind, &item.location, item.ms) {
                (ARCHIVED, _, Some(ms)) => {
                    if next.main_time(Main::Archived).shown {
                        let value = next.main_time(Main::Archived).value.with(Some(ms), false);
                        next.set_main(Main::Archived, value);
                    }
                }
                (FILE_MODIFIED, _, Some(ms)) => {
                    let value = next.main_time(Main::Modified).value.with(Some(ms), false);
                    next.set_main(Main::Modified, value);
                }
                (LAST_VIEWED, Location::Canvas(canvas), Some(ms)) => {
                    let which = match *canvas {
                        MEDIA_VIEWER => Main::Viewed,
                        PREVIEW => Main::Preview,
                        _ => continue,
                    };
                    if next.main_time(which).shown {
                        let value = next.main_time(which).value.with(Some(ms), false);
                        next.set_main(which, value);
                    }
                }
                (DOMAIN_MODIFIED, Location::Domain(domain), ms) => {
                    if !next.has_domain(domain) {
                        continue;
                    }
                    let Some(ms) = ms else {
                        // (gone from the list, and not deleted on apply)
                        next.domains.retain(|(d, _, _)| d != domain);
                        continue;
                    };
                    let Some(i) = next
                        .domains
                        .iter()
                        .position(|(d, _, listed)| *listed && d == domain)
                    else {
                        continue;
                    };
                    let existing = next.domains[i].1.value;
                    let mut files = next.domains[i].1.files.clone();
                    let mut value = existing.with(Some(ms), false);
                    if files.len() < n {
                        let Some(&all) = answers.next() else {
                            return Pasted::Ask(SOME_FILES_QUESTION);
                        };
                        if all {
                            files = (0..n).collect();
                            value = TimeRange::of(Some(ms), n);
                        }
                    }
                    if value == existing {
                        continue;
                    }
                    next.domains[i].1 = Row {
                        files,
                        value,
                        edited: true,
                    };
                }
                (
                    IMPORTED | DELETED | PREVIOUSLY_IMPORTED,
                    Location::Service(service),
                    Some(ms),
                ) => {
                    if let Some((_, _, row)) = next
                        .file_services
                        .iter_mut()
                        .find(|(s, k, _)| s == service && *k == item.kind)
                    {
                        let value = row.value.with(Some(ms), false);
                        if value != row.value {
                            row.value = value;
                            row.edited = true;
                        }
                    }
                }
                _ => {}
            }
        }
        *self = next;
        Pasted::Done(format!(
            "Pasted {} encoded times!",
            hydrus_core::numbers::human_int(items.len() as u64)
        ))
    }

    /// What "apply" writes (`GetContentUpdatePackage`): the times changed,
    /// a stepped time set file by file (in the dialog's order), a deleted
    /// web domain's time deleted.
    pub fn updates(&self) -> Vec<TimeUpdate> {
        let mut out = Vec::new();
        for (files, time, step) in self.time_datas(true) {
            match time.ms {
                Some(ms) if step != 0 => {
                    for (i, file) in files.iter().enumerate() {
                        out.push(TimeUpdate {
                            files: vec![*file],
                            time: TimeData {
                                ms: Some(ms + i as i64 * step),
                                ..time.clone()
                            },
                        });
                    }
                }
                _ => out.push(TimeUpdate { files, time }),
            }
        }
        out
    }

    /// What "apply" asks first: more than 100 changes.
    pub fn ok_question(&self) -> Option<&'static str> {
        let changes: usize = self.updates().iter().map(|u| u.files.len()).sum();
        (changes > 100).then_some(MANY_CHANGES_QUESTION)
    }

    /// The files whose modified time on disk changes too, the time and the
    /// step (`GetFileModifiedUpdateData`).
    pub fn file_modified_update(&self) -> Option<(Vec<usize>, i64, i64)> {
        self.time_datas(true)
            .into_iter()
            .find_map(|(files, time, step)| {
                let ms = time.ms.filter(|_| time.kind == FILE_MODIFIED)?;
                modified_time_is_ok(ms, self.dos_epoch).then_some((files, ms, step))
            })
    }

    /// A time as a serialisable tuple, for tests and recordings.
    pub fn serialised(&self, time: &TimeData) -> String {
        self.tuple(time)
    }
}

/// The pasted list of serialised timestamp data.
fn parse_times(text: &str, services: &[ServiceName]) -> Result<Vec<TimeData>, String> {
    let value = PyJson::parse(text).map_err(|e| format!("JSONDecodeError('{}')", e.message))?;
    let not_list = || "Exception('Not a timestamp data!')".to_owned();
    let list = value.as_list().ok_or_else(not_list)?;
    let [PyJson::Int(26), PyJson::Int(version), PyJson::List(items)] = list else {
        return Err(not_list());
    };
    let mut out = Vec::new();
    for item in items {
        // (version 1 lists hold the tuples, later ones a flag and each)
        let tuple = if *version == 1 {
            item
        } else {
            match item.as_list() {
                Some([_, tuple]) => tuple,
                _ => return Err(not_list()),
            }
        };
        let Some([PyJson::Int(121), _, PyJson::List(info)]) = tuple.as_list() else {
            return Err(not_list());
        };
        let [kind, location, ms] = info.as_slice() else {
            return Err(not_list());
        };
        let kind = kind.as_i64().ok_or_else(not_list)?;
        let location = match location {
            PyJson::Null => Location::None,
            PyJson::Int(i) => Location::Canvas(*i),
            PyJson::Str(s) if matches!(kind, IMPORTED | DELETED | PREVIOUSLY_IMPORTED) => {
                match services.iter().find(|n| n.key == *s) {
                    Some(service) => Location::Service(service.id),
                    None => Location::None,
                }
            }
            PyJson::Str(s) => Location::Domain(s.clone()),
            _ => return Err(not_list()),
        };
        let ms = match ms {
            PyJson::Null => None,
            PyJson::Int(i) => Some(*i),
            PyJson::Float(f) => Some(*f as i64),
            _ => return Err(not_list()),
        };
        out.push(TimeData { kind, location, ms });
    }
    Ok(out)
}
