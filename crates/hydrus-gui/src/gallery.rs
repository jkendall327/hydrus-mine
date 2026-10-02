//! A gallery downloader page's searches (the reference's
//! `SidebarImporterMultipleGallery`): each search's row in the page's
//! list, as the daemon works it, and the page's totals.

use hydrus_core::pages::DownloaderPageSettings;
use hydrus_store::live::{self, QueueLive};
use hydrus_store::queues::{self, SeedStatus, StatusCounts};
use rusqlite::Connection;

/// One of a gallery page's searches, as last read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GalleryQuery {
    pub queue: i64,
    pub query: String,
    /// The downloader's name.
    pub source: String,
    /// Its file log's seeds by status, and its search log's.
    pub files: StatusCounts,
    pub searches: StatusCounts,
    pub files_paused: bool,
    pub gallery_paused: bool,
    pub created: i64,
    pub live: QueueLive,
}

/// A gallery page's own part: its searches, as its list shows them, and
/// what it gives the searches it makes.
#[derive(Debug, Clone)]
pub struct GalleryView {
    /// The page, whose key and name the searches it makes carry.
    pub page_key: hydrus_core::pages::PageKey,
    pub page_name: String,
    /// Its searches' queues, in the page's order.
    pub queues: Vec<i64>,
    /// As last read, in the list's order.
    pub queries: Vec<GalleryQuery>,
    /// Its downloader, file limit and the rest, and the search it shows.
    pub state: hydrus_core::pages::DownloaderPageState,
    /// The list's sort.
    pub sort: (Column, bool),
    /// The search selected in the list.
    pub selected: Option<i64>,
    /// The downloaders offered for new queries (key, name, initial search
    /// text): those the client displays, by name, then the others.
    pub gugs: Vec<(String, String, String)>,
    /// The options it shows itself by (pause and stop characters, whether
    /// a new query is shown, the short status's counts).
    pub settings: DownloaderPageSettings,
    pub short_summary: (bool, bool),
}

impl GalleryView {
    /// Read the searches again, and those the page has made since (its
    /// key's) at its end; whether anything changed.
    pub fn refresh(&mut self, conn: &Connection) -> hydrus_store::Result<bool> {
        for queue in queues::queues_with_page_key(conn, &self.page_key.0)? {
            if queue.kind == queues::QueueKind::Gallery && !self.queues.contains(&queue.id) {
                self.queues.push(queue.id);
            }
        }
        let mut read = Vec::with_capacity(self.queues.len());
        let mut gone = Vec::new();
        for &queue in &self.queues {
            match GalleryQuery::read(conn, queue)? {
                Some(query) => read.push(query),
                None => gone.push(queue),
            }
        }
        self.queues.retain(|q| !gone.contains(q));
        sort(&mut read, self.sort.0, self.sort.1);
        let changed = read != self.queries || !gone.is_empty();
        self.queries = read;
        Ok(changed)
    }

    /// The gallery settings, a new client's if the page has none.
    pub fn gallery(&self) -> hydrus_core::pages::GalleryPageState {
        self.state
            .gallery
            .clone()
            .unwrap_or_else(|| hydrus_core::pages::GalleryPageState {
                gug_key: String::new(),
                gug_name: String::new(),
                file_limit: None,
                start_files_paused: false,
                start_gallery_paused: false,
                no_new_dupes: false,
                merge_pends: false,
            })
    }

    pub fn query(&self, queue: i64) -> Option<&GalleryQuery> {
        self.queries.iter().find(|q| q.queue == queue)
    }
}

/// The downloaders a page offers (`SelectGUGKeyAndName`): those the client
/// displays, by name, then the others, by name.
pub fn offered_gugs(gugs: &hydrus_core::url::Gugs) -> Vec<(String, String, String)> {
    let mut all: Vec<&hydrus_core::url::AnyGug> = gugs.gugs.iter().collect();
    all.sort_by(|a, b| a.name().cmp(b.name()));
    let (shown, others): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|g| gugs.keys_to_display.iter().any(|k| k == g.key()));
    shown
        .into_iter()
        .chain(others)
        .map(|g| {
            (
                g.key().to_owned(),
                g.name().to_owned(),
                g.initial_search_text().to_owned(),
            )
        })
        .collect()
}

/// A search's live status line (`GenerateLiveStatusText`): what its work is
/// doing, or "paused"; "paused - " or "pausing - " before what it is still
/// doing when paused.
pub fn live_line(text: &str, paused: bool, working: bool) -> String {
    let text = if text.is_empty() && paused {
        "paused"
    } else {
        text
    };
    if paused && text != "paused" {
        format!("{} - {text}", if working { "pausing" } else { "paused" })
    } else {
        text.to_owned()
    }
}

