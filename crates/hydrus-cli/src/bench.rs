//! `hydrus bench <store>`: time what the client does on a real collection.
//!
//! Strictly read-only: the store is opened with SQLite's read-only flag (see
//! `Store::open_read_only`), nothing is migrated, created or regenerated, and
//! the thumbnail step only reads files. It needs nothing but this binary and
//! runs the same on Linux and Windows. The report is plain text to paste
//! into a message (`--json` for a machine-readable one).

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use hydrus_core::service::builtin_keys;
use hydrus_core::{HashId, ServiceId, TagId};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, LocationContext, SortBy, SortOrder, TagContext,
    parse_api_search, search_files,
};
use hydrus_store::Store;
use hydrus_store::autocomplete::{
    AutocompleteInput, AutocompleteSettings, CountDomain, TagDisplayType, TagSearchScope,
    search_tags,
};
use hydrus_store::schema::MappingTables;

/// What to measure, from the command line.
#[derive(clap::Args, Debug, Clone)]
pub struct Options {
    /// The hydrus-rs store directory. It is only read.
    pub dir: std::path::PathBuf,
    /// Runs of each measurement (the report gives the first, median and p90).
    #[arg(long, default_value_t = 5)]
    pub runs: usize,
    /// A search to time as well, as tags separated by `;` (for example
    /// `blue sky; system:inbox`). May be given several times.
    #[arg(long = "search")]
    pub searches: Vec<String>,
    /// How many thumbnails to read and decode from disk.
    #[arg(long, default_value_t = 200)]
    pub thumbnails: usize,
    /// Print the report as JSON.
    #[arg(long)]
    pub json: bool,
}

/// One measurement: what ran, how long each run took, and a note on what it
/// found (so a fast result on nothing is not mistaken for a fast result).
#[derive(Debug, Clone)]
pub struct Timing {
    pub group: &'static str,
    pub name: String,
    pub runs_ms: Vec<f64>,
    pub note: String,
}

impl Timing {
    fn sorted(&self) -> Vec<f64> {
        let mut sorted = self.runs_ms.clone();
        sorted.sort_by(f64::total_cmp);
        sorted
    }

    pub fn first_ms(&self) -> f64 {
        self.runs_ms.first().copied().unwrap_or(0.0)
    }

    pub fn median_ms(&self) -> f64 {
        percentile(&self.sorted(), 50)
    }

    pub fn p90_ms(&self) -> f64 {
        percentile(&self.sorted(), 90)
    }
}

/// Nearest-rank percentile of an ascending list.
fn percentile(sorted: &[f64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted.len()).div_ceil(100).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// What the store holds, for the report's header.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub files: i64,
    pub tags: i64,
    pub mappings: i64,
    pub potential_pairs: i64,
    pub database_bytes: u64,
}

#[derive(Debug)]
pub struct Report {
    pub version: String,
    pub machine: Vec<(String, String)>,
    pub counts: Counts,
    pub runs: usize,
    pub timings: Vec<Timing>,
}

/// Run `f` `runs` times, timing each; the note is the last run's.
fn measure<T: ToString>(
    group: &'static str,
    name: impl Into<String>,
    runs: usize,
    mut f: impl FnMut() -> Result<T>,
) -> Result<Timing> {
    let mut runs_ms = Vec::with_capacity(runs);
    let mut note = String::new();
    for _ in 0..runs.max(1) {
        let started = Instant::now();
        let out = f()?;
        runs_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        note = out.to_string();
    }
    Ok(Timing {
        group,
        name: name.into(),
        runs_ms,
        note,
    })
}

