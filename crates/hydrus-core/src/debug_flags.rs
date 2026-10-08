//! The reference's `HydrusGlobals` debug switches that Help > debug flips at
//! runtime (`ClientGUI._SwitchBoolean`): process-wide, not saved, off at
//! boot. A *report mode* is a flag whose sites call [`report`]; the message
//! goes to the sink the GUI installs (a popup and the console), as the
//! reference's `HydrusData.ShowText` does.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

/// A debug switch, named as the reference's menu names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Flag {
    IdleReport,
    ShortcutReport,
    NetworkReport,
    NetworkReportSilent,
    SubprocessReport,
    SubscriptionReport,
    FileImportReport,
    DaemonReport,
    ShutdownReport,
    CacheReport,
    DbReport,
    GuiReport,
    MediaLoadReport,
    FileReport,
    FileSortReport,
    HoverWindowReport,
    PotentialDuplicatesReport,
    SimilarFilesMetadataGenerationReport,
    AutocompleteDelay,
    FakePetition,
    ThumbnailDebug,
    CanvasTileBorders,
    Blurhash,
}

impl Flag {
    pub const ALL: [Flag; 23] = [
        Flag::IdleReport,
        Flag::ShortcutReport,
        Flag::NetworkReport,
        Flag::NetworkReportSilent,
        Flag::SubprocessReport,
        Flag::SubscriptionReport,
        Flag::FileImportReport,
        Flag::DaemonReport,
        Flag::ShutdownReport,
        Flag::CacheReport,
        Flag::DbReport,
        Flag::GuiReport,
        Flag::MediaLoadReport,
        Flag::FileReport,
        Flag::FileSortReport,
        Flag::HoverWindowReport,
        Flag::PotentialDuplicatesReport,
        Flag::SimilarFilesMetadataGenerationReport,
        Flag::AutocompleteDelay,
        Flag::FakePetition,
        Flag::ThumbnailDebug,
        Flag::CanvasTileBorders,
        Flag::Blurhash,
    ];

    /// The menu entry's label.
    pub fn label(self) -> &'static str {
        match self {
            Flag::IdleReport => "idle report mode",
            Flag::ShortcutReport => "shortcut report mode",
            Flag::NetworkReport => "network report mode",
            Flag::NetworkReportSilent => "network report mode (silent)",
            Flag::SubprocessReport => "subprocess report mode",
            Flag::SubscriptionReport => "subscription report mode",
            Flag::FileImportReport => "file import report mode",
            Flag::DaemonReport => "daemon report mode",
            Flag::ShutdownReport => "shutdown report mode",
            Flag::CacheReport => "cache report mode",
            Flag::DbReport => "db report mode",
            Flag::GuiReport => "gui report mode",
            Flag::MediaLoadReport => "media load report mode",
            Flag::FileReport => "file report mode",
            Flag::FileSortReport => "file sort report mode",
            Flag::HoverWindowReport => "hover window report mode",
            Flag::PotentialDuplicatesReport => "potential duplicates report mode",
            Flag::SimilarFilesMetadataGenerationReport => {
                "similar files metadata generation report mode"
            }
            Flag::AutocompleteDelay => "autocomplete delay mode",
            Flag::FakePetition => "fake petition mode",
            Flag::ThumbnailDebug => "thumbnail debug mode",
            Flag::CanvasTileBorders => "canvas tile borders mode",
            Flag::Blurhash => "blurhash mode",
        }
    }

    /// The "report modes" submenu's switches, in the reference's order. Only
    /// switches something in hydrus-rs reads are offered.
    pub const REPORT_MODES: [Flag; 12] = [
        Flag::Blurhash,
        Flag::CacheReport,
        Flag::DaemonReport,
        Flag::FileReport,
        Flag::FileImportReport,
        Flag::IdleReport,
        Flag::NetworkReport,
        Flag::NetworkReportSilent,
        Flag::SimilarFilesMetadataGenerationReport,
        Flag::ShortcutReport,
        Flag::SubprocessReport,
        Flag::SubscriptionReport,
    ];

    fn cell(self) -> &'static AtomicBool {
        static CELLS: [AtomicBool; 23] = [const { AtomicBool::new(false) }; 23];
        &CELLS[Flag::ALL.iter().position(|f| *f == self).unwrap_or(0)]
    }

    pub fn is_on(self) -> bool {
        self.cell().load(Ordering::Relaxed)
    }

    pub fn set(self, on: bool) {
        self.cell().store(on, Ordering::Relaxed);
    }

    /// `HG.x = not HG.x`; the new value.
    pub fn flip(self) -> bool {
        !self.cell().fetch_xor(true, Ordering::Relaxed)
    }
}

type Sink = Box<dyn Fn(&str) + Send + Sync>;
static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Where reports go (`HydrusData.ShowText`); replaces any earlier sink.
pub fn set_sink(sink: Option<Sink>) {
    *SINK.lock().unwrap_or_else(PoisonError::into_inner) = sink;
}

/// Install `sink` unless one is already installed.
pub fn set_sink_if_none(sink: Sink) {
    let mut slot = SINK.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(sink);
    }
}

/// Show `message()` if `flag` is on; the text is only made then.
pub fn report(flag: Flag, message: impl FnOnce() -> String) {
    if !flag.is_on() {
        return;
    }
    let text = message();
    match SINK.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
        Some(sink) => sink(&text),
        None => eprintln!("{text}"),
    }
}

/// Network report mode (`ClientNetworkingFunctions.NetworkReportMode`): the
/// message goes to the sink, or only to the console in silent mode.
pub fn report_network(message: impl FnOnce() -> String) {
    if !Flag::NetworkReport.is_on() {
        return;
    }
    let text = message();
    if Flag::NetworkReportSilent.is_on() {
        eprintln!("{text}");
    } else {
        report(Flag::NetworkReport, || text);
    }
}
