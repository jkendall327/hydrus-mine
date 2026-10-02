//! The editors the reference opens for a system predicate it offers without
//! a value ("system:dimensions", "system:time"...): its "input predicate"
//! dialog (`FleshOutPredicatePanel`). An editor has pages (a notebook when
//! there are several, as "system:time" has: import, modified, last viewed
//! and archived), each with ready-made buttons, each adding fixed
//! predicates, and panels: a row of fields each, making a predicate from
//! what is set in them, with an "ok" button to add it. Each panel starts
//! at the reference's default (`GetDefaultPredicate`).
//!
//! Checked against the reference's own editors in
//! `oracle/fixtures/system_predicate_editors.json`.

use std::collections::BTreeSet;

use hydrus_core::search::number::{Comparison, NumberOp, NumberTest, RatioOp, TagNumberOp};
use hydrus_core::search::predicate::{
    FileProperty, NamespaceFilter, NumericProperty, PixelUnit, Predicate, Relationship, ServiceRef,
    SizeUnit, SystemPredicate, TagDisplayType, UrlRule, ViewCanvas, ViewCanvases, ViewingStat,
};
use hydrus_core::search::time::{CalendarDelta, CivilDateTime, RelativeOp, TimeKind, TimeTest};
use hydrus_core::{ContentStatus, ServiceKey, ServiceType, Tag};

/// A system predicate offered with no value, which opens an editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Blank {
    Dimensions,
    Duration,
    FileProperties,
    FileRelationships,
    FileService,
    FileViewingStats,
    Filesize,
    Filetype,
    Hash,
    Limit,
    Notes,
    NumTags,
    NumWords,
    Rating,
    SimilarFiles,
    TagAdvanced,
    TagAsNumber,
    Time,
    Urls,
}

impl Blank {
    /// Every one, in the order the reference lists them (by their text).
    pub const ALL: [Blank; 19] = [
        Blank::Dimensions,
        Blank::Duration,
        Blank::FileProperties,
        Blank::FileRelationships,
        Blank::FileService,
        Blank::FileViewingStats,
        Blank::Filesize,
        Blank::Filetype,
        Blank::Hash,
        Blank::Limit,
        Blank::Notes,
        Blank::NumTags,
        Blank::NumWords,
        Blank::Rating,
        Blank::SimilarFiles,
        Blank::TagAdvanced,
        Blank::TagAsNumber,
        Blank::Time,
        Blank::Urls,
    ];

    /// As the reference writes it.
    pub fn text(self) -> &'static str {
        match self {
            Blank::Dimensions => "system:dimensions",
            Blank::Duration => "system:duration",
            Blank::FileProperties => "system:file properties",
            Blank::FileRelationships => "system:file relationships",
            Blank::FileService => "system:file service",
            Blank::FileViewingStats => "system:file viewing statistics",
            Blank::Filesize => "system:filesize",
            Blank::Filetype => "system:filetype",
            Blank::Hash => "system:hash",
            Blank::Limit => "system:limit",
            Blank::Notes => "system:notes",
            Blank::NumTags => "system:number of tags",
            Blank::NumWords => "system:number of words",
            Blank::Rating => "system:rating",
            Blank::SimilarFiles => "system:similar files",
            Blank::TagAdvanced => "system:tag (advanced)",
            Blank::TagAsNumber => "system:tag as number",
            Blank::Time => "system:time",
            Blank::Urls => "system:urls",
        }
    }

    pub fn from_text(text: &str) -> Option<Blank> {
        Blank::ALL.into_iter().find(|b| b.text() == text)
    }

    /// Whether hydrus-rs has its editor yet.
    pub fn ported(self) -> bool {
        !matches!(
            self,
            Blank::Filetype | Blank::Hash | Blank::Rating | Blank::SimilarFiles
        )
    }

    /// Offered whatever the page searches: those that need no file's
    /// metadata (`_GetFileSystemPredicates`'s first set).
    fn offered_everywhere(self) -> bool {
        matches!(
            self,
            Blank::NumTags
                | Blank::Limit
                | Blank::Hash
                | Blank::FileService
                | Blank::FileRelationships
                | Blank::TagAdvanced
                | Blank::TagAsNumber
                | Blank::FileViewingStats
                | Blank::Rating
        )
    }
}

/// The blank predicates the reference offers in an empty search box, in its
/// order: on a search of all known files only those needing no file's
/// metadata, and "system:rating" only with a rating service.
pub fn offered(all_known_files: bool, rating_services: bool) -> Vec<Blank> {
    Blank::ALL
        .into_iter()
        .filter(|b| !all_known_files || b.offered_everywhere())
        .filter(|b| *b != Blank::Rating || rating_services)
        .collect()
}

/// What an editor needs to know of the client.
#[derive(Debug, Clone)]
pub struct Context {
    /// The file services a file service predicate can name, in the
    /// reference's order (`REAL_FILE_SERVICES`).
    pub file_services: Vec<(ServiceKey, String)>,
    /// Every tag service, all known tags last (`ALL_TAG_SERVICES`).
    pub tag_services: Vec<(ServiceKey, String)>,
    /// The URL classes whose URLs are kept with files, by name.
    pub url_classes: Vec<String>,
    /// The local date, which the date panels start at.
    pub today: CivilDateTime,
}

impl Context {
    pub fn new(
        services: &hydrus_store::services::ServiceRegistry,
        url_classes: Vec<String>,
        today: CivilDateTime,
    ) -> Self {
        let named = |types: &[ServiceType]| {
            crate::domains::in_order(services, types)
                .into_iter()
                .map(|(key, name, _)| (key, name))
                .collect()
        };
        Self {
            file_services: named(&[
                ServiceType::LocalFileDomain,
                ServiceType::LocalFileUpdateDomain,
                ServiceType::LocalFileTrashDomain,
                ServiceType::HydrusLocalFileStorage,
                ServiceType::CombinedLocalFileDomains,
                ServiceType::CombinedDeletedFile,
                ServiceType::FileRepository,
                ServiceType::Ipfs,
            ]),
            tag_services: named(&[
                ServiceType::LocalTag,
                ServiceType::TagRepository,
                ServiceType::CombinedTag,
            ]),
            url_classes,
            today,
        }
    }
}

/// One of a panel's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Text shown.
    Label(String),
    /// One of several (the reference's radio buttons and drop-downs).
    Choice { options: Vec<String>, chosen: usize },
    /// Any of several (the reference's lists of tick boxes).
    Ticks {
        options: Vec<String>,
        ticked: Vec<bool>,
    },
    /// A whole number, with text either side.
    Number {
        value: i64,
        min: i64,
        max: i64,
        before: String,
        after: String,
    },
    /// A line of text.
    Text { text: String, placeholder: String },
}

impl Field {
    fn label(text: &str) -> Self {
        Field::Label(text.to_owned())
    }

