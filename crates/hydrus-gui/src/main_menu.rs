//! The main window's menu bar, as the reference's (`FrameGUI`'s
//! `_InitialiseMenubar`, its `_InitialiseMenuInfo*` menus and the updaters
//! that fill them): its menus and entries as hydrus shows them, from what
//! the client knows at the time ([`Facts`]). An entry does what hydrus's
//! does where hydrus-rs can ([`Command`]); the others are shown greyed out
//! until it can. Left out (DIFFERENCES.md): help > debug, hydrus's own
//! debugging tools, and "about Qt"; the services menu's "administrate",
//! for repository admins; the database menu's backup entries as hydrus has
//! them for a database across several locations.

use hydrus_core::numbers::human_int;
use hydrus_core::pages::PageKey;
use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_store::Store;
use hydrus_store::file_maintenance::FileMaintenanceSettings;
use hydrus_store::settings::{FolderSettings, Pauses};

use crate::page_chooser::NewPage;

/// A menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// What it does (none: hydrus-rs can't yet), and whether hydrus
    /// enables it.
    Item {
        label: String,
        command: Option<Command>,
        enabled: bool,
    },
    /// An entry ticked or not.
    Check {
        label: String,
        command: Option<Command>,
        checked: bool,
    },
    Menu {
        label: String,
        entries: Vec<Entry>,
        enabled: bool,
    },
    Separator,
}

impl Entry {
    pub fn label(&self) -> &str {
        match self {
            Entry::Item { label, .. } | Entry::Check { label, .. } | Entry::Menu { label, .. } => {
                label
            }
            Entry::Separator => "",
        }
    }

    /// Whether it can be chosen: hydrus enables it and hydrus-rs does it
    /// (a submenu, whether it opens).
    pub fn usable(&self) -> bool {
        match self {
            Entry::Item {
                command, enabled, ..
            } => *enabled && command.is_some(),
            Entry::Check { command, .. } => command.is_some(),
            Entry::Menu { enabled, .. } => *enabled,
            Entry::Separator => false,
        }
    }
}

/// A global pause switch (network > pause, and file > import/export
/// folders > pause).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    ImportFolders,
    ExportFolders,
    NetworkTraffic,
    Subscriptions,
    PagedImporters,
    FileQueues,
    GallerySearches,
    WatcherCheckers,
}

impl Pause {
    /// Whether it is on.
    pub fn get(self, facts: &Facts) -> bool {
        let pauses = &facts.pauses;
        match self {
            Pause::ImportFolders => facts.folders.pause_import_folders,
            Pause::ExportFolders => facts.folders.pause_export_folders,
            Pause::NetworkTraffic => pauses.network_traffic,
            Pause::Subscriptions => pauses.subscriptions,
            Pause::PagedImporters => pauses.paged_importers,
            Pause::FileQueues => pauses.file_queues,
            Pause::GallerySearches => pauses.gallery_searches,
            Pause::WatcherCheckers => pauses.watcher_checkers,
        }
    }

    /// Its switch among the store's settings.
    pub fn field<'a>(
        self,
        pauses: &'a mut Pauses,
        folders: &'a mut FolderSettings,
    ) -> &'a mut bool {
        match self {
            Pause::ImportFolders => &mut folders.pause_import_folders,
            Pause::ExportFolders => &mut folders.pause_export_folders,
            Pause::NetworkTraffic => &mut pauses.network_traffic,
            Pause::Subscriptions => &mut pauses.subscriptions,
            Pause::PagedImporters => &mut pauses.paged_importers,
            Pause::FileQueues => &mut pauses.file_queues,
            Pause::GallerySearches => &mut pauses.gallery_searches,
            Pause::WatcherCheckers => &mut pauses.watcher_checkers,
        }
    }
}

/// What an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Copy a label's text (the reference's `AppendMenuLabel`).
    Copy(String),
    /// Switch a pause on or off.
    Pause(Pause),
    /// Check an import folder now (none: all of them).
    CheckImportFolder(Option<String>),
    /// Run an export folder now (none: all of them).
    RunExportFolder(Option<String>),
    OpenInstallDirectory,
    OpenDatabaseDirectory,
    Exit,
    /// Forget the closed pages, asking first.
    ClearClosedPages,
    /// Reopen a closed page (by its place among them, oldest first).
    Unclose(usize),
    /// Show a page from the history.
    ShowPage(PageKey),
    ClearHistory,
    Refresh,
    /// Add this session's pages to those open.
    AppendSession(String),
    /// Delete this saved session, asking first.
    DeleteSession(String),
    /// Choose a new page.
    ChooseNewPage,
    NewPage(NewPage),
    ClearWatcherHighlights,
    /// Switch file maintenance during idle (false: normal) time.
    FileMaintenance(bool),
    /// Forget a repository's pending content, asking first.
    ForgetPending(ServiceKey),
    OpenUrl(&'static str),
    AdvancedMode,
    /// Open the options window.
    Options,
    /// Open the "review files to import" window.
    ImportFiles,
}

/// A repository with content to upload (the pending menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub service: ServiceKey,
    pub name: String,
    /// What it has to upload, as the reference says it ("4 tag data to
    /// upload, 1 files to petition").
    pub to_go: String,
    /// How many pending and petitioned rows.
    pub count: u64,
}

/// What the menus show, as the reference's updaters read it.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub advanced: bool,
    pub folders: FolderSettings,
    /// The import and export folders' names.
    pub import_folders: Vec<String>,
    pub export_folders: Vec<String>,
    /// The closed pages' names for menus, oldest first.
    pub closed_pages: Vec<String>,
    /// The pages open, and their weight (files, and twenty a download).
    pub page_count: usize,
    pub session_weight: u64,
    /// The pages shown, the latest last; none before any has been.
    pub history: Option<Vec<(PageKey, String)>>,
    /// The saved sessions' names, a-z.
    pub sessions: Vec<String>,
    /// File search pages offered: the local file domains, the trash and
    /// the file repositories.
    pub search_domains: Vec<(ServiceKey, String)>,
    pub maintenance: FileMaintenanceSettings,
    pub pauses: Pauses,
    /// The repositories, and their pending content (none: no repositories).
    pub pending: Option<Vec<Pending>>,
}