/// Time everything on the store in `options.dir`.
pub fn run_bench(options: &Options) -> Result<Report> {
    let dir: &Path = &options.dir;
    let runs = options.runs.max(1);
    let mut timings = Vec::new();

    timings.push(measure("startup", "store open (read-only)", runs, || {
        let store = Store::open_read_only(dir)?;
        Ok(format!(
            "{} services",
            store.snapshot().services.all().count()
        ))
    })?);

    let store = Store::open_read_only(dir)
        .with_context(|| format!("opening {} read-only", dir.display()))?;
    let snapshot = store.snapshot();
    let my_files = snapshot.services.builtin(builtin_keys::MY_FILES)?.id;
    let my_tags = snapshot.services.builtin(builtin_keys::MY_TAGS)?.id;
    let tag_service_key = snapshot
        .services
        .builtin(builtin_keys::MY_TAGS)?
        .key
        .clone();

    let counts = counts(&store, my_files, my_tags, dir)?;

    // what the main window reads before it shows: its session settings, the
    // last session and its pages' file counts
    timings.push(measure("startup", "startup queries", runs, || {
        let (pages, files) = store.read(|conn| {
            let _: hydrus_store::settings::GuiSessionSettings = hydrus_store::settings::get(conn)?;
            let session = hydrus_store::sessions::load(conn, hydrus_store::sessions::LAST_SESSION)?;
            let files = hydrus_store::sessions::page_file_counts(conn)?;
            Ok((session.map_or(0, |s| s.pages.len()), files.len()))
        })?;
        Ok(format!("last session: {pages} pages, {files} with files"))
    })?);

    // searches
    let mut searches: Vec<(String, Vec<String>)> = Vec::new();
    let top = top_tags(&store, my_files, my_tags, 8)?;
    match top.as_slice() {
        [] => {}
        [one] => searches.push(("a single tag".to_owned(), vec![one.clone()])),
        [one, two, ..] => {
            searches.push(("a single tag".to_owned(), vec![one.clone()]));
            searches.push(("two tags".to_owned(), vec![one.clone(), two.clone()]));
            searches.push((
                "a negated tag".to_owned(),
                vec!["system:everything".to_owned(), format!("-{two}")],
            ));
        }
    }
    searches.push(("system:inbox".to_owned(), vec!["system:inbox".to_owned()]));
    searches.push((
        "system:limit".to_owned(),
        vec![
            "system:everything".to_owned(),
            "system:limit=500".to_owned(),
        ],
    ));
    if let Some(namespace) = top
        .iter()
        .find_map(|t| t.split_once(':').map(|(ns, _)| ns.to_owned()))
    {
        searches.push((
            "a namespace wildcard".to_owned(),
            vec![format!("{namespace}:*")],
        ));
    }
    for text in &options.searches {
        let tags: Vec<String> = text
            .split(';')
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .collect();
        if tags.is_empty() {
            bail!("--search {text:?} has no tags in it");
        }
        searches.push((format!("your search: {text}"), tags));
    }
    let clock = Clock::system();
    let sort = FileSort {
        by: SortBy::ImportTime,
        order: SortOrder::Descending,
    };
    for (name, tags) in searches {
        let predicates = parse_api_search(&json!(tags))
            .map_err(|e| anyhow::anyhow!("the search {tags:?} didn't parse: {e}"))?;
        let search = FileSearchContext {
            location: LocationContext::default(),
            tags: TagContext::new(tag_service_key.clone(), true, true),
            predicates,
        };
        timings.push(measure(
            "searches",
            format!("{name} [{}]", tags.join("; ")),
            runs,
            || {
                let found = store.read(|conn| {
                    search_files(conn, &snapshot, &search, sort, &clock)
                        .map_err(|e| hydrus_store::StoreError::Invalid(e.to_string()))
                })?;
                Ok(format!("{} files", found.len()))
            },
        )?);
    }

    // autocomplete for a short prefix of a popular tag
    if let Some(prefix) = top.first().map(|t| autocomplete_prefix(t)) {
        let input = AutocompleteInput::parse(&prefix);
        timings.push(measure(
            "autocomplete",
            format!("autocomplete for \"{prefix}\""),
            runs,
            || {
                let matches = store.read(|conn| {
                    let rules = hydrus_store::settings::get::<AutocompleteSettings>(conn)?
                        .rules(&tag_service_key);
                    let Some(query) = input.tag_query(&rules) else {
                        return Ok(0);
                    };
                    let scope = TagSearchScope {
                        domains: vec![CountDomain {
                            service: my_files,
                            exact: true,
                        }],
                        tag_service: Some(my_tags),
                        display: TagDisplayType::Display,
                        include_current: true,
                        include_pending: true,
                    };
                    Ok(
                        search_tags(conn, &snapshot.services, &snapshot.display, &scope, &query)?
                            .len(),
                    )
                })?;
                Ok(format!("{matches} tags"))
            },
        )?);
    }

    // a page of thumbnails: their metadata, then their pixels from disk
    let page = newest_files(&store, my_files, 500)?;
    timings.push(measure(
        "thumbnails",
        format!("load basic metadata of {} files (a page)", page.len()),
        runs,
        || {
            let loaded = store.read(|conn| hydrus_store::media::load_basic(conn, &page))?;
            Ok(format!("{} loaded", loaded.len()))
        },
    )?);
    timings.push(measure(
        "thumbnails",
        format!("load full metadata of {} files (tags, urls...)", page.len()),
        runs,
        || {
            let loaded = store.read(|conn| {
                hydrus_store::media::load(conn, &snapshot.services, Some(&snapshot.display), &page)
            })?;
            Ok(format!("{} loaded", loaded.results.len()))
        },
    )?);
    let wanted = &page[..page.len().min(options.thumbnails)];
    timings.push(measure(
        "thumbnails",
        format!("read and decode {} thumbnails from disk", wanted.len()),
        runs,
        || {
            let hashes = store.read(|conn| hydrus_store::master::hashes(conn, wanted))?;
            let (mut decoded, mut missing) = (0usize, 0usize);
            for id in wanted {
                let rendered = hashes
                    .get(id)
                    .and_then(|hash| snapshot.storage.thumbnail_path(hash))
                    .and_then(|path| std::fs::read(path).ok())
                    .and_then(|bytes| hydrus_media::decode_image(&bytes).ok());
                if rendered.is_some() {
                    decoded += 1;
                } else {
                    missing += 1;
                }
            }
            Ok(format!(
                "{decoded} decoded, {missing} missing or unreadable"
            ))
        },
    )?);

    // tag siblings and parents
    timings.push(measure(
        "tag relations",
        "load the sibling and parent graphs",
        runs,
        || {
            let graphs = store.read(|conn| {
                hydrus_store::display::DisplayGraphs::load(conn, &snapshot.services)
            })?;
            Ok(format!(
                "{} rules in my tags",
                graphs.get(my_tags).rule_count()
            ))
        },
    )?);
    let related = related_tags(&store, my_tags, 200)?;
    let graph = snapshot.display.get(my_tags);
    timings.push(measure(
        "tag relations",
        format!("look up siblings and parents of {} tags", related.len()),
        runs,
        || {
            let mut found = 0usize;
            for tag in &related {
                found += graph.chain(*tag).len() + graph.ancestors(*tag).len();
                found += graph.descendants(*tag).len();
                found += usize::from(graph.ideal(*tag) != *tag);
            }
            Ok(format!("{found} related tags"))
        },
    )?);

    // potential duplicates
    let search = potentials_everything(&snapshot, my_files)?;
    timings.push(measure(
        "duplicates",
        "count potential duplicate pairs",
        runs,
        || {
            let query =
                hydrus_duplicates::potentials::PotentialsQuery::from_search(&snapshot, &search)?;
            let count = store.read(|conn| query.count(conn, &snapshot))?;
            Ok(count.map_or_else(|| "search failed".to_owned(), |n| format!("{n} pairs")))
        },
    )?);

    Ok(Report {
        version: format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            env!("HYDRUS_GIT_DESCRIBE")
        ),
        machine: machine_info(),
        counts,
        runs,
        timings,
    })
}

