//! The options window (file > options): the reference's options dialog
//! (`ManageOptionsPanel`), its pages listed as it lists them (by name,
//! "advanced" last), each with the options hydrus-rs honours, in their
//! boxes and labelled as the reference's are (checked against the
//! reference's dialog, recorded by `oracle/record_options_dialog.py`).
//! Changes wait until "apply", which writes them to the store together.

use std::rc::Rc;

use hydrus_core::media_viewer::{
    InfoLineSettings, MediaViewerSettings, SlideshowSettings, ZoomCentre, ZoomType,
};
use hydrus_core::pages::{
    DownloaderPageSettings, FileCountDisplay, PageCollect, PageNameSettings, PageSort, SortSettings,
};
use hydrus_core::subscriptions::{CheckerDefaults, CheckerOptions, GalleryDefaults};
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
use hydrus_core::thumbnail::{ThumbnailRatingSettings, ThumbnailScale, ThumbnailSettings};
use hydrus_core::url::UrlClassSettings;
use hydrus_core::windows::WindowSettings;
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::delete_lock::DeleteLock;
use hydrus_store::duplicates::DuplicateFilterSettings;
use hydrus_store::duplicates::auto::AutoResolutionSettings;
use hydrus_store::file_maintenance::FileMaintenanceSettings;
use hydrus_store::network::NetworkSettings;
use hydrus_store::settings::{
    AdvancedMode, ExportSettings, FileHandlingSettings, FileViewingStatistics, FolderSettings,
    PageSettings, ThumbnailLayout,
};
use hydrus_store::similar::SimilarFilesSettings;
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
    auto_resolution: AutoResolutionSettings,
    bandwidth: BandwidthSettings,
    checker_defaults: CheckerDefaults,
    delete_lock: DeleteLock,
    downloader_pages: DownloaderPageSettings,
    duplicate_filter: DuplicateFilterSettings,
    export: ExportSettings,
    file_handling: FileHandlingSettings,
    file_maintenance: FileMaintenanceSettings,
    file_viewing: FileViewingStatistics,
    folders: FolderSettings,
    gallery: GalleryDefaults,
    info_line: InfoLineSettings,
    media_viewer: MediaViewerSettings,
    network: NetworkSettings,
    page_names: PageNameSettings,
    page_settings: PageSettings,
    similar_files: SimilarFilesSettings,
    slideshow: SlideshowSettings,
    sorts: SortSettings,
    tag_presentation: TagPresentation,
    thumbnails: ThumbnailSettings,
    thumbnail_layout: ThumbnailLayout,
    thumbnail_ratings: ThumbnailRatingSettings,
    trash: TrashSettings,
    url_classes: UrlClassSettings,
    windows: WindowSettings,
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
    /// Text, or none (the reference's `NoneableTextCtrl`); the text is
    /// kept while none, as its text box keeps it.
    NoneableText {
        none: bool,
        text: String,
    },
    /// A time, in seconds (the reference's `TimeDeltaWidget`).
    Duration(f64),
    /// A number per a time in seconds (the reference's `VelocityCtrl`).
    Velocity(i64, f64),
    /// A file sort: its type and order (the reference's
    /// `MediaSortControl`).
    Sort(PageSort),
    /// How files collect (the reference's `MediaCollectControl`).
    Collect(PageCollect),
    /// A tag list's sort (the reference's `TagSortControl`).
    TagSort(TagSort),
    /// Checker options (the reference's `CheckerOptionsButton`).
    Checker(CheckerOptions),
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
    NoneableText {
        none_phrase: &'static str,
    },
    /// A time shown as fields of these units, at least `min` seconds.
    Duration {
        units: &'static [Unit],
        min: f64,
    },
    /// A number in `number`'s range, `per` (the text between), then a time
    /// as a duration's.
    Velocity {
        number: (i64, i64),
        per: &'static str,
        units: &'static [Unit],
        min: f64,
    },
    /// A file sort, of the types a page's sort control offers.
    Sort,
    /// A collect, of the choices a page's collect control offers.
    Collect,
    /// A tag list's sort: its type, its order and its grouping.
    TagSort,
    /// Checker options: a "checker options" button opening their editor
    /// (`checker_options`).
    Checker,
}

/// A tag sort's types, as the reference's control names them
/// (`sort_type_str_lookup`), in its order.
pub const TAG_SORT_TYPES: [(&str, TagSortType); 3] = [
    ("sort by tag", TagSortType::Tag),
    ("sort by subtag", TagSortType::Subtag),
    ("sort by count", TagSortType::Count),
];

/// A tag sort's orders for text (ascending first) and for counts (most
/// first), as the reference's control names them.
pub const TAG_SORT_TEXT_ORDERS: [&str; 2] = ["a-z", "z-a"];
pub const TAG_SORT_COUNT_ORDERS: [&str; 2] = ["most first", "fewest first"];

/// A tag sort's groupings (`group_by_str_lookup`), in its order.
pub const TAG_SORT_GROUPS: [(&str, TagGroupBy); 3] = [
    ("no grouping", TagGroupBy::Nothing),
    ("namespace (a-z)", TagGroupBy::NamespaceAz),
    ("namespace (user)", TagGroupBy::NamespaceUser),
];

/// The order choices for a tag sort's type, and which is chosen.
pub fn tag_sort_orders(sort: &TagSort) -> ([&'static str; 2], usize) {
    if sort.sort_type == TagSortType::Count {
        (TAG_SORT_COUNT_ORDERS, usize::from(sort.ascending))
    } else {
        (TAG_SORT_TEXT_ORDERS, usize::from(!sort.ascending))
    }
}

/// A tag sort with its type, order or grouping chosen (each by its place
/// among the control's choices): a type takes its order button's first
/// choice ("a-z", "most first"), as the reference's separate buttons
/// start.
pub fn tag_sort_chosen(sort: &TagSort, part: usize, index: usize) -> TagSort {
    let mut out = *sort;
    match part {
        0 => {
            if let Some((_, sort_type)) = TAG_SORT_TYPES.get(index) {
                out.sort_type = *sort_type;
                out.ascending = *sort_type != TagSortType::Count;
            }
        }
        1 => {
            out.ascending = if sort.sort_type == TagSortType::Count {
                index == 1
            } else {
                index == 0
            };
        }
        _ => {
            if let Some((_, group_by)) = TAG_SORT_GROUPS.get(index) {
                out.group_by = *group_by;
            }
        }
    }
    out
}

/// A field of a time's control, as the reference's `TimeDeltaWidget`
/// shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Days,
    Hours,
    Minutes,
    Seconds,
    Milliseconds,
}