impl Facts {
    /// What the store says, as the reference's updaters read it from the
    /// database and options; the pages' facts are left for the window.
    pub fn from_store(store: &Store) -> hydrus_store::Result<Facts> {
        use hydrus_store::settings;
        let snapshot = store.snapshot();
        let services = &snapshot.services;
        let trash = builtin_keys::TRASH;
        // (each kind by name, as the reference's services manager sorts
        // its services)
        let by_name = |service_type: ServiceType| -> Vec<(ServiceKey, String)> {
            let mut found: Vec<(ServiceKey, String)> = services
                .of_type(service_type)
                .filter(|s| s.key.as_bytes() != trash)
                .map(|s| (s.key.clone(), s.name.clone()))
                .collect();
            found.sort_by_key(|(_, name)| name.to_lowercase());
            found
        };
        let mut search_domains = by_name(ServiceType::LocalFileDomain);
        if let Ok(service) = services.builtin(trash) {
            search_domains.push((service.key.clone(), service.name.clone()));
        }
        search_domains.extend(by_name(ServiceType::FileRepository));
        store.read(|conn| {
            let mut import_folders: Vec<String> =
                hydrus_store::import_folders::import_folders(conn)?
                    .iter()
                    .map(|f| f.name().to_owned())
                    .collect();
            import_folders.sort();
            let settings::ExportFolders(export) = settings::get(conn)?;
            let mut export_folders: Vec<String> = export.into_iter().map(|f| f.name).collect();
            export_folders.sort();
            let settings::AdvancedMode(advanced) = settings::get(conn)?;
            let pending = hydrus_store::pending::counts(conn, services)?;
            let pending = (!pending.is_empty()).then(|| {
                pending
                    .into_iter()
                    .filter_map(|(key, counts)| {
                        let service = services.by_key(&key).ok()?;
                        Some(pending_of(service, &counts))
                    })
                    .collect()
            });
            Ok(Facts {
                advanced,
                folders: settings::get(conn)?,
                import_folders,
                export_folders,
                sessions: hydrus_store::sessions::names(conn)?
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect(),
                search_domains,
                maintenance: settings::get(conn)?,
                pauses: settings::get(conn)?,
                pending,
                ..Facts::default()
            })
        })
    }
}

/// A repository's pending content as the pending menu's updater words it.
fn pending_of(service: &hydrus_store::services::Service, counts: &[(&str, i64)]) -> Pending {
    let sum = |prefix: &str| -> u64 {
        counts
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(_, n)| u64::try_from(*n).unwrap_or(0))
            .sum()
    };
    let (pending_phrase, petitioned_phrase) = match service.service_type() {
        ServiceType::TagRepository => ("tag data to upload", "tag data to petition"),
        ServiceType::FileRepository => ("files to upload", "files to petition"),
        _ => ("files to pin", "files to unpin"),
    };
    let (pending, petitioned) = (sum("pending_"), sum("petitioned_"));
    let mut said = Vec::new();
    if pending > 0 {
        said.push(format!("{} {pending_phrase}", human_int(pending)));
    }
    if petitioned > 0 {
        said.push(format!("{} {petitioned_phrase}", human_int(petitioned)));
    }
    Pending {
        service: service.key.clone(),
        name: service.name.clone(),
        to_go: said.join(", "),
        count: pending + petitioned,
    }
}

/// The menu bar's menus: each a [`Entry::Menu`] titled with its `&`
/// mnemonic, as the reference's.
pub fn menubar(facts: &Facts) -> Vec<Entry> {
    let mut menus = vec![
        file_menu(facts),
        undo_menu(facts),
        pages_menu(facts),
        database_menu(facts),
        network_menu(facts),
        services_menu(),
        tags_menu(),
    ];
    if let Some(pending) = &facts.pending {
        menus.push(pending_menu(pending));
    }
    menus.push(help_menu(facts));
    menus
}

fn item(label: impl Into<String>, command: Command) -> Entry {
    Entry::Item {
        label: label.into(),
        command: Some(command),
        enabled: true,
    }
}

/// An entry hydrus-rs can't do yet.
fn todo(label: impl Into<String>) -> Entry {
    Entry::Item {
        label: label.into(),
        command: None,
        enabled: true,
    }
}

/// A label that copies itself.
fn copy_label(label: impl Into<String>) -> Entry {
    let label = label.into();
    Entry::Item {
        command: Some(Command::Copy(label.clone())),
        label,
        enabled: true,
    }
}

/// A label that does nothing (`no_copy`).
fn plain_label(label: impl Into<String>) -> Entry {
    Entry::Item {
        label: label.into(),
        command: None,
        enabled: true,
    }
}

fn check(label: &str, command: Option<Command>, checked: bool) -> Entry {
    Entry::Check {
        label: label.into(),
        command,
        checked,
    }
}

fn pause(label: &str, pause: Pause, facts: &Facts) -> Entry {
    check(label, Some(Command::Pause(pause)), pause.get(facts))
}

fn menu(label: impl Into<String>, entries: Vec<Entry>) -> Entry {
    Entry::Menu {
        label: label.into(),
        entries,
        enabled: true,
    }
}

const SEP: Entry = Entry::Separator;

const ELLIPSIS: &str = "\u{2026}";

fn dots(label: &str) -> String {
    format!("{label}{ELLIPSIS}")
}

/// `_InitialiseMenuInfoFile`, as its updater fills it.
fn file_menu(facts: &Facts) -> Entry {
    let mut folders = vec![
        menu(
            "pause",
            vec![
                pause("import folders", Pause::ImportFolders, facts),
                pause("export folders", Pause::ExportFolders, facts),
            ],
        ),
        SEP,
    ];
    if !facts.import_folders.is_empty() {
        let mut entries = Vec::new();
        if facts.import_folders.len() > 1 {
            entries.push(item("check all", Command::CheckImportFolder(None)));
            entries.push(SEP);
        }
        for name in &facts.import_folders {
            entries.push(item(
                name.clone(),
                Command::CheckImportFolder(Some(name.clone())),
            ));
        }
        folders.push(menu("check import folder now", entries));
    }
    if !facts.export_folders.is_empty() {
        let mut entries = Vec::new();
        if facts.export_folders.len() > 1 {
            entries.push(item("run all", Command::RunExportFolder(None)));
            entries.push(SEP);
        }
        for name in &facts.export_folders {
            entries.push(item(
                name.clone(),
                Command::RunExportFolder(Some(name.clone())),
            ));
        }
        folders.push(menu("run export folder now", entries));
    }
    folders.extend([
        SEP,
        todo(dots("manage import folders")),
        todo(dots("manage export folders")),
    ]);
    // (no system tray to minimise to; and "restart" is offered, as hydrus
    // does when it isn't a frozen Linux build)
    menu(
        "&file",
        vec![
            item(dots("import files"), Command::ImportFiles),
            SEP,
            menu("import/export folders", folders),
            SEP,
            menu(
                "open",
                vec![
                    item("installation directory", Command::OpenInstallDirectory),
                    item("database directory", Command::OpenDatabaseDirectory),
                    todo("quick export directory"),
                ],
            ),
            SEP,
            item(dots("options"), Command::Options),
            SEP,
            todo("restart"),
            todo("exit/force maintenance"),
            item("exit", Command::Exit),
        ],
    )
}