    fn choice(options: &[&str], chosen: &str) -> Self {
        Field::Choice {
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            chosen: options.iter().position(|o| *o == chosen).unwrap_or(0),
        }
    }

    fn number(value: i64, min: i64, max: i64, after: &str) -> Self {
        Field::Number {
            value: value.clamp(min, max),
            min,
            max,
            before: String::new(),
            after: after.to_owned(),
        }
    }

    fn plus_or_minus(value: i64, max: i64, after: &str) -> Self {
        match Self::number(value, 0, max, after) {
            Field::Number {
                value,
                min,
                max,
                after,
                ..
            } => Field::Number {
                value,
                min,
                max,
                before: "\u{b1}".into(),
                after,
            },
            _ => unreachable!(),
        }
    }

    fn text(text: &str, placeholder: &str) -> Self {
        Field::Text {
            text: text.to_owned(),
            placeholder: placeholder.to_owned(),
        }
    }
}

/// When a field is shown, or can be set: while a choice is one of some
/// options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub field: usize,
    pub choice: usize,
    pub options: Vec<usize>,
    /// Hidden otherwise (else greyed out).
    pub hide: bool,
}

/// What a panel makes, by the reference's panel class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Width,
    Height,
    Ratio,
    NumPixels,
    Duration,
    Framerate,
    NumFrames,
    DuplicateRelationships,
    FileService,
    Views,
    Viewtime,
    Size,
    Limit,
    NumNotes,
    NoteName,
    NumTags,
    NumWords,
    TagAdvanced,
    TagAsNumber,
    TimeDelta(TimeKind),
    TimeDate(TimeKind),
    UrlExact,
    UrlDomain,
    UrlRegex,
    UrlClass,
    NumUrls,
}

impl Kind {
    /// The reference's class for it.
    pub fn class_name(self) -> &'static str {
        match self {
            Kind::Width => "PanelPredicateSystemWidth",
            Kind::Height => "PanelPredicateSystemHeight",
            Kind::Ratio => "PanelPredicateSystemRatio",
            Kind::NumPixels => "PanelPredicateSystemNumPixels",
            Kind::Duration => "PanelPredicateSystemDuration",
            Kind::Framerate => "PanelPredicateSystemFramerate",
            Kind::NumFrames => "PanelPredicateSystemNumFrames",
            Kind::DuplicateRelationships => "PanelPredicateSystemDuplicateRelationships",
            Kind::FileService => "PanelPredicateSystemFileService",
            Kind::Views => "PanelPredicateSystemFileViewingStatsViews",
            Kind::Viewtime => "PanelPredicateSystemFileViewingStatsViewtime",
            Kind::Size => "PanelPredicateSystemSize",
            Kind::Limit => "PanelPredicateSystemLimit",
            Kind::NumNotes => "PanelPredicateSystemNumNotes",
            Kind::NoteName => "PanelPredicateSystemHasNoteName",
            Kind::NumTags => "PanelPredicateSystemNumTags",
            Kind::NumWords => "PanelPredicateSystemNumWords",
            Kind::TagAdvanced => "PanelPredicateSystemTagAdvanced",
            Kind::TagAsNumber => "PanelPredicateSystemTagAsNumber",
            Kind::TimeDelta(TimeKind::Imported) => "PanelPredicateSystemAgeDelta",
            Kind::TimeDelta(TimeKind::Modified) => "PanelPredicateSystemModifiedDelta",
            Kind::TimeDelta(TimeKind::LastViewed) => "PanelPredicateSystemLastViewedDelta",
            Kind::TimeDelta(TimeKind::Archived) => "PanelPredicateSystemArchivedDelta",
            Kind::TimeDate(TimeKind::Imported) => "PanelPredicateSystemAgeDate",
            Kind::TimeDate(TimeKind::Modified) => "PanelPredicateSystemModifiedDate",
            Kind::TimeDate(TimeKind::LastViewed) => "PanelPredicateSystemLastViewedDate",
            Kind::TimeDate(TimeKind::Archived) => "PanelPredicateSystemArchivedDate",
            Kind::UrlExact => "PanelPredicateSystemKnownURLsExactURL",
            Kind::UrlDomain => "PanelPredicateSystemKnownURLsDomain",
            Kind::UrlRegex => "PanelPredicateSystemKnownURLsRegex",
            Kind::UrlClass => "PanelPredicateSystemKnownURLsURLClass",
            Kind::NumUrls => "PanelPredicateSystemNumURLs",
        }
    }
}

/// The operators of a number test (`NumberTestWidget`), in its order:
/// "≈" within an amount either side, "≈%" within a percentage.
const LESS: &str = "<";
const LESS_OR_EQUAL: &str = "\u{2264}";
const APPROX: &str = "\u{2248}";
const APPROX_PERCENT: &str = "\u{2248}%";
const EQUAL: &str = "=";
const NOT_EQUAL: &str = "\u{2260}";
const GREATER_OR_EQUAL: &str = "\u{2265}";
const GREATER: &str = ">";
const ALL_OPERATORS: [&str; 8] = [
    LESS,
    LESS_OR_EQUAL,
    APPROX,
    APPROX_PERCENT,
    EQUAL,
    NOT_EQUAL,
    GREATER_OR_EQUAL,
    GREATER,
];

/// A panel: its fields, in the order shown, and what it makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    pub kind: Kind,
    pub fields: Vec<Field>,
    pub conditions: Vec<Condition>,
}

/// The fields of a number test: its operator, the number (as one field, or
/// as several in time units), and the amount and percentage either side
/// that "≈" and "≈%" show.
struct NumberTestFields {
    operators: Vec<&'static str>,
    operator: usize,
    /// The first field of the value.
    value: usize,
    absolute: usize,
    percent: usize,
}