impl Unit {
    /// The text after its field.
    pub fn label(self) -> &'static str {
        match self {
            Self::Days => "days",
            Self::Hours => "hours",
            Self::Minutes => "minutes",
            Self::Seconds => "seconds",
            Self::Milliseconds => "ms",
        }
    }

    /// Its name (`_show_<name>` in the reference).
    pub fn name(self) -> &'static str {
        match self {
            Self::Milliseconds => "milliseconds",
            unit => unit.label(),
        }
    }

    fn seconds(self) -> f64 {
        match self {
            Self::Days => 86400.0,
            Self::Hours => 3600.0,
            Self::Minutes => 60.0,
            Self::Seconds => 1.0,
            Self::Milliseconds => 0.001,
        }
    }

    /// Its field's largest value.
    pub fn max(self) -> i64 {
        match self {
            Self::Days => 3523,
            Self::Hours => 23,
            Self::Minutes | Self::Seconds => 59,
            Self::Milliseconds => 999,
        }
    }
}

/// A time's fields, as the reference's control sets them (`SetValue`):
/// each unit takes what it can of what the larger ones leave (held to its
/// field's largest value).
#[allow(clippy::cast_possible_truncation)] // (whole numbers, in range)
pub fn duration_fields(seconds: f64, units: &[Unit]) -> Vec<i64> {
    let mut left = seconds.max(0.0);
    units
        .iter()
        .map(|&unit| {
            let n = if unit == Unit::Milliseconds {
                (left * 1000.0).round()
            } else {
                // (a hair over, so 0.3 / 0.1 is 3)
                ((left + 1e-9) / unit.seconds()).floor()
            };
            left = (left - n * unit.seconds()).max(0.0);
            (n as i64).min(unit.max())
        })
        .collect()
}