/// `_InitialiseMenuInfoUndo`, as its updater fills it: with nothing to
/// undo (hydrus-rs has no undo manager or search history yet), only the
/// closed pages, the latest first.
fn undo_menu(facts: &Facts) -> Entry {
    if facts.closed_pages.is_empty() {
        // (as hydrus leaves it: disabled, never filled)
        return Entry::Menu {
            label: "&undo".into(),
            entries: vec![
                todo("initialising"),
                todo("initialising"),
                SEP,
                Entry::Menu {
                    label: "closed pages".into(),
                    entries: Vec::new(),
                    enabled: false,
                },
                Entry::Menu {
                    label: "searching".into(),
                    entries: Vec::new(),
                    enabled: false,
                },
            ],
            enabled: false,
        };
    }
    let mut closed = vec![item(dots("clear all"), Command::ClearClosedPages), SEP];
    for (index, name) in facts.closed_pages.iter().enumerate().rev() {
        closed.push(item(name.clone(), Command::Unclose(index)));
    }
    menu("&undo", vec![menu("closed pages", closed)])
}

/// `_InitialiseMenuInfoPages`, as its updaters and `aboutToShow` fill it.
fn pages_menu(facts: &Facts) -> Entry {
    let weight = menu(
        "weight",
        vec![
            copy_label(format!("{} pages open", human_int(facts.page_count as u64))),
            todo(format!(
                "total session weight: {}",
                human_int(facts.session_weight)
            )),
        ],
    );
    let history = match &facts.history {
        None => vec![plain_label("no tab history")],
        Some(pages) => {
            let mut entries: Vec<Entry> = pages
                .iter()
                .rev()
                .enumerate()
                // (page_nav_history_max_entries)
                .take(20)
                .map(|(i, (key, name))| item(format!("{}: {name}", i + 1), Command::ShowPage(*key)))
                .collect();
            entries.push(SEP);
            entries.push(item("Clear History", Command::ClearHistory));
            entries
        }
    };
    let sidebar = menu(
        "sidebar",
        vec![
            todo("show/hide sidebar and preview panel"),
            SEP,
            check(
                "save current page's sidebar/preview size on client exit",
                None,
                true,
            ),
            SEP,
            todo("save current page's sidebar/preview size now"),
            SEP,
            todo("restore all pages' sidebar/preview sizes to saved value"),
        ],
    );
    let mut sessions = Vec::new();
    if !facts.sessions.is_empty() {
        // (loading a session in place of the pages, and saving one, are
        // still to come)
        sessions.push(menu(
            "clear and load",
            facts
                .sessions
                .iter()
                .map(|name| todo(name.clone()))
                .collect(),
        ));
        sessions.push(menu(
            "append",
            facts
                .sessions
                .iter()
                .map(|name| item(name.clone(), Command::AppendSession(name.clone())))
                .collect(),
        ));
    }
    let savable: Vec<&String> = facts
        .sessions
        .iter()
        .filter(|name| !RESERVED_SESSION_NAMES.contains(&name.as_str()))
        .collect();
    let mut save: Vec<Entry> = savable.iter().map(|name| todo((*name).clone())).collect();
    save.push(todo(dots("as new session")));
    sessions.push(menu("save", save));
    if !savable.is_empty() {
        sessions.push(menu(
            "delete",
            savable
                .iter()
                .map(|name| item((*name).clone(), Command::DeleteSession((*name).clone())))
                .collect(),
        ));
    }
    let search = facts
        .search_domains
        .iter()
        .map(|(key, name)| {
            let label = if key.as_bytes() == builtin_keys::TRASH {
                "new \"trash\" page".to_owned()
            } else {
                format!("new \"{name}\" search page")
            };
            item(
                label,
                Command::NewPage(NewPage::Search {
                    domain: key.clone(),
                    name: name.clone(),
                }),
            )
        })
        .collect();
    menu(
        "&pages",
        vec![
            weight,
            SEP,
            menu("history", history),
            SEP,
            item("refresh", Command::Refresh),
            SEP,
            sidebar,
            SEP,
            menu("sessions", sessions),
            SEP,
            item(dots("new page"), Command::ChooseNewPage),
            menu("file search", search),
            // (petition pages, for repository moderators, are left out)
            menu(
                "download",
                vec![
                    item("new url download page", Command::NewPage(NewPage::Urls)),
                    item("new watcher page", Command::NewPage(NewPage::Watcher)),
                    item("new gallery page", Command::NewPage(NewPage::Gallery)),
                    todo("new simple downloader page"),
                ],
            ),
            menu(
                "special",
                vec![
                    item("new page of pages", Command::NewPage(NewPage::Pages)),
                    item(
                        "new duplicates processing page",
                        Command::NewPage(NewPage::Duplicates),
                    ),
                ],
            ),
            SEP,
            menu(
                "clear",
                vec![item(
                    "all multiwatcher highlights",
                    Command::ClearWatcherHighlights,
                )],
            ),
        ],
    )
}

/// Sessions the client saves itself, which can't be saved over or deleted
/// (`ClientGUISession.RESERVED_SESSION_NAMES`).
pub const RESERVED_SESSION_NAMES: [&str; 2] = ["last session", "exit session"];

