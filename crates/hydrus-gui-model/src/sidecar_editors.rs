//! The sidecar editors (`ClientGUIMetadataMigration*`): a router's source
//! (importer) editor and destination (exporter) widget, the router editor
//! and the routers list, as an import (import folders, filename tagging:
//! .txt and .json sidecars into a file's tags, notes, urls and timestamps)
//! or an export (export folders: those out to sidecars) opens them.

use hydrus_core::ServiceKey;
use hydrus_core::content::TimestampType;
use hydrus_core::service::{ServiceType, builtin_keys};
use hydrus_core::url::strings::{
    MatchKind, ProcessingStep, SortKind, StringConverter, StringMatch, StringProcessor,
};
use hydrus_parse::formula::{Formula, FormulaKind, JsonContent, JsonRule};
use hydrus_parse::sidecar::{
    Exporter, Importer, Router, SidecarNaming, Source, TagDisplay, TimestampLocation, TimestampStub,
};
use hydrus_store::services::ServiceRegistry as Services;

/// Which way metadata moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Sidecars into a file's metadata (import folders, filename tagging).
    Import,
    /// A file's metadata out to sidecars (export folders).
    Export,
}

/// What a source or destination is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    MediaTags,
    MediaNotes,
    MediaUrls,
    MediaTimestamps,
    Txt,
    Json,
}

impl Kind {
    /// The "change type" button's label (`choice_tuple_label_lookup`).
    pub fn label(self) -> &'static str {
        match self {
            Kind::MediaNotes => "a file's notes",
            Kind::MediaTags => "a file's tags",
            Kind::MediaUrls => "a file's URLs",
            Kind::MediaTimestamps => "a file's timestamps",
            Kind::Txt => "a .txt sidecar",
            Kind::Json => "a .json sidecar",
        }
    }

    /// What "Which type?" says of it; a destination's timestamps say "the
    /// file" where a source's say "a file".
    pub fn description(self, destination: bool) -> &'static str {
        match self {
            Kind::MediaNotes => "The notes that a file has.",
            Kind::MediaTags => "The tags that a file has on a particular service.",
            Kind::MediaUrls => "The known URLs that a file has.",
            Kind::MediaTimestamps if destination => "A recorded timestamp the file has.",
            Kind::MediaTimestamps => "A recorded timestamp that a file has.",
            Kind::Txt => "A list of raw newline-separated texts in a .txt file.",
            Kind::Json => "Strings somewhere in a JSON file.",
        }
    }

    fn is_sidecar(self) -> bool {
        matches!(self, Kind::Txt | Kind::Json)
    }
}

impl Context {
    /// The kinds of source it allows, in the reference's order.
    pub fn importers(self) -> &'static [Kind] {
        match self {
            Context::Import => &[Kind::Txt, Kind::Json],
            Context::Export => &[
                Kind::MediaTags,
                Kind::MediaNotes,
                Kind::MediaUrls,
                Kind::MediaTimestamps,
            ],
        }
    }

    /// The kinds of destination it allows.
    pub fn exporters(self) -> &'static [Kind] {
        match self {
            Context::Import => &[
                Kind::MediaTags,
                Kind::MediaNotes,
                Kind::MediaUrls,
                Kind::MediaTimestamps,
            ],
            Context::Export => &[Kind::Txt, Kind::Json],
        }
    }
}

pub const TYPE_TITLE: &str = "Which type?";
pub const SOURCE_TYPE_QUESTION: &str = "Which kind of source are we going to use?";
pub const DESTINATION_TYPE_QUESTION: &str = "Which kind of destination are we going to use?";
pub const ONLY_ONE: &str = "Sorry, you can only have this one!";
pub const SELECT_SERVICE: &str = "select service";
pub const NO_DOMAIN: &str = "You have to enter a domain!";
pub const SOURCE_PROCESSING_NOTE: &str =
    "You can alter the texts that come in through this source here.";