/// The time these fields say.
pub fn duration_seconds(fields: &[i64], units: &[Unit]) -> f64 {
    units
        .iter()
        .zip(fields)
        .map(|(unit, &n)| n as f64 * unit.seconds())
        .sum()
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
                let name = label.trim_end();
                let f: f64 = text
                    .trim()
                    .parse()
                    .map_err(|_| format!("{name} \"{text}\" is not a number"))?;
                if !(min..=max).contains(&f) {
                    return Err(format!("{name} must be from {min} to {max}"));
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

fn sort(
    label: &'static str,
    get: fn(&Settings) -> PageSort,
    set: fn(&mut Settings, PageSort),
) -> Item {
    opt(
        label,
        Kind::Sort,
        Rc::new(move |s| Value::Sort(get(s))),
        Rc::new(move |s, v| match v {
            Value::Sort(sort) => {
                set(s, sort.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn collect(
    label: &'static str,
    get: fn(&Settings) -> PageCollect,
    set: fn(&mut Settings, PageCollect),
) -> Item {
    opt(
        label,
        Kind::Collect,
        Rc::new(move |s| Value::Collect(get(s))),
        Rc::new(move |s, v| match v {
            Value::Collect(collect) => {
                set(s, collect.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn checker(
    label: &'static str,
    get: fn(&Settings) -> CheckerOptions,
    set: fn(&mut Settings, CheckerOptions),
) -> Item {
    opt(
        label,
        Kind::Checker,
        Rc::new(move |s| Value::Checker(get(s))),
        Rc::new(move |s, v| match v {
            Value::Checker(options) => {
                set(s, options.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn tag_sort(
    label: &'static str,
    get: fn(&Settings) -> TagSort,
    set: fn(&mut Settings, TagSort),
) -> Item {
    opt(
        label,
        Kind::TagSort,
        Rc::new(move |s| Value::TagSort(get(s))),
        Rc::new(move |s, v| match v {
            Value::TagSort(sort) => {
                set(s, *sort);
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

fn noneable_text(
    label: &'static str,
    none_phrase: &'static str,
    get: fn(&Settings) -> Option<String>,
    set: fn(&mut Settings, Option<String>),
) -> Item {
    opt(
        label,
        Kind::NoneableText { none_phrase },
        Rc::new(move |s| {
            let value = get(s);
            Value::NoneableText {
                none: value.is_none(),
                text: value.unwrap_or_default(),
            }
        }),
        Rc::new(move |s, v| match v {
            Value::NoneableText { none, text } => {
                set(s, (!none).then(|| text.clone()));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// The units a time shows, and its least.
const fn time(units: &'static [Unit], min: f64) -> (&'static [Unit], f64) {
    (units, min)
}

fn duration(
    label: &'static str,
    (units, min): (&'static [Unit], f64),
    get: fn(&Settings) -> f64,
    set: fn(&mut Settings, f64),
) -> Item {
    opt(
        label,
        Kind::Duration { units, min },
        Rc::new(move |s| Value::Duration(get(s))),
        Rc::new(move |s, v| match v {
            // (less than its least is its least, as the reference's control
            // makes it)
            Value::Duration(d) => {
                set(s, d.max(min));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn velocity(
    label: &'static str,
    (number, per): ((i64, i64), &'static str),
    (units, min): (&'static [Unit], f64),
    get: fn(&Settings) -> (i64, f64),
    set: fn(&mut Settings, i64, f64),
) -> Item {
    opt(
        label,
        Kind::Velocity {
            number,
            per,
            units,
            min,
        },
        Rc::new(move |s| {
            let (n, seconds) = get(s);
            Value::Velocity(n, seconds)
        }),
        Rc::new(move |s, v| match v {
            Value::Velocity(n, seconds) => {
                set(s, (*n).clamp(number.0, number.1), seconds.max(min));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A duplicate comparison's score weight (-100 to 100).
fn score(label: &'static str, get: fn(&Settings) -> i32, set: fn(&mut Settings, i32)) -> Item {
    opt(
        label,
        Kind::Int {
            min: -100,
            max: 100,
        },
        Rc::new(move |s| Value::Int(i64::from(get(s)))),
        Rc::new(move |s, v| match v {
            Value::Int(n) => {
                set(s, (*n).clamp(-100, 100) as i32);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// Whole seconds (as the store keeps them) from a time.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // (a time, rounded)
fn whole(seconds: f64) -> u64 {
    seconds.round().max(0.0) as u64
}

fn boxed(title: &'static str, items: Vec<Item>) -> Item {
    Item::Box(title, items)
}

/// The downloaders' waits after an error (`TimeDeltaButton`s of days to
/// seconds).
const ERROR_DELAY: &[Unit] = &[Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];

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

/// Numbers (slideshow durations, media zooms) as the reference writes them
/// ("1.0,5.0,10.0").
fn numbers_text(numbers: &[f64]) -> String {
    numbers
        .iter()
        .map(|d| float_text(*d))
        .collect::<Vec<_>>()
        .join(",")
}

/// The reference's parse of comma-separated numbers (slideshow durations,
/// media zooms: `UpdateOptions`): each a number, those above zero kept;
/// `what` says what couldn't be read.
fn parse_numbers(text: &str, what: &str) -> Result<Vec<f64>, String> {
    let numbers: Vec<f64> = text
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("Could not parse those {what}, so they were not saved!"))?;
    Ok(numbers.into_iter().filter(|d| *d > 0.0).collect())
}

/// `ClientGUICanvasMedia.ZOOM_CENTERPOINT_TYPES`, as the reference lists
/// them.
const ZOOM_CENTRES: &[&str] = &[
    "viewer center",
    "mouse (or viewer center if mouse outside)",
    "media center",
    "media top-left",
];
const ZOOM_CENTRE_ORDER: [ZoomCentre; 4] = [
    ZoomCentre::ViewerCentre,
    ZoomCentre::Mouse,
    ZoomCentre::MediaCentre,
    ZoomCentre::MediaTopLeft,
];

/// `MEDIA_VIEWER_ZOOM_TYPES`, as the reference lists them.
const ZOOM_TYPES: &[&str] = &[
    "default for filetype",
    "100% zoom",
    "canvas fit",
    "fill horizontally",
    "fill vertically",
    "canvas fill",
];
const ZOOM_TYPE_ORDER: [ZoomType; 6] = [
    ZoomType::DefaultForFiletype,
    ZoomType::Full,
    ZoomType::Canvas,
    ZoomType::FillX,
    ZoomType::FillY,
    ZoomType::FillAuto,
];

/// `HydrusImageHandling.thumbnail_scale_str_lookup`, as the reference lists
/// them.
const THUMBNAIL_SCALES: &[&str] = &["scale down only", "scale to fit", "scale to fill"];
const THUMBNAIL_SCALE_ORDER: [ThumbnailScale; 3] = [
    ThumbnailScale::DownOnly,
    ThumbnailScale::ToFit,
    ThumbnailScale::ToFill,
];

/// `has_transparency_strictness_string_lookup`, as the reference lists
/// them: the strictest (2) first.
const TRANSPARENCY: &[&str] = &[
    "it has a transparency channel that a human might recognise",
    "it has a transparency channel that is not completely transparent or opaque",
    "it has a transparency channel",
];

fn signed(n: Option<u64>) -> Option<i64> {
    n.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
}

fn unsigned(n: Option<i64>) -> Option<u64> {
    n.map(|n| u64::try_from(n).unwrap_or(0))
}

/// The pages, in the reference's order: sorted by name, then "advanced".
/// Some ranges are `settings`' (as the reference's are the options' when
/// its dialog opens).
#[allow(clippy::too_many_lines)] // (a table)
pub fn pages(settings: &Settings) -> Vec<Page> {
    // (the thumbnails' rating sizes go up to their width)
    let thumbnail_width = f64::from(settings.thumbnails.bounding_width);
    let advanced = settings.advanced.0;
    let timeout_range = if advanced { (1, 30 * 86400) } else { (3, 600) };
    let retry_range = if advanced { (1, 30 * 86400) } else { (3, 1800) };
    let error_delay_min = if advanced { 1.0 } else { 600.0 };
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
            "connection",
            vec![
                boxed(
                    "general",
                    vec![
                        int(
                            "max connection attempts allowed per request: ",
                            (1, 10),
                            |s| i64::from(s.network.max_connection_attempts),
                            |s, v| s.network.max_connection_attempts = v as u32,
                        ),
                        int(
                            "max retries allowed per request: ",
                            (1, 10),
                            |s| i64::from(s.network.max_get_attempts),
                            |s, v| s.network.max_get_attempts = v as u32,
                        ),
                        int(
                            "network timeout (seconds): ",
                            timeout_range,
                            |s| s.network.network_timeout as i64,
                            |s, v| s.network.network_timeout = v as u64,
                        ),
                        int(
                            "connection error retry wait (seconds): ",
                            retry_range,
                            |s| s.network.connection_error_wait_time as i64,
                            |s, v| s.network.connection_error_wait_time = v as u64,
                        ),
                        int(
                            "serverside bandwidth retry wait (seconds): ",
                            retry_range,
                            |s| s.network.serverside_bandwidth_wait_time as i64,
                            |s, v| s.network.serverside_bandwidth_wait_time = v as u64,
                        ),
                        velocity(
                            "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
                            ((0, 100), "errors within"),
                            time(&[Unit::Hours, Unit::Minutes, Unit::Seconds], 30.0),
                            |s| {
                                (
                                    s.network.domain_error_number as i64,
                                    s.network.domain_error_window as f64,
                                )
                            },
                            |s, n, seconds| {
                                s.network.domain_error_number = n as usize;
                                s.network.domain_error_window = whole(seconds) as i64;
                            },
                        ),
                        int(
                            "max number of simultaneous active network jobs: ",
                            (1, if advanced { 1000 } else { 30 }),
                            |s| s.network.max_jobs as i64,
                            |s, v| s.network.max_jobs = v as usize,
                        ),
                        int(
                            "max number of simultaneous active network jobs per domain: ",
                            (1, if advanced { 100 } else { 5 }),
                            |s| s.network.max_jobs_per_domain as i64,
                            |s, v| s.network.max_jobs_per_domain = v as usize,
                        ),
                        check(
                            "DEBUG: do not verify regular https traffic:",
                            |s| !s.network.verify_https,
                            |s, v| s.network.verify_https = !v,
                        ),
                    ],
                ),
                boxed(
                    "proxy settings",
                    vec![
                        noneable_text(
                            "http: ",
                            "none",
                            |s| s.network.http_proxy.clone(),
                            |s, v| s.network.http_proxy = v,
                        ),
                        noneable_text(
                            "https: ",
                            "none",
                            |s| s.network.https_proxy.clone(),
                            |s, v| s.network.https_proxy = v,
                        ),
                        noneable_text(
                            "no_proxy: ",
                            "none",
                            |s| s.network.no_proxy.clone(),
                            |s, v| s.network.no_proxy = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "downloading",
            vec![
                boxed(
                    "gallery downloader",
                    vec![
                        int(
                            "Additional fixed time (in seconds) to wait between gallery page fetches:",
                            (1, 3600),
                            |s| s.bandwidth.gallery_page_wait_pages,
                            |s, v| s.bandwidth.gallery_page_wait_pages = v,
                        ),
                        noneable(
                            "By default, stop searching once this many files are found:",
                            none("no limit", 2000, (1, 1_000_000), None),
                            |s| signed(s.gallery.file_limit),
                            |s, v| s.gallery.file_limit = unsigned(v),
                        ),
                        check(
                            "If new query entered and no current highlight, highlight the new query:",
                            |s| s.downloader_pages.highlight_new_query,
                            |s, v| s.downloader_pages.highlight_new_query = v,
                        ),
                        check(
                            "Force file downloads to occur quickly after Post URL fetches:",
                            |s| s.bandwidth.override_on_file_urls_from_posts,
                            |s, v| s.bandwidth.override_on_file_urls_from_posts = v,
                        ),
                    ],
                ),
                boxed(
                    "subscriptions",
                    vec![
                        int(
                            "Additional fixed time (in seconds) to wait between gallery page fetches:",
                            (1, 3600),
                            |s| s.bandwidth.gallery_page_wait_subscriptions,
                            |s, v| s.bandwidth.gallery_page_wait_subscriptions = v,
                        ),
                        check(
                            "Sync subscriptions in random order:",
                            |s| s.network.process_subs_in_random_order,
                            |s, v| s.network.process_subs_in_random_order = v,
                        ),
                        checker(
                            "Default subscription checker options:",
                            |s| s.checker_defaults.subscriptions.clone(),
                            |s, v| s.checker_defaults.subscriptions = v,
                        ),
                    ],
                ),
                boxed(
                    "watchers",
                    vec![
                        int(
                            "Additional fixed time (in seconds) to wait between watcher checks:",
                            (1, 3600),
                            |s| s.bandwidth.watcher_page_wait,
                            |s, v| s.bandwidth.watcher_page_wait = v,
                        ),
                        check(
                            "If new watcher entered and no current highlight, highlight the new watcher:",
                            |s| s.downloader_pages.highlight_new_watcher,
                            |s, v| s.downloader_pages.highlight_new_watcher = v,
                        ),
                        checker(
                            "Default watcher checker options:",
                            |s| s.checker_defaults.watchers.clone(),
                            |s, v| s.checker_defaults.watchers = v,
                        ),
                    ],
                ),
                boxed(
                    "misc",
                    vec![
                        text(
                            "Pause character:",
                            |s| s.downloader_pages.pause_character.clone(),
                            |s, t| {
                                t.clone_into(&mut s.downloader_pages.pause_character);
                                Ok(())
                            },
                        ),
                        text(
                            "Stop character:",
                            |s| s.downloader_pages.stop_character.clone(),
                            |s, t| {
                                t.clone_into(&mut s.downloader_pages.stop_character);
                                Ok(())
                            },
                        ),
                        check(
                            "Show a 'N' (for 'new') count on short file import summaries:",
                            |s| s.page_names.short_summary_new,
                            |s, v| s.page_names.short_summary_new = v,
                        ),
                        check(
                            "Show a 'D' (for 'deleted') count on short file import summaries:",
                            |s| s.page_names.short_summary_deleted,
                            |s, v| s.page_names.short_summary_deleted = v,
                        ),
                        duration(
                            "Delay time on a gallery/watcher network error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.downloader_network_error_delay as f64,
                            |s, v| s.network.downloader_network_error_delay = whole(v),
                        ),
                        duration(
                            "Delay time on a subscription network error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.subscription_network_error_delay as f64,
                            |s, v| s.network.subscription_network_error_delay = whole(v) as i64,
                        ),
                        duration(
                            "Delay time on a subscription other error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.subscription_other_error_delay as f64,
                            |s, v| s.network.subscription_other_error_delay = whole(v) as i64,
                        ),
                        check(
                            "DEBUG: remove leading double-slashes from URL paths:",
                            |s| s.url_classes.collapse_leading_slashes,
                            |s, v| s.url_classes.collapse_leading_slashes = v,
                        ),
                        check(
                            "DEBUG: consider %20 the same as space in downloader query text inputs:",
                            |s| s.network.gug_percent_twenty_is_space,
                            |s, v| s.network.gug_percent_twenty_is_space = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "duplicates",
            vec![
                boxed(
                    "open in a new duplicates filter page",
                    vec![check(
                        "Set to \"combined local file domains\" when hitting \"Open files in a new duplicates filter page\":",
                        |s| s.page_settings.duplicate_filter_uses_all_my_files,
                        |s, v| s.page_settings.duplicate_filter_uses_all_my_files = v,
                    )],
                ),
                boxed(
                    "duplicate filter batches",
                    vec![
                        int(
                            "Max size of duplicate filter pair batches (in mixed mode):",
                            (5, 1024),
                            |s| i64::from(s.duplicate_filter.max_batch_size),
                            |s, v| s.duplicate_filter.max_batch_size = v as u32,
                        ),
                        noneable(
                            "Auto-commit completed batches of this size or smaller:",
                            none("no, always confirm", 1, (1, 50), None),
                            |s| s.duplicate_filter.auto_commit_batch_size.map(i64::from),
                            |s, v| s.duplicate_filter.auto_commit_batch_size = v.map(|n| n as u32),
                        ),
                    ],
                ),
                boxed(
                    "duplicate filter comparison score weights",
                    vec![
                        score(
                            "Score for jpeg with non-trivially higher jpeg quality:",
                            |s| s.duplicate_filter.scores.higher_jpeg_quality,
                            |s, v| s.duplicate_filter.scores.higher_jpeg_quality = v,
                        ),
                        score(
                            "Score for jpeg with significantly higher jpeg quality:",
                            |s| s.duplicate_filter.scores.much_higher_jpeg_quality,
                            |s, v| s.duplicate_filter.scores.much_higher_jpeg_quality = v,
                        ),
                        score(
                            "Score for file with non-trivially higher filesize:",
                            |s| s.duplicate_filter.scores.higher_filesize,
                            |s, v| s.duplicate_filter.scores.higher_filesize = v,
                        ),
                        score(
                            "Score for file with significantly higher filesize:",
                            |s| s.duplicate_filter.scores.much_higher_filesize,
                            |s, v| s.duplicate_filter.scores.much_higher_filesize = v,
                        ),
                        score(
                            "Score for file with higher resolution (as num pixels):",
                            |s| s.duplicate_filter.scores.higher_resolution,
                            |s, v| s.duplicate_filter.scores.higher_resolution = v,
                        ),
                        score(
                            "Score for file with significantly higher resolution (as num pixels):",
                            |s| s.duplicate_filter.scores.much_higher_resolution,
                            |s, v| s.duplicate_filter.scores.much_higher_resolution = v,
                        ),
                        score(
                            "Score for file with more tags:",
                            |s| s.duplicate_filter.scores.more_tags,
                            |s, v| s.duplicate_filter.scores.more_tags = v,
                        ),
                        score(
                            "Score for file with non-trivially earlier import time:",
                            |s| s.duplicate_filter.scores.older,
                            |s, v| s.duplicate_filter.scores.older = v,
                        ),
                        score(
                            "Score for file with 'nicer' resolution ratio:",
                            |s| s.duplicate_filter.scores.nicer_ratio,
                            |s, v| s.duplicate_filter.scores.nicer_ratio = v,
                        ),
                        score(
                            "Score for file with audio:",
                            |s| s.duplicate_filter.scores.has_audio,
                            |s, v| s.duplicate_filter.scores.has_audio = v,
                        ),
                    ],
                ),
            ],
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
            "file sort/collect",
            vec![boxed(
                "file sort",
                vec![
                    sort(
                        "Default file sort: ",
                        |s| s.sorts.default_sort.clone(),
                        |s, v| s.sorts.default_sort = v,
                    ),
                    sort(
                        "Secondary file sort (when primary gives two equal values): ",
                        |s| s.sorts.fallback_sort.clone(),
                        |s, v| s.sorts.fallback_sort = v,
                    ),
                    check(
                        "Update default file sort every time a new sort is manually chosen: ",
                        |s| s.sorts.save_page_sort_on_change,
                        |s, v| s.sorts.save_page_sort_on_change = v,
                    ),
                    collect(
                        "Default collect: ",
                        |s| s.sorts.default_collect.clone(),
                        |s, v| s.sorts.default_collect = v,
                    ),
                ],
            )],
        ),
        page(
            "file viewing statistics",
            vec![check(
                "Enable file viewing statistics tracking?:",
                |s| s.file_viewing.active,
                |s, v| s.file_viewing.active = v,
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
            "gui",
            vec![boxed(
                "frame locations",
                vec![check(
                    "Save media viewer window size and position on close: ",
                    |s| s.windows.save_media_viewer_on_close,
                    |s, v| s.windows.save_media_viewer_on_close = v,
                )],
            )],
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
            "importing",
            vec![boxed(
                "filetypes",
                vec![check(
                    "Inspect for .cbz properties when importing/rescanning .zip files:",
                    |s| s.file_handling.comic_book_detection,
                    |s, v| s.file_handling.comic_book_detection = v,
                )],
            )],
        ),
        page(
            "maintenance and processing",
            vec![
                boxed(
                    "file maintenance",
                    vec![
                        check(
                            "Run file maintenance during normal time: ",
                            |s| s.file_maintenance.during_active,
                            |s, v| s.file_maintenance.during_active = v,
                        ),
                        velocity(
                            "Normal throttle: ",
                            ((1, 1000), "heavy work units every"),
                            time(&[Unit::Minutes, Unit::Seconds], 1.0),
                            |s| {
                                (
                                    s.file_maintenance.active_files as i64,
                                    s.file_maintenance.active_seconds as f64,
                                )
                            },
                            |s, n, seconds| {
                                s.file_maintenance.active_files = n as u64;
                                s.file_maintenance.active_seconds = whole(seconds);
                            },
                        ),
                    ],
                ),
                boxed(
                    "potential duplicates search",
                    vec![
                        check(
                            "Search for potential duplicates in \"idle\" time: ",
                            |s| s.similar_files.during_idle,
                            |s, v| s.similar_files.during_idle = v,
                        ),
                        check(
                            "Search for potential duplicates in \"normal\" time: ",
                            |s| s.similar_files.during_active,
                            |s, v| s.similar_files.during_active = v,
                        ),
                    ],
                ),
                boxed(
                    "duplicates auto-resolution",
                    vec![
                        check(
                            "Work duplicates auto-resolution in \"normal\" time: ",
                            |s| s.auto_resolution.during_active,
                            |s, v| s.auto_resolution.during_active = v,
                        ),
                        duration(
                            "\"Normal\" ideal work packet time: ",
                            time(&[Unit::Seconds, Unit::Milliseconds], 0.1),
                            |s| f64::from(s.auto_resolution.work_time_ms_active) / 1000.0,
                            |s, v| {
                                s.auto_resolution.work_time_ms_active = whole(v * 1000.0) as u32;
                            },
                        ),
                        int(
                            "\"Normal\" rest time percentage: ",
                            (0, 100_000),
                            |s| i64::from(s.auto_resolution.rest_percentage_active),
                            |s, v| s.auto_resolution.rest_percentage_active = v as u32,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "media playback",
            vec![
                boxed(
                    "zoom and position",
                    vec![
                        choice(
                            "Centerpoint for media zooming:",
                            ZOOM_CENTRES,
                            |s| {
                                ZOOM_CENTRE_ORDER
                                    .iter()
                                    .position(|c| *c == s.media_viewer.zoom_centre)
                                    .unwrap_or(0)
                            },
                            |s, i| s.media_viewer.zoom_centre = ZOOM_CENTRE_ORDER[i],
                        ),
                        text(
                            "Media zooms:",
                            |s| numbers_text(&s.media_viewer.media_zooms),
                            |s, t| {
                                // (none above zero: left as they were)
                                let zooms = parse_numbers(t, "zooms")?;
                                if !zooms.is_empty() {
                                    s.media_viewer.media_zooms = zooms;
                                }
                                Ok(())
                            },
                        ),
                        choice(
                            "Media Viewer default zoom:",
                            ZOOM_TYPES,
                            |s| {
                                ZOOM_TYPE_ORDER
                                    .iter()
                                    .position(|t| *t == s.media_viewer.default_zoom_type)
                                    .unwrap_or(0)
                            },
                            |s, i| s.media_viewer.default_zoom_type = ZOOM_TYPE_ORDER[i],
                        ),
                    ],
                ),
                boxed(
                    "transparency",
                    vec![choice(
                        "Consider a file as \"having transparency\" when:",
                        TRANSPARENCY,
                        |s| {
                            2_usize.saturating_sub(usize::from(
                                s.file_handling.transparency_strictness,
                            ))
                        },
                        |s, i| s.file_handling.transparency_strictness = 2 - i.min(2) as u8,
                    )],
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
                        |s| numbers_text(&s.slideshow.durations),
                        |s, t| {
                            // (none above zero: left as they were)
                            let durations = parse_numbers(t, "slideshow durations")?;
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
            vec![
                boxed(
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
                ),
                boxed(
                    "thumbnails",
                    vec![
                        float(
                            "Thumbnail like/dislike and numerical rating icon size: ",
                            (1.0, thumbnail_width),
                            |s| s.thumbnail_ratings.icon_size,
                            |s, v| s.thumbnail_ratings.icon_size = v,
                        ),
                        float(
                            "Thumbnail inc/dec rating height: ",
                            (2.0, thumbnail_width),
                            |s| s.thumbnail_ratings.incdec_height,
                            |s, v| s.thumbnail_ratings.incdec_height = v,
                        ),
                        check(
                            "Give thumbnail ratings a flat background: ",
                            |s| s.thumbnail_ratings.background,
                            |s, v| s.thumbnail_ratings.background = v,
                        ),
                        check(
                            "Always draw thumbnail numerical ratings collapsed: ",
                            |s| s.thumbnail_ratings.numerical_collapsed,
                            |s, v| s.thumbnail_ratings.numerical_collapsed = v,
                        ),
                    ],
                ),
            ],
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
            "tag sort",
            vec![boxed(
                "tag sort",
                vec![
                    tag_sort(
                        "Default tag sort in search pages: ",
                        |s| s.tag_presentation.search_page_sort,
                        |s, v| s.tag_presentation.search_page_sort = v,
                    ),
                    tag_sort(
                        "Default tag sort in the media viewer: ",
                        |s| s.tag_presentation.media_viewer_sort,
                        |s, v| s.tag_presentation.media_viewer_sort = v,
                    ),
                ],
            )],
        ),
        page(
            "thumbnails",
            vec![
                boxed(
                    "appearance",
                    vec![
                        int(
                            "Thumbnail width: ",
                            (20, 2048),
                            |s| i64::from(s.thumbnails.bounding_width),
                            |s, v| s.thumbnails.bounding_width = v as u32,
                        ),
                        int(
                            "Thumbnail height: ",
                            (20, 2048),
                            |s| i64::from(s.thumbnails.bounding_height),
                            |s, v| s.thumbnails.bounding_height = v as u32,
                        ),
                        int(
                            "Thumbnail border: ",
                            (0, 20),
                            |s| i64::from(s.thumbnail_layout.border),
                            |s, v| s.thumbnail_layout.border = v as u32,
                        ),
                        int(
                            "Thumbnail margin: ",
                            (0, 20),
                            |s| i64::from(s.thumbnail_layout.margin),
                            |s, v| s.thumbnail_layout.margin = v as u32,
                        ),
                        choice(
                            "Thumbnail scaling: ",
                            THUMBNAIL_SCALES,
                            |s| {
                                THUMBNAIL_SCALE_ORDER
                                    .iter()
                                    .position(|t| *t == s.thumbnails.scale)
                                    .unwrap_or(0)
                            },
                            |s, i| s.thumbnails.scale = THUMBNAIL_SCALE_ORDER[i],
                        ),
                        int(
                            "Thumbnail UI-scale supersampling %: ",
                            (100, 800),
                            |s| i64::from(s.thumbnails.dpr_percent),
                            |s, v| s.thumbnails.dpr_percent = v as u32,
                        ),
                        int(
                            "Generate video thumbnails this % in: ",
                            (0, 100),
                            |s| i64::from(s.thumbnails.video_percentage_in),
                            |s, v| s.thumbnails.video_percentage_in = v as u32,
                        ),
                    ],
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

/// The search box's placeholder, as the reference's.
pub const SEARCH_PLACEHOLDER: &str = "Search options... (Experimental!)";

/// How many suggestions show at once (more scroll), as the reference's.
pub const SEARCH_SHOWN: usize = 10;

/// Something the options search offers, as the reference's does: a box's
/// title or an option's label, then its page ("text (page)"), and the row
/// it is on that page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub text: String,
    pub page: usize,
    pub row: usize,
}

/// What the options search offers, page by page and row by row.
pub fn suggestions(pages: &[Page]) -> Vec<Suggestion> {
    fn walk(items: &[Item], page: usize, name: &str, row: &mut usize, out: &mut Vec<Suggestion>) {
        for item in items {
            let text = match item {
                Item::Box(title, _) => title,
                Item::Opt(option) => option.label,
            };
            out.push(Suggestion {
                text: format!("{text} ({name})"),
                page,
                row: *row,
            });
            *row += 1;
            if let Item::Box(_, items) = item {
                walk(items, page, name, row, out);
            }
        }
    }
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        walk(&page.items, i, page.name, &mut 0, &mut out);
    }
    out
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
    suggestions: Vec<Suggestion>,
    /// The rows gone to from the search (page, row): highlighted while the
    /// window is open, as the reference leaves them.
    found: std::collections::BTreeSet<(usize, usize)>,
}

impl Editor {
    pub fn new(settings: Settings) -> Self {
        let pages = pages(&settings);
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
        let suggestions = suggestions(&pages);
        Self {
            pages,
            before: settings,
            values,
            numbers,
            page: 0,
            suggestions,
            found: std::collections::BTreeSet::new(),
        }
    }

    /// The suggestions whose text has `query` in it, ignoring case (as the
    /// reference's completer matches); none for no query.
    pub fn search(&self, query: &str) -> Vec<&Suggestion> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.suggestions
            .iter()
            .filter(|s| s.text.to_lowercase().contains(&query))
            .collect()
    }

    /// Show the page of a suggestion chosen, its row highlighted.
    pub fn go_to(&mut self, suggestion: &Suggestion) {
        self.show_page(suggestion.page);
        self.found.insert((suggestion.page, suggestion.row));
    }

    /// Whether the page shown's row was gone to from the search.
    pub fn found(&self, row: usize) -> bool {
        self.found.contains(&(self.page, row))
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
            Kind::Velocity { .. } => {
                if let Value::Velocity(_, seconds) = self.values[self.page][i] {
                    self.values[self.page][i] = Value::Velocity(number, seconds);
                }
            }
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
        let Some(i) = self.option_at(row) else {
            return;
        };
        let value = &mut self.values[self.page][i];
        if let Value::NoneableText { none: was, .. } = value {
            *was = none;
        } else {
            let number = self.numbers[self.page][i];
            *value = Value::Noneable((!none).then_some(number));
        }
    }

    pub fn text(&mut self, row: usize, text: &str) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        let value = &mut self.values[self.page][i];
        *value = match (self.pages[self.page].options()[i].kind.clone(), &*value) {
            (Kind::Float { .. }, _) => Value::Float(text.to_owned()),
            (Kind::NoneableText { .. }, Value::NoneableText { none, .. }) => Value::NoneableText {
                none: *none,
                text: text.to_owned(),
            },
            _ => Value::Text(text.to_owned()),
        };
    }

    /// A time's field (of a time, or a rate's time) set to `n`.
    pub fn field(&mut self, row: usize, field: usize, n: i64) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        let units = match self.kind(i) {
            Kind::Duration { units, .. } | Kind::Velocity { units, .. } => *units,
            _ => return,
        };
        let set = |seconds: f64| {
            let mut fields = duration_fields(seconds, units);
            if let (Some(f), Some(unit)) = (fields.get_mut(field), units.get(field)) {
                *f = n.clamp(0, unit.max());
            }
            duration_seconds(&fields, units)
        };
        let value = &mut self.values[self.page][i];
        *value = match *value {
            Value::Duration(seconds) => Value::Duration(set(seconds)),
            Value::Velocity(number, seconds) => Value::Velocity(number, set(seconds)),
            _ => return,
        };
    }

    pub fn choose(&mut self, row: usize, index: usize) {
        if let Some(i) = self.option_at(row) {
            self.values[self.page][i] = Value::Choice(index);
        }
    }

    /// A sort's type or order chosen.
    pub fn sort(&mut self, row: usize, sort: PageSort) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Sort(_))
        {
            self.values[self.page][i] = Value::Sort(sort);
        }
    }

    /// A tag sort's type, order or grouping chosen (`part` 0, 1 or 2; each
    /// by its place among its choices).
    pub fn tag_sort(&mut self, row: usize, part: usize, index: usize) {
        if let Some(i) = self.option_at(row)
            && let Value::TagSort(sort) = &self.values[self.page][i]
        {
            self.values[self.page][i] = Value::TagSort(tag_sort_chosen(sort, part, index));
        }
    }

    /// A collect's choice checked or not, or its unmatched files' choice.
    pub fn collect(&mut self, row: usize, collect: PageCollect) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Collect(_))
        {
            self.values[self.page][i] = Value::Collect(collect);
        }
    }

    /// Checker options edited (their editor's "ok").
    pub fn checker(&mut self, row: usize, options: CheckerOptions) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Checker(_))
        {
            self.values[self.page][i] = Value::Checker(options);
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
        let before = settings();
        let pages = pages(&before);
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
        let before = settings();
        let pages = pages(&before);
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
        // (media zooms are read as the reference reads them)
        let playback = pages
            .iter()
            .position(|p| p.name == "media playback")
            .unwrap();
        values[playback][1] = Value::Text("0.5,big".into());
        assert_eq!(
            applied(&pages, &before, &values).1,
            ["Could not parse those zooms, so they were not saved!"]
        );
        values[playback][1] = Value::Text("0.5, 2".into());
        assert_eq!(
            applied(&pages, &before, &values).0.media_viewer.media_zooms,
            [0.5, 2.0]
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
    #[allow(clippy::float_cmp)] // (values set, not computed)
    fn thumbnail_rating_sizes_go_up_to_the_thumbnails_width() {
        let mut before = settings();
        before.thumbnails.bounding_width = 200;
        let pages = pages(&before);
        let ratings = pages.iter().position(|p| p.name == "ratings").unwrap();
        let mut values = values(&pages, &before);
        // (the media viewer's two, then the thumbnails')
        values[ratings][2] = Value::Float("200".into());
        values[ratings][3] = Value::Float("201".into());
        values[ratings][4] = Value::Check(false);
        values[ratings][5] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert_eq!(
            problems,
            ["Thumbnail inc/dec rating height: must be from 2 to 200"]
        );
        let ratings_after = after.thumbnail_ratings;
        assert_eq!(ratings_after.icon_size, 200.0);
        assert_eq!(
            ratings_after.incdec_height,
            before.thumbnail_ratings.incdec_height
        );
        assert!(!ratings_after.background);
        assert!(ratings_after.numerical_collapsed);
        values[ratings][3] = Value::Float("1.5".into());
        assert_eq!(applied(&pages, &before, &values).1.len(), 1, "below 2");
        values[ratings][3] = Value::Float("2".into());
        let (after, problems) = applied(&pages, &before, &values);
        assert!(problems.is_empty());
        assert_eq!(after.thumbnail_ratings.incdec_height, 2.0);
        // (and they show as they are)
        let shown = super::values(&pages, &after);
        assert_eq!(
            shown[ratings][2..],
            [
                Value::Float("200.0".into()),
                Value::Float("2.0".into()),
                Value::Check(false),
                Value::Check(true),
            ]
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // (whole numbers of ms)
    fn times_show_as_the_references_fields() {
        let dhms = [Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];
        assert_eq!(duration_fields(129_600.0, &dhms), [1, 12, 0, 0]);
        assert_eq!(duration_fields(5400.0, &dhms), [0, 1, 30, 0]);
        let sms = [Unit::Seconds, Unit::Milliseconds];
        assert_eq!(duration_fields(0.1, &sms), [0, 100]);
        assert_eq!(duration_fields(30.0, &sms), [30, 0]);
        assert_eq!(duration_fields(2.25, &sms), [2, 250]);
        // (a unit without the larger ones takes what it can hold)
        let hms = [Unit::Hours, Unit::Minutes, Unit::Seconds];
        assert_eq!(duration_fields(600.0, &hms), [0, 10, 0]);
        assert_eq!(duration_fields(100_000.0, &hms)[0], 23);
        assert_eq!(duration_seconds(&[1, 12, 0, 0], &dhms), 129_600.0);
        assert_eq!(duration_seconds(&[2, 250], &sms), 2.25);
    }

    #[test]
    fn times_rates_and_text_that_may_be_none_are_edited() {
        let before = settings();
        let mut editor = Editor::new(before.clone());
        let page = |editor: &mut Editor, name: &str| {
            let at = editor.page_names().iter().position(|n| *n == name).unwrap();
            editor.show_page(at);
        };
        let row = |editor: &Editor, label: &str| {
            editor
                .rows()
                .iter()
                .position(|r| matches!(r, Row::Opt { option, .. } if option.label == label))
                .unwrap()
        };
        page(&mut editor, "downloading");
        // a day more on a wait of 1 hour 30 minutes
        let wait = row(&editor, "Delay time on a gallery/watcher network error:");
        editor.field(wait, 0, 1);
        // less than the least is the least
        let other = row(&editor, "Delay time on a subscription other error:");
        for field in 0..4 {
            editor.field(other, field, 0);
        }
        page(&mut editor, "connection");
        let errors = row(
            &editor,
            "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
        );
        editor.number(errors, 7);
        editor.field(errors, 1, 2);
        let http = row(&editor, "http: ");
        editor.text(http, "http://127.0.0.1:8080");
        editor.none(http, false);
        // the text is kept while none
        let no_proxy = row(&editor, "no_proxy: ");
        editor.none(no_proxy, true);
        let (after, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        let n = &after.network;
        assert_eq!(
            n.downloader_network_error_delay,
            before.network.downloader_network_error_delay + 86400
        );
        assert_eq!(n.subscription_other_error_delay, 600);
        assert_eq!(n.domain_error_number, 7);
        assert_eq!(
            n.domain_error_window,
            2 * 60 + before.network.domain_error_window % 60
        );
        assert_eq!(n.http_proxy.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(n.no_proxy, None);
        editor.none(no_proxy, false);
        assert_eq!(editor.applied().0.network.no_proxy, before.network.no_proxy);
    }

    #[test]
    fn the_search_finds_options_by_their_labels_and_boxes() {
        let mut editor = Editor::new(settings());
        let texts = |editor: &Editor, query| -> Vec<String> {
            editor
                .search(query)
                .iter()
                .map(|s| s.text.clone())
                .collect()
        };
        assert!(texts(&editor, "").is_empty());
        // (any case, anywhere in the text, the page's name too)
        assert_eq!(
            texts(&editor, "PROXY"),
            ["proxy settings (connection)", "no_proxy:  (connection)"]
        );
        assert!(texts(&editor, "(media viewer hovers)").len() > 5);
        // gone to: its page shown, its row highlighted
        let found = editor.search("Maximum size of trash")[0].clone();
        editor.go_to(&found);
        assert_eq!(editor.page_names()[editor.page()], "files and trash");
        let Row::Opt { option, .. } = &editor.rows()[found.row] else {
            panic!("an option's row");
        };
        assert_eq!(option.label, "Maximum size of trash (MB): ");
        assert!(editor.found(found.row) && !editor.found(found.row + 1));
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
