//! An importer's file log window (the reference's `EditFileSeedCachePanel`)
//! and its menus: each file's row, the right-click menu on selected rows
//! (`_GetListCtrlMenu`), and the whole log's menu (`PopulateFileSeedCache
//! Menu`, also the file log button's). Menus are trees of [`Entry`], each
//! item carrying the [`Action`] it does. Recorded by
//! `oracle/record_file_log.py`.

use hydrus_core::numbers::human_int;
use hydrus_store::queues::{FileSeed, SeedStatus, SeedType, StatusCounts};
use hydrus_store::settings::GuiFormatting;

/// The file log's column titles.
pub const COLUMNS: [&str; 7] = [
    "#",
    "source",
    "status",
    "added",
    "last modified",
    "source time",
    "note",
];

/// A URL as the reference shows it (`ConvertURLToHumanString`):
/// percent-decoded, as UTF-8 (`urllib.parse.unquote`).
pub fn human_url(url: &str) -> String {
    let bytes = url.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(&h), Some(&l)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(h), Some(l)) = (hex(h), hex(l))
        {
            out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A status as the file log names it (`status_string_lookup`; unknown is
/// blank).
pub fn status_text(status: SeedStatus) -> &'static str {
    match status {
        SeedStatus::Unknown => "",
        SeedStatus::SuccessfulAndNew => "successful",
        SeedStatus::SuccessfulButRedundant => "already in db",
        SeedStatus::Deleted => "deleted",
        SeedStatus::Error => "error",
        SeedStatus::Vetoed => "ignored",
        SeedStatus::Skipped => "skipped",
        SeedStatus::SuccessfulAndChildFiles => "created children",
    }
}

/// A file's row (`_ConvertFileSeedToDisplayTuple`): its place in the log,
/// its source, status, when it was added and last changed, when it was
/// posted, and its note's first line.
pub fn row(seed: &FileSeed, index: usize, now: i64) -> Vec<String> {
    row_with_format(seed, index, now, &GuiFormatting::default())
}

/// The live client formatting preference, over an unchanged seed.
pub fn row_with_format(
    seed: &FileSeed,
    index: usize,
    now: i64,
    formatting: &GuiFormatting,
) -> Vec<String> {
    let source = if seed.seed_type == SeedType::Url {
        human_url(&seed.data_for_comparison)
    } else {
        seed.data_for_comparison.clone()
    };
    vec![
        human_int(index as u64),
        source,
        status_text(seed.status).to_owned(),
        crate::gui_format::timestamp(formatting, Some(seed.created), now),
        crate::gui_format::timestamp(formatting, Some(seed.modified), now),
        seed.source_time.map_or_else(
            || "unknown".to_owned(),
            |t| crate::gui_format::timestamp(formatting, Some(t), now),
        ),
        seed.note.lines().next().unwrap_or_default().to_owned(),
    ]
}

/// Parse the reference clipboard source batch. Its first source sets the type
/// for every seed; URL encoding also applies to URL-looking lines in path batches.
pub fn pasted_sources(
    text: &str,
    classes: &hydrus_core::url::UrlClasses,
) -> Result<Vec<hydrus_store::queues::NewFileSeed>, String> {
    let sources: Vec<String> = text
        .split([
            '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
            '\u{2028}', '\u{2029}',
        ])
        .map(|s| s.trim_start_matches('\u{feff}').trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            hydrus_core::url::ensure_url_is_encoded(
                s,
                false,
                classes.settings().collapse_leading_slashes,
            )
        })
        .collect();
    let first = sources.first().ok_or_else(|| {
        "Could not understand the clipboard as Lines of URLs or file paths: no sources.".to_owned()
    })?;
    let seed_type = if first.starts_with("http") {
        SeedType::Url
    } else {
        SeedType::Path
    };
    Ok(sources
        .into_iter()
        .map(|source| {
            let (data, data_for_comparison) = if seed_type == SeedType::Url {
                match classes.normalise(&source, true).and_then(|data| {
                    classes
                        .normalise(&source, false)
                        .map(|comparison| (data, comparison))
                }) {
                    Ok(pair) => pair,
                    Err(_) => (source.clone(), source),
                }
            } else {
                (source.clone(), source)
            };
            hydrus_store::queues::NewFileSeed {
                seed_type,
                data,
                data_for_comparison,
                source_time: None,
                referral_url: None,
                meta: hydrus_store::queues::FileSeedMeta::default(),
            }
        })
        .collect())
}