pub const ROUTER_PROCESSING_NOTE: &str = "You can alter all the texts before export here.";
pub const SOURCES_TITLE: &str = "edit metadata migration source";
pub const ROUTER_TITLE: &str = "edit metadata migration router";
pub const ROUTERS_TITLE: &str = "edit metadata migration routers";
pub const SIDECAR_HELP: &str = "Sidecars are typically named just as their associated file but with the additional extension. 'image.jpg' makes 'image.jpg.txt', and so on.\n\nSidecar exporters will overwrite whatever is at their set destination, so be careful if you intend to set up multiple simultaneous exports, or the second will overwrite the first. You can safely export to two or more different locations in the same .json file, but if you export to .txt, use the 'suffix' control to export to different files.\n\nIf there is no content to write, no new file will be created.";
pub const NOTES_TO_TXT_QUESTION: &str = "Hey, you are exporing notes to a .txt file but have selected a \"newline\" separator to split multiple notes. This will break any notes that have multiple lines--are you sure you do not want to select \"||||\" (or something else unlikely to appear in your note text) as your separator instead? Another option is to export to JSON instead.";
pub const NOTES_FROM_TXT_QUESTION: &str = "Hey, you are importing notes from a .txt file but have selected the \"newline\" separator to specify where multiple notes split. If any of your notes have multiple lines, they will be broken by this! Are you sure this is how the notes in the .txt are formatted, with a newline separator? If you created these sidecars, it would be better if you used a different separator like \"||||\" or just went for JSON instead. Are you sure you want to go ahead?";
pub const SEPARATOR_CHOICES: [&str; 3] = ["newline", "four pipes (||||)", "custom text"];
pub const TAG_DISPLAY_CHOICES: [&str; 2] = ["stored tags", "display tags"];
pub const CANVAS_CHOICES: [&str; 2] = ["media viewer", "preview viewer"];

/// A source's sidecar test path (`SetExampleInput`).
pub const IMPORT_EXAMPLE: &str = "my_image.jpg";
/// A destination's.
pub const EXPORT_EXAMPLE: &str = "01234564789abcdef.jpg";

fn hex(key: &[u8]) -> String {
    hex::encode(key)
}

/// A converter that converts nothing, with an example.
fn converter(example: &str) -> StringConverter {
    StringConverter {
        conversions: Vec::new(),
        example: example.to_owned(),
    }
}

/// The reference's new routers' processing: a human sort.
pub fn default_router_processor() -> StringProcessor {
    StringProcessor {
        steps: vec![ProcessingStep::Sort {
            kind: SortKind::Human,
            ascending: true,
            regex: None,
        }],
    }
}

/// A JSON importer's formula: every item, as strings.
fn every_item() -> Formula {
    Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: FormulaKind::Json {
            rules: vec![JsonRule::AllItems],
            content: JsonContent::Strings,
        },
        processor: StringProcessor::default(),
    }
}

/// The formula the source editor starts with, for a .json sidecar it
/// changes to (`ParseFormulaJSON()`: the "posts" key).
fn posts() -> Formula {
    Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: FormulaKind::Json {
            rules: vec![JsonRule::DictKey(StringMatch {
                kind: MatchKind::Fixed("posts".to_owned()),
                min_chars: None,
                max_chars: None,
                example: "posts".to_owned(),
            })],
            content: JsonContent::Strings,
        },
        processor: StringProcessor::default(),
    }
}

/// A new importer of a kind, as the reference makes one (`importer_class(
/// string_processor )`).
pub fn new_importer(kind: Kind, processor: StringProcessor) -> Importer {
    let naming = |ext: &str| SidecarNaming {
        remove_actual_filename_ext: false,
        suffix: String::new(),
        filename_converter: converter(&format!("my_image.jpg.{ext}")),
    };
    let source = match kind {
        Kind::MediaNotes => Source::MediaNotes,
        Kind::MediaTags => Source::MediaTags {
            service_key: hex(builtin_keys::COMBINED_TAG),
            display: TagDisplay::DisplayActual,
        },
        Kind::MediaUrls => Source::MediaUrls,
        Kind::MediaTimestamps => Source::MediaTimestamp(archived()),
        Kind::Txt => Source::Txt {
            naming: naming("txt"),
            separator: "\n".to_owned(),
        },
        Kind::Json => Source::Json {
            naming: naming("json"),
            formula: Box::new(every_item()),
        },
    };
    Importer { source, processor }
}