impl Panel {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            fields: Vec::new(),
            conditions: Vec::new(),
        }
    }

    fn push(&mut self, field: Field) -> usize {
        self.fields.push(field);
        self.fields.len() - 1
    }

    /// Whether field `i` is shown.
    pub fn shown(&self, i: usize) -> bool {
        self.conditions
            .iter()
            .filter(|c| c.field == i && c.hide)
            .all(|c| self.condition_holds(c))
    }

    /// Whether field `i` can be set.
    pub fn enabled(&self, i: usize) -> bool {
        self.conditions
            .iter()
            .filter(|c| c.field == i && !c.hide)
            .all(|c| self.condition_holds(c))
    }

    fn condition_holds(&self, c: &Condition) -> bool {
        c.options.contains(&self.chosen(c.choice))
    }

    /// The option chosen in field `i` (0 if it isn't a choice).
    pub fn chosen(&self, i: usize) -> usize {
        match &self.fields[i] {
            Field::Choice { chosen, .. } => *chosen,
            _ => 0,
        }
    }

    fn chosen_text(&self, i: usize) -> &str {
        match &self.fields[i] {
            Field::Choice { options, chosen } => &options[*chosen],
            _ => "",
        }
    }

    fn number(&self, i: usize) -> i64 {
        match &self.fields[i] {
            Field::Number { value, .. } => *value,
            _ => 0,
        }
    }

    fn unsigned(&self, i: usize) -> u64 {
        u64::try_from(self.number(i)).unwrap_or(0)
    }

    fn small(&self, i: usize) -> u32 {
        u32::try_from(self.number(i)).unwrap_or(0)
    }

    fn text_of(&self, i: usize) -> &str {
        match &self.fields[i] {
            Field::Text { text, .. } => text,
            _ => "",
        }
    }

    fn ticked(&self, i: usize) -> Vec<bool> {
        match &self.fields[i] {
            Field::Ticks { ticked, .. } => ticked.clone(),
            _ => Vec::new(),
        }
    }

    /// Choose option `option` of field `i`.
    pub fn choose(&mut self, i: usize, option: usize) {
        if let Some(Field::Choice { options, chosen }) = self.fields.get_mut(i)
            && option < options.len()
        {
            *chosen = option;
        }
    }

    /// Set field `i`'s number, kept in its range.
    pub fn set_number(&mut self, i: usize, to: i64) {
        if let Some(Field::Number {
            value, min, max, ..
        }) = self.fields.get_mut(i)
        {
            *value = to.clamp(*min, *max);
        }
    }

    pub fn set_text(&mut self, i: usize, to: &str) {
        if let Some(Field::Text { text, .. }) = self.fields.get_mut(i) {
            to.clone_into(text);
        }
    }

    pub fn tick(&mut self, i: usize, option: usize, on: bool) {
        if let Some(Field::Ticks { ticked, .. }) = self.fields.get_mut(i)
            && let Some(t) = ticked.get_mut(option)
        {
            *t = on;
        }
    }

    /// A number test's fields: its operators (of those allowed), the value
    /// (`value_fields` pushes it), the amount either side for "≈" and the
    /// percentage for "≈%".
    fn number_test(
        &mut self,
        allowed: &[&'static str],
        test: NumberTest,
        value_fields: impl FnOnce(&mut Panel, u64) -> usize,
        absolute_fields: impl FnOnce(&mut Panel, u64) -> usize,
        absolute_default: u64,
        percent_default: u32,
    ) -> NumberTestFields {
        let operators: Vec<&'static str> = ALL_OPERATORS
            .into_iter()
            .filter(|o| allowed.contains(o))
            .collect();
        let (chosen, absolute, percent) = match test.op {
            NumberOp::Less => (LESS, absolute_default, percent_default),
            NumberOp::LessOrEqual => (LESS_OR_EQUAL, absolute_default, percent_default),
            NumberOp::Equal => (EQUAL, absolute_default, percent_default),
            NumberOp::NotEqual => (NOT_EQUAL, absolute_default, percent_default),
            NumberOp::GreaterOrEqual => (GREATER_OR_EQUAL, absolute_default, percent_default),
            NumberOp::Greater => (GREATER, absolute_default, percent_default),
            NumberOp::ApproxAbsolute { tolerance } => (APPROX, tolerance, percent_default),
            NumberOp::ApproxPercent { percent } => (APPROX_PERCENT, absolute_default, percent),
        };
        let operator = self.push(Field::choice(&operators, chosen));
        let value = value_fields(self, test.value);
        let absolute_at = self.fields.len();
        let absolute_end = absolute_fields(self, absolute);
        let percent_at = self.push(Field::plus_or_minus(i64::from(percent), 10_000, "%"));
        let shown_for = |op: &str| -> Vec<usize> {
            operators
                .iter()
                .position(|o| *o == op)
                .into_iter()
                .collect()
        };
        for field in absolute_at..absolute_end {
            self.conditions.push(Condition {
                field,
                choice: operator,
                options: shown_for(APPROX),
                hide: true,
            });
        }
        self.conditions.push(Condition {
            field: percent_at,
            choice: operator,
            options: shown_for(APPROX_PERCENT),
            hide: true,
        });
        NumberTestFields {
            operators,
            operator,
            value,
            absolute: absolute_at,
            percent: percent_at,
        }
    }

    /// A plain number test (`NumberTestWidget`): the value in `unit`, at
    /// most `max`, and an amount either side of up to half that.
    fn plain_number_test(
        &mut self,
        allowed: &[&'static str],
        test: NumberTest,
        max: i64,
        unit: &str,
        absolute_default: u64,
        percent_default: u32,
    ) -> NumberTestFields {
        self.number_test(
            allowed,
            test,
            |panel, value| {
                panel.push(Field::number(
                    i64::try_from(value).unwrap_or(max),
                    0,
                    max,
                    unit,
                ))
            },
            |panel, absolute| {
                panel.push(Field::plus_or_minus(
                    i64::try_from(absolute).unwrap_or(0),
                    max / 2,
                    unit,
                ));
                panel.fields.len()
            },
            absolute_default,
            percent_default,
        )
    }

    /// The number test set in a panel's number test fields; `value` reads
    /// the value from its first field, `absolute` the amount either side.
    fn read_number_test(
        &self,
        fields: &NumberTestFields,
        value: impl Fn(&Panel, usize) -> u64,
        absolute: impl Fn(&Panel, usize) -> u64,
    ) -> NumberTest {
        let op = match fields.operators[self.chosen(fields.operator)] {
            LESS => NumberOp::Less,
            LESS_OR_EQUAL => NumberOp::LessOrEqual,
            NOT_EQUAL => NumberOp::NotEqual,
            GREATER_OR_EQUAL => NumberOp::GreaterOrEqual,
            GREATER => NumberOp::Greater,
            APPROX => NumberOp::ApproxAbsolute {
                tolerance: absolute(self, fields.absolute),
            },
            APPROX_PERCENT => NumberOp::ApproxPercent {
                percent: self.small(fields.percent),
            },
            _ => NumberOp::Equal,
        };
        NumberTest::new(op, value(self, fields.value))
    }

    /// The predicates the panel makes as set, or why it can't.
    pub fn predicates(&self, context: &Context) -> Result<Vec<Predicate>, String> {
        let system = |p: SystemPredicate| Ok(vec![Predicate::System(p)]);
        match self.kind {
            Kind::Width | Kind::Height | Kind::NumFrames | Kind::NumWords => {
                let property = match self.kind {
                    Kind::Width => NumericProperty::Width,
                    Kind::Height => NumericProperty::Height,
                    Kind::NumFrames => NumericProperty::NumFrames,
                    _ => NumericProperty::NumWords,
                };
                system(SystemPredicate::Number {
                    property,
                    test: self.read_plain_number_test(1),
                })
            }
            Kind::NumNotes | Kind::NumUrls => system(SystemPredicate::Number {
                property: if self.kind == Kind::NumNotes {
                    NumericProperty::NumNotes
                } else {
                    NumericProperty::NumUrls
                },
                test: self.read_plain_number_test(1),
            }),
            Kind::Framerate => system(SystemPredicate::Number {
                property: NumericProperty::Framerate,
                test: self.read_plain_number_test(2),
            }),
            Kind::Duration => {
                let fields = Self::duration_fields();
                system(SystemPredicate::Number {
                    property: NumericProperty::Duration,
                    test: self.read_number_test(
                        &fields,
                        |p, at| hms_ms(p, at, &[3_600_000, 60_000, 1000, 1]),
                        |p, at| hms_ms(p, at, &[60_000, 1000, 1]),
                    ),
                })
            }
            Kind::Ratio => {
                let op = match self.chosen_text(1) {
                    "wider than" => RatioOp::WiderThan,
                    "taller than" => RatioOp::TallerThan,
                    APPROX => RatioOp::Approx,
                    NOT_EQUAL => RatioOp::NotEqual,
                    _ => RatioOp::Equal,
                };
                system(SystemPredicate::Ratio {
                    op,
                    width: self.unsigned(2),
                    height: self.unsigned(4),
                })
            }
            Kind::NumPixels => system(SystemPredicate::NumPixels {
                op: comparison(self.chosen_text(1)),
                count: self.unsigned(2),
                unit: [
                    PixelUnit::Pixels,
                    PixelUnit::Kilopixels,
                    PixelUnit::Megapixels,
                ][self.chosen(3)],
            }),
            Kind::Size => system(SystemPredicate::FileSize {
                op: comparison(self.chosen_text(1)),
                size: self.unsigned(2),
                unit: [
                    SizeUnit::Bytes,
                    SizeUnit::Kilobytes,
                    SizeUnit::Megabytes,
                    SizeUnit::Gigabytes,
                    SizeUnit::Terabytes,
                ][self.chosen(3)],
            }),
            Kind::DuplicateRelationships => system(SystemPredicate::FileRelationshipCount {
                op: comparison(self.chosen_text(1)),
                count: self.unsigned(2),
                relationship: [
                    Relationship::Duplicates,
                    Relationship::Alternates,
                    Relationship::FalsePositives,
                    Relationship::PotentialDuplicates,
                ][self.chosen(3)],
            }),
            Kind::FileService => {
                let Some((key, _)) = context.file_services.get(self.chosen(3)) else {
                    return Err("There is no file service to choose.".into());
                };
                system(SystemPredicate::FileService {
                    service: ServiceRef::Key(key.clone()),
                    status: [
                        ContentStatus::Current,
                        ContentStatus::Deleted,
                        ContentStatus::Pending,
                        ContentStatus::Petitioned,
                    ][self.chosen(2)],
                    is_in: self.chosen(1) == 0,
                })
            }
            Kind::Views | Kind::Viewtime => {
                let canvases: BTreeSet<ViewCanvas> = [
                    ViewCanvas::MediaViewer,
                    ViewCanvas::Preview,
                    ViewCanvas::ClientApi,
                ]
                .into_iter()
                .zip(self.ticked(1))
                .filter_map(|(canvas, on)| on.then_some(canvas))
                .collect();
                // (none ticked counts the media viewer's)
                let canvases = if canvases.is_empty() {
                    [ViewCanvas::MediaViewer].into_iter().collect()
                } else {
                    canvases
                };
                let (stat, value) = if self.kind == Kind::Views {
                    (ViewingStat::Views, self.unsigned(3))
                } else {
                    // (in seconds: the milliseconds are dropped)
                    let ms = hms_ms(self, 3, &[86_400_000, 3_600_000, 60_000, 1000, 1]);
                    (ViewingStat::ViewTime, ms / 1000)
                };
                system(SystemPredicate::FileViewingStats {
                    stat,
                    canvases: ViewCanvases::Specific(canvases),
                    op: comparison(self.chosen_text(2)),
                    value,
                })
            }
            Kind::Limit => system(SystemPredicate::Limit(self.unsigned(1))),
            Kind::NoteName => {
                let name = match self.text_of(2) {
                    "" => "notes",
                    name => name,
                };
                system(SystemPredicate::NoteName {
                    name: name.to_owned(),
                    has: self.chosen(1) == 0,
                })
            }
            Kind::NumTags => {
                let namespace = self.namespace_filter(1, 2);
                let op = comparison(self.chosen_text(3));
                let count = self.unsigned(4);
                // a namespace (or unnamespaced) with none, or with any: the
                // namespace's own predicate
                if namespace != NamespaceFilter::Any {
                    let namespace = namespace.as_reference();
                    let zero =
                        matches!((op, count), (Comparison::Equal, 0) | (Comparison::Less, 1));
                    let any =
                        matches!((op, count), (Comparison::Greater | Comparison::NotEqual, 0));
                    if zero || any {
                        return Ok(vec![Predicate::Namespace {
                            namespace: namespace.to_owned(),
                            inclusive: any,
                        }]);
                    }
                }
                system(SystemPredicate::NumTags {
                    namespace,
                    op,
                    count,
                })
            }
            Kind::TagAsNumber => system(SystemPredicate::TagAsNumber {
                namespace: self.namespace_filter(1, 2),
                op: match self.chosen_text(3) {
                    LESS => TagNumberOp::Less,
                    APPROX => TagNumberOp::Approx,
                    _ => TagNumberOp::Greater,
                },
                value: self.number(4),
            }),
            Kind::TagAdvanced => {
                let service = match self.chosen(2) {
                    0 => None,
                    n => context
                        .tag_services
                        .get(n - 1)
                        .map(|(key, _)| ServiceRef::Key(key.clone())),
                };
                let statuses: BTreeSet<ContentStatus> = [
                    ContentStatus::Current,
                    ContentStatus::Pending,
                    ContentStatus::Deleted,
                    ContentStatus::Petitioned,
                ]
                .into_iter()
                .zip(self.ticked(4))
                .filter_map(|(status, on)| on.then_some(status))
                .collect();
                let statuses = if statuses.is_empty() {
                    [ContentStatus::Current, ContentStatus::Pending]
                        .into_iter()
                        .collect()
                } else {
                    statuses
                };
                let tag = Tag::new(self.text_of(5))
                    .or_else(|| Tag::new("invalid tag"))
                    .ok_or("That tag is not valid.")?;
                system(SystemPredicate::TagAdvanced {
                    service,
                    display: if self.chosen(3) == 0 {
                        TagDisplayType::Display
                    } else {
                        TagDisplayType::Storage
                    },
                    statuses,
                    tag,
                    inclusive: self.chosen(1) == 0,
                })
            }
            Kind::TimeDelta(kind) => system(SystemPredicate::Time {
                kind,
                test: TimeTest::Relative {
                    op: [RelativeOp::Greater, RelativeOp::Less, RelativeOp::Approx][self.chosen(1)],
                    age: CalendarDelta {
                        years: self.small(2),
                        months: self.small(3),
                        days: self.small(4),
                        hours: self.small(5),
                        ..CalendarDelta::ZERO
                    },
                },
            }),
            Kind::TimeDate(kind) => {
                let at = civil(self.text_of(2), self.text_of(3)).ok_or(
                    "Please enter the date as year-month-day and the time as hours:minutes.",
                )?;
                system(SystemPredicate::Time {
                    kind,
                    test: TimeTest::Absolute {
                        op: [
                            Comparison::Less,
                            Comparison::Greater,
                            Comparison::Equal,
                            Comparison::Approx,
                        ][self.chosen(1)],
                        at,
                    },
                })
            }
            Kind::UrlExact | Kind::UrlDomain | Kind::UrlRegex => {
                let text = self.text_of(3).to_owned();
                let rule = match self.kind {
                    Kind::UrlExact => UrlRule::ExactMatch(text),
                    Kind::UrlDomain => UrlRule::Domain(text),
                    _ => {
                        if let Err(e) = regex_check(&text) {
                            return Err(format!("Cannot compile that regex: {e}"));
                        }
                        UrlRule::Regex(text)
                    }
                };
                system(SystemPredicate::KnownUrl {
                    rule,
                    has: self.chosen(1) == 0,
                })
            }
            Kind::UrlClass => {
                let Some(class) = context.url_classes.get(self.chosen(3)) else {
                    return Err("There is no URL class to choose.".into());
                };
                system(SystemPredicate::KnownUrl {
                    rule: UrlRule::UrlClass(class.clone()),
                    has: self.chosen(1) == 0,
                })
            }
        }
    }

    /// A plain number test's value and amount either side, its operator
    /// at field `operator`.
    fn read_plain_number_test(&self, operator: usize) -> NumberTest {
        let operators: Vec<&'static str> = match &self.fields[operator] {
            Field::Choice { options, .. } => ALL_OPERATORS
                .into_iter()
                .filter(|o| options.iter().any(|x| x == o))
                .collect(),
            _ => Vec::new(),
        };
        let fields = NumberTestFields {
            operators,
            operator,
            value: operator + 1,
            absolute: operator + 2,
            percent: operator + 3,
        };
        self.read_number_test(&fields, Panel::unsigned, Panel::unsigned)
    }

    fn duration_fields() -> NumberTestFields {
        let operator = 1;
        NumberTestFields {
            operators: ALL_OPERATORS.to_vec(),
            operator,
            value: operator + 1,
            absolute: operator + 5,
            percent: operator + 8,
        }
    }

    /// A namespace widget's namespace: any, none, or the one typed (none if
    /// nothing is typed).
    fn namespace_filter(&self, choice: usize, text: usize) -> NamespaceFilter {
        match self.chosen(choice) {
            0 => NamespaceFilter::Any,
            1 => NamespaceFilter::Unnamespaced,
            _ => NamespaceFilter::from_reference(self.text_of(text)),
        }
    }
}