/// What a search is up to (`GetSimpleStatus`), in the list's sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimpleStatus {
    Done,
    Working,
    Pending,
    Pausing,
    Paused,
}

impl SimpleStatus {
    pub fn text(self) -> &'static str {
        match self {
            SimpleStatus::Done => "DONE",
            SimpleStatus::Working => "working",
            SimpleStatus::Pending => "pending",
            SimpleStatus::Pausing => "pausing\u{2026}",
            SimpleStatus::Paused => "",
        }
    }
}

/// The list's columns (`COLUMN_LIST_GALLERY_IMPORTERS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Query,
    Source,
    Files,
    Search,
    Status,
    Items,
    Added,
}

impl Column {
    pub const ALL: [Column; 7] = [
        Column::Query,
        Column::Source,
        Column::Files,
        Column::Search,
        Column::Status,
        Column::Items,
        Column::Added,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Column::Query => "query",
            Column::Source => "source",
            Column::Files => "files status",
            Column::Search => "search status",
            Column::Status => "status",
            Column::Items => "items",
            Column::Added => "added",
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// A finished, paused or going log, as the list sorts and shows it.
fn pause_rank(finished: bool, paused: bool) -> i8 {
    if finished { -1 } else { i8::from(!paused) }
}

fn has_work(counts: &StatusCounts) -> bool {
    counts.get(&SeedStatus::Unknown).is_some_and(|&n| n > 0)
}

impl GalleryQuery {
    /// The search on `queue`, if it is one.
    pub fn read(conn: &Connection, queue: i64) -> hydrus_store::Result<Option<Self>> {
        let Some(row) = queues::queue(conn, queue)? else {
            return Ok(None);
        };
        let search: hydrus_core::gallery::GallerySearch =
            serde_json::from_value(row.extra.clone()).unwrap_or_default();
        Ok(Some(Self {
            queue,
            query: search.query,
            source: search.source_name,
            files: queues::file_seed_counts(conn, queue)?,
            searches: queues::gallery_seed_counts(conn, queue)?,
            files_paused: row.files_paused,
            gallery_paused: row.gallery_paused,
            created: row.created,
            live: live::live(conn, &[queue])?
                .remove(&queue)
                .unwrap_or_default(),
        }))
    }

    pub fn files_finished(&self) -> bool {
        !has_work(&self.files)
    }

    pub fn gallery_finished(&self) -> bool {
        !has_work(&self.searches)
    }

    /// Whether the daemon is downloading for it now.
    pub fn working(&self) -> bool {
        self.live.file_job.is_some() || self.live.gallery_job.is_some()
    }

    /// Whether it still has files to get and isn't paused
    /// (`CurrentlyWorking`, which closing its page asks about).
    pub fn importing(&self) -> bool {
        !self.files_finished() && !self.files_paused
    }

    pub fn simple_status(&self) -> SimpleStatus {
        let gallery_work = !self.gallery_finished();
        let files_work = !self.files_finished();
        if !(gallery_work || files_work) {
            SimpleStatus::Done
        } else if (gallery_work && !self.gallery_paused) || (files_work && !self.files_paused) {
            if self.working() {
                SimpleStatus::Working
            } else {
                SimpleStatus::Pending
            }
        } else if self.working() {
            SimpleStatus::Pausing
        } else {
            SimpleStatus::Paused
        }
    }

    /// Its row in the page's list (`_ConvertDataToDisplayTuple`): the query
    /// (starred if shown), its downloader, its files' and search's pause
    /// or finish, its status, its short file log status and when it was
    /// added.
    pub fn row(
        &self,
        highlighted: bool,
        settings: &DownloaderPageSettings,
        short_summary: (bool, bool),
        now: i64,
    ) -> [String; 7] {
        let pause = |finished: bool, paused: bool| match pause_rank(finished, paused) {
            -1 => settings.stop_character.clone(),
            0 => settings.pause_character.clone(),
            _ => String::new(),
        };
        [
            if highlighted {
                format!("* {}", self.query)
            } else {
                self.query.clone()
            },
            self.source.clone(),
            pause(self.files_finished(), self.files_paused),
            pause(self.gallery_finished(), self.gallery_paused),
            self.simple_status().text().to_owned(),
            queues::file_log_short_status(&self.files, short_summary.0, short_summary.1),
            hydrus_core::time::timestamp_to_pretty_time_delta_minutes(self.created, now, " ago"),
        ]
    }
}

/// Sort searches by a column, as the reference's list sorts
/// (`_ConvertDataToSortTuple`, stable).
pub fn sort(queries: &mut [GalleryQuery], column: Column, ascending: bool) {
    queries.sort_by(|a, b| {
        let order = match column {
            Column::Query => a.query.cmp(&b.query),
            Column::Source => a.source.cmp(&b.source),
            Column::Files => pause_rank(a.files_finished(), a.files_paused)
                .cmp(&pause_rank(b.files_finished(), b.files_paused)),
            Column::Search => pause_rank(a.gallery_finished(), a.gallery_paused)
                .cmp(&pause_rank(b.gallery_finished(), b.gallery_paused)),
            Column::Status => a.simple_status().cmp(&b.simple_status()),
            Column::Items => {
                let progress = |q: &GalleryQuery| {
                    let (done, total) = queues::file_log_value_range(&q.files);
                    (total, done)
                };
                progress(a).cmp(&progress(b))
            }
            Column::Added => a.created.cmp(&b.created),
        };
        if ascending { order } else { order.reverse() }
    });
}

/// The page's progress, for its tab (`GetValueRange`): its unfinished
/// searches' files done and in all.
pub fn value_range(queries: &[GalleryQuery]) -> (usize, usize) {
    queries
        .iter()
        .map(|q| queues::file_log_value_range(&q.files))
        .filter(|(done, all)| done != all)
        .fold((0, 0), |(v, r), (done, all)| (v + done, r + all))
}

/// Why closing a gallery page needs asking about (`CheckAbleToClose`):
/// searches still importing, or (if `confirm_non_empty`) any files held.
pub fn close_veto(queries: &[GalleryQuery], confirm_non_empty: bool) -> Option<String> {
    let working = queries.iter().filter(|q| q.importing()).count();
    if working > 0 {
        return Some(format!(
            "{} queries are still importing.",
            hydrus_core::numbers::human_int(working as u64)
        ));
    }
    let held: usize = queries
        .iter()
        .map(|q| q.files.values().sum::<usize>())
        .sum();
    (confirm_non_empty && held > 0).then(|| {
        format!(
            "This is a gallery downloader page holding {} import objects.",
            hydrus_core::numbers::human_int(held as u64)
        )
    })
}

/// The page's totals over its searches' file logs (`GetTotalStatus`):
/// "3 queries - 12/40" and the full status, or "waiting for new queries"
/// with none.
pub fn totals(queries: &[GalleryQuery]) -> (String, String) {
    if queries.is_empty() {
        return ("waiting for new queries".into(), String::new());
    }
    let mut total = StatusCounts::new();
    for query in queries {
        for (status, n) in &query.files {
            *total.entry(*status).or_default() += n;
        }
    }
    let (done, all) = queues::file_log_value_range(&total);
    (
        format!(
            "{} queries - {}",
            hydrus_core::numbers::human_int(queries.len() as u64),
            queues::value_range_text(done, all)
        ),
        queues::file_log_status(&total),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(files: &[(SeedStatus, usize)], searches: &[(SeedStatus, usize)]) -> GalleryQuery {
        GalleryQuery {
            queue: 1,
            query: "blue eyes".into(),
            source: "site tag search".into(),
            files: files.iter().copied().collect(),
            searches: searches.iter().copied().collect(),
            created: 1000,
            ..GalleryQuery::default()
        }
    }

    #[test]
    fn a_searchs_status_as_the_reference_reckons_it() {
        use SeedStatus::{SuccessfulAndNew as New, Unknown};
        let downloading = || QueueLive {
            file_job: Some(live::JobLive::default()),
            ..QueueLive::default()
        };
        // nothing left: done, both finished
        let done = query(&[(New, 3)], &[(New, 1)]);
        assert_eq!(done.simple_status(), SimpleStatus::Done);
        // work to do: pending, or working while downloading
        let mut q = query(&[(Unknown, 2)], &[(New, 1)]);
        assert_eq!(q.simple_status(), SimpleStatus::Pending);
        q.live = downloading();
        assert_eq!(q.simple_status(), SimpleStatus::Working);
        // paused with work: pausing while still downloading, then paused
        q.files_paused = true;
        assert_eq!(q.simple_status(), SimpleStatus::Pausing);
        q.live = QueueLive::default();
        assert_eq!(q.simple_status(), SimpleStatus::Paused);
        // (gallery work left and not paused keeps it pending)
        q.searches.insert(Unknown, 1);
        assert_eq!(q.simple_status(), SimpleStatus::Pending);
    }

    #[test]
    fn a_searchs_row_as_the_reference_shows_it() {
        let settings = DownloaderPageSettings::default();
        let mut q = query(
            &[(SeedStatus::SuccessfulAndNew, 3), (SeedStatus::Unknown, 1)],
            &[(SeedStatus::SuccessfulAndNew, 2)],
        );
        q.files_paused = true;
        assert_eq!(
            q.row(true, &settings, (false, false), 1000 + 7200),
            [
                "* blue eyes",
                "site tag search",
                "\u{23F8}",
                "\u{23F9}",
                "",
                "3/4",
                "2 hours ago",
            ]
            .map(String::from)
        );
        assert_eq!(
            q.row(false, &settings, (false, false), 1030)[0],
            "blue eyes"
        );
        assert_eq!(q.row(false, &settings, (false, false), 1030)[6], "now");
    }

    #[test]
    fn the_list_sorts_by_each_column() {
        // a: still going; b: done; c: paused with work left
        let mut a = query(&[(SeedStatus::Unknown, 4)], &[(SeedStatus::Unknown, 1)]);
        a.query = "a".into();
        a.created = 9;
        let mut b = query(&[(SeedStatus::SuccessfulAndNew, 1)], &[]);
        b.query = "b".into();
        b.created = 5;
        let mut c = query(&[(SeedStatus::Unknown, 1)], &[]);
        c.query = "c".into();
        c.files_paused = true;
        c.created = 1;
        let order = |column, ascending| {
            let mut queries = vec![a.clone(), b.clone(), c.clone()];
            sort(&mut queries, column, ascending);
            queries.iter().map(|q| q.query.clone()).collect::<Vec<_>>()
        };
        assert_eq!(order(Column::Query, true), ["a", "b", "c"]);
        assert_eq!(order(Column::Query, false), ["c", "b", "a"]);
        assert_eq!(order(Column::Added, true), ["c", "b", "a"]);
        // finished, paused, going
        assert_eq!(order(Column::Files, true), ["b", "c", "a"]);
        // done, pending, paused
        assert_eq!(order(Column::Status, true), ["b", "a", "c"]);
        // by how many in all, then how many done
        assert_eq!(order(Column::Items, true), ["c", "b", "a"]);
        // (the same source: as they were)
        assert_eq!(order(Column::Source, true), ["a", "b", "c"]);
    }

    #[test]
    fn the_pages_progress_and_close_question() {
        let done = query(&[(SeedStatus::SuccessfulAndNew, 5)], &[]);
        let mut going = query(
            &[(SeedStatus::SuccessfulAndNew, 1), (SeedStatus::Unknown, 3)],
            &[],
        );
        // (finished searches don't count)
        assert_eq!(value_range(&[done.clone(), going.clone()]), (1, 4));
        assert_eq!(
            close_veto(&[done.clone(), going.clone()], false).as_deref(),
            Some("1 queries are still importing.")
        );
        going.files_paused = true;
        assert_eq!(
            close_veto(&[done.clone(), going.clone()], true).as_deref(),
            Some("This is a gallery downloader page holding 9 import objects.")
        );
        assert_eq!(close_veto(&[done, going], false), None);
        assert_eq!(close_veto(&[], true), None);
    }

    #[test]
    fn live_lines_as_the_reference_writes_them() {
        assert_eq!(live_line("", false, false), "");
        assert_eq!(live_line("", true, false), "paused");
        assert_eq!(live_line("working", false, true), "working");
        assert_eq!(live_line("working", true, true), "pausing - working");
        assert_eq!(live_line("checking", true, false), "paused - checking");
    }

    #[test]
    fn the_downloaders_offered() {
        use hydrus_core::url::{AnyGug, Gug, Gugs};
        let gug = |name: &str, key: &str| {
            AnyGug::Single(Gug {
                name: name.into(),
                key: key.into(),
                url_template: String::new(),
                replacement_phrase: String::new(),
                separator: String::new(),
                initial_search_text: format!("{name} tags"),
                example_search_text: String::new(),
            })
        };
        let gugs = Gugs {
            gugs: vec![gug("zed", "01"), gug("hidden", "02"), gug("alpha", "03")],
            keys_to_display: vec!["01".into(), "03".into()],
        };
        let names: Vec<String> = offered_gugs(&gugs).into_iter().map(|g| g.1).collect();
        assert_eq!(names, ["alpha", "zed", "hidden"]);
        assert_eq!(offered_gugs(&gugs)[0].2, "alpha tags");
    }

    #[test]
    fn the_pages_totals() {
        assert_eq!(
            totals(&[]),
            ("waiting for new queries".into(), String::new())
        );
        let queries = [
            query(
                &[(SeedStatus::SuccessfulAndNew, 2), (SeedStatus::Unknown, 1)],
                &[],
            ),
            query(&[(SeedStatus::Error, 1)], &[]),
        ];
        assert_eq!(
            totals(&queries),
            ("2 queries - 3/4".into(), "2 successful, 1 failed".into())
        );
    }
}
