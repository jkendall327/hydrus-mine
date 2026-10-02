//! The options window (file > options): the reference's options dialog
//! (`ManageOptionsPanel`), its pages listed as it lists them (by name,
//! "advanced" last), each with the options hydrus-rs honours, in their
//! boxes and labelled as the reference's are (checked against the
//! reference's dialog, recorded by `oracle/record_options_dialog.py`).
//! Changes wait until "apply", which writes them to the store together.

use std::rc::Rc;

use hydrus_core::media_viewer::{InfoLineSettings, MediaViewerSettings, SlideshowSettings};
use hydrus_core::pages::{DownloaderPageSettings, FileCountDisplay, PageNameSettings};
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_core::thumbnail::ThumbnailSettings;
use hydrus_store::delete_lock::DeleteLock;
use hydrus_store::settings::{AdvancedMode, ExportSettings, FileHandlingSettings, FolderSettings};
use hydrus_store::trash::TrashSettings;
use rusqlite::Connection;

macro_rules! settings {
    ($($field:ident: $ty:ty),* $(,)?) => {
        /// The settings the options window edits, as the store has them.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Settings {
            $(pub $field: $ty,)*
        }

        impl Settings {
            pub fn load(conn: &Connection) -> hydrus_store::Result<Self> {
                Ok(Self {
                    $($field: hydrus_store::settings::get(conn)?,)*
                })
            }

            /// Write those changed since `before`.
            pub fn save(&self, conn: &Connection, before: &Self) -> hydrus_store::Result<()> {
                $(
                    if self.$field != before.$field {
                        hydrus_store::settings::set(conn, &self.$field)?;
                    }
                )*
                Ok(())
            }
        }
    };
}

settings! {
    advanced: AdvancedMode,
    delete_lock: DeleteLock,
    downloader_pages: DownloaderPageSettings,
    export: ExportSettings,
    file_handling: FileHandlingSettings,
    folders: FolderSettings,
    info_line: InfoLineSettings,
    media_viewer: MediaViewerSettings,
    page_names: PageNameSettings,
    slideshow: SlideshowSettings,
    tag_presentation: TagPresentation,
    thumbnails: ThumbnailSettings,
    trash: TrashSettings,
}

/// An option's value as its control holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Check(bool),
    Int(i64),
    /// A number, or none (the reference's `NoneableSpinCtrl`).
    Noneable(Option<i64>),
    /// A number with a fraction, as typed (read when applied).
    Float(String),
    /// The index of the item chosen.
    Choice(usize),
    Text(String),
}

/// What kind of control an option has.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Check,
    Int {
        min: i64,
        max: i64,
    },
    /// The number shown while it is none is `default`.
    Noneable {
        none_phrase: &'static str,
        default: i64,
        min: i64,
        max: i64,
        unit: Option<&'static str>,
    },
    Float {
        min: f64,
        max: f64,
    },
    Choice(&'static [&'static str]),
    Text,
}

type Get = Rc<dyn Fn(&Settings) -> Value>;
/// Set the value, or say why it wasn't set.
type Set = Rc<dyn Fn(&mut Settings, &Value) -> Result<(), String>>;

/// An option: its label (the reference's, before its control), control,
/// and where its value is kept.
#[derive(Clone)]
pub struct Opt {
    pub label: &'static str,
    pub kind: Kind,
    pub get: Get,
    pub set: Set,
}