/// Fields of time units from `at` on, each `scale` milliseconds, summed.
fn hms_ms(panel: &Panel, at: usize, scales: &[u64]) -> u64 {
    scales
        .iter()
        .enumerate()
        .map(|(i, scale)| panel.unsigned(at + i) * scale)
        .sum()
}

fn comparison(op: &str) -> Comparison {
    match op {
        LESS => Comparison::Less,
        GREATER => Comparison::Greater,
        APPROX => Comparison::Approx,
        NOT_EQUAL => Comparison::NotEqual,
        _ => Comparison::Equal,
    }
}

/// A date typed as year-month-day and a time as hours:minutes (seconds
/// ignored).
fn civil(date: &str, time: &str) -> Option<CivilDateTime> {
    let mut d = date.trim().splitn(3, '-').map(|p| p.trim().parse::<u16>());
    let (year, month, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.trim().split(':').map(|p| p.trim().parse::<u8>());
    let hour = t.next().map_or(Some(0), Result::ok)?;
    let minute = t.next().map_or(Some(0), Result::ok)?;
    CivilDateTime::new(
        year,
        u8::try_from(month).ok()?,
        u8::try_from(day).ok()?,
        hour,
        minute,
    )
}

fn regex_check(text: &str) -> Result<(), String> {
    hydrus_search::check_url_regex(text)
}

/// A page of an editor: its name (empty with only one page), its
/// ready-made buttons and its panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub name: String,
    pub buttons: Vec<Button>,
    pub panels: Vec<Panel>,
}