/// A new exporter of a kind.
pub fn new_exporter(kind: Kind) -> Exporter {
    let naming = |ext: &str| SidecarNaming {
        remove_actual_filename_ext: false,
        suffix: String::new(),
        filename_converter: converter(&format!("0123456789abcdef.jpg.{ext}")),
    };
    match kind {
        Kind::MediaNotes => Exporter::MediaNotes { forced_name: None },
        Kind::MediaTags => Exporter::MediaTags {
            service_key: hex(builtin_keys::MY_TAGS),
        },
        Kind::MediaUrls => Exporter::MediaUrls,
        Kind::MediaTimestamps => Exporter::MediaTimestamp(archived()),
        Kind::Txt => Exporter::Txt {
            naming: naming("txt"),
            separator: "\n".to_owned(),
        },
        Kind::Json => Exporter::Json {
            naming: naming("json"),
            nested_object_names: Vec::new(),
        },
    }
}

fn archived() -> TimestampStub {
    TimestampStub {
        kind: TimestampType::Archived,
        location: TimestampLocation::None,
    }
}

pub fn importer_kind(importer: &Importer) -> Kind {
    match importer.source {
        Source::MediaNotes => Kind::MediaNotes,
        Source::MediaTags { .. } => Kind::MediaTags,
        Source::MediaTimestamp(_) => Kind::MediaTimestamps,
        Source::MediaUrls => Kind::MediaUrls,
        Source::Txt { .. } => Kind::Txt,
        Source::Json { .. } => Kind::Json,
    }
}

pub fn exporter_kind(exporter: &Exporter) -> Kind {
    match exporter {
        Exporter::MediaNotes { .. } => Kind::MediaNotes,
        Exporter::MediaTags { .. } => Kind::MediaTags,
        Exporter::MediaTimestamp(_) => Kind::MediaTimestamps,
        Exporter::MediaUrls => Kind::MediaUrls,
        Exporter::Txt { .. } => Kind::Txt,
        Exporter::Json { .. } => Kind::Json,
    }
}

/// The sidecar filename box (`EditSidecarDetailsPanel`): how the sidecar
/// is named, and a test path with the sidecar it would have.
#[derive(Debug, Clone, PartialEq)]
pub struct NamingBox {
    pub naming: SidecarNaming,
    pub example: String,
    pub extension: &'static str,
}

impl NamingBox {
    /// The sidecar the test path would have ("Resulting sidecar path").
    pub fn result(&self) -> String {
        self.naming.path(&self.example, self.extension)
    }

    /// The filename conversion button's label.
    pub fn converter_label(&self) -> String {
        let text = self.naming.filename_converter.describe(false, false);
        elide(&text, 64)
    }
}

fn elide(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut short: String = text.chars().take(max - 1).collect();
        short.push('\u{2026}');
        short
    } else {
        text.to_owned()
    }
}

/// The .txt separator box (`EditSidecarTXTSeparator`): newline, four
/// pipes, or custom text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeparatorBox {
    pub choice: usize,
    pub custom: String,
}

impl SeparatorBox {
    pub fn new(separator: &str) -> Self {
        match separator {
            "\n" => Self {
                choice: 0,
                custom: String::new(),
            },
            "||||" => Self {
                choice: 1,
                custom: String::new(),
            },
            custom => Self {
                choice: 2,
                custom: custom.to_owned(),
            },
        }
    }

    pub fn custom_enabled(&self) -> bool {
        self.choice == 2
    }

    pub fn value(&self) -> String {
        match self.choice {
            0 => "\n".to_owned(),
            1 => "||||".to_owned(),
            _ => self.custom.clone(),
        }
    }
}

/// The timestamp type box (`TimestampDataStubCtrl`): which timestamp, and
/// for some, which file service, viewer or domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampBox {
    /// The types offered (aggregate modified time only for a source).
    pub kinds: Vec<TimestampType>,
    pub kind: usize,
    /// The file services an imported time can be for, by name.
    pub file_services: Vec<(ServiceKey, String)>,
    pub file_service: usize,
    /// Those a deleted time can be for.
    pub deleted_services: Vec<(ServiceKey, String)>,
    pub deleted_service: usize,
    /// 0 the media viewer, 1 the preview viewer.
    pub canvas: usize,
    pub domain: String,
}

/// What a timestamp type box shows below its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampDetail {
    None,
    FileService,
    DeletedService,
    Canvas,
    Domain,
}

const FILE_SERVICES: [ServiceType; 8] = [
    ServiceType::LocalFileDomain,
    ServiceType::LocalFileUpdateDomain,
    ServiceType::LocalFileTrashDomain,
    ServiceType::HydrusLocalFileStorage,
    ServiceType::CombinedLocalFileDomains,
    ServiceType::CombinedDeletedFile,
    ServiceType::FileRepository,
    ServiceType::Ipfs,
];