/// `_InitialiseMenuInfoDatabase`, as its updater fills it for a database
/// in its default location with no backup location set.
fn database_menu(facts: &Facts) -> Entry {
    let all_todo = |labels: &[&str]| -> Vec<Entry> {
        labels
            .iter()
            .map(|l| if *l == "---" { SEP } else { todo(*l) })
            .collect()
    };
    menu(
        "&database",
        vec![
            menu(
                "backup",
                vec![
                    todo(dots("set up a database backup location")),
                    SEP,
                    todo(dots("restore from a database backup")),
                ],
            ),
            SEP,
            todo(dots("locations")),
            SEP,
            todo("how boned am I?"),
            todo("view file history"),
            SEP,
            menu(
                "file maintenance",
                vec![
                    todo(dots("manage scheduled jobs")),
                    SEP,
                    check(
                        "work file jobs during idle time",
                        Some(Command::FileMaintenance(true)),
                        facts.maintenance.during_idle,
                    ),
                    check(
                        "work file jobs during normal time",
                        Some(Command::FileMaintenance(false)),
                        facts.maintenance.during_active,
                    ),
                    SEP,
                    todo(dots("clear orphan files")),
                    SEP,
                    todo(dots("fix missing file archived times")),
                ],
            ),
            menu(
                "db maintenance",
                vec![
                    todo("review deferred delete table data"),
                    SEP,
                    check("work deferred delete jobs during idle time", None, true),
                    check("work deferred delete jobs during normal time", None, true),
                    SEP,
                    todo(dots("analyze")),
                    todo(dots("review vacuum data")),
                    SEP,
                    todo(dots("clear/fix orphan file records")),
                    todo(dots("clear orphan URL mappings")),
                    todo(dots("clear orphan tables")),
                    todo(dots("clear orphan hashed serialisables")),
                    SEP,
                    todo(dots("get tables using definitions")),
                ],
            ),
            menu(
                "check and repair",
                all_todo(&[
                    &dots("fix invalid tags"),
                    &dots("fix logically inconsistent mappings"),
                    "---",
                    &dots("repopulate truncated mappings tables"),
                    "---",
                    &dots("resync combined deleted files"),
                    &dots("resync tag mappings cache files"),
                ]),
            ),
            menu(
                "regenerate",
                all_todo(&[
                    &dots("total pending count, in the pending menu"),
                    &dots(
                        "tag storage mappings cache (all, with deferred siblings & parents calculation)",
                    ),
                    &dots("tag storage mappings cache (just pending tags, instant calculation)"),
                    &dots(
                        "tag display mappings cache (all, deferred siblings & parents calculation)",
                    ),
                    &dots("tag display mappings cache (just pending tags, instant calculation)"),
                    &dots("tag display mappings cache (missing file repopulation)"),
                    &dots("tag siblings lookup cache"),
                    &dots("tag parents lookup cache"),
                    &dots("tag text search cache"),
                    &dots("tag text search cache (subtags repopulation)"),
                    &dots("tag text search cache (searchable subtag maps)"),
                    "---",
                    &dots("local hashes cache"),
                    &dots("local tags cache"),
                    "---",
                    &dots("service info numbers"),
                    &dots("similar files search tree"),
                ]),
            ),
            menu(
                "clear",
                all_todo(&[
                    &dots("clear all file viewing statistics"),
                    &dots("cull file viewing statistics based on current min/max values"),
                ]),
            ),
            SEP,
            todo(dots("set a password")),
        ],
    )
}

/// `_InitialiseMenuInfoNetwork`, as its updater fills it.
fn network_menu(facts: &Facts) -> Entry {
    let mut pauses = vec![
        pause("all new network traffic", Pause::NetworkTraffic, facts),
        check(
            "always boot the client with paused network traffic",
            None,
            false,
        ),
        SEP,
        pause("subscriptions", Pause::Subscriptions, facts),
    ];
    if facts.advanced {
        // (the daemon's subscriptions can't be woken from here yet)
        pauses.push(todo("nudge subscriptions awake"));
    }
    pauses.extend([
        SEP,
        pause("all paged importer work", Pause::PagedImporters, facts),
        SEP,
        pause("paged file importing", Pause::FileQueues, facts),
        pause("paged gallery searching", Pause::GallerySearches, facts),
        pause("paged watcher checking", Pause::WatcherCheckers, facts),
    ]);
    menu(
        "&network",
        vec![
            menu("pause", pauses),
            SEP,
            todo(dots("subscriptions")),
            SEP,
            menu(
                "data",
                vec![
                    todo("review bandwidth usage and edit rules"),
                    todo("review current network jobs"),
                    todo("review session cookies"),
                    todo(dots("manage http headers")),
                ],
            ),
            menu(
                "downloaders",
                vec![
                    todo(dots("import downloaders")),
                    item(
                        "user-run downloader repository",
                        Command::OpenUrl(
                            "https://github.com/CuddleBear92/Hydrus-Presets-and-Scripts",
                        ),
                    ),
                    todo(dots("export downloaders")),
                    SEP,
                    todo(dots("downloader and url display")),
                    menu(
                        "watch clipboard for urls",
                        vec![
                            check("watcher urls", None, false),
                            check("other recognised urls", None, false),
                        ],
                    ),
                    SEP,
                    todo(dots("gallery url generators")),
                    todo(dots("url classes")),
                    todo(dots("parsers")),
                    SEP,
                    todo(dots("url class links")),
                    SEP,
                    todo(dots("LEGACY: lookup scripts")),
                ],
            ),
            menu(
                "logins",
                vec![
                    copy_label("THIS SYSTEM IS LEGACY"),
                    copy_label("TRY TO MIGRATE AWAY FROM IT"),
                    SEP,
                    todo(dots("logins")),
                    SEP,
                    todo(dots("login scripts")),
                ],
            ),
        ],
    )
}

/// `_InitialiseMenuInfoServices` (with no repository to administrate).
fn services_menu() -> Entry {
    menu(
        "&services",
        vec![
            menu(
                "pause",
                vec![check("all repository synchronisation", None, false)],
            ),
            SEP,
            todo("review"),
            todo(dots("edit")),
            SEP,
            menu(
                "advanced",
                vec![todo(dots("import repository update files"))],
            ),
        ],
    )
}

/// `_InitialiseMenuInfoTags`.
fn tags_menu() -> Entry {
    menu(
        "&tags",
        vec![
            todo(dots("migrate")),
            SEP,
            todo(dots("display/search")),
            SEP,
            todo(dots("siblings")),
            todo(dots("parents")),
            menu(
                "advanced",
                vec![todo(dots("manage where tag siblings and parents apply"))],
            ),
            menu(
                "sync",
                vec![
                    todo("review current sibling/parent sync"),
                    SEP,
                    todo("sync now"),
                    SEP,
                    check("sync tag display during idle time", None, true),
                    check("sync tag display during normal time", None, true),
                ],
            ),
        ],
    )
}