/// A ready-made button: the predicates it adds, under its own label or
/// (with none) theirs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub label: Option<String>,
    pub predicates: Vec<SystemPredicate>,
}

/// An editor: a note over it, and its pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    pub blank: Blank,
    pub note: Option<String>,
    pub pages: Vec<Page>,
}

impl Editor {
    /// The editor for a blank predicate, its panels at their defaults;
    /// `None` for those not ported yet.
    pub fn new(blank: Blank, context: &Context) -> Option<Self> {
        let page = |buttons: Vec<Button>, panels: Vec<Panel>| Page {
            name: String::new(),
            buttons,
            panels,
        };
        let (note, pages) = match blank {
            Blank::Dimensions => (
                None,
                vec![page(
                    dimension_buttons(),
                    vec![
                        size_panel(Kind::Width, "system:width", 1920),
                        size_panel(Kind::Height, "system:height", 1080),
                        ratio_panel(),
                        pixels_panel(),
                    ],
                )],
            ),
            Blank::Duration => (
                None,
                vec![page(
                    vec![
                        one(duration(NumberTest::nonzero())),
                        one(duration(NumberTest::zero())),
                        one(framerate(30)),
                        one(framerate(60)),
                    ],
                    vec![duration_panel(), framerate_panel(), frames_panel()],
                )],
            ),
            Blank::FileProperties => (None, vec![page(property_buttons(), Vec::new())]),
            Blank::FileRelationships => (
                None,
                vec![page(
                    vec![
                        one(SystemPredicate::BestQualityOfGroup { is_best: false }),
                        one(SystemPredicate::BestQualityOfGroup { is_best: true }),
                    ],
                    vec![relationships_panel()],
                )],
            ),
            Blank::FileService => (
                None,
                vec![page(Vec::new(), vec![file_service_panel(context)])],
            ),
            Blank::FileViewingStats => (
                None,
                vec![page(Vec::new(), vec![views_panel(), viewtime_panel()])],
            ),
            Blank::Filesize => (None, vec![page(Vec::new(), vec![filesize_panel()])]),
            Blank::Limit => (
                Some(
                    "system:limit clips a large search result down to the given number of \
                     files. It is very useful for processing in smaller batches."
                        .to_owned(),
                ),
                vec![page(
                    [64, 256, 1024]
                        .into_iter()
                        .map(|n| one(SystemPredicate::Limit(n)))
                        .collect(),
                    vec![limit_panel()],
                )],
            ),
            Blank::Notes => (
                None,
                vec![page(
                    vec![
                        one(number(NumericProperty::NumNotes, NumberTest::nonzero())),
                        one(number(NumericProperty::NumNotes, NumberTest::zero())),
                    ],
                    vec![notes_panel(), note_name_panel()],
                )],
            ),
            Blank::NumTags => (
                None,
                vec![page(
                    vec![
                        one(SystemPredicate::NumTags {
                            namespace: NamespaceFilter::Any,
                            op: Comparison::Greater,
                            count: 0,
                        }),
                        one(SystemPredicate::NumTags {
                            namespace: NamespaceFilter::Any,
                            op: Comparison::Equal,
                            count: 0,
                        }),
                    ],
                    vec![num_tags_panel()],
                )],
            ),
            Blank::NumWords => (None, vec![page(Vec::new(), vec![words_panel()])]),
            Blank::TagAdvanced => (
                Some(
                    "This predicate is only needed to solve advanced problems. It may run very \
                     slow."
                        .to_owned(),
                ),
                vec![page(Vec::new(), vec![tag_advanced_panel(context)])],
            ),
            Blank::TagAsNumber => (None, vec![page(Vec::new(), vec![tag_as_number_panel()])]),
            Blank::Time => (
                None,
                [
                    ("import", TimeKind::Imported),
                    ("modified", TimeKind::Modified),
                    ("last viewed", TimeKind::LastViewed),
                    ("archived", TimeKind::Archived),
                ]
                .into_iter()
                .map(|(name, kind)| Page {
                    name: name.to_owned(),
                    buttons: if kind == TimeKind::Imported {
                        [(0, 1), (0, 7), (1, 0)]
                            .into_iter()
                            .map(|(months, days)| {
                                one(SystemPredicate::Time {
                                    kind,
                                    test: TimeTest::Relative {
                                        op: RelativeOp::Less,
                                        age: CalendarDelta {
                                            months,
                                            days,
                                            ..CalendarDelta::ZERO
                                        },
                                    },
                                })
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    panels: vec![time_delta_panel(kind), time_date_panel(kind, context.today)],
                })
                .collect(),
            ),
            Blank::Urls => (
                Some(
                    "Note that \"number of urls\" counts all URLs, regardless of how important."
                        .to_owned(),
                ),
                vec![
                    Page {
                        name: "known urls".into(),
                        buttons: Vec::new(),
                        panels: vec![
                            url_panel(Kind::UrlExact, " url "),
                            url_panel(Kind::UrlDomain, " url with domain "),
                            url_panel(Kind::UrlRegex, " url that matches regex "),
                            url_class_panel(context),
                        ],
                    },
                    Page {
                        name: "number of urls".into(),
                        buttons: vec![
                            one(number(NumericProperty::NumUrls, NumberTest::nonzero())),
                            one(number(NumericProperty::NumUrls, NumberTest::zero())),
                        ],
                        panels: vec![urls_panel()],
                    },
                ],
            ),
            Blank::Filetype | Blank::Hash | Blank::Rating | Blank::SimilarFiles => return None,
        };
        Some(Editor { blank, note, pages })
    }
}

fn one(predicate: SystemPredicate) -> Button {
    Button {
        label: None,
        predicates: vec![predicate],
    }
}

fn number(property: NumericProperty, test: NumberTest) -> SystemPredicate {
    SystemPredicate::Number { property, test }
}

fn duration(test: NumberTest) -> SystemPredicate {
    number(NumericProperty::Duration, test)
}

fn framerate(fps: u64) -> SystemPredicate {
    number(
        NumericProperty::Framerate,
        NumberTest::new(NumberOp::ApproxAbsolute { tolerance: 1 }, fps),
    )
}

fn dimension_buttons() -> Vec<Button> {
    let ratio = |op, width, height| one(SystemPredicate::Ratio { op, width, height });
    let resolution = |label: &str, width, height| Button {
        label: Some(label.to_owned()),
        predicates: vec![
            number(
                NumericProperty::Width,
                NumberTest::new(NumberOp::Equal, width),
            ),
            number(
                NumericProperty::Height,
                NumberTest::new(NumberOp::Equal, height),
            ),
        ],
    };
    vec![
        ratio(RatioOp::Equal, 16, 9),
        ratio(RatioOp::Equal, 9, 16),
        ratio(RatioOp::Equal, 4, 3),
        ratio(RatioOp::Equal, 1, 1),
        ratio(RatioOp::TallerThan, 1, 1),
        ratio(RatioOp::WiderThan, 1, 1),
        resolution("1080p", 1920, 1080),
        resolution("720p", 1280, 720),
        resolution("4k", 3840, 2160),
    ]
}

fn property_buttons() -> Vec<Button> {
    let has = |property, has| one(SystemPredicate::FileProperty { property, has });
    let mut out = vec![
        has(FileProperty::Audio, true),
        has(FileProperty::Audio, false),
        one(duration(NumberTest::nonzero())),
        one(duration(NumberTest::zero())),
    ];
    for property in [
        FileProperty::Transparency,
        FileProperty::HumanReadableEmbeddedMetadata,
        FileProperty::SoftwareSourceMetadata,
        FileProperty::IccProfile,
        FileProperty::Exif,
        FileProperty::Xmp,
        FileProperty::Iptc,
        FileProperty::ForcedFiletype,
    ] {
        out.push(has(property, true));
        out.push(has(property, false));
    }
    out
}

/// Width or height: any operator, up to 200,000 pixels, ±200.
fn size_panel(kind: Kind, label: &str, value: u64) -> Panel {
    let mut panel = Panel::new(kind);
    panel.push(Field::label(label));
    panel.plain_number_test(
        &ALL_OPERATORS,
        NumberTest::new(NumberOp::Equal, value),
        200_000,
        "px",
        200,
        15,
    );
    panel
}

fn ratio_panel() -> Panel {
    let mut panel = Panel::new(Kind::Ratio);
    panel.push(Field::label("system:ratio"));
    panel.push(Field::choice(
        &[EQUAL, "wider than", "taller than", APPROX, NOT_EQUAL],
        "wider than",
    ));
    panel.push(Field::number(16, 0, 50_000, ""));
    panel.push(Field::label(":"));
    panel.push(Field::number(9, 0, 50_000, ""));
    panel
}

fn pixels_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumPixels);
    panel.push(Field::label("system:number of pixels"));
    panel.push(Field::choice(
        &[LESS, APPROX, EQUAL, NOT_EQUAL, GREATER],
        APPROX,
    ));
    panel.push(Field::number(2, 0, 1_048_576, ""));
    panel.push(Field::choice(
        &["pixels", "kilopixels", "megapixels"],
        "megapixels",
    ));
    panel
}

/// Fields of a span of time, in `units` (of "days", "hours", "minutes",
/// "seconds", "ms"), set to `ms` milliseconds; the largest unit takes what
/// the others don't (but no more than its limit).
fn time_fields(panel: &mut Panel, ms: u64, units: &[&str], before: &str) -> usize {
    let first = panel.fields.len();
    let mut rest = ms;
    for (i, unit) in units.iter().enumerate() {
        let (scale, max) = match *unit {
            "days" => (86_400_000, 3523),
            "hours" => (3_600_000, 23),
            "minutes" => (60_000, 59),
            "seconds" => (1000, 59),
            _ => (1, 999),
        };
        let value = if i == 0 {
            rest / scale
        } else {
            (rest / scale) % (max + 1)
        };
        rest -= (rest / scale).min(value) * scale;
        let at = panel.push(Field::number(
            i64::try_from(value).unwrap_or(max as i64),
            0,
            max as i64,
            unit,
        ));
        if i == 0
            && !before.is_empty()
            && let Field::Number { before: b, .. } = &mut panel.fields[at]
        {
            before.clone_into(b);
        }
    }
    first
}

fn duration_panel() -> Panel {
    let mut panel = Panel::new(Kind::Duration);
    panel.push(Field::label("system:duration"));
    panel.number_test(
        &ALL_OPERATORS,
        NumberTest::nonzero(),
        |panel, ms| time_fields(panel, ms, &["hours", "minutes", "seconds", "ms"], ""),
        |panel, ms| {
            time_fields(panel, ms, &["minutes", "seconds", "ms"], "\u{b1}");
            panel.fields.len()
        },
        30_000,
        5,
    );
    panel
}

fn framerate_panel() -> Panel {
    let mut panel = Panel::new(Kind::Framerate);
    panel.push(Field::label(
        "Framerate is approximate. A \"30fps\" file may actually be 30.005fps internally.\nIf \
         you use < or >, include some padding: \"> 59\", not \"> 60\".",
    ));
    panel.push(Field::label("system:framerate"));
    panel.plain_number_test(
        &[LESS, APPROX, APPROX_PERCENT, GREATER],
        NumberTest::new(NumberOp::ApproxPercent { percent: 5 }, 60),
        1_000_000,
        "fps",
        1,
        5,
    );
    panel
}

fn frames_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumFrames);
    panel.push(Field::label("system:number of frames"));
    panel.plain_number_test(
        &ALL_OPERATORS,
        NumberTest::new(NumberOp::Greater, 600),
        1_000_000,
        "",
        300,
        15,
    );
    panel
}