fn timestamp_label(kind: TimestampType) -> String {
    let base = match kind {
        TimestampType::ModifiedDomain => "domain modified time",
        TimestampType::ModifiedFile => "file modified time",
        TimestampType::ModifiedAggregate => "aggregate modified time",
        TimestampType::Imported => "imported time",
        TimestampType::Deleted => "deleted time",
        TimestampType::Archived => "archived time",
        TimestampType::LastViewed => "last viewed time",
        TimestampType::PreviouslyImported => "previous imported time (for undelete)",
    };
    if kind == TimestampType::Archived {
        format!("{base} (will auto-archive on import!)")
    } else {
        base.to_owned()
    }
}

impl TimestampBox {
    pub fn new(stub: &TimestampStub, with_aggregate: bool, services: &Services) -> Self {
        let mut kinds = vec![
            TimestampType::Archived,
            TimestampType::ModifiedFile,
            TimestampType::ModifiedDomain,
        ];
        if with_aggregate {
            kinds.push(TimestampType::ModifiedAggregate);
        }
        kinds.extend([
            TimestampType::Imported,
            TimestampType::Deleted,
            TimestampType::PreviouslyImported,
            TimestampType::LastViewed,
        ]);
        let named = |types: &[ServiceType]| -> Vec<(ServiceKey, String)> {
            crate::domains::in_order(services, types)
                .into_iter()
                .map(|(k, n, _)| (k, n))
                .collect()
        };
        let file_services = named(&FILE_SERVICES);
        // (those with a deletion record: not the trash or the deleted ones)
        let deleted_services = named(
            &FILE_SERVICES
                .iter()
                .copied()
                .filter(|t| {
                    !matches!(
                        t,
                        ServiceType::LocalFileTrashDomain | ServiceType::CombinedDeletedFile
                    )
                })
                .collect::<Vec<_>>(),
        );
        let mut b = Self {
            kind: kinds.iter().position(|k| *k == stub.kind).unwrap_or(0),
            kinds,
            file_services,
            file_service: 0,
            deleted_services,
            deleted_service: 0,
            canvas: 0,
            domain: String::new(),
        };
        let index_of = |list: &[(ServiceKey, String)], key: &str| {
            list.iter().position(|(k, _)| hex(k.as_bytes()) == key)
        };
        match (&stub.kind, &stub.location) {
            (TimestampType::Imported, TimestampLocation::Service(key)) => {
                if let Some(i) = index_of(&b.file_services, key) {
                    b.file_service = i;
                }
            }
            (
                TimestampType::Deleted | TimestampType::PreviouslyImported,
                TimestampLocation::Service(key),
            ) => {
                if let Some(i) = index_of(&b.deleted_services, key) {
                    b.deleted_service = i;
                }
            }
            (TimestampType::LastViewed, TimestampLocation::Canvas(code)) => {
                b.canvas = usize::from(*code == 1);
            }
            (TimestampType::ModifiedDomain, TimestampLocation::Domain(domain)) => {
                b.domain.clone_from(domain);
            }
            _ => {}
        }
        b
    }

    /// The type choice's labels.
    pub fn kind_labels(&self) -> Vec<String> {
        self.kinds.iter().map(|k| timestamp_label(*k)).collect()
    }

    pub fn current(&self) -> TimestampType {
        self.kinds
            .get(self.kind)
            .copied()
            .unwrap_or(TimestampType::Archived)
    }

    pub fn detail(&self) -> TimestampDetail {
        match self.current() {
            TimestampType::Imported => TimestampDetail::FileService,
            TimestampType::Deleted | TimestampType::PreviouslyImported => {
                TimestampDetail::DeletedService
            }
            TimestampType::LastViewed => TimestampDetail::Canvas,
            TimestampType::ModifiedDomain => TimestampDetail::Domain,
            _ => TimestampDetail::None,
        }
    }

    pub fn value(&self) -> Result<TimestampStub, &'static str> {
        let kind = self.current();
        let key = |list: &[(ServiceKey, String)], i: usize| {
            list.get(i)
                .map_or_else(String::new, |(k, _)| hex(k.as_bytes()))
        };
        let location = match self.detail() {
            TimestampDetail::None => TimestampLocation::None,
            TimestampDetail::FileService => {
                TimestampLocation::Service(key(&self.file_services, self.file_service))
            }
            TimestampDetail::DeletedService => {
                TimestampLocation::Service(key(&self.deleted_services, self.deleted_service))
            }
            TimestampDetail::Canvas => TimestampLocation::Canvas(i64::from(self.canvas == 1)),
            TimestampDetail::Domain => {
                if self.domain.is_empty() {
                    return Err(NO_DOMAIN);
                }
                TimestampLocation::Domain(self.domain.clone())
            }
        };
        Ok(TimestampStub { kind, location })
    }
}