/// "Every file, any pair within 8": the duplicates page's widest search.
fn potentials_everything(
    snapshot: &hydrus_store::Snapshot,
    _my_files: ServiceId,
) -> Result<hydrus_core::duplicates::DuplicatesSearch> {
    use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
    let _ = snapshot;
    let everything = FileSearchContext {
        location: LocationContext::default(),
        tags: TagContext::default(),
        predicates: parse_api_search(&json!(["system:everything"]))
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    };
    Ok(DuplicatesSearch {
        search_1: everything.clone(),
        search_2: everything,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 8,
    })
}

fn counts(store: &Store, my_files: ServiceId, my_tags: ServiceId, dir: &Path) -> Result<Counts> {
    let tables = MappingTables::new(my_tags);
    let counts = store.read(|conn| {
        let files = conn.query_row(
            "SELECT COUNT(*) FROM file_domain_current WHERE service_id = ?1",
            [my_files],
            |r| r.get(0),
        )?;
        let tags = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))?;
        let mappings = conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(current), 0) FROM {} WHERE domain_id = ?1",
                tables.counts
            ),
            [my_files],
            |r| r.get(0),
        )?;
        let potential_pairs =
            conn.query_row("SELECT COUNT(*) FROM potential_pairs", [], |r| r.get(0))?;
        Ok(Counts {
            files,
            tags,
            mappings,
            potential_pairs,
            database_bytes: 0,
        })
    })?;
    let database_bytes =
        std::fs::metadata(dir.join(hydrus_store::store::DB_FILE_NAME)).map_or(0, |m| m.len());
    Ok(Counts {
        database_bytes,
        ..counts
    })
}

/// The most used tags in my tags over my files, most first.
fn top_tags(
    store: &Store,
    my_files: ServiceId,
    my_tags: ServiceId,
    n: usize,
) -> Result<Vec<String>> {
    let tables = MappingTables::new(my_tags);
    let ids: Vec<TagId> = store.read(|conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT tag_id FROM {} WHERE domain_id = ?1 AND current > 0 \
             ORDER BY current DESC LIMIT ?2",
            tables.counts
        ))?;
        let rows = stmt.query_map(rusqlite::params![my_files, n as i64], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<TagId>, _>>()?)
    })?;
    let tags = store.read(|conn| hydrus_store::master::tags(conn, &ids))?;
    Ok(ids
        .iter()
        .filter_map(|id| tags.get(id).map(|t| t.as_str().to_owned()))
        .collect())
}