/// One OR container of exact URL predicates, as the selected-row action uses.
pub fn url_search(urls: &[String]) -> Vec<hydrus_core::search::predicate::Predicate> {
    use hydrus_core::search::predicate::{Predicate, SystemPredicate, UrlRule};
    vec![Predicate::Or(
        urls.iter()
            .filter(|url| url.starts_with("http"))
            .map(|url| {
                Predicate::System(SystemPredicate::KnownUrl {
                    rule: UrlRule::ExactMatch(url.clone()),
                    has: true,
                })
            })
            .collect(),
    )]
}

/// Reference confirmation before re-normalising and discarding later duplicates.
pub const RENORMALISE_QUESTION: &str = "Are you sure you want to renormalise all the URLs in here (and discard any subsequent duplicates)? This typically only makes sense if you have changed the URL Class rules after this list was created (e.g. to remove an ephemeral token parameter) and you now need to collapse the existing list to catch future duplicates better.\n\nIf you do not know exactly what this does, click no.";

/// Selected import objects in the reference SerialisableList v3/FileSeed v8
/// layout, including progress, headers, hashes, tags, source URLs and notes.
pub fn export_objects(seeds: &[&FileSeed]) -> Result<String, String> {
    use serde_json::json;
    let objects = seeds
        .iter()
        .map(|s| {
            let headers: serde_json::Map<String, serde_json::Value> = s
                .meta
                .request_headers
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            json!([
                2,
                [
                    57,
                    8,
                    [
                        s.seed_type as i64,
                        s.data,
                        s.data_for_comparison,
                        s.created,
                        s.modified,
                        s.source_time,
                        s.status.code(),
                        s.note,
                        s.referral_url,
                        headers,
                        s.meta.external_filterable_tags,
                        [77, 1, s.meta.external_additional_tags],
                        s.meta.primary_urls,
                        s.meta.source_urls,
                        s.meta.tags,
                        s.meta.notes,
                        s.meta.hashes
                    ]
                ]
            ])
        })
        .collect::<Vec<_>>();
    let text = serde_json::to_string(&json!([26, 3, objects])).map_err(|e| e.to_string())?;
    hydrus_core::pyjson::PyJson::parse(&text)
        .map(|v| v.to_python_string())
        .map_err(|e| e.to_string())
}

/// What a menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Failed files to be tried again.
    RetryFailed,
    /// Ignored files to be tried again.
    RetryIgnored,
    /// Remove files with these statuses from the log.
    DeleteStatuses(Vec<SeedStatus>),
    /// Unstarted files skipped.
    SkipUnknown,
    /// The log's files (the new ones only, or all) in a new page.
    ShowFiles { new_only: bool },
    /// The log's order reversed.
    Reverse,
    /// Every source to the clipboard, a line each.
    ExportToClipboard,
    /// New sources (URLs or paths) from the clipboard.
    ImportFromClipboard,
    /// New source lines from a reference string PNG carrier.
    ImportFromPng,
    /// All source lines exported as a reference string PNG carrier.
    ExportToPng,
    /// Selected complete FileSeed objects as reference clipboard JSON.
    ExportObjects,
    /// Confirm normalization of all URLs and discard subsequent duplicates.
    Renormalise,
    /// The selected files in a new page.
    OpenSelectedFiles,
    /// The selected's sources to the clipboard.
    CopySources,
    /// The selected's notes to the clipboard.
    CopyNotes,
    /// Open the selected's URLs, or their files' locations.
    OpenSources,
    /// A new page searching for the selected's URLs.
    SearchUrls,
    /// The selected set to be tried again.
    TryAgain,
    /// The selected skipped.
    Skip,
    /// The selected removed, after a question.
    DeleteSelected,
    /// Not in hydrus-rs yet (sources as png, the advanced entries).
    NotYet,
}

/// A menu entry, its items doing an `A`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry<A = Action> {
    Item(String, A),
    /// Text shown, which does nothing.
    Label(String),
    Separator,
    Menu(String, Vec<Entry<A>>),
}

fn item(label: impl Into<String>, action: Action) -> Entry {
    Entry::Item(label.into(), action)
}

/// What the whole log's menu needs to know: its files' counts by status,
/// how many there are, and whether they are URLs (an empty log's are).
#[derive(Debug, Clone, Default)]
pub struct LogFacts {
    pub counts: StatusCounts,
    pub len: usize,
    pub urls: bool,
}

impl LogFacts {
    fn count(&self, status: SeedStatus) -> usize {
        self.counts.get(&status).copied().unwrap_or(0)
    }
}