/// The parts a source and a destination editor share.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBoxes {
    pub kind: Kind,
    pub naming: NamingBox,
    pub separator: SeparatorBox,
    pub timestamp: TimestampBox,
    /// The tag service, in hex.
    pub service_key: String,
}

impl NodeBoxes {
    fn new(kind: Kind, with_aggregate: bool, services: &Services, example: &str) -> Self {
        Self {
            kind,
            naming: NamingBox {
                naming: SidecarNaming {
                    remove_actual_filename_ext: false,
                    suffix: String::new(),
                    filename_converter: StringConverter::default(),
                },
                example: example.to_owned(),
                extension: "txt",
            },
            separator: SeparatorBox::new("\n"),
            timestamp: TimestampBox::new(&archived(), with_aggregate, services),
            service_key: String::new(),
        }
    }

    /// Show a sidecar's naming (resetting the test path, as `_SetValue`
    /// does).
    fn show_naming(&mut self, naming: &SidecarNaming, extension: &'static str, example: &str) {
        self.naming.naming = naming.clone();
        self.naming.extension = extension;
        example.clone_into(&mut self.naming.example);
    }
}

/// Which boxes an editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shown {
    pub naming: bool,
    pub separator: bool,
    pub timestamp: bool,
    pub tags: bool,
    /// A source's JSON formula button.
    pub json_formula: bool,
    /// A destination's forced note name.
    pub forced_name: bool,
    /// A destination's JSON object names.
    pub nested: bool,
}

/// "change type"'s question: the other kinds allowed, with their
/// descriptions; or `None`, with the reference's "Sorry, you can only
/// have this one!", when there are none.
pub fn change_type_choices(
    allowed: &[Kind],
    current: Kind,
    destination: bool,
) -> Option<Vec<(&'static str, &'static str, Kind)>> {
    let choices: Vec<(&'static str, &'static str, Kind)> = allowed
        .iter()
        .filter(|k| **k != current)
        .map(|k| (k.label(), k.description(destination), *k))
        .collect();
    (!choices.is_empty()).then_some(choices)
}

/// The source editor (`EditSingleFileMetadataImporterPanel`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImporterEditor {
    pub boxes: NodeBoxes,
    pub display: TagDisplay,
    pub formula: Formula,
    pub processor: StringProcessor,
}

impl ImporterEditor {
    pub fn new(importer: &Importer, services: &Services) -> Self {
        let mut editor = Self {
            boxes: NodeBoxes::new(importer_kind(importer), true, services, IMPORT_EXAMPLE),
            display: TagDisplay::DisplayActual,
            formula: posts(),
            processor: StringProcessor::default(),
        };
        editor.boxes.service_key = hex(builtin_keys::COMBINED_TAG);
        editor.show(importer, services);
        editor
    }

    /// Show an importer (`_SetValue`).
    fn show(&mut self, importer: &Importer, services: &Services) {
        self.boxes.kind = importer_kind(importer);
        self.processor = importer.processor.clone();
        match &importer.source {
            Source::MediaTags {
                service_key,
                display,
            } => {
                self.boxes.service_key.clone_from(service_key);
                self.display = *display;
            }
            Source::MediaTimestamp(stub) => {
                self.boxes.timestamp = TimestampBox::new(stub, true, services);
            }
            Source::Txt { naming, separator } => {
                self.boxes.show_naming(naming, "txt", IMPORT_EXAMPLE);
                self.boxes.separator = SeparatorBox::new(separator);
            }
            Source::Json { naming, formula } => {
                self.boxes.show_naming(naming, "json", IMPORT_EXAMPLE);
                self.formula = (**formula).clone();
            }
            Source::MediaNotes | Source::MediaUrls => {}
        }
    }

    pub fn shown(&self) -> Shown {
        let kind = self.boxes.kind;
        Shown {
            naming: kind.is_sidecar(),
            separator: kind == Kind::Txt,
            timestamp: kind == Kind::MediaTimestamps,
            tags: kind == Kind::MediaTags,
            json_formula: kind == Kind::Json,
            ..Shown::default()
        }
    }