/// What a user types before the list shows: the first three characters of
/// the tag's subtag, with the namespace if it has one.
fn autocomplete_prefix(tag: &str) -> String {
    let (namespace, subtag) = tag.split_once(':').unwrap_or(("", tag));
    let start: String = subtag.chars().take(3).collect();
    if namespace.is_empty() {
        start
    } else {
        format!("{namespace}:{start}")
    }
}

/// The newest files in my files: the first page a search page shows.
fn newest_files(store: &Store, my_files: ServiceId, n: usize) -> Result<Vec<HashId>> {
    Ok(store.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 \
             ORDER BY added_ms DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![my_files, n as i64], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<HashId>, _>>()?)
    })?)
}

/// Tags with a sibling or parent rule, to look up (else the popular tags').
fn related_tags(store: &Store, my_tags: ServiceId, n: usize) -> Result<Vec<TagId>> {
    Ok(store.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT bad_tag_id FROM tag_siblings WHERE service_id = ?1 \
             UNION SELECT child_tag_id FROM tag_parents WHERE service_id = ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![my_tags, n as i64], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<TagId>, _>>()?)
    })?)
}

fn machine_info() -> Vec<(String, String)> {
    let mut info = vec![
        (
            "os".to_owned(),
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        ),
        (
            "logical cpus".to_owned(),
            std::thread::available_parallelism()
                .map_or_else(|_| "unknown".to_owned(), |n| n.to_string()),
        ),
    ];
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, name)| name.trim().to_owned())
        })
        .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok());
    if let Some(cpu) = cpu {
        info.push(("cpu".to_owned(), cpu));
    }
    let memory_kib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
        });
    if let Some(kib) = memory_kib {
        info.push((
            "memory".to_owned(),
            format!("{:.1} GiB", kib as f64 / 1_048_576.0),
        ));
    }
    info
}

impl Report {
    /// The plain-text report.
    pub fn text(&self) -> String {
        let mut out = String::new();
        out.push_str("hydrus bench\n");
        out.push_str(&format!("version: {}\n", self.version));
        for (key, value) in &self.machine {
            out.push_str(&format!("{key}: {value}\n"));
        }
        let c = &self.counts;
        out.push_str(&format!(
            "store: {} files, {} tags, {} mappings, {} potential pairs, database {:.1} MiB\n",
            c.files,
            c.tags,
            c.mappings,
            c.potential_pairs,
            c.database_bytes as f64 / 1_048_576.0
        ));
        out.push_str(&format!(
            "runs of each: {}  (times in milliseconds)\n",
            self.runs
        ));
        let mut group = "";
        for t in &self.timings {
            if t.group != group {
                group = t.group;
                out.push_str(&format!("\n[{group}]\n"));
                out.push_str(&format!(
                    "  {:>10} {:>10} {:>10}  what\n",
                    "first", "median", "p90"
                ));
            }
            out.push_str(&format!(
                "  {:>10.2} {:>10.2} {:>10.2}  {} ({})\n",
                t.first_ms(),
                t.median_ms(),
                t.p90_ms(),
                t.name,
                t.note
            ));
        }
        out
    }

    pub fn json(&self) -> Value {
        json!({
            "version": self.version,
            "machine": self.machine.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
            "store": {
                "files": self.counts.files,
                "tags": self.counts.tags,
                "mappings": self.counts.mappings,
                "potential_pairs": self.counts.potential_pairs,
                "database_bytes": self.counts.database_bytes,
            },
            "runs": self.runs,
            "timings": self.timings.iter().map(|t| json!({
                "group": t.group,
                "name": t.name,
                "note": t.note,
                "first_ms": t.first_ms(),
                "median_ms": t.median_ms(),
                "p90_ms": t.p90_ms(),
                "runs_ms": t.runs_ms,
            })).collect::<Vec<_>>(),
        })
    }
}

/// `hydrus bench`: run it and print the report.
pub fn run(options: &Options) -> Result<()> {
    if !options
        .dir
        .join(hydrus_store::store::DB_FILE_NAME)
        .is_file()
    {
        bail!(
            "{} is not a hydrus-rs store (no {})",
            options.dir.display(),
            hydrus_store::store::DB_FILE_NAME
        );
    }
    let report = run_bench(options)?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report.json())?);
    } else {
        print!("{}", report.text());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let sorted = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert!((percentile(&sorted, 50) - 5.0).abs() < 1e-9);
        assert!((percentile(&sorted, 90) - 9.0).abs() < 1e-9);
        assert!((percentile(&[7.0], 90) - 7.0).abs() < 1e-9);
        assert!(percentile(&[], 90).abs() < 1e-9);
    }

    #[test]
    fn the_autocomplete_prefix_keeps_the_namespace() {
        assert_eq!(autocomplete_prefix("character:samus aran"), "character:sam");
        assert_eq!(autocomplete_prefix("blue sky"), "blu");
    }
}