/// The whole log's menu (`PopulateFileSeedCacheMenu`); `any_selected`
/// adds the advanced submenu's export of the selected.
pub fn log_menu(log: &LogFacts, any_selected: bool) -> Vec<Entry> {
    use SeedStatus::{
        Deleted, Error, Skipped, SuccessfulAndChildFiles, SuccessfulAndNew, SuccessfulButRedundant,
        Unknown, Vetoed,
    };
    let already_in = log.count(SuccessfulButRedundant);
    let successful = log.count(SuccessfulAndNew) + already_in;
    let vetoed = log.count(Vetoed);
    let deleted = log.count(Deleted);
    let errors = log.count(Error);
    let skipped = log.count(Skipped);
    let unknown = log.count(Unknown);
    let n = |n: usize| human_int(n as u64);
    let delete = |count: usize, what: &str, statuses: Vec<SeedStatus>| {
        item(
            format!(
                "delete {} '{what}' file import items from the queue",
                n(count)
            ),
            Action::DeleteStatuses(statuses),
        )
    };
    let mut menu = Vec::new();
    if errors > 0 {
        menu.push(item(
            format!("retry {} failures", n(errors)),
            Action::RetryFailed,
        ));
    }
    if vetoed > 0 {
        menu.push(item(
            format!("retry {} ignored", n(vetoed)),
            Action::RetryIgnored,
        ));
    }
    menu.push(Entry::Separator);
    if successful > 0 {
        menu.push(delete(
            successful,
            "successful",
            vec![
                SuccessfulAndNew,
                SuccessfulButRedundant,
                SuccessfulAndChildFiles,
            ],
        ));
    }
    if already_in > 0 {
        menu.push(delete(
            already_in,
            "already in db",
            vec![SuccessfulButRedundant],
        ));
    }
    if deleted > 0 {
        menu.push(delete(deleted, "previously deleted", vec![Deleted]));
    }
    if errors > 0 {
        menu.push(delete(errors, "failed", vec![Error]));
    }
    if vetoed > 0 {
        menu.push(delete(vetoed, "ignored", vec![Vetoed]));
    }
    if skipped > 0 {
        menu.push(delete(skipped, "skipped", vec![Skipped]));
    }
    if unknown > 0 {
        menu.push(item(
            format!(
                "delete {} 'unknown' (i.e. unstarted) file import items from the queue",
                n(unknown)
            ),
            Action::DeleteStatuses(vec![Unknown]),
        ));
    }
    let started = vec![
        SuccessfulAndNew,
        SuccessfulButRedundant,
        Deleted,
        Error,
        Vetoed,
        Skipped,
        SuccessfulAndChildFiles,
    ];
    if log.len > 0 {
        menu.push(Entry::Separator);
        let non_unknown = log.len - unknown;
        if unknown > 0 && non_unknown > 0 {
            menu.push(item(
                format!(
                    "delete everything except 'unknown' (i.e. unstarted) ({} items) from the queue",
                    n(non_unknown)
                ),
                Action::DeleteStatuses(started.clone()),
            ));
        }
        let mut everything = vec![Unknown];
        everything.extend(started);
        menu.push(item(
            format!("delete everything ({} items) from the queue", n(log.len)),
            Action::DeleteStatuses(everything),
        ));
    }
    if unknown > 0 {
        menu.push(Entry::Separator);
        menu.push(item(
            format!(
                "set {} 'unknown' (i.e. unstarted) file import items to 'skipped'",
                n(unknown)
            ),
            Action::SkipUnknown,
        ));
    }
    menu.push(Entry::Separator);
    if successful > 0 {
        menu.push(item(
            "show new files in a new page",
            Action::ShowFiles { new_only: true },
        ));
        menu.push(item(
            "show all files in a new page",
            Action::ShowFiles { new_only: false },
        ));
    }
    menu.push(Entry::Separator);
    if log.len > 0 {
        menu.push(item("reverse import order", Action::Reverse));
        menu.push(Entry::Separator);
        menu.push(Entry::Menu(
            "export all sources".into(),
            vec![
                item("to clipboard", Action::ExportToClipboard),
                item("to png", Action::ExportToPng),
            ],
        ));
    }
    menu.push(Entry::Menu(
        "ADVANCED: import new sources".into(),
        vec![
            item("from clipboard", Action::ImportFromClipboard),
            item("from png", Action::ImportFromPng),
        ],
    ));
    if any_selected || log.urls {
        let mut advanced = Vec::new();
        if any_selected {
            advanced.push(item(
                "export selected import objects to clipboard",
                Action::ExportObjects,
            ));
        }
        if log.urls {
            advanced.push(item("re-normalise all URLs", Action::Renormalise));
        }
        menu.push(Entry::Menu("advanced".into(), advanced));
    }
    tidy(menu)
}

/// Separators as Qt shows them: none leading, trailing or doubled.
pub(crate) fn tidy<A>(entries: Vec<Entry<A>>) -> Vec<Entry<A>> {
    let mut out: Vec<Entry<A>> = Vec::new();
    for entry in entries {
        if matches!(entry, Entry::Separator)
            && out.last().is_none_or(|e| matches!(e, Entry::Separator))
        {
            continue;
        }
        out.push(entry);
    }
    if matches!(out.last(), Some(Entry::Separator)) {
        out.pop();
    }
    out
}