fn relationships_panel() -> Panel {
    let mut panel = Panel::new(Kind::DuplicateRelationships);
    panel.push(Field::label("system:num file relationships"));
    panel.push(Field::choice(&[LESS, APPROX, EQUAL, GREATER], GREATER));
    panel.push(Field::number(0, 0, 65_535, ""));
    panel.push(Field::choice(
        &[
            "duplicates",
            "alternates",
            "not related/false positive",
            "potential duplicates",
        ],
        "duplicates",
    ));
    panel
}

fn file_service_panel(context: &Context) -> Panel {
    let mut panel = Panel::new(Kind::FileService);
    panel.push(Field::label("system:file service:"));
    panel.push(Field::choice(&["is", "is not"], "is"));
    panel.push(Field::choice(
        &[
            "currently in",
            "deleted from",
            "pending to",
            "petitioned from",
        ],
        "currently in",
    ));
    panel.push(Field::Choice {
        options: context
            .file_services
            .iter()
            .map(|(_, name)| name.clone())
            .collect(),
        chosen: 0,
    });
    panel
}

fn viewing_panel(kind: Kind, what: &str) -> Panel {
    let mut panel = Panel::new(kind);
    panel.push(Field::label("system:"));
    panel.push(Field::Ticks {
        options: ["media", "preview", "client api"]
            .iter()
            .map(|canvas| format!("{canvas} {what}"))
            .collect(),
        ticked: vec![true, false, false],
    });
    panel.push(Field::choice(&[LESS, APPROX, EQUAL, GREATER], GREATER));
    panel
}