    /// Change to another kind, keeping what the two share (`_ChangeType`).
    pub fn change_type(&mut self, kind: Kind, services: &Services) {
        let mut importer = new_importer(kind, self.processor.clone());
        let naming = self.boxes.naming.naming.clone();
        match &mut importer.source {
            Source::MediaTags { service_key, .. } => {
                service_key.clone_from(&self.boxes.service_key);
            }
            Source::MediaTimestamp(stub) => {
                if let Ok(value) = self.boxes.timestamp.value() {
                    *stub = value;
                }
            }
            Source::Txt {
                naming: n,
                separator,
            } => {
                *n = naming;
                *separator = self.boxes.separator.value();
            }
            Source::Json { naming: n, formula } => {
                *n = naming;
                **formula = self.formula.clone();
            }
            Source::MediaNotes | Source::MediaUrls => {}
        }
        self.show(&importer, services);
    }

    /// The importer as edited (`GetValue`), or why it can't be had.
    pub fn value(&self) -> Result<Importer, &'static str> {
        let processor = self.processor.clone();
        let source = match self.boxes.kind {
            Kind::MediaTags => Source::MediaTags {
                service_key: self.boxes.service_key.clone(),
                display: self.display,
            },
            Kind::MediaNotes => Source::MediaNotes,
            Kind::MediaUrls => Source::MediaUrls,
            Kind::MediaTimestamps => Source::MediaTimestamp(self.boxes.timestamp.value()?),
            Kind::Txt => Source::Txt {
                naming: self.boxes.naming.naming.clone(),
                separator: self.boxes.separator.value(),
            },
            Kind::Json => Source::Json {
                naming: self.boxes.naming.naming.clone(),
                formula: Box::new(self.formula.clone()),
            },
        };
        Ok(Importer { source, processor })
    }

    /// The tag services "service" offers: all tag services but the one
    /// chosen, by name.
    pub fn service_choices(&self, services: &Services) -> Vec<(String, ServiceKey)> {
        service_choices(
            services,
            &[
                ServiceType::LocalTag,
                ServiceType::TagRepository,
                ServiceType::CombinedTag,
            ],
            &self.boxes.service_key,
        )
    }
}

fn service_choices(
    services: &Services,
    types: &[ServiceType],
    current: &str,
) -> Vec<(String, ServiceKey)> {
    let mut choices: Vec<(String, ServiceKey)> = crate::domains::in_order(services, types)
        .into_iter()
        .filter(|(k, _, _)| hex(k.as_bytes()) != current)
        .map(|(k, n, _)| (n, k))
        .collect();
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    choices
}

/// The destination widget (`EditSingleFileMetadataExporterWidget`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExporterEditor {
    pub boxes: NodeBoxes,
    pub forced_name: Option<String>,
    pub nested: Vec<String>,
}

impl ExporterEditor {
    pub fn new(exporter: &Exporter, services: &Services) -> Self {
        let mut editor = Self {
            boxes: NodeBoxes::new(exporter_kind(exporter), false, services, EXPORT_EXAMPLE),
            forced_name: None,
            nested: Vec::new(),
        };
        editor.boxes.service_key = hex(builtin_keys::MY_TAGS);
        editor.show(exporter, services);
        editor
    }

    fn show(&mut self, exporter: &Exporter, services: &Services) {
        self.boxes.kind = exporter_kind(exporter);
        match exporter {
            Exporter::MediaTags { service_key } => {
                self.boxes.service_key.clone_from(service_key);
            }
            Exporter::MediaNotes { forced_name } => self.forced_name.clone_from(forced_name),
            Exporter::MediaTimestamp(stub) => {
                self.boxes.timestamp = TimestampBox::new(stub, false, services);
            }
            Exporter::Txt { naming, separator } => {
                self.boxes.show_naming(naming, "txt", EXPORT_EXAMPLE);
                self.boxes.separator = SeparatorBox::new(separator);
            }
            Exporter::Json {
                naming,
                nested_object_names,
            } => {
                self.boxes.show_naming(naming, "json", EXPORT_EXAMPLE);
                self.nested.clone_from(nested_object_names);
            }
            Exporter::MediaUrls => {}
        }
    }