/// The URL details of one selected URL file (`_GetListCtrlMenu`): the
/// normalised and request URLs where they differ, its referral URL, and
/// its primary and source URLs.
fn url_entries(seed: &FileSeed) -> Vec<Entry> {
    let mut urls = Vec::new();
    let pretty = human_url(&seed.data_for_comparison);
    if seed.data_for_comparison != pretty {
        urls.push(Entry::Label(format!(
            "normalised url: {}",
            seed.data_for_comparison
        )));
    }
    if seed.data != seed.data_for_comparison {
        urls.push(Entry::Label(format!("request url: {}", seed.data)));
    }
    if let Some(referral) = &seed.referral_url {
        urls.push(Entry::Label(format!("referral url: {referral}")));
    }
    if !seed.meta.primary_urls.is_empty() {
        urls.push(Entry::Separator);
        for url in &seed.meta.primary_urls {
            urls.push(Entry::Label(format!("primary url: {url}")));
        }
    }
    if !seed.meta.source_urls.is_empty() {
        urls.push(Entry::Separator);
        for url in &seed.meta.source_urls {
            urls.push(Entry::Label(format!("source url: {url}")));
        }
    }
    tidy(urls)
}

/// A labelled list as a submenu, or its "no ..." label.
fn list_or_none(title: &str, none: &str, items: Vec<String>) -> Entry {
    if items.is_empty() {
        Entry::Label(none.into())
    } else {
        Entry::Menu(title.into(), items.into_iter().map(Entry::Label).collect())
    }
}

/// The right-click menu on the selected files (`_GetListCtrlMenu`); with
/// none selected, the whole log's.
pub fn row_menu(selected: &[&FileSeed], log: &LogFacts) -> Vec<Entry> {
    if selected.is_empty() {
        return log_menu(log, false);
    }
    let urls = log.urls;
    let mut menu = vec![Entry::Separator];
    if selected.iter().any(|s| s.meta.hash("sha256").is_some()) {
        menu.push(item(
            "open selected files in a new page",
            Action::OpenSelectedFiles,
        ));
        menu.push(Entry::Separator);
    }
    menu.push(item(
        if urls { "copy urls" } else { "copy paths" },
        Action::CopySources,
    ));
    menu.push(item("copy notes", Action::CopyNotes));
    menu.push(Entry::Separator);
    menu.push(item(
        if urls {
            "open URLs"
        } else {
            "open files' locations"
        },
        Action::OpenSources,
    ));
    if urls {
        menu.push(item("search for URLs", Action::SearchUrls));
    }
    if let [seed] = selected {
        menu.push(Entry::Separator);
        let hashes: Vec<String> = ["sha256", "md5", "sha1", "sha512"]
            .iter()
            .filter_map(|t| seed.meta.hash(t).map(|h| format!("{t}:{h}")))
            .collect();
        menu.push(list_or_none("hashes", "no hashes yet", hashes));
        if seed.seed_type == SeedType::Url {
            let details = url_entries(seed);
            menu.push(if details.is_empty() {
                Entry::Label("no additional urls".into())
            } else {
                Entry::Menu("additional urls".into(), details)
            });
            let headers: Vec<String> = seed
                .meta
                .request_headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect();
            menu.push(list_or_none(
                "additional headers",
                "no additional headers",
                headers,
            ));
            let mut parsed: Vec<String> = seed.meta.tags.iter().cloned().collect();
            parsed.sort_by_key(|t| hydrus_core::sort::human_sort_key(t));
            menu.push(list_or_none("parsed tags", "no parsed tags", parsed));
            let mut inherited: Vec<String> =
                seed.meta.external_filterable_tags.iter().cloned().collect();
            inherited.sort_by_key(|t| hydrus_core::sort::human_sort_key(t));
            menu.push(list_or_none(
                "inherited tags",
                "no inherited tags",
                inherited,
            ));
        }
    }
    menu.push(Entry::Separator);
    menu.push(item("try again", Action::TryAgain));
    menu.push(item("skip", Action::Skip));
    menu.push(item("delete from list", Action::DeleteSelected));
    menu.push(Entry::Separator);
    menu.push(Entry::Menu("whole log".into(), log_menu(log, true)));
    tidy(menu)
}

/// What deleting the selected asks.
pub fn delete_question(n: usize) -> String {
    format!(
        "Are you sure you want to delete the {} selected entries?",
        human_int(n as u64)
    )
}

/// What opening many selected asks first.
pub const OPEN_MANY_QUESTION: &str =
    "You have many objects selected--are you sure you want to open them all?";
