//! The right-click menus of a gallery or watcher downloader page's list
//! (the reference's `_GetListCtrlMenu` in `ClientGUISidebarImporters`),
//! and presentation options' summaries (`PresentationImportOptions.
//! GetSummary`). Menus are the file log's [`Entry`] trees, the selected
//! importer's file and search logs' whole menus nested in them. Recorded
//! by `oracle/record_importer_menus.py`.

use hydrus_core::import_options::{PresentationInbox, PresentationOptions, PresentationStatus};
use hydrus_core::service::builtin_keys;

use crate::file_log::{self, Entry};
use crate::search_log;

/// What a list menu's item does, to the selected importers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Their queries (a gallery's) to the clipboard.
    CopyQueries,
    /// Their URLs (a watcher's) to the clipboard, or opened.
    CopyUrls,
    OpenUrls,
    CopySubjects,
    /// Their files shown in the page, as these options (or each
    /// importer's own) present them.
    ShowFiles(Option<PresentationOptions>),
    /// The first selected's file log window.
    ShowFileLog,
    /// Its search (or check) log window.
    ShowSearchLog,
    /// An action of the one selected's file log's whole menu.
    FileLog(file_log::Action),
    SearchLog(search_log::Action),
    Remove,
    PausePlayFiles,
    /// Their searches (or a watcher's checking).
    PausePlaySearch,
    RetryFailed,
    RetryIgnored,
}

/// What the menus need to know of the one selected importer.
#[derive(Debug, Clone, Default)]
pub struct Single {
    /// Its own presentation options, if not the default's.
    pub presentation: Option<PresentationOptions>,
    pub files: file_log::LogFacts,
    pub searches: search_log::LogFacts,
}

/// What the menus need to know of the selection.
#[derive(Debug, Clone, Default)]
pub struct Selected {
    pub count: usize,
    /// With one selected, it.
    pub single: Option<Single>,
    /// Whether any selected has failed, or ignored, files (a watcher
    /// list's retry entries).
    pub any_failed: bool,
    pub any_ignored: bool,
}

/// What presentation options say they show (`GetSummary`).
pub fn presentation_summary(options: &PresentationOptions) -> String {
    use PresentationInbox as I;
    use PresentationStatus as S;
    if options.status == S::None {
        return "not presenting any files".into();
    }
    let mut summary = match (options.status, options.inbox) {
        (S::AnyGood, I::RequireInbox) => "inbox files",
        (_, I::RequireInbox) => "new inbox files",
        (S::NewOnly, I::AndIncludeAllInbox) => "new or inbox files",
        (S::NewOnly, _) => "new files",
        _ => "all files",
    }
    .to_owned();
    let combined = hex::encode(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS);
    let storage = hex::encode(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE);
    if options.location != [combined] {
        if options.location == [storage] {
            summary.push_str(", including if trashed");
        } else {
            summary.push_str(", in another location");
        }
    }
    format!("presenting {summary}")
}

/// The options "show files" offers besides the importers' own
/// (`AddPresentationSubmenu`): new files, inbox files, all files, and all
/// files including the trash.
pub fn offered_presentations() -> Vec<PresentationOptions> {
    let new_only = PresentationOptions {
        status: PresentationStatus::NewOnly,
        ..PresentationOptions::default()
    };
    let inbox = PresentationOptions {
        inbox: PresentationInbox::RequireInbox,
        ..PresentationOptions::default()
    };
    let trashed = PresentationOptions {
        location: vec![hex::encode(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)],
        ..PresentationOptions::default()
    };
    vec![new_only, inbox, PresentationOptions::default(), trashed]
}

fn item(label: impl Into<String>, action: Action) -> Entry<Action> {
    Entry::Item(label.into(), action)
}

/// A menu's entries, their actions wrapped.
fn wrap<A>(entries: Vec<Entry<A>>, f: &dyn Fn(A) -> Action) -> Vec<Entry<Action>> {
    entries
        .into_iter()
        .map(|e| match e {
            Entry::Item(label, a) => Entry::Item(label, f(a)),
            Entry::Label(label) => Entry::Label(label),
            Entry::Separator => Entry::Separator,
            Entry::Menu(label, entries) => Entry::Menu(label, wrap(entries, f)),
        })
        .collect()
}

fn show_files(selected: &Selected) -> Entry<Action> {
    let own = selected
        .single
        .as_ref()
        .and_then(|s| s.presentation.clone());
    let mut entries = vec![match &own {
        Some(o) => item(
            format!("default presented files ({})", presentation_summary(o)),
            Action::ShowFiles(None),
        ),
        None => item("default presented files", Action::ShowFiles(None)),
    }];
    for options in offered_presentations() {
        if own.as_ref() == Some(&options) {
            continue;
        }
        entries.push(item(
            presentation_summary(&options),
            Action::ShowFiles(Some(options)),
        ));
    }
    Entry::Menu("show files".into(), entries)
}

/// The logs' entries: with one selected, its logs' submenus; with more,
/// "show" items. `kind` is "search" or "check".
fn logs(selected: &Selected, kind: &str, menu: &mut Vec<Entry<Action>>) {
    if let Some(single) = &selected.single {
        let mut files = vec![item("show file log", Action::ShowFileLog), Entry::Separator];
        files.extend(wrap(
            file_log::log_menu(&single.files, false),
            &Action::FileLog,
        ));
        menu.push(Entry::Menu("file log".into(), files));
        let mut searches = vec![
            item(format!("show {kind} log"), Action::ShowSearchLog),
            Entry::Separator,
        ];
        searches.extend(wrap(
            search_log::log_menu(&single.searches, false),
            &Action::SearchLog,
        ));
        menu.push(Entry::Menu(format!("{kind} log"), file_log::tidy(searches)));
    } else {
        menu.push(item("show file logs", Action::ShowFileLog));
        menu.push(item(format!("show {kind} log"), Action::ShowSearchLog));
    }
}

/// A gallery downloader's list menu; none with nothing selected.
pub fn gallery_menu(selected: &Selected) -> Vec<Entry<Action>> {
    if selected.count == 0 {
        return Vec::new();
    }
    let mut menu = vec![
        item("copy queries", Action::CopyQueries),
        Entry::Separator,
        show_files(selected),
        Entry::Separator,
    ];
    logs(selected, "search", &mut menu);
    menu.extend([
        Entry::Separator,
        item("remove", Action::Remove),
        Entry::Separator,
        item("pause/play files", Action::PausePlayFiles),
        item("pause/play search", Action::PausePlaySearch),
    ]);
    file_log::tidy(menu)
}

/// A watcher downloader's list menu; none with nothing selected.
pub fn watcher_menu(selected: &Selected) -> Vec<Entry<Action>> {
    if selected.count == 0 {
        return Vec::new();
    }
    let mut menu = vec![
        item("copy urls", Action::CopyUrls),
        item("open urls", Action::OpenUrls),
        Entry::Separator,
        item("copy subjects", Action::CopySubjects),
        Entry::Separator,
        show_files(selected),
        Entry::Separator,
    ];
    logs(selected, "check", &mut menu);
    if selected.any_failed || selected.any_ignored {
        menu.push(Entry::Separator);
        if selected.any_failed {
            menu.push(item("retry failed", Action::RetryFailed));
        }
        if selected.any_ignored {
            menu.push(item("retry ignored", Action::RetryIgnored));
        }
    }
    menu.extend([
        Entry::Separator,
        item("remove selected", Action::Remove),
        Entry::Separator,
        item("pause/play files", Action::PausePlayFiles),
        item("pause/play checking", Action::PausePlaySearch),
    ]);
    file_log::tidy(menu)
}