    pub fn shown(&self) -> Shown {
        let kind = self.boxes.kind;
        Shown {
            naming: kind.is_sidecar(),
            separator: kind == Kind::Txt,
            timestamp: kind == Kind::MediaTimestamps,
            tags: kind == Kind::MediaTags,
            forced_name: kind == Kind::MediaNotes,
            nested: kind == Kind::Json,
            ..Shown::default()
        }
    }

    /// Whether the tag service chosen is the client's; the reference warns
    /// when it isn't ("...does not seem to exist!").
    pub fn service_exists(&self, services: &Services) -> bool {
        hex::decode(&self.boxes.service_key)
            .ok()
            .is_some_and(|k| services.by_key(&ServiceKey::new(k)).is_ok())
    }

    pub fn change_type(&mut self, kind: Kind, services: &Services) {
        let mut exporter = new_exporter(kind);
        let naming = self.boxes.naming.naming.clone();
        match &mut exporter {
            Exporter::MediaTags { service_key } => {
                service_key.clone_from(&self.boxes.service_key);
            }
            Exporter::MediaNotes { forced_name } => forced_name.clone_from(&self.forced_name),
            Exporter::MediaTimestamp(stub) => {
                if let Ok(value) = self.boxes.timestamp.value() {
                    *stub = value;
                }
            }
            Exporter::Txt {
                naming: n,
                separator,
            } => {
                *n = naming;
                *separator = self.boxes.separator.value();
            }
            Exporter::Json {
                naming: n,
                nested_object_names,
            } => {
                *n = naming;
                nested_object_names.clone_from(&self.nested);
            }
            Exporter::MediaUrls => {}
        }
        self.show(&exporter, services);
    }

    pub fn value(&self) -> Result<Exporter, &'static str> {
        Ok(match self.boxes.kind {
            Kind::MediaTags => Exporter::MediaTags {
                service_key: self.boxes.service_key.clone(),
            },
            Kind::MediaNotes => Exporter::MediaNotes {
                forced_name: self.forced_name.clone(),
            },
            Kind::MediaUrls => Exporter::MediaUrls,
            Kind::MediaTimestamps => Exporter::MediaTimestamp(self.boxes.timestamp.value()?),
            Kind::Txt => Exporter::Txt {
                naming: self.boxes.naming.naming.clone(),
                separator: self.boxes.separator.value(),
            },
            Kind::Json => Exporter::Json {
                naming: self.boxes.naming.naming.clone(),
                nested_object_names: self.nested.clone(),
            },
        })
    }

    /// The tag services "service" offers: the real ones but the one
    /// chosen.
    pub fn service_choices(&self, services: &Services) -> Vec<(String, ServiceKey)> {
        service_choices(
            services,
            &[ServiceType::LocalTag, ServiceType::TagRepository],
            &self.boxes.service_key,
        )
    }
}

/// A new router, as the list's "add" makes one: no sources, a human sort,
/// and the first destination allowed (for a file's tags, the default
/// local tag service).
pub fn new_router(context: Context) -> Router {
    Router {
        importers: Vec::new(),
        processor: default_router_processor(),
        exporter: new_exporter(context.exporters()[0]),
    }
}

/// What "ok" on the router editor asks first (`UserIsOKToOK`): notes
/// split by newlines into or out of a .txt sidecar.
pub fn ok_questions(router: &Router) -> Vec<&'static str> {
    let mut questions = Vec::new();
    let notes_in = router
        .importers
        .iter()
        .any(|i| matches!(i.source, Source::MediaNotes));
    if notes_in && matches!(&router.exporter, Exporter::Txt { separator, .. } if separator == "\n")
    {
        questions.push(NOTES_TO_TXT_QUESTION);
    }
    let txt_newline = router
        .importers
        .iter()
        .any(|i| matches!(&i.source, Source::Txt { separator, .. } if separator == "\n"));
    if txt_newline && matches!(router.exporter, Exporter::MediaNotes { .. }) {
        questions.push(NOTES_FROM_TXT_QUESTION);
    }
    questions
}