/// The pending menu's updater: "pending (n)", with each repository that
/// has content to upload (a-z), enabled when there is any.
fn pending_menu(pending: &[Pending]) -> Entry {
    let mut shown: Vec<&Pending> = pending.iter().filter(|p| p.count > 0).collect();
    shown.sort_by(|a, b| a.name.cmp(&b.name));
    let total: u64 = pending.iter().map(|p| p.count).sum();
    Entry::Menu {
        label: format!("pending ({})", human_int(total)),
        entries: shown
            .into_iter()
            .map(|p| {
                menu(
                    p.name.clone(),
                    vec![
                        plain_label(p.to_go.clone()),
                        SEP,
                        todo("commit"),
                        item("forget", Command::ForgetPending(p.service.clone())),
                    ],
                )
            })
            .collect(),
        enabled: total > 0,
    }
}

/// `_InitialiseMenuInfoHelp`, less its debug menu and "about Qt".
fn help_menu(facts: &Facts) -> Entry {
    let link = |label: &str, url: &'static str| item(label, Command::OpenUrl(url));
    menu(
        "&help",
        vec![
            link("open help", "https://hydrusnetwork.github.io/hydrus/"),
            menu(
                "links",
                vec![
                    link("site", "https://hydrusnetwork.github.io/hydrus/"),
                    link(
                        "github repository",
                        "https://github.com/hydrusnetwork/hydrus",
                    ),
                    link(
                        "latest build",
                        "https://github.com/hydrusnetwork/hydrus/releases/latest",
                    ),
                    link(
                        "issue tracker",
                        "https://github.com/hydrusnetwork/hydrus/issues",
                    ),
                    link(
                        "8chan.moe /t/ (Hydrus Network General)",
                        "https://8chan.moe/t/catalog.html",
                    ),
                    link("x", "https://x.com/hydrusnetwork"),
                    link("tumblr", "https://hydrus.tumblr.com/"),
                    link("discord", "https://discord.gg/wPHPCUZ"),
                    link("patreon", "https://www.patreon.com/hydrus_dev"),
                ],
            ),
            link(
                "changelog",
                "https://hydrusnetwork.github.io/hydrus/changelog.html",
            ),
            SEP,
            todo(dots("add the PTR")),
            SEP,
            check("darkmode", None, false),
            check("advanced mode", Some(Command::AdvancedMode), facts.advanced),
            SEP,
            todo("about"),
        ],
    )
}

/// The entry at `path` (indices from the menu bar down).
pub fn entry_at<'a>(menus: &'a [Entry], path: &[usize]) -> Option<&'a Entry> {
    let (&first, rest) = path.split_first()?;
    let entry = menus.get(first)?;
    if rest.is_empty() {
        return Some(entry);
    }
    match entry {
        Entry::Menu { entries, .. } => entry_at(entries, rest),
        _ => None,
    }
}

/// A menu's entries as they show: separators not first, last, or doubled
/// (as Qt collapses them).
pub fn shown(entries: &[Entry]) -> Vec<&Entry> {
    let mut out: Vec<&Entry> = Vec::new();
    for entry in entries {
        if *entry == Entry::Separator && out.last().is_none_or(|last| **last == Entry::Separator) {
            continue;
        }
        out.push(entry);
    }
    while out.last().is_some_and(|last| **last == Entry::Separator) {
        out.pop();
    }
    out
}

/// A menu bar title without its mnemonic's `&`.
pub fn title(label: &str) -> String {
    label.replacen('&', "", 1)
}

/// A line of an open menu, as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Item,
    Check,
    Separator,
    Menu,
}

/// An open menu as drawn: where it opens, its lines (label, kind, usable,
/// ticked), and the line highlighted.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneView {
    pub x: f32,
    pub y: f32,
    /// With no room on the right, it opens to the left of this.
    pub left: f32,
    pub lines: Vec<(String, LineKind, bool, bool)>,
    pub current: Option<usize>,
}

#[derive(Debug, Clone)]
struct Pane {
    /// The menu's place in the bar's tree.
    path: Vec<usize>,
    /// Each line's entry (separators shown as Qt shows them).
    lines: Vec<usize>,
    x: f32,
    y: f32,
    left: f32,
    current: Option<usize>,
    /// Where the window drew it: its left edge, top and width.
    drawn: Option<(f32, f32, f32)>,
}

/// A menu's lines' heights, and the space around them, as drawn.
pub const LINE_HEIGHT: f32 = 22.0;
pub const SEPARATOR_HEIGHT: f32 = 7.0;
pub const PANE_PADDING: f32 = 2.0;

/// A key the open menus take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
}

/// The menu bar's open menus (the reference's are Qt's): one opened from
/// the bar, and the submenus opened from it, each beside its entry.
#[derive(Debug, Clone, Default)]
pub struct OpenMenus {
    menus: Vec<Entry>,
    panes: Vec<Pane>,
}

impl OpenMenus {
    /// The bar's menu open, if one is.
    pub fn top(&self) -> Option<usize> {
        self.panes.first().map(|p| p.path[0])
    }

    pub fn is_open(&self) -> bool {
        !self.panes.is_empty()
    }

    /// Open the bar's menu `top` (as `menus` are now) under its title, at
    /// `x`, `y`; a disabled one doesn't open.
    pub fn open(&mut self, menus: Vec<Entry>, top: usize, x: f32, y: f32) {
        self.menus = menus;
        self.panes.clear();
        if self.menus.get(top).is_some_and(Entry::usable) {
            self.push(vec![top], x, y, x);
        }
    }

    pub fn close(&mut self) {
        self.panes.clear();
    }

    fn entries(&self, path: &[usize]) -> &[Entry] {
        match entry_at(&self.menus, path) {
            Some(Entry::Menu { entries, .. }) => entries,
            _ => &[],
        }
    }

    fn push(&mut self, path: Vec<usize>, x: f32, y: f32, left: f32) {
        let entries = self.entries(&path);
        let shown = shown(entries);
        let lines = shown
            .iter()
            .map(|e| {
                entries
                    .iter()
                    .position(|x| std::ptr::eq(x, *e))
                    .expect("shown from these")
            })
            .collect();
        self.panes.push(Pane {
            path,
            lines,
            x,
            y,
            left,
            current: None,
            drawn: None,
        });
    }

    /// Where the window drew a menu.
    pub fn placed(&mut self, pane: usize, x: f32, y: f32, width: f32) {
        if let Some(pane) = self.panes.get_mut(pane) {
            pane.drawn = Some((x, y, width));
        }
    }