fn views_panel() -> Panel {
    let mut panel = viewing_panel(Kind::Views, "views");
    panel.push(Field::number(10, 0, 1_000_000, ""));
    panel
}

fn viewtime_panel() -> Panel {
    let mut panel = viewing_panel(Kind::Viewtime, "viewtime");
    time_fields(
        &mut panel,
        600_000,
        &["days", "hours", "minutes", "seconds", "ms"],
        "",
    );
    panel
}

fn filesize_panel() -> Panel {
    let mut panel = Panel::new(Kind::Size);
    panel.push(Field::label("system:filesize"));
    panel.push(Field::choice(
        &[LESS, APPROX, EQUAL, NOT_EQUAL, GREATER],
        LESS,
    ));
    panel.push(Field::number(200, 0, 1_048_576, ""));
    panel.push(Field::choice(&["B", "KB", "MB", "GB", "TB"], "KB"));
    panel
}

fn limit_panel() -> Panel {
    let mut panel = Panel::new(Kind::Limit);
    panel.push(Field::label("system:limit="));
    panel.push(Field::number(256, 1, 1_000_000, ""));
    panel
}

fn notes_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumNotes);
    panel.push(Field::label("system:number of notes"));
    panel.plain_number_test(
        &[
            LESS,
            LESS_OR_EQUAL,
            EQUAL,
            NOT_EQUAL,
            GREATER_OR_EQUAL,
            GREATER,
        ],
        NumberTest::new(NumberOp::Equal, 2),
        200_000,
        "",
        1,
        15,
    );
    panel
}

fn note_name_panel() -> Panel {
    let mut panel = Panel::new(Kind::NoteName);
    panel.push(Field::label("system:note name"));
    panel.push(Field::choice(
        &["has note with name ", "does not have note with name"],
        "has note with name ",
    ));
    panel.push(Field::text("", ""));
    panel
}

/// A namespace widget (`NamespaceWidget`): any namespace, unnamespaced, or
/// the one typed (which can only be typed when chosen).
fn namespace_fields(panel: &mut Panel, namespace: &NamespaceFilter) {
    let (chosen, text) = match namespace {
        NamespaceFilter::Any => ("any namespace", ""),
        NamespaceFilter::Unnamespaced => ("unnamespaced", ""),
        NamespaceFilter::Namespace(ns) => ("namespace", ns.as_str()),
    };
    let choice = panel.push(Field::choice(
        &["any namespace", "unnamespaced", "namespace"],
        chosen,
    ));
    let text = panel.push(Field::text(text, "e.g. character"));
    panel.conditions.push(Condition {
        field: text,
        choice,
        options: vec![2],
        hide: false,
    });
}