/// A JSON importer of the keys `path`, then every item.
fn json_at(path: &[&str]) -> Importer {
    let mut rules: Vec<JsonRule> = path
        .iter()
        .map(|key| {
            JsonRule::DictKey(StringMatch {
                kind: MatchKind::Fixed((*key).to_owned()),
                min_chars: None,
                max_chars: None,
                example: "example string".to_owned(),
            })
        })
        .collect();
    rules.push(JsonRule::AllItems);
    let mut importer = new_importer(Kind::Json, StringProcessor::default());
    if let Source::Json { formula, .. } = &mut importer.source {
        **formula = Formula {
            reference_auxiliary: None,
            name: String::new(),
            kind: FormulaKind::Json {
                rules,
                content: JsonContent::Strings,
            },
            processor: StringProcessor::default(),
        };
    }
    importer
}

fn json_to(path: &[String]) -> Exporter {
    let mut exporter = new_exporter(Kind::Json);
    if let Exporter::Json {
        nested_object_names,
        ..
    } = &mut exporter
    {
        nested_object_names.clone_from(&path.to_vec());
    }
    exporter
}

/// The routers list's templates menu ("easy one-click JSON that covers
/// the basics"): its label, description and the routers it adds.
pub fn templates(
    context: Context,
    services: &Services,
) -> Vec<(&'static str, &'static str, Vec<Router>)> {
    let router = |importers: Vec<Importer>, exporter: Exporter| Router {
        importers,
        processor: default_router_processor(),
        exporter,
    };
    let tag_services: Vec<(ServiceKey, String)> = crate::domains::in_order(
        services,
        &[ServiceType::LocalTag, ServiceType::TagRepository],
    )
    .into_iter()
    .map(|(k, n, _)| (k, n))
    .collect();
    let stub = |kind: TimestampType, location: TimestampLocation| TimestampStub { kind, location };
    let times: Vec<(&str, TimestampStub)> = vec![
        ("archived", archived()),
        (
            "imported",
            stub(
                TimestampType::Imported,
                TimestampLocation::Service(hex(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)),
            ),
        ),
        (
            "last_viewed_media_viewer",
            stub(TimestampType::LastViewed, TimestampLocation::Canvas(0)),
        ),
        (
            "last_viewed_preview_viewer",
            stub(TimestampType::LastViewed, TimestampLocation::Canvas(1)),
        ),
    ];
    let names = |path: &[&str]| path.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match context {
        Context::Export => {
            let mut routers = vec![
                router(
                    vec![new_importer(Kind::MediaNotes, StringProcessor::default())],
                    json_to(&names(&["notes"])),
                ),
                router(
                    vec![new_importer(Kind::MediaUrls, StringProcessor::default())],
                    json_to(&names(&["urls"])),
                ),
            ];
            for (key, name) in &tag_services {
                for (display, word) in [
                    (TagDisplay::Storage, "storage"),
                    (TagDisplay::DisplayActual, "display"),
                ] {
                    routers.push(router(
                        vec![Importer {
                            source: Source::MediaTags {
                                service_key: hex(key.as_bytes()),
                                display,
                            },
                            processor: StringProcessor::default(),
                        }],
                        json_to(&["tags".to_owned(), word.to_owned(), name.clone()]),
                    ));
                }
            }
            for (name, stub) in times {
                routers.push(router(
                    vec![Importer {
                        source: Source::MediaTimestamp(stub),
                        processor: StringProcessor::default(),
                    }],
                    json_to(&names(&["times", name])),
                ));
            }
            vec![(
                "easy one-click JSON that covers the basics",
                "this will export all notes, urls, tags, and basic times as they are currently defined",
                routers,
            )]
        }
        Context::Import => {
            let mut routers = vec![
                router(vec![json_at(&["notes"])], new_exporter(Kind::MediaNotes)),
                router(vec![json_at(&["urls"])], Exporter::MediaUrls),
            ];
            for (key, name) in &tag_services {
                routers.push(router(
                    vec![json_at(&["tags", "storage", name])],
                    Exporter::MediaTags {
                        service_key: hex(key.as_bytes()),
                    },
                ));
            }
            for (name, stub) in times {
                routers.push(router(
                    vec![json_at(&["times", name])],
                    Exporter::MediaTimestamp(stub),
                ));
            }
            vec![(
                "easy one-click JSON that covers the basics",
                "this will import all notes, urls, tags, and basic times as they are currently defined. it will match the easy-export JSON and if tag service names match up, tags will work too",
                routers,
            )]
        }
    }
}

/// What deleting from a list asks first (`AddEditDeleteListBox._Delete`).
pub fn remove_question(n: usize) -> String {
    format!(
        "Remove {} selected?",
        hydrus_core::numbers::human_int(n as u64)
    )
}