    /// Where a submenu opened from a menu's line goes, as the pointer
    /// would open it: beside the line, from its menu's right edge.
    fn beside(&self, pane: usize, line: usize) -> (f32, f32, f32) {
        let me = &self.panes[pane];
        let (x, y, width) = me.drawn.unwrap_or((me.x, me.y, 200.0));
        let above: f32 = me.lines[..line]
            .iter()
            .map(|&i| {
                if self.entries(&me.path)[i] == Entry::Separator {
                    SEPARATOR_HEIGHT
                } else {
                    LINE_HEIGHT
                }
            })
            .sum();
        // (its first line level with this one)
        (x + width, y + above, x)
    }

    fn line(&self, pane: usize, line: usize) -> Option<(Vec<usize>, &Entry)> {
        let pane = self.panes.get(pane)?;
        let index = *pane.lines.get(line)?;
        let mut path = pane.path.clone();
        path.push(index);
        let entry = entry_at(&self.menus, &path)?;
        Some((path, entry))
    }

    /// The pointer is over a line of a menu: it is highlighted, menus
    /// opened from others close, and a submenu opens beside it (`right`
    /// and `top` its line's, `left` its menu's left edge).
    pub fn hover(&mut self, pane: usize, line: usize, right: f32, top: f32, left: f32) {
        let Some((path, entry)) = self.line(pane, line) else {
            return;
        };
        let opens = matches!(entry, Entry::Menu { .. }) && entry.usable();
        let separator = *entry == Entry::Separator;
        if self.panes.get(pane + 1).is_some_and(|p| p.path == path) {
            self.panes[pane].current = Some(line);
            return;
        }
        self.panes.truncate(pane + 1);
        self.panes[pane].current = (!separator).then_some(line);
        if opens {
            self.push(path, right, top, left);
        }
    }

    /// A line is clicked: a submenu opens, and an entry that can be
    /// chosen closes the menus and is what to do.
    pub fn click(
        &mut self,
        pane: usize,
        line: usize,
        right: f32,
        top: f32,
        left: f32,
    ) -> Option<Command> {
        let (_, entry) = self.line(pane, line)?;
        if matches!(entry, Entry::Menu { .. }) {
            self.hover(pane, line, right, top, left);
            return None;
        }
        if !entry.usable() {
            return None;
        }
        let command = match entry {
            Entry::Item { command, .. } | Entry::Check { command, .. } => command.clone(),
            _ => None,
        };
        self.close();
        command
    }

    /// A key, while a menu is open: up and down move the highlight (past
    /// separators, round), right opens the highlighted submenu (or the
    /// bar's next menu) and left closes one (or opens the bar's last
    /// menu), enter chooses, escape closes the last menu opened. Returns
    /// what to do, and which of the bar's menus to open instead.
    pub fn key(&mut self, key: MenuKey) -> (Option<Command>, Option<usize>) {
        let Some(last) = self.panes.len().checked_sub(1) else {
            return (None, None);
        };
        let count = self.panes[last].lines.len();
        let usable = |me: &Self, line: usize| {
            me.line(last, line)
                .is_some_and(|(_, e)| *e != Entry::Separator)
        };
        match key {
            MenuKey::Up | MenuKey::Down => {
                if count == 0 {
                    return (None, None);
                }
                let mut at = self.panes[last].current;
                for _ in 0..count {
                    let next = match (key, at) {
                        (MenuKey::Down, None) => 0,
                        (MenuKey::Down, Some(i)) => (i + 1) % count,
                        (_, None | Some(0)) => count - 1,
                        (_, Some(i)) => i - 1,
                    };
                    at = Some(next);
                    if usable(self, next) {
                        break;
                    }
                }
                self.panes[last].current = at;
                (None, None)
            }
            MenuKey::Right => {
                let opened = self.panes[last].current.and_then(|line| {
                    let (path, entry) = self.line(last, line)?;
                    (matches!(entry, Entry::Menu { .. }) && entry.usable()).then_some((path, line))
                });
                if let Some((path, line)) = opened {
                    let (x, y, left) = self.beside(last, line);
                    self.push(path, x, y, left);
                    let new = self.panes.len() - 1;
                    self.panes[new].current = (0..self.panes[new].lines.len()).find(|&l| {
                        self.line(new, l)
                            .is_some_and(|(_, e)| *e != Entry::Separator)
                    });
                    (None, None)
                } else {
                    (None, self.neighbour(1))
                }
            }
            MenuKey::Left => {
                if last > 0 {
                    self.panes.truncate(last);
                    (None, None)
                } else {
                    (None, self.neighbour(-1))
                }
            }
            MenuKey::Enter => {
                let Some(line) = self.panes[last].current else {
                    return (None, None);
                };
                if self
                    .line(last, line)
                    .is_some_and(|(_, e)| matches!(e, Entry::Menu { .. }))
                {
                    return self.key(MenuKey::Right);
                }
                let (x, y, left) = self.beside(last, line);
                (self.click(last, line, x, y, left), None)
            }
            MenuKey::Escape => {
                self.panes.truncate(last);
                (None, None)
            }
        }
    }

    /// The bar's next usable menu one way, round.
    fn neighbour(&self, step: isize) -> Option<usize> {
        let top = self.top()?;
        let count = self.menus.len() as isize;
        let mut at = top as isize;
        for _ in 0..count {
            at = (at + step).rem_euclid(count);
            if self.menus[at as usize].usable() {
                return Some(at as usize);
            }
        }
        None
    }

    /// The open menus as drawn.
    pub fn view(&self) -> Vec<PaneView> {
        self.panes
            .iter()
            .map(|pane| {
                let entries = self.entries(&pane.path);
                PaneView {
                    x: pane.x,
                    y: pane.y,
                    left: pane.left,
                    lines: pane
                        .lines
                        .iter()
                        .map(|&i| {
                            let entry = &entries[i];
                            let (kind, checked) = match entry {
                                Entry::Item { .. } => (LineKind::Item, false),
                                Entry::Check { checked, .. } => (LineKind::Check, *checked),
                                Entry::Separator => (LineKind::Separator, false),
                                Entry::Menu { .. } => (LineKind::Menu, false),
                            };
                            (entry.label().to_owned(), kind, entry.usable(), checked)
                        })
                        .collect(),
                    current: pane.current,
                }
            })
            .collect()
    }
}