fn num_tags_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumTags);
    panel.push(Field::label("system:number of tags: namespace:"));
    namespace_fields(&mut panel, &NamespaceFilter::Any);
    panel.push(Field::choice(&[LESS, APPROX, EQUAL, GREATER], GREATER));
    panel.push(Field::number(4, 0, 2000, ""));
    panel
}

fn words_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumWords);
    panel.push(Field::label("system:number of words"));
    panel.plain_number_test(
        &ALL_OPERATORS,
        NumberTest::new(NumberOp::Less, 30_000),
        100_000_000,
        "",
        5000,
        15,
    );
    panel
}

fn urls_panel() -> Panel {
    let mut panel = Panel::new(Kind::NumUrls);
    panel.push(Field::label("system:number of urls"));
    panel.plain_number_test(
        &[
            LESS,
            LESS_OR_EQUAL,
            EQUAL,
            NOT_EQUAL,
            GREATER_OR_EQUAL,
            GREATER,
        ],
        NumberTest::nonzero(),
        200_000,
        "",
        1,
        15,
    );
    panel
}

fn tag_advanced_panel(context: &Context) -> Panel {
    let mut panel = Panel::new(Kind::TagAdvanced);
    panel.push(Field::label("system:"));
    panel.push(Field::choice(&["has tag", "does not have tag"], "has tag"));
    let mut domains = vec!["current tag domain".to_owned()];
    domains.extend(context.tag_services.iter().map(|(_, name)| name.clone()));
    panel.push(Field::Choice {
        options: domains,
        chosen: 0,
    });
    panel.push(Field::choice(
        &["including siblings/parents", "ignoring siblings/parents"],
        "ignoring siblings/parents",
    ));
    panel.push(Field::Ticks {
        options: ["current", "pending", "deleted", "petitioned"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        ticked: vec![true, true, false, false],
    });
    panel.push(Field::text("", "tag"));
    panel
}

fn tag_as_number_panel() -> Panel {
    let mut panel = Panel::new(Kind::TagAsNumber);
    panel.push(Field::label("system:tag as number: "));
    namespace_fields(&mut panel, &NamespaceFilter::Namespace("page".into()));
    panel.push(Field::choice(&[LESS, APPROX, GREATER], GREATER));
    panel.push(Field::number(
        0,
        i64::from(i32::MIN),
        i64::from(i32::MAX),
        "",
    ));
    panel
}

fn time_label(kind: TimeKind) -> &'static str {
    match kind {
        TimeKind::Imported => "system:import time",
        TimeKind::Modified => "system:modified time",
        TimeKind::LastViewed => "system:last viewed time",
        TimeKind::Archived => "system:archived time",
    }
}

fn time_delta_panel(kind: TimeKind) -> Panel {
    let mut panel = Panel::new(Kind::TimeDelta(kind));
    panel.push(Field::label(time_label(kind)));
    panel.push(Field::choice(&["before", "since", "+/- 15% of"], "since"));
    panel.push(Field::number(0, 0, 50_000, "years"));
    panel.push(Field::number(0, 0, 1000, "months"));
    panel.push(Field::number(7, 0, 1000, "days"));
    panel.push(Field::number(0, 0, 10_000, "hours ago"));
    panel
}

fn time_date_panel(kind: TimeKind, today: CivilDateTime) -> Panel {
    let mut panel = Panel::new(Kind::TimeDate(kind));
    panel.push(Field::label(time_label(kind)));
    // (the import time's starts "before" today; the others' "since")
    let chosen = if kind == TimeKind::Imported {
        "before"
    } else {
        "since"
    };
    panel.push(Field::choice(
        &["before", "since", "the day of", "+/- a month of"],
        chosen,
    ));
    panel.push(Field::text(
        &format!(
            "{:04}-{:02}-{:02}",
            today.year(),
            today.month(),
            today.day()
        ),
        "year-month-day",
    ));
    panel.push(Field::text("00:00", "hours:minutes"));
    panel
}

fn url_panel(kind: Kind, what: &str) -> Panel {
    let mut panel = Panel::new(kind);
    panel.push(Field::label("system:"));
    panel.push(Field::choice(&["has", "does not have"], "has"));
    panel.push(Field::label(what));
    let placeholder = if kind == Kind::UrlDomain {
        "example.com"
    } else {
        ""
    };
    panel.push(Field::text("", placeholder));
    panel
}

fn url_class_panel(context: &Context) -> Panel {
    let mut panel = Panel::new(Kind::UrlClass);
    panel.push(Field::label("system:"));
    panel.push(Field::choice(&["has", "does not have"], "has"));
    panel.push(Field::label(" url matching class "));
    panel.push(Field::Choice {
        options: context.url_classes.clone(),
        chosen: 0,
    });
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            file_services: vec![(ServiceKey::new(b"x".to_vec()), "my files".into())],
            tag_services: Vec::new(),
            url_classes: Vec::new(),
            today: CivilDateTime::new(2026, 10, 2, 0, 0).unwrap(),
        }
    }

    #[test]
    fn rating_is_offered_with_a_rating_service_and_the_rest_everywhere() {
        assert!(!offered(false, false).contains(&Blank::Rating));
        assert!(offered(false, true).contains(&Blank::Rating));
        assert_eq!(offered(false, true), Blank::ALL);
        let everywhere = offered(true, true);
        assert!(everywhere.contains(&Blank::Hash) && everywhere.contains(&Blank::Rating));
        assert!(!everywhere.contains(&Blank::Dimensions));
    }

    #[test]
    fn time_fields_split_a_span_into_its_units() {
        let mut panel = Panel::new(Kind::Duration);
        time_fields(
            &mut panel,
            3_723_004,
            &["hours", "minutes", "seconds", "ms"],
            "",
        );
        let values: Vec<i64> = (0..4).map(|i| panel.number(i)).collect();
        assert_eq!(values, [1, 2, 3, 4]);
        assert_eq!(hms_ms(&panel, 0, &[3_600_000, 60_000, 1000, 1]), 3_723_004);
    }

    #[test]
    fn a_date_is_read_as_typed() {
        assert_eq!(
            civil("2011-06-04", "13:05"),
            CivilDateTime::new(2011, 6, 4, 13, 5)
        );
        assert_eq!(civil("2011-02-30", "00:00"), None);
        assert_eq!(civil("yesterday", "00:00"), None);
        let editor = Editor::new(Blank::Time, &context()).unwrap();
        let mut date = editor.pages[0].panels[1].clone();
        date.set_text(2, "June");
        assert!(date.predicates(&context()).is_err());
    }
}