impl std::fmt::Debug for Opt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opt")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// What a page lays out: options, and titled boxes of them.
#[derive(Debug, Clone)]
pub enum Item {
    Opt(Opt),
    Box(&'static str, Vec<Item>),
}

#[derive(Debug, Clone)]
pub struct Page {
    pub name: &'static str,
    pub items: Vec<Item>,
}

impl Page {
    /// Its options, in order.
    pub fn options(&self) -> Vec<&Opt> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Opt>) {
            for item in items {
                match item {
                    Item::Opt(opt) => out.push(opt),
                    Item::Box(_, items) => walk(items, out),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.items, &mut out);
        out
    }
}

fn wrong(label: &str) -> String {
    format!("{label}: not a value it can have")
}

fn opt(label: &'static str, kind: Kind, get: Get, set: Set) -> Item {
    Item::Opt(Opt {
        label,
        kind,
        get,
        set,
    })
}

fn check(label: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool)) -> Item {
    opt(
        label,
        Kind::Check,
        Rc::new(move |s| Value::Check(get(s))),
        Rc::new(move |s, v| match v {
            Value::Check(b) => {
                set(s, *b);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn int(
    label: &'static str,
    (min, max): (i64, i64),
    get: fn(&Settings) -> i64,
    set: fn(&mut Settings, i64),
) -> Item {
    opt(
        label,
        Kind::Int { min, max },
        Rc::new(move |s| Value::Int(get(s))),
        Rc::new(move |s, v| match v {
            Value::Int(n) => {
                set(s, (*n).clamp(min, max));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A noneable number's control: its none phrase, the number it shows when
/// none, its range and unit.
#[derive(Clone, Copy)]
struct NoneableKind {
    none_phrase: &'static str,
    default: i64,
    range: (i64, i64),
    unit: Option<&'static str>,
}

const fn none(
    none_phrase: &'static str,
    default: i64,
    range: (i64, i64),
    unit: Option<&'static str>,
) -> NoneableKind {
    NoneableKind {
        none_phrase,
        default,
        range,
        unit,
    }
}

fn noneable(
    label: &'static str,
    kind: NoneableKind,
    get: fn(&Settings) -> Option<i64>,
    set: fn(&mut Settings, Option<i64>),
) -> Item {
    let (min, max) = kind.range;
    opt(
        label,
        Kind::Noneable {
            none_phrase: kind.none_phrase,
            default: kind.default,
            min,
            max,
            unit: kind.unit,
        },
        Rc::new(move |s| Value::Noneable(get(s))),
        Rc::new(move |s, v| match v {
            Value::Noneable(n) => {
                set(s, n.map(|n| n.clamp(min, max)));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A float as the reference's spin boxes show it ("12.0").
fn float_text(f: f64) -> String {
    format!("{f:?}")
}

fn float(
    label: &'static str,
    (min, max): (f64, f64),
    get: fn(&Settings) -> f64,
    set: fn(&mut Settings, f64),
) -> Item {
    opt(
        label,
        Kind::Float { min, max },
        Rc::new(move |s| Value::Float(float_text(get(s)))),
        Rc::new(move |s, v| match v {
            Value::Float(text) => {
                let f: f64 = text
                    .trim()
                    .parse()
                    .map_err(|_| format!("{label} \"{text}\" is not a number"))?;
                if !(min..=max).contains(&f) {
                    return Err(format!("{label} must be from {min} to {max}"));
                }
                set(s, f);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn choice(
    label: &'static str,
    items: &'static [&'static str],
    get: fn(&Settings) -> usize,
    set: fn(&mut Settings, usize),
) -> Item {
    opt(
        label,
        Kind::Choice(items),
        Rc::new(move |s| Value::Choice(get(s))),
        Rc::new(move |s, v| match v {
            Value::Choice(i) if *i < items.len() => {
                set(s, *i);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn text(
    label: &'static str,
    get: fn(&Settings) -> String,
    set: fn(&mut Settings, &str) -> Result<(), String>,
) -> Item {
    opt(
        label,
        Kind::Text,
        Rc::new(move |s| Value::Text(get(s))),
        Rc::new(move |s, v| match v {
            Value::Text(t) => set(s, t),
            _ => Err(wrong(label)),
        }),
    )
}

fn boxed(title: &'static str, items: Vec<Item>) -> Item {
    Item::Box(title, items)
}

/// `CC.page_file_count_display_string_lookup`, in the order the reference
/// lists them.
const FILE_COUNTS: &[&str] = &[
    "for all pages",
    "for import pages",
    "for no pages",
    "for all pages, but only if greater than zero",
];

fn file_count_index(display: FileCountDisplay) -> usize {
    match display {
        FileCountDisplay::All => 0,
        FileCountDisplay::OnlyImporters => 1,
        FileCountDisplay::None => 2,
        FileCountDisplay::AllIfAny => 3,
    }
}

fn file_count_display(index: usize) -> FileCountDisplay {
    match index {
        0 => FileCountDisplay::All,
        1 => FileCountDisplay::OnlyImporters,
        2 => FileCountDisplay::None,
        _ => FileCountDisplay::AllIfAny,
    }
}

/// Slideshow durations as the reference writes them ("1.0,5.0,10.0").
fn durations_text(durations: &[f64]) -> String {
    durations
        .iter()
        .map(|d| float_text(*d))
        .collect::<Vec<_>>()
        .join(",")
}

/// The reference's parse of them (`MediaViewerPanel.UpdateOptions`): each
/// a number, those above zero kept.
fn parse_durations(text: &str) -> Result<Vec<f64>, String> {
    let durations: Vec<f64> = text
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| "Could not parse those slideshow durations, so they were not saved!")?;
    Ok(durations.into_iter().filter(|d| *d > 0.0).collect())
}

fn signed(n: Option<u64>) -> Option<i64> {
    n.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
}

fn unsigned(n: Option<i64>) -> Option<u64> {
    n.map(|n| u64::try_from(n).unwrap_or(0))
}

/// The pages, in the reference's order: sorted by name, then "advanced".
#[allow(clippy::too_many_lines)] // (a table)
pub fn pages() -> Vec<Page> {
    let page = |name, items| Page { name, items };
    vec![
        page(
            "audio",
            vec![text(
                "Label for files with audio: ",
                |s| s.info_line.has_audio_label.clone(),
                |s, t| {
                    t.clone_into(&mut s.info_line.has_audio_label);
                    Ok(())
                },
            )],
        ),
        page(
            "exporting",
            vec![boxed(
                "all exports",
                vec![
                    check(
                        "ADVANCED: Always apply NTFS filename rules to export filenames: ",
                        |s| s.export.always_apply_ntfs_rules,
                        |s, v| s.export.always_apply_ntfs_rules = v,
                    ),
                    noneable(
                        "ADVANCED: Export path length limit (characters/bytes): ",
                        none("let hydrus decide", 250, (96, 8192), None),
                        |s| s.export.path_character_limit,
                        |s, v| s.export.path_character_limit = v,
                    ),
                    noneable(
                        "ADVANCED: Export dirname length limit (characters/bytes): ",
                        none("let hydrus decide", 64, (16, 8192), None),
                        |s| s.export.dirname_character_limit,
                        |s, v| s.export.dirname_character_limit = v,
                    ),
                    int(
                        "ADVANCED: Export filename length limit (characters/bytes): ",
                        (16, 8192),
                        |s| s.export.filename_character_limit,
                        |s, v| s.export.filename_character_limit = v,
                    ),
                ],
            )],
        ),
        page(
            "files and trash",
            vec![
                check(
                    "When physically deleting files or folders, send them to the OS's recycle bin: ",
                    |s| s.folders.delete_to_recycle_bin,
                    |s, v| s.folders.delete_to_recycle_bin = v,
                ),
                noneable(
                    "Number of hours a file will stay in the trash before being deleted: ",
                    none("no age limit", 72, (0, 8640), None),
                    |s| signed(s.trash.max_age_hours),
                    |s, v| s.trash.max_age_hours = unsigned(v),
                ),
                noneable(
                    "Maximum size of trash (MB): ",
                    none("no size limit", 2048, (0, 20480), None),
                    |s| signed(s.trash.max_size_mb),
                    |s, v| s.trash.max_size_mb = unsigned(v),
                ),
                check(
                    "TEST: Import local files directly from source, do not copy to temp dir beforehand.",
                    |s| !s.folders.copy_import_files_to_temp_dir,
                    |s, v| s.folders.copy_import_files_to_temp_dir = !v,
                ),
                check(
                    "ADVANCED: Do not do chmod when copying files",
                    |s| s.file_handling.do_not_chmod,
                    |s, v| s.file_handling.do_not_chmod = v,
                ),
                boxed(
                    "delete lock",
                    vec![
                        check(
                            "Do not permit archived files to be deleted from the trash: ",
                            |s| s.delete_lock.archived,
                            |s, v| s.delete_lock.archived = v,
                        ),
                        check(
                            "After archive/delete filter, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_after_archive_delete,
                            |s, v| s.delete_lock.reinbox_after_archive_delete = v,
                        ),
                        check(
                            "After duplicate filter, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_after_duplicate_filter,
                            |s, v| s.delete_lock.reinbox_after_duplicate_filter = v,
                        ),
                        check(
                            "In duplicates auto-resolution, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_in_auto_resolution,
                            |s, v| s.delete_lock.reinbox_in_auto_resolution = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "gui pages",
            vec![
                boxed(
                    "opening and closing",
                    vec![check(
                        "Confirm when closing a non-empty importer page: ",
                        |s| s.downloader_pages.confirm_non_empty_close,
                        |s, v| s.downloader_pages.confirm_non_empty_close = v,
                    )],
                ),
                boxed(
                    "page tab names",
                    vec![
                        int(
                            "Max characters to display in a page name: ",
                            (1, 256),
                            |s| s.page_names.max_chars as i64,
                            |s, v| s.page_names.max_chars = v as usize,
                        ),
                        choice(
                            "Show page file count after its name: ",
                            FILE_COUNTS,
                            |s| file_count_index(s.page_names.file_counts),
                            |s, i| s.page_names.file_counts = file_count_display(i),
                        ),
                        check(
                            "Show import page x/y progress after its name: ",
                            |s| s.page_names.import_progress,
                            |s, v| s.page_names.import_progress = v,
                        ),
                        check(
                            "Suffix 'page of pages' tab names with a decorator string: ",
                            |s| s.page_names.decorate_notebooks,
                            |s, v| s.page_names.decorate_notebooks = v,
                        ),
                        text(
                            "  Decorator string: ",
                            |s| s.page_names.notebook_decorator.clone(),
                            |s, t| {
                                t.clone_into(&mut s.page_names.notebook_decorator);
                                Ok(())
                            },
                        ),
                    ],
                ),
            ],
        ),
        page(
            "media viewer",
            vec![boxed(
                "slideshows",
                vec![
                    text(
                        "Slideshow durations:",
                        |s| durations_text(&s.slideshow.durations),
                        |s, t| {
                            // (none above zero: left as they were)
                            let durations = parse_durations(t)?;
                            if !durations.is_empty() {
                                s.slideshow.durations = durations;
                            }
                            Ok(())
                        },
                    ),
                    check(
                        "Always play media once through before moving on:",
                        |s| s.slideshow.once_through,
                        |s, v| s.slideshow.once_through = v,
                    ),
                    noneable(
                        "Slideshow short-media skip seconds threshold:",
                        none("do not use", 10, (1, 86400), Some("s")),
                        |s| s.slideshow.short_loop_seconds,
                        |s, v| s.slideshow.short_loop_seconds = v,
                    ),
                    noneable(
                        "Slideshow short-media skip percentage threshold:",
                        none("do not use", 20, (1, 99), Some("%")),
                        |s| s.slideshow.short_loop_percentage,
                        |s, v| s.slideshow.short_loop_percentage = v,
                    ),
                    noneable(
                        "Slideshow shorter-media cutoff percentage threshold:",
                        none("do not use", 75, (1, 99), Some("%")),
                        |s| s.slideshow.short_cutoff_percentage,
                        |s, v| s.slideshow.short_cutoff_percentage = v,
                    ),
                    noneable(
                        "Slideshow long-media allowed delay percentage threshold:",
                        none("do not use", 50, (1, 500), Some("%")),
                        |s| s.slideshow.long_overspill_percentage,
                        |s, v| s.slideshow.long_overspill_percentage = v,
                    ),
                ],
            )],
        ),
        page(
            "media viewer hovers",
            vec![boxed(
                "top hover file summary",
                vec![
                    check(
                        "Show archived status: ",
                        |s| s.info_line.archived_interesting,
                        |s, v| s.info_line.archived_interesting = v,
                    ),
                    check(
                        "Show archived time: ",
                        |s| s.info_line.archived_time_interesting,
                        |s, v| s.info_line.archived_time_interesting = v,
                    ),
                    check(
                        "Show file services: ",
                        |s| s.info_line.file_services_interesting,
                        |s, v| s.info_line.file_services_interesting = v,
                    ),
                    check(
                        "Show file service add times: ",
                        |s| s.info_line.file_services_import_times_interesting,
                        |s, v| s.info_line.file_services_import_times_interesting = v,
                    ),
                    check(
                        "Show file trash times: ",
                        |s| s.info_line.trash_time_interesting,
                        |s, v| s.info_line.trash_time_interesting = v,
                    ),
                    check(
                        "Show file trash reasons: ",
                        |s| s.info_line.trash_reason_interesting,
                        |s, v| s.info_line.trash_reason_interesting = v,
                    ),
                    check(
                        "Hide uninteresting modified times: ",
                        |s| s.info_line.hide_uninteresting_modified_time,
                        |s, v| s.info_line.hide_uninteresting_modified_time = v,
                    ),
                    check(
                        "Swap in common resolution labels:",
                        |s| s.info_line.nice_resolutions,
                        |s, v| s.info_line.nice_resolutions = v,
                    ),
                ],
            )],
        ),
        page(
            "ratings",
            vec![boxed(
                "media viewer",
                vec![
                    float(
                        "Media viewer like/dislike and numerical rating icon size:",
                        (1.0, 255.0),
                        |s| s.media_viewer.rating_icon_size,
                        |s, v| s.media_viewer.rating_icon_size = v,
                    ),
                    float(
                        "Media viewer inc/dec rating icon height:",
                        (2.0, 255.0),
                        |s| s.media_viewer.rating_incdec_height,
                        |s, v| s.media_viewer.rating_incdec_height = v,
                    ),
                ],
            )],
        ),
        page(
            "tag presentation",
            vec![
                boxed(
                    "namespace rendering",
                    vec![
                        check(
                            "Show namespaces: ",
                            |s| s.tag_presentation.show_namespaces,
                            |s, v| s.tag_presentation.show_namespaces = v,
                        ),
                        check(
                            "Show namespace if it is a number: ",
                            |s| s.tag_presentation.show_number_namespaces,
                            |s, v| s.tag_presentation.show_number_namespaces = v,
                        ),
                        check(
                            "Show namespace if subtag is a number: ",
                            |s| s.tag_presentation.show_subtag_number_namespaces,
                            |s, v| s.tag_presentation.show_subtag_number_namespaces = v,
                        ),
                        text(
                            "If shown, namespace connecting string: ",
                            |s| s.tag_presentation.namespace_connector.clone(),
                            |s, t| {
                                t.clone_into(&mut s.tag_presentation.namespace_connector);
                                Ok(())
                            },
                        ),
                    ],
                ),
                boxed(
                    "other rendering",
                    vec![
                        check(
                            "EXPERIMENTAL: Replace all underscores with spaces: ",
                            |s| s.tag_presentation.replace_underscores,
                            |s, v| s.tag_presentation.replace_underscores = v,
                        ),
                        check(
                            "EXPERIMENTAL: Replace all emojis with □: ",
                            |s| s.tag_presentation.replace_emojis,
                            |s, v| s.tag_presentation.replace_emojis = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "thumbnails",
            vec![
                boxed(
                    "appearance",
                    vec![int(
                        "Generate video thumbnails this % in: ",
                        (0, 100),
                        |s| i64::from(s.thumbnails.video_percentage_in),
                        |s, v| s.thumbnails.video_percentage_in = v as u32,
                    )],
                ),
                boxed(
                    "interaction",
                    vec![check(
                        "When a single thumbnail is selected, show the media viewer's normal top hover file text in the status bar: ",
                        |s| s.info_line.single_file_in_status_bar,
                        |s, v| s.info_line.single_file_in_status_bar = v,
                    )],
                ),
            ],
        ),
        page(
            "advanced",
            vec![check(
                "Advanced mode: ",
                |s| s.advanced.0,
                |s, v| s.advanced.0 = v,
            )],
        ),
    ]
}

/// The options' values as the controls start with them.
pub fn values(pages: &[Page], settings: &Settings) -> Vec<Vec<Value>> {
    pages
        .iter()
        .map(|page| page.options().iter().map(|o| (o.get)(settings)).collect())
        .collect()
}

/// `settings` with the values changed from what they were set: each
/// option the user changed is set, the rest left as they are; and why any
/// changed couldn't be (they are left as they were, as the reference
/// leaves slideshow durations it can't read).
pub fn applied(
    pages: &[Page],
    settings: &Settings,
    values: &[Vec<Value>],
) -> (Settings, Vec<String>) {
    let mut out = settings.clone();
    let mut problems = Vec::new();
    for (page, values) in pages.iter().zip(values) {
        for (option, value) in page.options().into_iter().zip(values) {
            if (option.get)(settings) != *value
                && let Err(why) = (option.set)(&mut out, value)
            {
                problems.push(why);
            }
        }
    }
    (out, problems)
}

/// A row of the page shown: a box's title, or an option.
#[derive(Debug)]
pub enum Row<'a> {
    Title {
        title: &'static str,
        depth: usize,
    },
    Opt {
        option: &'a Opt,
        depth: usize,
        value: &'a Value,
        /// For a number that may be none, the number it shows.
        number: i64,
    },
}

/// The options window's state: the page shown, and the values as edited.
#[derive(Debug)]
pub struct Editor {
    pages: Vec<Page>,
    before: Settings,
    values: Vec<Vec<Value>>,
    /// Each noneable number's number while it is none (as the reference's
    /// spin box keeps it).
    numbers: Vec<Vec<i64>>,
    page: usize,
}

impl Editor {
    pub fn new(settings: Settings) -> Self {
        let pages = pages();
        let values = values(&pages, &settings);
        let numbers = pages
            .iter()
            .zip(&values)
            .map(|(page, values)| {
                page.options()
                    .iter()
                    .zip(values)
                    .map(|(option, value)| match (&option.kind, value) {
                        (Kind::Noneable { default, .. }, Value::Noneable(n)) => {
                            n.unwrap_or(*default)
                        }
                        _ => 0,
                    })
                    .collect()
            })
            .collect();
        Self {
            pages,
            before: settings,
            values,
            numbers,
            page: 0,
        }
    }

    pub fn page_names(&self) -> Vec<&'static str> {
        self.pages.iter().map(|p| p.name).collect()
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn show_page(&mut self, page: usize) {
        if page < self.pages.len() {
            self.page = page;
        }
    }

    /// The page shown's rows.
    pub fn rows(&self) -> Vec<Row<'_>> {
        fn walk<'a>(
            items: &'a [Item],
            depth: usize,
            values: &'a [Value],
            numbers: &[i64],
            next: &mut usize,
            out: &mut Vec<Row<'a>>,
        ) {
            for item in items {
                match item {
                    Item::Box(title, items) => {
                        out.push(Row::Title { title, depth });
                        walk(items, depth + 1, values, numbers, next, out);
                    }
                    Item::Opt(option) => {
                        out.push(Row::Opt {
                            option,
                            depth,
                            value: &values[*next],
                            number: numbers[*next],
                        });
                        *next += 1;
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &self.pages[self.page].items,
            0,
            &self.values[self.page],
            &self.numbers[self.page],
            &mut 0,
            &mut out,
        );
        out
    }

    /// Which option the page shown's row `row` is.
    fn option_at(&self, row: usize) -> Option<usize> {
        let rows = self.rows();
        let Row::Opt { .. } = rows.get(row)? else {
            return None;
        };
        Some(
            rows[..row]
                .iter()
                .filter(|r| matches!(r, Row::Opt { .. }))
                .count(),
        )
    }

    fn kind(&self, option: usize) -> &Kind {
        &self.pages[self.page].options()[option].kind
    }

    pub fn check(&mut self, row: usize, checked: bool) {
        if let Some(i) = self.option_at(row) {
            self.values[self.page][i] = Value::Check(checked);
        }
    }

    pub fn number(&mut self, row: usize, number: i64) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        match self.kind(i) {
            Kind::Int { .. } => self.values[self.page][i] = Value::Int(number),
            Kind::Noneable { .. } => {
                self.numbers[self.page][i] = number;
                if let Value::Noneable(Some(_)) = self.values[self.page][i] {
                    self.values[self.page][i] = Value::Noneable(Some(number));
                }
            }
            _ => {}
        }
    }

    pub fn none(&mut self, row: usize, none: bool) {
        if let Some(i) = self.option_at(row) {
            let number = self.numbers[self.page][i];
            self.values[self.page][i] = Value::Noneable((!none).then_some(number));
        }
    }

    pub fn text(&mut self, row: usize, text: &str) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        self.values[self.page][i] = match self.kind(i) {
            Kind::Float { .. } => Value::Float(text.to_owned()),
            _ => Value::Text(text.to_owned()),
        };
    }

    pub fn choose(&mut self, row: usize, index: usize) {
        if let Some(i) = self.option_at(row) {
            self.values[self.page][i] = Value::Choice(index);
        }
    }

    /// The settings as edited, those they started as, and why any edits
    /// couldn't be made.
    pub fn applied(&self) -> (Settings, &Settings, Vec<String>) {
        let (after, problems) = applied(&self.pages, &self.before, &self.values);
        (after, &self.before, problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        store.read(Settings::load).unwrap()
    }

    #[test]
    fn only_the_options_changed_are_set() {
        let pages = pages();
        let before = settings();
        let mut values = values(&pages, &before);
        assert_eq!(applied(&pages, &before, &values), (before.clone(), vec![]));
        let trash = pages
            .iter()
            .position(|p| p.name == "files and trash")
            .unwrap();
        values[trash][1] = Value::Noneable(None);
        values[trash][3] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert!(problems.is_empty());
        assert_eq!(after.trash.max_age_hours, None);
        assert!(
            !after.folders.copy_import_files_to_temp_dir,
            "(the option is its opposite)"
        );
        assert_eq!(after.trash.max_size_mb, before.trash.max_size_mb);
    }

    #[test]
    #[allow(clippy::float_cmp)] // (values set, not computed)
    fn values_that_cant_be_had_are_left_saying_why() {
        let pages = pages();
        let before = settings();
        let mut values = values(&pages, &before);
        let viewer = pages.iter().position(|p| p.name == "media viewer").unwrap();
        // (the rest is set regardless)
        values[viewer][0] = Value::Text("1.0,soon".into());
        values[viewer][1] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert_eq!(
            problems,
            ["Could not parse those slideshow durations, so they were not saved!"]
        );
        assert_eq!(after.slideshow.durations, before.slideshow.durations);
        assert!(after.slideshow.once_through);
        // those above zero are kept; with none, they are left
        values[viewer][0] = Value::Text("2.5, 7, 0".into());
        assert_eq!(
            applied(&pages, &before, &values).0.slideshow.durations,
            [2.5, 7.0]
        );
        values[viewer][0] = Value::Text("0".into());
        assert_eq!(
            applied(&pages, &before, &values).0.slideshow.durations,
            before.slideshow.durations
        );
        let ratings = pages.iter().position(|p| p.name == "ratings").unwrap();
        for bad in ["big", "300"] {
            values[ratings][0] = Value::Float(bad.into());
            let (after, problems) = applied(&pages, &before, &values);
            assert_eq!(problems.len(), 1, "{bad}");
            assert_eq!(
                after.media_viewer.rating_icon_size,
                before.media_viewer.rating_icon_size
            );
        }
        values[ratings][0] = Value::Float("16.5".into());
        assert_eq!(
            applied(&pages, &before, &values)
                .0
                .media_viewer
                .rating_icon_size,
            16.5
        );
    }

    #[test]
    fn the_editor_edits_the_page_shown() {
        let before = settings();
        let mut editor = Editor::new(before.clone());
        let trash = editor
            .page_names()
            .iter()
            .position(|n| *n == "files and trash")
            .unwrap();
        editor.show_page(trash);
        let rows = editor.rows();
        // (the delete lock's box title is a row of its own)
        let titles: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Title { title, depth } => Some((*title, *depth)),
                Row::Opt { .. } => None,
            })
            .collect();
        assert_eq!(titles, [("delete lock", 0)]);
        assert!(matches!(rows[6], Row::Opt { depth: 1, .. }));
        // a noneable number keeps its number while none
        editor.none(1, true);
        editor.number(1, 99);
        editor.check(6, true);
        let (after, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(after.trash.max_age_hours, None);
        assert!(after.delete_lock.archived);
        editor.none(1, false);
        assert_eq!(editor.applied().0.trash.max_age_hours, Some(99));
        // (a title is no option)
        editor.check(5, true);
        assert_eq!(editor.applied().0.file_handling, before.file_handling);
    }
}