/// The bar's menu a mnemonic opens: alt and the letter after its `&`.
pub fn mnemonic(menus: &[Entry], letter: &str) -> Option<usize> {
    let letter = letter.to_lowercase();
    menus.iter().position(|menu| {
        let label = menu.label();
        label.find('&').is_some_and(|at| {
            label[at + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.to_lowercase().to_string() == letter)
        }) && menu.usable()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            sessions: vec!["exit session".into(), "last session".into()],
            ..Facts::default()
        }
    }

    fn labels(entries: &[Entry]) -> Vec<&str> {
        shown(entries).into_iter().map(Entry::label).collect()
    }

    #[test]
    fn separators_show_as_qt_shows_them() {
        let entries = [SEP, todo("a"), SEP, SEP, todo("b"), SEP];
        assert_eq!(labels(&entries), ["a", "", "b"]);
    }

    #[test]
    fn several_folders_get_a_check_all_and_one_does_not() {
        let mut facts = facts();
        facts.import_folders = vec!["drop box".into()];
        facts.export_folders = vec!["a".into(), "b".into()];
        let menus = menubar(&facts);
        let folders = entry_at(&menus, &[0, 2]).unwrap();
        let Entry::Menu { entries, .. } = folders else {
            panic!()
        };
        let check = entries
            .iter()
            .find(|e| e.label() == "check import folder now")
            .unwrap();
        let Entry::Menu { entries: check, .. } = check else {
            panic!()
        };
        assert_eq!(labels(check), ["drop box"]);
        let run = entries
            .iter()
            .find(|e| e.label() == "run export folder now")
            .unwrap();
        let Entry::Menu { entries: run, .. } = run else {
            panic!()
        };
        assert_eq!(labels(run), ["run all", "", "a", "b"]);
        assert_eq!(
            entry_at(run, &[0]),
            Some(&item("run all", Command::RunExportFolder(None)))
        );
    }

    #[test]
    fn closed_pages_are_offered_latest_first() {
        let mut facts = facts();
        facts.closed_pages = vec!["first".into(), "second".into()];
        let menus = menubar(&facts);
        let undo = &menus[1];
        assert!(undo.usable());
        assert_eq!(
            entry_at(&menus, &[1, 0, 2]),
            Some(&item("second", Command::Unclose(1)))
        );
        assert_eq!(
            entry_at(&menus, &[1, 0, 3]),
            Some(&item("first", Command::Unclose(0)))
        );
        // with none, the menu is disabled
        assert!(!menubar(&Facts::default())[1].usable());
    }

    #[test]
    fn the_history_is_numbered_latest_first_and_kept_to_twenty() {
        let mut facts = facts();
        let keys: Vec<PageKey> = (0..25).map(|_| PageKey::random()).collect();
        facts.history = Some(
            keys.iter()
                .enumerate()
                .map(|(i, k)| (*k, format!("page {i}")))
                .collect(),
        );
        let menus = menubar(&facts);
        let Some(Entry::Menu { entries, .. }) = entry_at(&menus, &[2, 2]) else {
            panic!()
        };
        assert_eq!(entries.len(), 22);
        assert_eq!(entries[0], item("1: page 24", Command::ShowPage(keys[24])));
        assert_eq!(entries[19].label(), "20: page 5");
        assert_eq!(entries[21], item("Clear History", Command::ClearHistory));
    }

    #[test]
    fn saved_sessions_of_its_own_can_be_saved_over_and_deleted() {
        let mut facts = facts();
        facts.sessions = vec!["exit session".into(), "last session".into(), "work".into()];
        let menus = menubar(&facts);
        let Some(Entry::Menu { entries, .. }) = entry_at(&menus, &[2, 8]) else {
            panic!()
        };
        assert_eq!(
            labels(entries),
            ["clear and load", "append", "save", "delete"]
        );
        let Entry::Menu { entries: save, .. } = &entries[2] else {
            panic!()
        };
        assert_eq!(labels(save), ["work", "as new session\u{2026}"]);
        assert_eq!(save[0], todo("work"));
        let Entry::Menu {
            entries: delete, ..
        } = &entries[3]
        else {
            panic!()
        };
        assert_eq!(
            delete[0],
            item("work", Command::DeleteSession("work".into()))
        );
        // with none saved, only "save" is there
        let menus = menubar(&Facts::default());
        let Some(Entry::Menu { entries, .. }) = entry_at(&menus, &[2, 8]) else {
            panic!()
        };
        assert_eq!(labels(entries), ["save"]);
    }

    #[test]
    fn the_pending_menu_lists_repositories_with_content_a_to_z() {
        let mut facts = facts();
        let repository = |name: &str, count: u64| Pending {
            service: ServiceKey::new(name.as_bytes().to_vec()),
            name: name.into(),
            to_go: format!("{count} files to upload"),
            count,
        };
        facts.pending = Some(vec![
            repository("b", 2),
            repository("empty", 0),
            repository("a", 4),
        ]);
        let menus = menubar(&facts);
        let pending = &menus[7];
        assert_eq!(pending.label(), "pending (6)");
        assert!(pending.usable());
        let Entry::Menu { entries, .. } = pending else {
            panic!()
        };
        assert_eq!(labels(entries), ["a", "b"]);
        // with nothing to upload it is disabled, and with no repositories
        // it isn't there
        facts.pending = Some(vec![repository("empty", 0)]);
        assert!(!menubar(&facts)[7].usable());
        assert_eq!(menubar(&Facts::default())[7].label(), "&help");
    }

    #[test]
    fn advanced_mode_offers_to_nudge_subscriptions() {
        let mut facts = facts();
        let pause_labels = |facts: &Facts| -> Vec<String> {
            let menus = menubar(facts);
            let Some(Entry::Menu { entries, .. }) = entry_at(&menus, &[4, 0]) else {
                panic!()
            };
            entries.iter().map(|e| e.label().to_owned()).collect()
        };
        assert!(!pause_labels(&facts).contains(&"nudge subscriptions awake".to_owned()));
        facts.advanced = true;
        assert!(pause_labels(&facts).contains(&"nudge subscriptions awake".to_owned()));
    }

    fn line_labels(view: &PaneView) -> Vec<&str> {
        view.lines.iter().map(|l| l.0.as_str()).collect()
    }

    #[test]
    fn hovering_opens_submenus_beside_their_entries() {
        let mut open = OpenMenus::default();
        open.open(menubar(&facts()), 0, 10.0, 22.0);
        assert_eq!(open.top(), Some(0));
        let view = open.view();
        assert_eq!(view.len(), 1);
        assert_eq!((view[0].x, view[0].y), (10.0, 22.0));
        assert_eq!(
            line_labels(&view[0]),
            [
                "import files\u{2026}",
                "",
                "import/export folders",
                "",
                "open",
                "",
                "options\u{2026}",
                "",
                "restart",
                "exit/force maintenance",
                "exit"
            ]
        );
        assert_eq!(
            view[0].lines[2],
            (
                "import/export folders".to_owned(),
                LineKind::Menu,
                true,
                false
            )
        );
        assert_eq!(
            view[0].lines[0],
            (
                "import files\u{2026}".to_owned(),
                LineKind::Item,
                true,
                false
            )
        );
        assert_eq!(
            view[0].lines[8],
            ("restart".to_owned(), LineKind::Item, false, false)
        );
        open.hover(0, 2, 150.0, 40.0, 10.0);
        let view = open.view();
        assert_eq!(view.len(), 2);
        assert_eq!((view[1].x, view[1].y, view[1].left), (150.0, 40.0, 10.0));
        assert_eq!(view[0].current, Some(2));
        assert_eq!(line_labels(&view[1])[0], "pause");
        // deeper, then back: another submenu replaces it, an entry closes it
        open.hover(1, 0, 300.0, 40.0, 150.0);
        assert_eq!(open.view().len(), 3);
        assert_eq!(
            open.view()[2].lines[0],
            ("import folders".to_owned(), LineKind::Check, true, false)
        );
        open.hover(1, 0, 300.0, 40.0, 150.0);
        assert_eq!(
            open.view().len(),
            3,
            "hovering the same entry keeps it open"
        );
        open.hover(0, 4, 150.0, 80.0, 10.0);
        assert_eq!(open.view().len(), 2);
        assert_eq!(line_labels(&open.view()[1])[0], "installation directory");
        open.hover(0, 10, 150.0, 200.0, 10.0);
        assert_eq!(open.view().len(), 1);
        // a separator isn't highlighted
        open.hover(0, 1, 150.0, 30.0, 10.0);
        assert_eq!(open.view()[0].current, None);
    }

    #[test]
    fn clicking_an_entry_chooses_it_unless_it_cannot_be() {
        let mut open = OpenMenus::default();
        open.open(menubar(&facts()), 0, 0.0, 22.0);
        assert_eq!(open.click(0, 8, 0.0, 0.0, 0.0), None);
        assert!(open.is_open(), "a greyed out entry does nothing");
        assert_eq!(open.click(0, 4, 100.0, 50.0, 0.0), None);
        assert_eq!(open.view().len(), 2, "a submenu opens");
        assert_eq!(
            open.click(1, 1, 0.0, 0.0, 0.0),
            Some(Command::OpenDatabaseDirectory)
        );
        assert!(!open.is_open());
        // a tick box
        open.open(menubar(&facts()), 4, 0.0, 22.0);
        open.hover(0, 0, 100.0, 22.0, 0.0);
        assert_eq!(
            open.click(1, 0, 0.0, 0.0, 0.0),
            Some(Command::Pause(Pause::NetworkTraffic))
        );
    }

    #[test]
    fn a_disabled_menu_does_not_open() {
        let mut open = OpenMenus::default();
        open.open(menubar(&facts()), 1, 0.0, 22.0);
        assert!(!open.is_open());
        assert_eq!(open.top(), None);
    }

    #[test]
    fn keys_move_through_the_menus() {
        let mut open = OpenMenus::default();
        open.open(menubar(&facts()), 0, 0.0, 22.0);
        let current = |open: &OpenMenus| open.view().last().unwrap().current;
        assert_eq!(open.key(MenuKey::Down), (None, None));
        assert_eq!(current(&open), Some(0));
        open.key(MenuKey::Down);
        assert_eq!(current(&open), Some(2), "past the separator");
        open.key(MenuKey::Up);
        open.key(MenuKey::Up);
        assert_eq!(current(&open), Some(10), "round to the end");
        open.key(MenuKey::Down);
        assert_eq!(current(&open), Some(0), "and back");
        // right opens a submenu beside its line, where the window drew the
        // menu, on its first entry; left closes it
        open.placed(0, 5.0, 22.0, 180.0);
        open.key(MenuKey::Down);
        open.key(MenuKey::Right);
        let view = open.view();
        assert_eq!(view.len(), 2);
        assert_eq!(
            (view[1].x, view[1].y, view[1].left),
            (185.0, 22.0 + LINE_HEIGHT + SEPARATOR_HEIGHT, 5.0)
        );
        assert_eq!(view[1].current, Some(0));
        open.key(MenuKey::Left);
        assert_eq!(open.view().len(), 1);
        // on an entry with no submenu, right and left go to the bar's next
        // menus (past the disabled undo menu)
        open.key(MenuKey::Up);
        assert_eq!(current(&open), Some(0));
        assert_eq!(open.key(MenuKey::Left), (None, Some(7)));
        assert_eq!(open.key(MenuKey::Right), (None, Some(2)));
        // enter chooses; escape closes the last menu opened
        open.key(MenuKey::Up);
        assert_eq!(current(&open), Some(10));
        assert_eq!(open.key(MenuKey::Enter), (Some(Command::Exit), None));
        assert!(!open.is_open());
        open.open(menubar(&facts()), 0, 0.0, 22.0);
        open.key(MenuKey::Escape);
        assert!(!open.is_open());
    }

    #[test]
    fn mnemonics_open_the_bar_s_menus() {
        let menus = menubar(&facts());
        assert_eq!(mnemonic(&menus, "f"), Some(0));
        assert_eq!(mnemonic(&menus, "P"), Some(2));
        assert_eq!(mnemonic(&menus, "h"), Some(7));
        assert_eq!(mnemonic(&menus, "u"), None, "undo is disabled");
        assert_eq!(mnemonic(&menus, "x"), None);
    }

    #[test]
    fn entries_hydrus_rs_cannot_do_are_not_usable() {
        let menus = menubar(&facts());
        let restart = entry_at(&menus, &[0, 8]).unwrap();
        assert_eq!(restart.label(), "restart");
        assert!(!restart.usable());
        let exit = entry_at(&menus, &[0, 10]).unwrap();
        assert_eq!(exit.label(), "exit");
        assert!(exit.usable());
        assert_eq!(title("&file"), "file");
    }
}
