//! The options-system pages against their consumers: each test changes the
//! option in the real options window (the control as the reference shows it),
//! applies, and checks the behaviour the saved value changes.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, OptionRow, OptionsWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

pub(crate) fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// The main window, bound to a fresh store.
pub(crate) struct Client {
    dirs: Vec<tempfile::TempDir>,
    pub(crate) store: Arc<Store>,
    pub(crate) ui: MainWindow,
    pub(crate) bound: hydrus_gui::Bound,
    _windows: hydrus_gui::headless::Windows,
}

pub(crate) fn client() -> Client {
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    Client {
        dirs: dirs.into(),
        store,
        ui,
        bound,
        _windows: windows,
    }
}

impl Client {
    /// file > options…, on `page`.
    pub(crate) fn options(&self, page: &str) -> OptionsWindow {
        let ui = &self.ui;
        ui.invoke_menu_title_pressed(0, 20.0, 22.0);
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let at = (0..lines.row_count())
            .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
            .expect("file > options");
        ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
        let window = self.bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, page);
        window
    }

    /// Use `store` (a fresh one, whose directories are `dirs`) from now on.
    pub(crate) fn use_store(&mut self, dirs: [tempfile::TempDir; 2], store: Arc<Store>) {
        self.dirs.extend(dirs);
        self.store = store;
    }

    pub(crate) fn get<T: hydrus_store::settings::Setting>(&self) -> T {
        self.store.read(hydrus_store::settings::get::<T>).unwrap()
    }
}

fn show_page(options: &OptionsWindow, name: &str) {
    let pages = options.get_pages();
    let i = (0..pages.row_count())
        .position(|i| pages.row_data(i).unwrap().text == name)
        .unwrap_or_else(|| panic!("page {name:?}")) as i32;
    options.set_page(i);
    options.invoke_page_chosen(i);
}

pub(crate) fn row(options: &OptionsWindow, label: &str) -> (i32, OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

/// A checkbox row, set.
pub(crate) fn check(options: &OptionsWindow, label: &str, on: bool) {
    let (i, found) = row(options, label);
    assert_eq!(found.kind, 1, "{label:?} is a checkbox");
    options.invoke_check_toggled(i, on);
}

/// A number row (kind 2): its limits are the reference's.
pub(crate) fn number(options: &OptionsWindow, label: &str, limits: (i32, i32), n: i32) {
    let (i, found) = row(options, label);
    assert_eq!(
        (found.kind, found.minimum, found.maximum),
        (2, limits.0, limits.1),
        "{label:?}"
    );
    options.invoke_number_edited(i, n);
}

/// A number that may be none (kind 3): its none phrase is the reference's.
pub(crate) fn noneable(options: &OptionsWindow, label: &str, phrase: &str, n: Option<i32>) {
    let (i, found) = row(options, label);
    assert_eq!((found.kind, found.none_phrase.as_str()), (3, phrase));
    if let Some(n) = n {
        options.invoke_number_edited(i, n);
    }
    options.invoke_none_toggled(i, n.is_none());
}

/// Text that may be none (kind 7).
fn noneable_text(options: &OptionsWindow, label: &str, text: Option<&str>) {
    let (i, found) = row(options, label);
    assert_eq!((found.kind, found.none_phrase.as_str()), (7, "none"));
    if let Some(text) = text {
        options.invoke_text_edited(i, text.into());
    }
    options.invoke_none_toggled(i, text.is_none());
}

#[test]
fn connection_options_reach_the_running_network_engine() {
    use hydrus_net::{NetEngine, NetOptions};
    use hydrus_store::network::NetworkSettings;

    let client = client();
    let before: NetworkSettings = client.get();
    let engine = NetEngine::new(client.store.clone(), NetOptions::from_settings(&before)).unwrap();
    assert!(!engine.reload_settings().unwrap(), "nothing changed yet");
    let was = engine.options();
    assert!(was.verify_https);
    assert_eq!(
        (&was.http_proxy, &was.https_proxy, was.no_proxy.as_deref()),
        (&None, &None, Some("127.0.0.1")),
        "(the defaults)"
    );

    let window = client.options("connection");
    number(
        &window,
        "max connection attempts allowed per request: ",
        (1, 10),
        7,
    );
    number(&window, "max retries allowed per request: ", (1, 10), 4);
    check(&window, "DEBUG: do not verify regular https traffic:", true);
    noneable_text(&window, "http: ", Some("http://127.0.0.1:8080"));
    noneable_text(&window, "https: ", Some("http://127.0.0.1:8443"));
    noneable_text(&window, "no_proxy: ", Some("localhost,10.0.0.0/8"));
    let (i, errors) = row(
        &window,
        "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
    );
    assert_eq!(
        (
            errors.kind,
            errors.minimum,
            errors.maximum,
            errors.per.as_str()
        ),
        (9, 0, 100, "errors within")
    );
    let units: Vec<String> = errors.fields.iter().map(|f| f.label.to_string()).collect();
    assert_eq!(units.len(), 3, "hours, minutes and seconds: {units:?}");
    window.invoke_number_edited(i, 9);
    window.invoke_field_edited(i, 0, 1);
    window.invoke_field_edited(i, 1, 20);
    window.invoke_field_edited(i, 2, 5);
    // nothing reaches the engine before apply
    assert!(!engine.reload_settings().unwrap());
    window.invoke_apply();

    assert!(
        engine.reload_settings().unwrap(),
        "the engine took the change"
    );
    let now = engine.options();
    assert_eq!(now.max_connection_attempts, 7);
    assert_eq!(now.max_get_attempts, 4);
    assert!(!now.verify_https);
    assert_eq!(now.http_proxy.as_deref(), Some("http://127.0.0.1:8080"));
    assert_eq!(now.https_proxy.as_deref(), Some("http://127.0.0.1:8443"));
    assert_eq!(now.no_proxy.as_deref(), Some("localhost,10.0.0.0/8"));
    assert_eq!(now.domain_error_number, 9);
    assert_eq!(now.domain_error_window, 3600 + 20 * 60 + 5);
    // the untouched ones stay
    assert_eq!(now.network_timeout, was.network_timeout);
    assert_eq!(now.max_jobs, was.max_jobs);

    // none again: the proxies are cleared in the engine too
    let window = client.options("connection");
    check(
        &window,
        "DEBUG: do not verify regular https traffic:",
        false,
    );
    noneable_text(&window, "http: ", None);
    window.invoke_apply();
    assert!(engine.reload_settings().unwrap());
    let cleared = engine.options();
    assert!(cleared.verify_https);
    assert_eq!(cleared.http_proxy, None);
    assert_eq!(
        cleared.https_proxy.as_deref(),
        Some("http://127.0.0.1:8443")
    );
}

/// The file handling options are the process's: tests of them take turns.
static FILE_HANDLING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn local_files(store: &Store) -> Vec<HashId> {
    let storage = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .local_file_storage;
    store
        .read(move |c| {
            let mut q = c.prepare(
                "SELECT hash_id FROM file_domain_current WHERE service_id = ? ORDER BY hash_id",
            )?;
            Ok(q.query_map([storage], |r| r.get(0))?
                .collect::<Result<_, _>>()?)
        })
        .unwrap()
}

// leaf: audit-options-files-and-trash-delete-lock-do-not-permit-archived-files-to-be-deleted-from-the-trash
// leaf: audit-options-files-and-trash-delete-lock-after-duplicate-filter-ensure-deletees-are-inboxed-before-delete
// leaf: audit-options-files-and-trash-delete-lock-in-duplicates-auto-resolution-ensure-deletees-are-inboxed-before-delete
#[test]
fn delete_lock_options_decide_which_archived_files_can_be_deleted_and_reinboxed() {
    use hydrus_store::delete_lock::{DeleteLock, Reinbox};

    let client = client();
    let files = local_files(&client.store);
    assert!(files.len() >= 4);
    let archived = files[..2].to_vec();
    let inbox = files[2..4].to_vec();
    {
        let (a, i) = (archived.clone(), inbox.clone());
        client
            .store
            .write_content(move |w| {
                w.inbox(&i)?;
                w.archive(&a)
            })
            .unwrap();
    }
    let storage = hydrus_store::content::DomainRoles::new(&client.store.snapshot().services)
        .unwrap()
        .local_file_storage;
    let locked = |lock_files: &[HashId]| {
        let lock_files = lock_files.to_vec();
        client
            .store
            .read(move |c| hydrus_store::delete_lock::locked(c, storage, &lock_files))
            .unwrap()
    };
    let all: Vec<HashId> = archived.iter().chain(&inbox).copied().collect();
    assert_eq!(client.get::<DeleteLock>(), DeleteLock::default());
    assert!(locked(&all).is_empty(), "no lock to begin with");
    assert!(!Reinbox::AfterDuplicateFilter.applies(&client.get()));

    let archived_label = "Do not permit archived files to be deleted from the trash: ";
    let dup_label = "After duplicate filter, ensure deletees are inboxed before delete: ";
    let auto_label = "In duplicates auto-resolution, ensure deletees are inboxed before delete: ";

    // the reinbox options alone do nothing without the lock itself
    let window = client.options("files and trash");
    check(&window, dup_label, true);
    check(&window, auto_label, true);
    window.invoke_apply();
    let lock: DeleteLock = client.get();
    assert!(lock.reinbox_after_duplicate_filter && lock.reinbox_in_auto_resolution);
    assert!(!Reinbox::AfterDuplicateFilter.applies(&lock));
    assert!(!Reinbox::InAutoResolution.applies(&lock));
    assert!(locked(&all).is_empty());

    // the lock keeps the archived files, and only those, from deletion
    let window = client.options("files and trash");
    assert!(row(&window, dup_label).1.checked, "shown again as saved");
    check(&window, archived_label, true);
    window.invoke_apply();
    let lock: DeleteLock = client.get();
    assert!(lock.archived);
    assert_eq!(locked(&all), archived);
    assert!(Reinbox::AfterDuplicateFilter.applies(&lock));
    assert!(Reinbox::InAutoResolution.applies(&lock));
    assert!(!Reinbox::Never.applies(&lock));

    // each reinbox option is its own consumer's
    let window = client.options("files and trash");
    check(&window, dup_label, false);
    window.invoke_apply();
    let lock: DeleteLock = client.get();
    assert!(!Reinbox::AfterDuplicateFilter.applies(&lock));
    assert!(Reinbox::InAutoResolution.applies(&lock));
    let window = client.options("files and trash");
    check(&window, auto_label, false);
    check(&window, archived_label, false);
    window.invoke_apply();
    assert!(locked(&all).is_empty(), "the lock lifted");
}

// leaf: audit-options-files-and-trash-advanced-do-not-do-chmod-when-copying-files
#[test]
fn the_chmod_option_reaches_the_process_when_file_handling_is_applied() {
    let _global = FILE_HANDLING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let client = client();
    let label = "ADVANCED: Do not do chmod when copying files";
    hydrus_import::apply_file_handling(&client.store);
    assert!(!hydrus_store::paths::do_not_chmod());
    let window = client.options("files and trash");
    check(&window, label, true);
    window.invoke_apply();
    hydrus_import::apply_file_handling(&client.store);
    assert!(hydrus_store::paths::do_not_chmod());
    let window = client.options("files and trash");
    assert!(row(&window, label).1.checked, "shown again as saved");
    check(&window, label, false);
    window.invoke_apply();
    hydrus_import::apply_file_handling(&client.store);
    assert!(!hydrus_store::paths::do_not_chmod());
}

/// `row`, but the one in the box titled `in_box` (the same label is in
/// several boxes of the downloading page).
pub(crate) fn row_in(options: &OptionsWindow, in_box: &str, label: &str) -> (i32, OptionRow) {
    let rows = options.get_rows();
    let mut current = String::new();
    for i in 0..rows.row_count() {
        let r = rows.row_data(i).unwrap();
        if r.kind == 0 {
            current = r.label.to_string();
        } else if current == in_box && r.label == label {
            return (i as i32, r);
        }
    }
    panic!("{in_box:?} {label:?}")
}

// leaf: audit-options-downloading-gallery-downloader-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
// leaf: audit-options-downloading-subscriptions-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
// leaf: audit-options-downloading-watchers-additional-fixed-time-in-seconds-to-wait-between-watcher-checks
// leaf: audit-options-downloading-gallery-downloader-force-file-downloads-to-occur-quickly-after-post-url-fetches
#[test]
fn downloading_waits_and_post_url_override_reach_the_running_network_engine() {
    use hydrus_net::{NetEngine, NetOptions};

    let client = client();
    let network = client.get();
    let engine = NetEngine::new(client.store.clone(), NetOptions::from_settings(&network)).unwrap();
    let was = engine.bandwidth_settings();
    assert!(was.override_on_file_urls_from_posts);

    let window = client.options("downloading");
    let waits = "Additional fixed time (in seconds) to wait between gallery page fetches:";
    for (in_box, label, n) in [
        ("gallery downloader", waits, 31),
        ("subscriptions", waits, 42),
        (
            "watchers",
            "Additional fixed time (in seconds) to wait between watcher checks:",
            53,
        ),
    ] {
        let (i, found) = row_in(&window, in_box, label);
        assert_eq!(
            (found.kind, found.minimum, found.maximum),
            (2, 1, 3600),
            "{in_box}"
        );
        window.invoke_number_edited(i, n);
    }
    let (i, found) = row_in(
        &window,
        "gallery downloader",
        "Force file downloads to occur quickly after Post URL fetches:",
    );
    assert_eq!(found.kind, 1);
    assert!(found.checked);
    window.invoke_check_toggled(i, false);
    window.invoke_apply();

    assert!(engine.reload_settings().unwrap());
    let now = engine.bandwidth_settings();
    // (each in its own place: galleries, subscriptions, watchers)
    assert_eq!(now.gallery_page_wait_pages, 31);
    assert_eq!(now.gallery_page_wait_subscriptions, 42);
    assert_eq!(now.watcher_page_wait, 53);
    assert!(!now.override_on_file_urls_from_posts);
}

/// A new download page, gallery or watcher, as the page chooser makes one.
fn new_page(ui: &MainWindow, gallery: bool) {
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(if gallery { 6 } else { 4 });
}

// leaf: audit-options-downloading-gallery-downloader-by-default-stop-searching-once-this-many-files-are-found
#[test]
fn the_default_file_limit_is_the_new_gallery_pages_and_searches() {
    let client = client();
    // (a new gallery page's limit for the queries entered in it)
    let limit = |client: &Client| {
        new_page(&client.ui, true);
        let data = client.ui.get_gallery_data();
        (!data.no_limit).then_some(data.file_limit)
    };
    assert_eq!(limit(&client), Some(2000));
    let window = client.options("downloading");
    let label = "By default, stop searching once this many files are found:";
    let (_, found) = row_in(&window, "gallery downloader", label);
    assert_eq!(
        (
            found.kind,
            found.number,
            found.minimum,
            found.maximum,
            found.is_none
        ),
        (3, 2000, 1, 1_000_000, false)
    );
    assert_eq!(found.none_phrase, "no limit");
    let (i, _) = row_in(&window, "gallery downloader", label);
    window.invoke_number_edited(i, 345);
    window.invoke_apply();
    assert_eq!(limit(&client), Some(345));

    let window = client.options("downloading");
    let (i, _) = row_in(&window, "gallery downloader", label);
    window.invoke_none_toggled(i, true);
    window.invoke_apply();
    assert_eq!(limit(&client), None, "no limit");
    let window = client.options("downloading");
    assert!(row_in(&window, "gallery downloader", label).1.is_none);
}

/// A downloader for the gallery pages' queries.
fn add_example_downloader(store: &Store) {
    add_downloader_with_template(store, "https://booru.example/search/%tags%/1");
}

fn add_downloader_with_template(store: &Store, template: &str) {
    let gug = hydrus_core::url::AnyGug::Single(hydrus_core::url::Gug {
        name: "example tag search".into(),
        key: "aa".into(),
        url_template: template.into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "blue_eyes".into(),
    });
    let downloaders = hydrus_parse::Downloaders {
        gugs: hydrus_core::url::Gugs {
            gugs: vec![gug],
            keys_to_display: vec!["aa".into()],
        },
        ..hydrus_parse::Downloaders::default()
    };
    let defaults = hydrus_core::subscriptions::GalleryDefaults {
        file_limit: Some(2000),
        gug: Some(("aa".into(), "example tag search".into())),
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
}

fn gallery_cells(ui: &MainWindow) -> Vec<Vec<String>> {
    let rows = ui.get_gallery_rows();
    (0..rows.row_count())
        .map(|i| {
            let cells = rows.row_data(i).unwrap().cells;
            cells.iter().map(|c| c.to_string()).collect()
        })
        .collect()
}

// leaf: audit-options-downloading-gallery-downloader-if-new-query-entered-and-no-current-highlight-highlight-the-new-query
// leaf: audit-options-downloading-watchers-if-new-watcher-entered-and-no-current-highlight-highlight-the-new-watcher
#[test]
fn new_queries_and_watchers_are_highlighted_only_when_the_options_say() {
    let client = client();
    add_example_downloader(&client.store);
    let query_label = "If new query entered and no current highlight, highlight the new query:";
    let watcher_label =
        "If new watcher entered and no current highlight, highlight the new watcher:";
    let enter = |gallery: bool, text: &str| -> bool {
        new_page(&client.ui, gallery);
        if gallery {
            client.ui.invoke_gallery_queries(text.into());
        } else {
            client.ui.invoke_watcher_urls(text.into());
        }
        let page = client.bound.current.borrow();
        let page = page.borrow();
        if gallery {
            page.gallery().unwrap().state.highlighted.is_some()
        } else {
            page.watchers().unwrap().state.highlighted.is_some()
        }
    };
    assert!(
        enter(true, "blue_eyes"),
        "by default the new query is shown"
    );
    assert!(
        enter(false, "https://booru.example/thread/1"),
        "by default the new watcher is shown"
    );

    let window = client.options("downloading");
    let (i, q) = row_in(&window, "gallery downloader", query_label);
    assert_eq!((q.kind, q.checked), (1, true));
    window.invoke_check_toggled(i, false);
    let (i, w) = row_in(&window, "watchers", watcher_label);
    assert_eq!((w.kind, w.checked), (1, true));
    window.invoke_check_toggled(i, false);
    window.invoke_apply();
    assert!(!enter(true, "red_eyes"));
    assert!(!enter(false, "https://booru.example/thread/2"));

    // each its own: the watchers' stays off when the queries' is on again
    let window = client.options("downloading");
    let (i, _) = row_in(&window, "gallery downloader", query_label);
    window.invoke_check_toggled(i, true);
    window.invoke_apply();
    assert!(enter(true, "green_eyes"));
    assert!(!enter(false, "https://booru.example/thread/3"));
    let window = client.options("downloading");
    let (i, _) = row_in(&window, "watchers", watcher_label);
    window.invoke_check_toggled(i, true);
    window.invoke_apply();
    assert!(enter(false, "https://booru.example/thread/4"));
}

#[test]
fn gallery_list_marks_and_short_summaries_follow_the_misc_options() {
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

    let client = client();
    add_example_downloader(&client.store);
    let seed = |url: &str| NewFileSeed {
        seed_type: SeedType::Url,
        data: url.into(),
        data_for_comparison: url.into(),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    // a page with one query, paused in its files and with a new and a
    // previously deleted file in the log, then the same with all finished
    let make_page = |client: &Client, query: &str| -> Vec<String> {
        new_page(&client.ui, true);
        client.ui.invoke_gallery_queries(query.into());
        let queue = client
            .bound
            .current
            .borrow()
            .borrow()
            .gallery()
            .unwrap()
            .queries[0]
            .queue;
        client
            .store
            .write(move |ctx| {
                queues::add_file_seeds(
                    ctx.conn(),
                    queue,
                    &[
                        seed("https://booru.example/post/1"),
                        seed("https://booru.example/post/2"),
                    ],
                    false,
                    0,
                )?;
                let ids: Vec<i64> = queues::file_seeds(ctx.conn(), queue)?
                    .iter()
                    .map(|s| s.id)
                    .collect();
                queues::set_file_seed_statuses(
                    ctx.conn(),
                    &ids[..1],
                    SeedStatus::SuccessfulAndNew,
                    1,
                )?;
                queues::set_file_seed_statuses(ctx.conn(), &ids[1..], SeedStatus::Deleted, 1)
            })
            .unwrap();
        client.bound.downloader_updates.force();
        (client.bound.sync)();
        client.ui.invoke_gallery_row_clicked(0, false, false);
        gallery_cells(&client.ui).remove(0)
    };
    let first = make_page(&client, "blue_eyes");
    // (the reference's defaults: neither count; every file finished, so the
    // stop character in the files' column)
    assert_eq!(first[5], "2", "{first:?}");
    assert_eq!(first[2], "\u{23F9}", "{first:?}");
    client.ui.invoke_gallery_pause_play(true, false);
    let paused = gallery_cells(&client.ui).remove(0);
    assert_eq!(
        paused[3], "\u{23F8}",
        "the default pause character: {paused:?}"
    );

    let change = |new: bool, deleted: bool| {
        let window = client.options("downloading");
        let (i, pause) = row_in(&window, "misc", "Pause character:");
        assert_eq!(pause.kind, 6);
        window.invoke_text_edited(i, "PAUSED".into());
        let (i, stop) = row_in(&window, "misc", "Stop character:");
        assert_eq!(stop.kind, 6);
        window.invoke_text_edited(i, "STOPPED".into());
        let (i, shown) = row_in(
            &window,
            "misc",
            "Show a 'N' (for 'new') count on short file import summaries:",
        );
        assert_eq!(shown.kind, 1);
        window.invoke_check_toggled(i, new);
        let (i, shown) = row_in(
            &window,
            "misc",
            "Show a 'D' (for 'deleted') count on short file import summaries:",
        );
        assert_eq!(shown.kind, 1);
        window.invoke_check_toggled(i, deleted);
        window.invoke_apply();
    };
    change(true, false);
    let second = make_page(&client, "red_eyes");
    assert_eq!(second[5], "2 - 1N", "{second:?}");
    assert_eq!(second[2], "STOPPED", "{second:?}");
    client.ui.invoke_gallery_pause_play(true, false);
    let paused = gallery_cells(&client.ui).remove(0);
    assert_eq!(paused[3], "PAUSED", "{paused:?}");
    change(false, true);
    let third = make_page(&client, "green_eyes");
    assert_eq!(third[5], "2 - 1D", "{third:?}");
}

/// The URLs a new gallery page makes for `query`.
fn first_page_urls(client: &Client, query: &str) -> Vec<String> {
    new_page(&client.ui, true);
    client.ui.invoke_gallery_queries(query.into());
    let queue = client
        .bound
        .current
        .borrow()
        .borrow()
        .gallery()
        .unwrap()
        .queries[0]
        .queue;
    client
        .store
        .read(move |c| {
            Ok(hydrus_store::queues::gallery_seeds(c, queue)?
                .into_iter()
                .map(|s| s.url)
                .collect())
        })
        .unwrap()
}

#[test]
fn percent_twenty_option_changes_the_gallery_urls_made_from_a_query() {
    let client = client();
    add_example_downloader(&client.store);
    let label = "DEBUG: consider %20 the same as space in downloader query text inputs:";
    assert_eq!(
        first_page_urls(&client, "blue%20eyes"),
        ["https://booru.example/search/blue%20eyes/1"]
    );
    let window = client.options("downloading");
    let (i, found) = row_in(&window, "misc", label);
    assert_eq!((found.kind, found.checked), (1, false));
    window.invoke_check_toggled(i, true);
    window.invoke_apply();
    assert_eq!(
        first_page_urls(&client, "blue%20eyes"),
        ["https://booru.example/search/blue+eyes/1"],
        "a %20 is a space, joined as the downloader joins words"
    );
    // a typed space was always one
    assert_eq!(
        first_page_urls(&client, "blue eyes"),
        ["https://booru.example/search/blue+eyes/1"]
    );
    let window = client.options("downloading");
    assert!(row_in(&window, "misc", label).1.checked);
    let (i, _) = row_in(&window, "misc", label);
    window.invoke_check_toggled(i, false);
    window.invoke_apply();
    assert_eq!(
        first_page_urls(&client, "blue%20eyes"),
        ["https://booru.example/search/blue%20eyes/1"]
    );
}

#[test]
fn leading_double_slash_option_changes_the_gallery_urls_made_from_a_query() {
    let client = client();
    add_downloader_with_template(&client.store, "https://booru.example//search/%tags%/1");
    let label = "DEBUG: remove leading double-slashes from URL paths:";
    let before = first_page_urls(&client, "blue_eyes");
    let window = client.options("downloading");
    let (i, found) = row_in(&window, "misc", label);
    assert_eq!((found.kind, found.checked), (1, false));
    window.invoke_check_toggled(i, true);
    window.invoke_apply();
    let after = first_page_urls(&client, "blue_eyes");
    assert_eq!(before, ["https://booru.example//search/blue_eyes/1"]);
    assert_eq!(after, ["https://booru.example/search/blue_eyes/1"]);
}

// leaf: audit-options-exporting-all-exports-advanced-always-apply-ntfs-filename-rules-to-export-filenames
// leaf: audit-options-exporting-all-exports-advanced-export-filename-length-limit-characters-bytes
// leaf: audit-options-exporting-all-exports-advanced-export-dirname-length-limit-characters-bytes
// leaf: audit-options-exporting-all-exports-advanced-export-path-length-limit-characters-bytes
#[test]
fn export_name_options_shape_the_names_the_export_dialog_previews() {
    use hydrus_gui_model::export_files::preview;

    let client = client();
    let file = local_files(&client.store)[0];
    // (the manual export dialog's names for one file, as a path)
    let name_of = |phrase: &str| -> (String, String) {
        let rows = preview(&client.store, &[file], "/export/here", phrase)
            .unwrap_or_else(|e| panic!("{phrase:?}: {e}"));
        let destination = rows[0].destination.clone();
        let name = destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let dir = destination
            .parent()
            .unwrap()
            .strip_prefix("/export/here")
            .unwrap()
            .to_string_lossy()
            .into_owned();
        (dir, name)
    };
    let long = "x".repeat(150);
    let (dir, name) = name_of(&long);
    assert_eq!(dir, "");
    let ext_len = name.len() - 150;
    assert!(
        name.starts_with(&long) && name[150..].starts_with('.'),
        "{name}"
    );
    let (_, plain) = name_of("a:b|c");
    assert_eq!(&plain[..5], "a:b|c", "(not a Windows filesystem)");
    let deep = format!("{}/leaf", "d".repeat(50));
    assert_eq!(name_of(&deep).0, "d".repeat(50));

    let ntfs = "ADVANCED: Always apply NTFS filename rules to export filenames: ";
    let path_limit = "ADVANCED: Export path length limit (characters/bytes): ";
    let dir_limit = "ADVANCED: Export dirname length limit (characters/bytes): ";
    let name_limit = "ADVANCED: Export filename length limit (characters/bytes): ";

    let window = client.options("exporting");
    let (_, found) = row(&window, ntfs);
    assert_eq!((found.kind, found.checked), (1, false));
    let (_, found) = row(&window, path_limit);
    assert_eq!(
        (
            found.kind,
            found.number,
            found.minimum,
            found.maximum,
            found.is_none
        ),
        (3, 250, 96, 8192, true)
    );
    assert_eq!(found.none_phrase, "let hydrus decide");
    let (_, found) = row(&window, dir_limit);
    assert_eq!(
        (
            found.kind,
            found.number,
            found.minimum,
            found.maximum,
            found.is_none
        ),
        (3, 64, 16, 8192, true)
    );
    assert_eq!(found.none_phrase, "let hydrus decide");
    let (_, found) = row(&window, name_limit);
    assert_eq!(
        (found.kind, found.number, found.minimum, found.maximum),
        (2, 220, 16, 8192)
    );

    // the Windows rules turn the characters it forbids into underscores
    check(&window, ntfs, true);
    window.invoke_apply();
    let (_, windows) = name_of("a:b|c");
    assert_eq!(&windows[..5], "a_b_c");
    let window = client.options("exporting");
    check(&window, ntfs, false);

    // the filename limit counts the extension
    number(&window, name_limit, (16, 8192), 40);
    window.invoke_apply();
    let (_, short) = name_of(&long);
    assert_eq!(short.len(), 40, "{short}");
    assert!(short.starts_with(&"x".repeat(40 - ext_len)));

    // the directory limit shortens each directory the phrase makes
    let window = client.options("exporting");
    noneable(&window, dir_limit, "let hydrus decide", Some(20));
    window.invoke_apply();
    assert_eq!(name_of(&deep).0, "d".repeat(20));

    // the path limit squeezes the filename to fit with the destination
    let window = client.options("exporting");
    number(&window, name_limit, (16, 8192), 220);
    noneable(&window, dir_limit, "let hydrus decide", None);
    noneable(&window, path_limit, "let hydrus decide", Some(96));
    window.invoke_apply();
    let (_, squeezed) = name_of(&long);
    assert!(
        squeezed.len() < 150 + ext_len && squeezed.len() > 40,
        "{squeezed}"
    );
    assert!(
        "/export/here".len() + 1 + squeezed.len() <= 96,
        "{squeezed}"
    );
    // (a directory that leaves the filename under 18 characters is refused)
    assert!(preview(&client.store, &[file], "/export/here", &deep).is_err());
}

/// A time row (kind 8), its seconds and milliseconds set.
fn packet(options: &OptionsWindow, in_box: &str, label: &str, seconds: i32, ms: i32) {
    let (i, found) = row_in(options, in_box, label);
    assert_eq!(found.kind, 8, "{label:?}");
    let units: Vec<String> = found.fields.iter().map(|f| f.label.to_string()).collect();
    assert_eq!(units.len(), 2, "seconds and milliseconds: {units:?}");
    options.invoke_field_edited(i, 0, seconds);
    options.invoke_field_edited(i, 1, ms);
}

/// The background workers' pace (`oracle/fixtures/maintenance_pace.json`).
mod pace {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use serde_json::Value;

    use hydrus_core::HashId;
    use hydrus_store::Store;
    use hydrus_store::duplicates::auto::AutoResolutionSettings;
    use hydrus_store::file_maintenance::{self, FileMaintenanceSettings, JobType};
    use hydrus_store::similar::SimilarFilesSettings;
    use hydrus_store::workers::{self, HeldClock, WorkClock as _};

    pub fn recording() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/fixtures/maintenance_pace.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    pub fn int(saved: &Value, key: &str) -> i32 {
        i32::try_from(saved[key].as_i64().unwrap_or_else(|| panic!("{key}"))).unwrap()
    }

    pub fn flag(saved: &Value, key: &str) -> bool {
        saved[key].as_bool().unwrap_or_else(|| panic!("{key}"))
    }

    /// The settings the workers read, as the reference saved them.
    pub fn assert_saved(store: &Store, saved: &Value) {
        let n = |key| u32::try_from(int(saved, key)).unwrap();
        let similar: SimilarFilesSettings = store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(
            (
                similar.during_idle,
                similar.during_active,
                similar.work_time_ms_idle,
                similar.rest_percentage_idle,
                similar.work_time_ms_active,
                similar.rest_percentage_active
            ),
            (
                flag(saved, "maintain_similar_files_duplicate_pairs_during_idle"),
                flag(
                    saved,
                    "maintain_similar_files_duplicate_pairs_during_active"
                ),
                n("potential_duplicates_search_work_time_ms_idle"),
                n("potential_duplicates_search_rest_percentage_idle"),
                n("potential_duplicates_search_work_time_ms_active"),
                n("potential_duplicates_search_rest_percentage_active"),
            )
        );
        let auto: AutoResolutionSettings = store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(
            (
                auto.during_idle,
                auto.during_active,
                auto.work_time_ms_idle,
                auto.rest_percentage_idle,
                auto.work_time_ms_active,
                auto.rest_percentage_active
            ),
            (
                flag(saved, "duplicates_auto_resolution_during_idle"),
                flag(saved, "duplicates_auto_resolution_during_active"),
                n("duplicates_auto_resolution_work_time_ms_idle"),
                n("duplicates_auto_resolution_rest_percentage_idle"),
                n("duplicates_auto_resolution_work_time_ms_active"),
                n("duplicates_auto_resolution_rest_percentage_active"),
            )
        );
        let files: FileMaintenanceSettings = store.read(hydrus_store::settings::get).unwrap();
        let n = |key| u64::from(n(key));
        assert_eq!(
            files,
            FileMaintenanceSettings {
                during_idle: flag(saved, "file_maintenance_during_idle"),
                during_active: flag(saved, "file_maintenance_during_active"),
                idle_files: n("file_maintenance_idle_throttle_files"),
                idle_seconds: n("file_maintenance_idle_throttle_time_delta"),
                active_files: n("file_maintenance_active_throttle_files"),
                active_seconds: n("file_maintenance_active_throttle_time_delta"),
            }
        );
    }

    fn seconds(value: &Value) -> Duration {
        Duration::from_secs_f64(value.as_f64().unwrap())
    }

    /// Everything a recorded pass waited, in all.
    fn waited(events: &[Value]) -> Duration {
        events
            .iter()
            .filter_map(|e| match e[0].as_str() {
                Some("sleep") => Some(seconds(&e[1])),
                Some("wait") => Some(seconds(&e[2])),
                _ => None,
            })
            .sum()
    }

    /// The packet a recorded pass was given, if it worked.
    fn worked(events: &[Value]) -> Option<Duration> {
        events
            .iter()
            .find(|e| e[0] == "work")
            .map(|e| seconds(&e[1]))
    }

    fn close(native: Duration, reference: Duration, what: &str) {
        let gap = native.abs_diff(reference);
        assert!(
            gap <= Duration::from_millis(1),
            "{what}: {native:?}, the reference {reference:?}"
        );
    }

    /// The GUI's idle state, published as of the clock: idle or not, and if
    /// idle whether it is a good time to start background work.
    fn publish(store: &Store, clock: &HeldClock, idle: bool, good_time: bool) {
        hydrus_store::idle_state::publish_state(store.dir(), idle, good_time, clock.now_ms())
            .unwrap();
    }

    fn good_time(pass: &Value) -> bool {
        pass["good_time"].as_bool().unwrap()
    }

    /// A recorded potential duplicates search or auto-resolution pass, run
    /// through the real step: the work takes the recorded share of its packet.
    pub fn replay_packet(store: &Store, t0_ms: i64, pass: &Value) {
        let what = format!("{pass}");
        let events = pass["events"].as_array().unwrap();
        let idle = pass["idle"].as_bool().unwrap();
        let share = pass["packet"][0].as_f64().unwrap();
        let more = pass["packet"][1].as_bool().unwrap();
        let files = usize::try_from(pass["packet"][2].as_u64().unwrap()).unwrap();
        let clock = HeldClock::at(t0_ms);
        publish(store, &clock, idle, good_time(pass));
        let reference_packet = worked(events);
        let reference_wait = waited(events);
        match pass["worker"].as_str().unwrap() {
            "similar" => {
                // one search call a pass, given the packet: it takes the
                // recorded share of it
                let calls = std::cell::Cell::new(0);
                let wait = workers::similar_files_step(store, &clock, |_, budget| {
                    calls.set(calls.get() + 1);
                    clock.advance(budget.mul_f64(share));
                    Ok(files)
                });
                if reference_packet.is_none() {
                    assert_eq!(calls.get(), 0, "held: {what}");
                    assert_eq!(reference_wait, Duration::from_secs(30), "{what}");
                    assert_eq!(wait, workers::SIMILAR_FILES_HOLD, "{what}");
                } else {
                    assert_eq!(calls.get(), 1, "{what}");
                    close(wait, reference_wait, &what);
                }
            }
            "auto" => {
                let given = std::cell::Cell::new(None);
                let wait = workers::auto_resolution_step(store, &clock, |_, budget| {
                    given.set(Some(budget));
                    clock.advance(budget.mul_f64(share));
                    Ok(more)
                });
                match (reference_packet, given.get()) {
                    (None, None) => {
                        close(wait, reference_wait, &what);
                        assert_eq!(wait, workers::AUTO_RESOLUTION_HOLD);
                    }
                    (Some(reference), Some(native)) => {
                        close(native, reference, &format!("packet: {what}"));
                        if more {
                            close(wait, reference_wait, &what);
                        } else {
                            // the reference rests ten minutes, woken by new
                            // work; the daemon checks every minute instead
                            close(reference_wait, Duration::from_secs(600), &what);
                            assert_eq!(wait, workers::AUTO_RESOLUTION_DONE, "{what}");
                        }
                    }
                    other => panic!("{other:?}: {what}"),
                }
            }
            other => panic!("{other}"),
        }
    }

    /// One trace line: a job (seconds from the start, its weight), a run of
    /// one-second polls, the wait after a batch, or the wait for new jobs.
    #[derive(Debug, PartialEq)]
    enum Line {
        Job(i64, &'static str),
        Polls(u64),
        AfterWork,
        NothingDue,
        GaveUp,
    }

    /// How many jobs of each type are queued.
    fn due_jobs(store: &Store) -> BTreeMap<i64, i64> {
        store
            .read(|conn| {
                let mut q = conn.prepare(
                    "SELECT job_type, COUNT(*) FROM file_maintenance_jobs GROUP BY job_type",
                )?;
                Ok(
                    q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                        .collect::<rusqlite::Result<_>>()?,
                )
            })
            .unwrap()
    }

    fn recorded_trace(events: &[Value]) -> Vec<Line> {
        let mut out = Vec::new();
        for e in events {
            match (e[0].as_str().unwrap(), e[1].as_str()) {
                ("job", _) => {
                    let name = match e[2].as_str().unwrap() {
                        "metadata" => "metadata",
                        "exif" => "exif",
                        "presence" => "presence",
                        other => panic!("{other}"),
                    };
                    out.push(Line::Job(e[1].as_i64().unwrap(), name));
                }
                ("sleeps", _) if e[1].as_f64() == Some(1.0) => {
                    out.push(Line::Polls(e[2].as_u64().unwrap()));
                }
                // (a tenth of a millisecond's pause after each batch)
                ("sleeps", _) => {}
                ("wait", Some("work")) => {
                    assert_eq!(e[2].as_f64(), Some(0.5));
                    out.push(Line::AfterWork);
                }
                ("wait", Some("idle")) => {
                    assert_eq!(e[2].as_f64(), Some(600.0));
                    out.push(Line::NothingDue);
                }
                ("stopped", _) => out.push(Line::GaveUp),
                other => panic!("{other:?}"),
            }
        }
        out
    }

    /// The recorded queue on the store's first files: one job each.
    pub fn queue(store: &Store, jobs: &[Value]) {
        let jobs: Vec<JobType> = jobs
            .iter()
            .map(|j| match j.as_str().unwrap() {
                "metadata" => JobType::FileMetadata,
                "exif" => JobType::HasExif,
                "presence" => JobType::IntegrityPresenceLogOnly,
                other => panic!("{other}"),
            })
            .collect();
        let limit = i64::try_from(jobs.len()).unwrap();
        store
            .write(move |ctx| {
                let files: Vec<HashId> = ctx
                    .conn()
                    .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT ?1")?
                    .query_map([limit], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                assert_eq!(files.len(), jobs.len());
                file_maintenance::cancel_jobs(ctx.conn(), &JobType::ALL)?;
                for (file, job) in files.into_iter().zip(jobs) {
                    file_maintenance::add_jobs(ctx.conn(), &[file], job, 0)?;
                }
                Ok(())
            })
            .unwrap();
    }

    /// A recorded file maintenance run, through the real throttle and the
    /// real maintenance batches, pass after pass on the held clock.
    pub fn replay_files(store: &std::sync::Arc<Store>, recording: &Value, pass: &Value) {
        let what = format!("{pass}");
        let t0 = recording["t0"].as_i64().unwrap();
        queue(store, recording["jobs"].as_array().unwrap());
        let idle = pass["idle"].as_bool().unwrap();
        let expected = recorded_trace(pass["events"].as_array().unwrap());
        let importer =
            hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
        let clock = HeldClock::at(t0 * 1000);
        let throttle = workers::FileMaintenanceThrottle::new(&clock);
        let mut trace = Vec::new();
        // as many polls as the reference waited before it was given up on
        let most_polls: u64 = expected
            .iter()
            .map(|l| if let Line::Polls(n) = l { *n } else { 0 })
            .sum();
        let mut polls = 0;
        loop {
            publish(store, &clock, idle, good_time(pass));
            let before = due_jobs(store);
            let wait = importer.file_maintenance_pass(&throttle, &clock, &|_| {});
            // (the jobs the pass did, in the order the reference ran them)
            let after = due_jobs(store);
            for job in JobType::RUN_ORDER {
                for _ in after.get(&job.code()).copied().unwrap_or(0)
                    ..before.get(&job.code()).copied().unwrap_or(0)
                {
                    trace.push(Line::Job(
                        clock.now_ms() / 1000 - t0,
                        match job {
                            JobType::FileMetadata => "metadata",
                            JobType::HasExif => "exif",
                            JobType::IntegrityPresenceLogOnly => "presence",
                            other => panic!("{other:?}"),
                        },
                    ));
                }
            }
            clock.advance(wait);
            match wait {
                workers::FILE_MAINTENANCE_POLL => {
                    polls += 1;
                    if let Some(Line::Polls(n)) = trace.last_mut() {
                        *n += 1;
                    } else {
                        trace.push(Line::Polls(1));
                    }
                    if polls >= most_polls && expected.last() == Some(&Line::GaveUp) {
                        trace.push(Line::GaveUp);
                        break;
                    }
                }
                workers::FILE_MAINTENANCE_AFTER_WORK => trace.push(Line::AfterWork),
                workers::FILE_MAINTENANCE_NOTHING_DUE => {
                    trace.push(Line::NothingDue);
                    break;
                }
                other => panic!("{other:?}: {what}"),
            }
            assert!(trace.len() < 200, "{trace:?}");
        }
        assert_eq!(trace, expected, "{what}");
    }
}

// leaf: audit-options-maintenance-and-processing-file-maintenance-run-file-maintenance-during-idle-time
// leaf: audit-options-maintenance-and-processing-file-maintenance-run-file-maintenance-during-normal-time
// leaf: audit-options-maintenance-and-processing-file-maintenance-idle-throttle
// leaf: audit-options-maintenance-and-processing-file-maintenance-normal-throttle
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-search-for-potential-duplicates-in-idle-time
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-search-for-potential-duplicates-in-normal-time
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-idle-ideal-work-packet-time
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-idle-rest-time-percentage
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-normal-ideal-work-packet-time
// leaf: audit-options-maintenance-and-processing-potential-duplicates-search-normal-rest-time-percentage
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-work-duplicates-auto-resolution-in-idle-time
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-work-duplicates-auto-resolution-in-normal-time
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-idle-ideal-work-packet-time
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-idle-rest-time-percentage
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-normal-ideal-work-packet-time
// leaf: audit-options-maintenance-and-processing-duplicates-auto-resolution-normal-rest-time-percentage
#[test]
fn background_work_options_set_the_pace_the_workers_take_in_idle_and_normal_time() {
    use pace::{flag, int};

    let recording = pace::recording();
    let client = client();
    let page = "maintenance and processing";
    let (files_box, similar_box, auto_box) = (
        "file maintenance",
        "potential duplicates search",
        "duplicates auto-resolution",
    );
    let (idle, normal, rest_idle, rest_normal) = (
        "\"Idle\" ideal work packet time: ",
        "\"Normal\" ideal work packet time: ",
        "\"Idle\" rest time percentage: ",
        "\"Normal\" rest time percentage: ",
    );
    let t0_ms = recording["t0"].as_i64().unwrap() * 1000;
    for case in recording["cases"].as_array().unwrap() {
        let saved = &case["saved"];
        // typed into the page, as the reference's were, and applied
        let window = client.options(page);
        for (in_box, prefix) in [
            (similar_box, "potential_duplicates_search"),
            (auto_box, "duplicates_auto_resolution"),
        ] {
            for (label, rest_label, time) in
                [(idle, rest_idle, "idle"), (normal, rest_normal, "active")]
            {
                let ms = int(saved, &format!("{prefix}_work_time_ms_{time}"));
                packet(&window, in_box, label, ms / 1000, ms % 1000);
                let (i, found) = row_in(&window, in_box, rest_label);
                assert_eq!((found.kind, found.minimum, found.maximum), (2, 0, 100_000));
                window.invoke_number_edited(
                    i,
                    int(saved, &format!("{prefix}_rest_percentage_{time}")),
                );
            }
        }
        for (label, time) in [("Idle throttle: ", "idle"), ("Normal throttle: ", "active")] {
            let (i, found) = row_in(&window, files_box, label);
            assert_eq!(
                (found.kind, found.minimum, found.maximum, found.per.as_str()),
                (9, 1, 1000, "heavy work units every"),
                "{label:?}"
            );
            let seconds = int(
                saved,
                &format!("file_maintenance_{time}_throttle_time_delta"),
            );
            window.invoke_number_edited(
                i,
                int(saved, &format!("file_maintenance_{time}_throttle_files")),
            );
            window.invoke_field_edited(i, 0, seconds / 60);
            window.invoke_field_edited(i, 1, seconds % 60);
        }
        for (in_box, label, key) in [
            (
                files_box,
                "Run file maintenance during idle time: ",
                "file_maintenance_during_idle",
            ),
            (
                files_box,
                "Run file maintenance during normal time: ",
                "file_maintenance_during_active",
            ),
            (
                similar_box,
                "Search for potential duplicates in \"idle\" time: ",
                "maintain_similar_files_duplicate_pairs_during_idle",
            ),
            (
                similar_box,
                "Search for potential duplicates in \"normal\" time: ",
                "maintain_similar_files_duplicate_pairs_during_active",
            ),
            (
                auto_box,
                "Work duplicates auto-resolution in \"idle\" time: ",
                "duplicates_auto_resolution_during_idle",
            ),
            (
                auto_box,
                "Work duplicates auto-resolution in \"normal\" time: ",
                "duplicates_auto_resolution_during_active",
            ),
        ] {
            let (i, found) = row_in(&window, in_box, label);
            assert_eq!(found.kind, 1, "{label:?}");
            window.invoke_check_toggled(i, flag(saved, key));
        }
        window.invoke_apply();
        pace::assert_saved(&client.store, saved);
        // reopened, the page shows what was saved
        let window = client.options(page);
        let (_, found) = row_in(
            &window,
            files_box,
            "Run file maintenance during idle time: ",
        );
        assert_eq!(found.checked, flag(saved, "file_maintenance_during_idle"));

        // and the workers take that pace, in idle and normal time
        for pass in case["passes"].as_array().unwrap() {
            if pass["worker"] == "files" {
                pace::replay_files(&client.store, &recording, pass);
            } else {
                pace::replay_packet(&client.store, t0_ms, pass);
            }
        }
    }
}

// leaf: audit-options-importing-filetypes-inspect-for-cbz-properties-when-importing-rescanning-zip-files
#[test]
fn the_cbz_option_decides_whether_zips_of_images_are_comic_books() {
    use hydrus_core::Mime;

    let _global = FILE_HANDLING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let client = client();
    let tools = hydrus_media::MediaTools::new();
    let mime_of = |name: &str| {
        tools
            .inspect(&hydrus_testkit::fixture_path(name))
            .unwrap()
            .mime
    };
    let label = "Inspect for .cbz properties when importing/rescanning .zip files:";
    hydrus_import::apply_file_handling(&client.store);
    assert_eq!(mime_of("media/cbz_flat.cbz"), Mime::ApplicationCbz);
    assert_eq!(mime_of("media/cbz_folder.zip"), Mime::ApplicationCbz);
    assert_eq!(mime_of("media/zip_plain.zip"), Mime::ApplicationZip);

    let window = client.options("importing");
    let (i, found) = row(&window, label);
    assert_eq!((found.kind, found.checked), (1, true));
    window.invoke_check_toggled(i, false);
    window.invoke_apply();
    hydrus_import::apply_file_handling(&client.store);
    assert_eq!(mime_of("media/cbz_flat.cbz"), Mime::ApplicationZip);
    assert_eq!(mime_of("media/cbz_folder.zip"), Mime::ApplicationZip);
    assert_eq!(mime_of("media/zip_plain.zip"), Mime::ApplicationZip);

    let window = client.options("importing");
    assert!(!row(&window, label).1.checked, "shown again as saved");
    check(&window, label, true);
    window.invoke_apply();
    hydrus_import::apply_file_handling(&client.store);
    assert_eq!(mime_of("media/cbz_flat.cbz"), Mime::ApplicationCbz);
}

// leaf: audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking
#[test]
fn the_tracking_option_decides_whether_viewing_a_file_is_recorded() {
    use hydrus_core::CanvasType;
    use hydrus_gui_model::viewing_statistics::Tracker;

    let client = client();
    let file = local_files(&client.store)[0];
    let views = || {
        client
            .store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
            .unwrap()
            .into_iter()
            .find(|s| s.canvas == CanvasType::MediaViewer)
            .map_or((0, 0), |s| (s.views, s.viewtime_ms))
    };
    let start = hydrus_core::TimestampMs::now().0;
    let view = |at: i64| {
        let mut tracker = Tracker::new(client.store.clone(), CanvasType::MediaViewer);
        tracker.show(Some(file), at).unwrap();
        tracker.close(at + 5000).unwrap();
    };
    let label = "Enable file viewing statistics tracking?:";
    let was = views();
    view(start);
    let on = views();
    assert_eq!(on.0, was.0 + 1, "tracked by default");
    assert!(on.1 > was.1);

    let window = client.options("file viewing statistics");
    let (i, found) = row(&window, label);
    assert_eq!((found.kind, found.checked), (1, true));
    window.invoke_check_toggled(i, false);
    window.invoke_apply();
    view(start + 10_000);
    assert_eq!(views(), on, "not tracked once switched off");

    let window = client.options("file viewing statistics");
    assert!(!row(&window, label).1.checked, "shown again as saved");
    check(&window, label, true);
    window.invoke_apply();
    view(start + 20_000);
    assert_eq!(views().0, on.0 + 1);
}

// leaf: audit-options-files-and-trash-test-import-local-files-directly-from-source-do-not-copy-to-temp-dir-beforehand
#[tokio::test(flavor = "multi_thread")]
async fn the_direct_import_row_decides_whether_a_local_import_copies_to_a_temp_path_first() {
    use hydrus_core::import_options::ImportOptionsSlice;
    use hydrus_download::{Downloader, QueueRunner};
    use hydrus_import::FileImporter;
    use hydrus_media::MediaTools;
    use hydrus_net::{NetEngine, NetOptions};
    use hydrus_store::queues::{self, LocalImport};
    use hydrus_store::settings::FolderSettings;

    const LABEL: &str =
        "TEST: Import local files directly from source, do not copy to temp dir beforehand.";
    let client = client();
    let store = client.store.clone();
    let net = Arc::new(
        NetEngine::new(
            Arc::clone(&store),
            NetOptions {
                obey_bandwidth: false,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let probe = importer.clone();
    let downloader = Arc::new(Downloader::new(Arc::clone(&store), net, importer).unwrap());
    let worker = QueueRunner::new(downloader, 60);
    let work = tempfile::tempdir().unwrap();
    let place = |name: &str| {
        let to = work.path().join(name);
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../oracle/fixtures/media")
                .join(name),
            &to,
        )
        .unwrap();
        to
    };
    let import = |path: &std::path::Path| {
        let path = path.to_string_lossy().into_owned();
        let queue = store
            .write(move |ctx| {
                queues::create_local_import(
                    ctx.conn(),
                    None,
                    &ImportOptionsSlice::default(),
                    &[(path, None)],
                    &queues::PathTags::new(),
                    LocalImport::default(),
                    0,
                )
            })
            .unwrap();
        worker.start_all().unwrap();
        queue
    };
    let finished = |queue: i64| {
        let store = store.clone();
        async move {
            for _ in 0..400 {
                if store
                    .read(|conn| queues::next_file_seed(conn, queue))
                    .unwrap()
                    .is_none()
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            let seeds = store.read(|conn| queues::file_seeds(conn, queue)).unwrap();
            panic!(
                "the import did not finish: {:?}",
                seeds
                    .iter()
                    .map(|s| (s.status, s.note.clone()))
                    .collect::<Vec<_>>()
            );
        }
    };

    // The row is the opposite of the stored copy flag, and starts unchecked
    // (the reference copies by default).
    let window = client.options("files and trash");
    let (_, shown) = row(&window, LABEL);
    assert_eq!(shown.kind, 1);
    assert!(!shown.checked);
    assert!(client.get::<FolderSettings>().copy_import_files_to_temp_dir);
    // Cancel leaves the setting alone.
    check(&window, LABEL, true);
    window.invoke_cancel();
    assert!(client.get::<FolderSettings>().copy_import_files_to_temp_dir);
    // The default import makes a temp copy.
    let first = import(&place("bmp_24.bmp"));
    finished(first).await;
    assert_eq!(probe.temp_copies_made(), 1);

    // Ticking the row and applying imports directly from the source.
    let window = client.options("files and trash");
    check(&window, LABEL, true);
    window.invoke_apply();
    assert!(!client.get::<FolderSettings>().copy_import_files_to_temp_dir);
    let reopened = client.options("files and trash");
    assert!(row(&reopened, LABEL).1.checked, "shown again as saved");
    reopened.invoke_cancel();
    let second = import(&place("apng_rgba.png"));
    finished(second).await;
    assert_eq!(probe.temp_copies_made(), 1, "no second copy was made");
    let seeds = store.read(|conn| queues::file_seeds(conn, second)).unwrap();
    assert!(seeds[0].status.is_successful(), "{}", seeds[0].note);

    // Unticking it copies again.
    let window = client.options("files and trash");
    check(&window, LABEL, false);
    window.invoke_apply();
    assert!(client.get::<FolderSettings>().copy_import_files_to_temp_dir);
    let third = import(&place("apng_3frames.png"));
    finished(third).await;
    assert_eq!(probe.temp_copies_made(), 2);
}

fn misc_recording() -> serde_json::Value {
    hydrus_testkit::fixture_json("downloading_misc_options.json")
}

/// Tick (or not) the row of the downloading page's misc box and apply.
fn set_misc_checks(client: &Client, rows: &[(&str, bool)]) {
    let window = client.options("downloading");
    for (label, on) in rows {
        let (i, found) = row_in(&window, "misc", label);
        assert_eq!(found.kind, 1, "{label}");
        window.invoke_check_toggled(i, *on);
    }
    window.invoke_apply();
}

/// The short file import summary the gallery list shows, for every mix of
/// statuses the reference's own status text was asked about
/// (oracle/record_downloading_misc_options.py), under each pair of the 'N' and
/// 'D' options set in the real options window. One page holds a search for
/// each mix, built once.
// leaf: audit-options-downloading-misc-show-a-n-for-new-count-on-short-file-import-summaries
// leaf: audit-options-downloading-misc-show-a-d-for-deleted-count-on-short-file-import-summaries
#[test]
fn short_file_import_summaries_are_the_reference_s_for_every_mix_and_option() {
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
    let recorded = misc_recording();
    let client = client();
    add_example_downloader(&client.store);
    // the distinct mixes, in the order recorded
    let mut mixes: Vec<serde_json::Value> = Vec::new();
    for case in recorded["summaries"].as_array().unwrap() {
        if !mixes.contains(&case["mix"]) {
            mixes.push(case["mix"].clone());
        }
    }
    new_page(&client.ui, true);
    let names: Vec<String> = (0..mixes.len()).map(|n| format!("mix_{n}")).collect();
    client.ui.invoke_gallery_queries(names.join("\n").into());
    let queues_made: Vec<(String, i64)> = client
        .bound
        .current
        .borrow()
        .borrow()
        .gallery()
        .unwrap()
        .queries
        .iter()
        .map(|q| (q.query.clone(), q.queue))
        .collect();
    assert_eq!(queues_made.len(), mixes.len(), "{queues_made:?}");
    let status_of = |name: &str| match name {
        "new" => SeedStatus::SuccessfulAndNew,
        "redundant" => SeedStatus::SuccessfulButRedundant,
        "ignored" => SeedStatus::Vetoed,
        "deleted" => SeedStatus::Deleted,
        "failed" => SeedStatus::Error,
        "skipped" => SeedStatus::Skipped,
        _ => SeedStatus::Unknown,
    };
    for (n, mix) in mixes.iter().enumerate() {
        let queue = queues_made
            .iter()
            .find(|(query, _)| *query == names[n])
            .unwrap()
            .1;
        let mix: Vec<(String, usize)> = mix
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, count)| {
                (
                    name.clone(),
                    usize::try_from(count.as_u64().unwrap()).unwrap(),
                )
            })
            .collect();
        client
            .store
            .write(move |ctx| {
                let conn = ctx.conn();
                let mut made = 0;
                for (name, count) in &mix {
                    let news: Vec<NewFileSeed> = (0..*count)
                        .map(|_| {
                            made += 1;
                            let url = format!("https://booru.example/post/{made}");
                            NewFileSeed {
                                seed_type: SeedType::Url,
                                data: url.clone(),
                                data_for_comparison: url,
                                source_time: None,
                                referral_url: None,
                                meta: FileSeedMeta::default(),
                            }
                        })
                        .collect();
                    queues::add_file_seeds(conn, queue, &news, false, 0)?;
                    let status = status_of(name);
                    if status != SeedStatus::Unknown {
                        let ids: Vec<i64> = queues::file_seeds(conn, queue)?
                            .iter()
                            .filter(|s| s.status == SeedStatus::Unknown)
                            .map(|s| s.id)
                            .collect();
                        queues::set_file_seed_statuses(conn, &ids, status, 1)?;
                    }
                }
                Ok(())
            })
            .unwrap();
    }
    let new_label = "Show a 'N' (for 'new') count on short file import summaries:";
    let deleted_label = "Show a 'D' (for 'deleted') count on short file import summaries:";
    let mut checked = 0;
    for (show_new, show_deleted) in [(false, false), (false, true), (true, false), (true, true)] {
        set_misc_checks(
            &client,
            &[(new_label, show_new), (deleted_label, show_deleted)],
        );
        client.bound.downloader_updates.force();
        (client.bound.sync)();
        let rows = gallery_cells(&client.ui);
        for case in recorded["summaries"].as_array().unwrap() {
            if case["show_new"] != show_new || case["show_deleted"] != show_deleted {
                continue;
            }
            let n = mixes.iter().position(|mix| *mix == case["mix"]).unwrap();
            let row = rows
                .iter()
                .find(|row| row[0].trim_start_matches("* ") == names[n])
                .unwrap();
            assert_eq!(row[5], case["text"].as_str().unwrap(), "{case}");
            checked += 1;
        }
    }
    assert_eq!(checked, recorded["summaries"].as_array().unwrap().len());
}

/// The gallery URLs made from queries, in a template with the search in its
/// parameters and in its path, with the "%20 is a space" option off and on in
/// the real options window, as the reference's own generator made them (the
/// first page's URLs: later pages are made from the URL class's index).
// leaf: audit-options-downloading-misc-debug-consider-20-the-same-as-space-in-downloader-query-text-inputs
#[test]
fn gallery_urls_made_from_queries_are_the_reference_s_with_and_without_percent_twenty() {
    let recorded = misc_recording();
    let client = client();
    let label = "DEBUG: consider %20 the same as space in downloader query text inputs:";
    let mut checked = 0;
    for on in [false, true] {
        set_misc_checks(&client, &[(label, on)]);
        for (kind, template) in [
            ("params", "https://booru.example/posts?tags=%tags%"),
            ("path", "https://booru.example/artist/%tags%/list"),
        ] {
            add_downloader_with_template(&client.store, template);
            for case in recorded["gug"].as_array().unwrap() {
                if case["on"] != on || case["template"] != kind {
                    continue;
                }
                let urls = first_page_urls(&client, case["query"].as_str().unwrap());
                assert_eq!(urls, [case["url"].as_str().unwrap()], "{case}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, recorded["gug"].as_array().unwrap().len());
}

/// A URL class for paths of `images/<anything>`, with the "remove leading
/// double slashes" option off and on in the real options window: the URLs it
/// matches and what it makes of them are the reference's own class's.
// leaf: audit-options-downloading-misc-debug-remove-leading-double-slashes-from-url-paths
#[test]
fn url_classes_match_double_slash_urls_as_the_reference_s_do_with_the_option() {
    use hydrus_core::url::{DomainMask, StringMatch, UrlClass, UrlClassSettings};
    let recorded = misc_recording();
    let client = client();
    client
        .store
        .write_and_refresh(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &UrlClassSettings {
                    url_classes: vec![UrlClass {
                        name: "images".into(),
                        key: vec![7],
                        domain_mask: DomainMask::new(
                            vec!["booru.example".into()],
                            Vec::new(),
                            false,
                            false,
                        ),
                        path_components: vec![
                            (StringMatch::fixed("images"), None),
                            (StringMatch::any(), None),
                        ],
                        parameters: Vec::new(),
                        ..UrlClass::default()
                    }],
                    ..UrlClassSettings::default()
                },
            )
        })
        .unwrap();
    let label = "DEBUG: remove leading double-slashes from URL paths:";
    let mut checked = 0;
    for on in [false, true] {
        set_misc_checks(&client, &[(label, on)]);
        let classes = client.store.snapshot().url_classes.clone();
        // the path split itself, as the reference's ConvertPathTextToList
        for case in recorded["paths"].as_array().unwrap() {
            if case["on"] != on {
                continue;
            }
            let theirs: Vec<&str> = case["components"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap())
                .collect();
            assert_eq!(
                hydrus_core::url::functions::path_components(
                    case["path"].as_str().unwrap(),
                    classes.settings().collapse_leading_slashes
                ),
                theirs,
                "{case}"
            );
            checked += 1;
        }
        for case in recorded["url_class"].as_array().unwrap() {
            if case["on"] != on {
                continue;
            }
            let url = case["url"].as_str().unwrap();
            assert_eq!(
                classes.class_for(url).is_some(),
                case["matches"].as_bool().unwrap(),
                "{case}"
            );
            if case["matches"].as_bool().unwrap() {
                assert_eq!(
                    classes.normalise(url, false).unwrap(),
                    case["normalised"].as_str().unwrap(),
                    "{case}"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(
        checked,
        recorded["url_class"].as_array().unwrap().len()
            + recorded["paths"].as_array().unwrap().len()
    );
}

fn watcher_cells(ui: &MainWindow) -> Vec<String> {
    let rows = ui.get_watcher_rows();
    rows.row_data(0)
        .unwrap()
        .cells
        .iter()
        .map(|c| c.to_string())
        .collect()
}

/// A gallery page and a watcher page left open while the pause and stop
/// characters are changed in the real options window: their lists show the new
/// characters at once, as the reference's draw them from its options, in the
/// gallery list's files and gallery columns and the watcher list's files and
/// checking columns (a dead watcher's checking column is the stop character).
// leaf: audit-options-downloading-misc-pause-character
// leaf: audit-options-downloading-misc-stop-character
#[test]
fn open_gallery_and_watcher_pages_show_the_changed_pause_and_stop_characters() {
    use hydrus_core::watchers::CheckerStatus;
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
    let client = client();
    add_example_downloader(&client.store);
    // the gallery page: one finished file, so its files column is stopped
    new_page(&client.ui, true);
    client.ui.invoke_gallery_queries("blue_eyes".into());
    let queue = client
        .bound
        .current
        .borrow()
        .borrow()
        .gallery()
        .unwrap()
        .queries[0]
        .queue;
    client
        .store
        .write(move |ctx| {
            let url = "https://booru.example/post/1".to_owned();
            queues::add_file_seeds(
                ctx.conn(),
                queue,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: url.clone(),
                    data_for_comparison: url,
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                }],
                false,
                0,
            )?;
            let ids: Vec<i64> = queues::file_seeds(ctx.conn(), queue)?
                .iter()
                .map(|s| s.id)
                .collect();
            queues::set_file_seed_statuses(ctx.conn(), &ids, SeedStatus::SuccessfulAndNew, 1)
        })
        .unwrap();
    client.bound.downloader_updates.force();
    (client.bound.sync)();
    client.ui.invoke_gallery_row_clicked(0, false, false);
    client.ui.invoke_gallery_pause_play(true, false);
    // the watcher page: files and checking both paused
    new_page(&client.ui, false);
    client
        .ui
        .invoke_watcher_urls("https://booru.example/thread/1".into());
    let watcher_queue = client
        .bound
        .current
        .borrow()
        .borrow()
        .watchers()
        .unwrap()
        .watchers[0]
        .queue;
    client.ui.invoke_watcher_row_clicked(0, false, false);
    client.ui.invoke_watcher_pause_play(false, true);
    client.ui.invoke_watcher_pause_play(true, true);
    let show_gallery = || {
        client.ui.invoke_tab_chosen(0, 1);
        client.bound.downloader_updates.force();
        (client.bound.sync)();
        gallery_cells(&client.ui).remove(0)
    };
    let show_watcher = || {
        client.ui.invoke_tab_chosen(0, 2);
        client.bound.downloader_updates.force();
        (client.bound.sync)();
        watcher_cells(&client.ui)
    };
    let (gallery, watcher) = (show_gallery(), show_watcher());
    assert_eq!((&gallery[2][..], &gallery[3][..]), ("\u{23F9}", "\u{23F8}"));
    assert_eq!((&watcher[1][..], &watcher[2][..]), ("\u{23F8}", "\u{23F8}"));
    // change both characters with both pages open
    let window = client.options("downloading");
    let (i, _) = row_in(&window, "misc", "Pause character:");
    window.invoke_text_edited(i, "PAUSED".into());
    let (i, _) = row_in(&window, "misc", "Stop character:");
    window.invoke_text_edited(i, "STOPPED".into());
    window.invoke_apply();
    let (gallery, watcher) = (show_gallery(), show_watcher());
    assert_eq!((&gallery[2][..], &gallery[3][..]), ("STOPPED", "PAUSED"));
    assert_eq!((&watcher[1][..], &watcher[2][..]), ("PAUSED", "PAUSED"));
    // a dead watcher, paused, has the stop character where it checks
    let mut state = client
        .store
        .read(move |c| {
            Ok(hydrus_store::watchers::watcher_state(
                &queues::queue(c, watcher_queue)?.unwrap(),
            ))
        })
        .unwrap()
        .unwrap();
    state.status = CheckerStatus::Dead;
    state.checking_paused = true;
    let extra = serde_json::to_value(&state).unwrap();
    client
        .store
        .write(move |ctx| queues::set_queue_extra(ctx.conn(), watcher_queue, &extra))
        .unwrap();
    let watcher = show_watcher();
    assert_eq!((&watcher[1][..], &watcher[2][..]), ("PAUSED", "STOPPED"));
}

/// The connection page's general and proxy options, each changed in the real
/// options window and then met by the real network engine, which is asked for
/// things by a local server. The engine is reloaded by the test calling
/// `reload_settings` as the app's poll does (the poll itself is not driven),
/// and, as the app's, honours proxies named in the environment.
mod network_consumers {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;
    use hydrus_net::{Job, NetEngine, NetError, NetOptions, Request};
    use hydrus_store::network::NetworkSettings;
    use hydrus_store::network_runtime::WaitReason;

    use super::{Client, client, noneable_text, number, row};

    #[derive(Default)]
    struct Hits {
        counts: Mutex<HashMap<String, usize>>,
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    impl Hits {
        fn hit(&self, key: &str) -> usize {
            let mut counts = self.counts.lock().unwrap();
            let n = counts.entry(key.to_owned()).or_default();
            *n += 1;
            *n
        }
        fn count(&self, key: &str) -> usize {
            self.counts.lock().unwrap().get(key).copied().unwrap_or(0)
        }
    }

    async fn status(State(hits): State<Arc<Hits>>, Path(code): Path<u16>) -> Response {
        hits.hit(&format!("status{code}"));
        (StatusCode::from_u16(code).unwrap(), "no").into_response()
    }

    async fn hold(State(hits): State<Arc<Hits>>) -> &'static str {
        let now = hits.active.fetch_add(1, Ordering::SeqCst) + 1;
        hits.max_active.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        hits.active.fetch_sub(1, Ordering::SeqCst);
        "held"
    }

    async fn stall() -> &'static str {
        tokio::time::sleep(Duration::from_secs(60)).await;
        "late"
    }

    async fn whole_uri(State(hits): State<Arc<Hits>>, uri: axum::http::Uri) -> String {
        hits.hit("uri");
        uri.to_string()
    }

    /// A local server and what it was asked.
    async fn serve() -> (String, Arc<Hits>) {
        let hits = Arc::new(Hits::default());
        let app = Router::new()
            .route("/status/{code}", get(status))
            .route("/hold", get(hold))
            .route("/stall", get(stall))
            .route("/uri", get(whole_uri))
            .with_state(Arc::clone(&hits));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, hits)
    }

    /// A client in advanced mode (its waits and timeouts may be a second),
    /// and a network engine on its store.
    fn with_engine() -> (Client, NetEngine) {
        let client = client();
        client
            .store
            .write(|ctx| {
                hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
            })
            .unwrap();
        // (the fixture's client has all new network traffic paused)
        let mut pauses: hydrus_store::settings::Pauses = client.get();
        pauses.network_traffic = false;
        client
            .store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &pauses))
            .unwrap();
        let settings: NetworkSettings = client.get();
        // (the local server takes requests as fast as they come: the default
        // bandwidth rules, which space them out, are not what is tested)
        let options = NetOptions {
            obey_bandwidth: false,
            ..NetOptions::from_settings(&settings)
        };
        let engine = NetEngine::new(client.store.clone(), options).unwrap();
        (client, engine)
    }

    /// Edit the connection page, apply, and let the engine take it.
    fn apply(client: &Client, engine: &NetEngine, edit: impl FnOnce(&hydrus_gui::OptionsWindow)) {
        let window = client.options("connection");
        edit(&window);
        window.invoke_apply();
        assert!(engine.reload_settings().unwrap(), "the engine took it");
    }

    /// How a request ended, how long it took, and every status its job showed
    /// (sampled every 20 ms).
    struct Fetched {
        result: Result<(), NetError>,
        seconds: f64,
        statuses: Vec<String>,
    }

    async fn fetch(engine: &NetEngine, url: &str) -> Fetched {
        let started = Instant::now();
        let job = Job::new();
        let request = Request::get(url);
        let mut statuses: Vec<String> = Vec::new();
        let mut call = Box::pin(engine.fetch(&request, &job));
        let result = loop {
            tokio::select! {
                r = &mut call => break r.map(|_| ()),
                () = tokio::time::sleep(Duration::from_millis(20)) => {
                    let status = job.state().status;
                    if statuses.last() != Some(&status) {
                        statuses.push(status);
                    }
                    assert!(started.elapsed() < Duration::from_secs(40), "stuck: {statuses:?}");
                }
            }
        };
        Fetched {
            result,
            seconds: started.elapsed().as_secs_f64(),
            statuses,
        }
    }

    /// Six requests for `url` at once.
    async fn six(engine: &NetEngine, url: &str) {
        let f = || fetch(engine, url);
        let _ = tokio::join!(f(), f(), f(), f(), f(), f());
    }

    /// A URL on a port nothing listens on: a socket bound and never listened
    /// on refuses connections, and is held for as long as the test.
    fn closed_port() -> (tokio::net::TcpSocket, String) {
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let url = format!("http://{}/x", socket.local_addr().unwrap());
        (socket, url)
    }

    /// The seconds the connection waits announced, in order.
    fn retry_waits(statuses: &[String]) -> Vec<u64> {
        statuses
            .iter()
            .filter_map(|s| {
                let n = s.strip_prefix("connection failed - retrying in ")?;
                n.strip_suffix(" seconds")?.parse().ok()
            })
            .collect()
    }

    // leaf: audit-options-connection-general-max-connection-attempts-allowed-per-request
    // leaf: audit-options-connection-general-connection-error-retry-wait-seconds
    #[tokio::test]
    async fn failed_connections_are_retried_as_often_and_as_slowly_as_the_options_say() {
        let (client, engine) = with_engine();
        let (_held, url) = closed_port();
        // three attempts, a second of wait per attempt gone: a wait of 1 s
        // after the first failure and one of 2 s after the second
        apply(&client, &engine, |w| {
            number(
                w,
                "max connection attempts allowed per request: ",
                (1, 10),
                3,
            );
            number(
                w,
                "connection error retry wait (seconds): ",
                (1, 2_592_000),
                1,
            );
        });
        let three = fetch(&engine, &url).await;
        assert!(matches!(three.result, Err(NetError::Connection(_))));
        assert_eq!(retry_waits(&three.statuses), [1, 2], "{:?}", three.statuses);
        assert!(three.seconds >= 3.0, "{}", three.seconds);
        // two attempts: the one wait
        apply(&client, &engine, |w| {
            number(
                w,
                "max connection attempts allowed per request: ",
                (1, 10),
                2,
            );
        });
        let two = fetch(&engine, &url).await;
        assert!(matches!(two.result, Err(NetError::Connection(_))));
        assert_eq!(retry_waits(&two.statuses), [1], "{:?}", two.statuses);
        // the same two attempts, with a wait of two seconds
        apply(&client, &engine, |w| {
            number(
                w,
                "connection error retry wait (seconds): ",
                (1, 2_592_000),
                2,
            );
        });
        let slower = fetch(&engine, &url).await;
        assert_eq!(retry_waits(&slower.statuses), [2], "{:?}", slower.statuses);
        assert!(slower.seconds >= 2.0, "{}", slower.seconds);
    }

    // leaf: audit-options-connection-general-max-retries-allowed-per-request
    #[tokio::test]
    async fn a_failing_server_is_asked_as_many_times_as_the_retries_option_says() {
        let (client, engine) = with_engine();
        let (base, hits) = serve().await;
        for retries in [2, 4] {
            apply(&client, &engine, |w| {
                number(w, "max retries allowed per request: ", (1, 10), retries);
            });
            let before = hits.count("status503");
            let fetched = fetch(&engine, &format!("{base}/status/503")).await;
            assert!(
                matches!(fetched.result, Err(NetError::Infrastructure(_))),
                "{:?}",
                fetched.result
            );
            assert_eq!(
                hits.count("status503") - before,
                usize::try_from(retries).unwrap()
            );
        }
    }

    // leaf: audit-options-connection-general-serverside-bandwidth-retry-wait-seconds
    #[tokio::test]
    async fn a_server_that_limits_bandwidth_is_waited_for_as_long_as_the_option_says() {
        let (client, engine) = with_engine();
        let (base, hits) = serve().await;
        // two asks, one wait of 1.25^2 times the option
        apply(&client, &engine, |w| {
            number(w, "max retries allowed per request: ", (1, 10), 2);
            number(
                w,
                "serverside bandwidth retry wait (seconds): ",
                (1, 2_592_000),
                1,
            );
        });
        let short = fetch(&engine, &format!("{base}/status/429")).await;
        assert!(
            matches!(short.result, Err(NetError::Bandwidth(_))),
            "{:?}",
            short.result
        );
        assert_eq!(hits.count("status429"), 2);
        assert!(
            short
                .statuses
                .iter()
                .any(|s| s == "server reported limited bandwidth - retrying")
        );
        assert!(short.seconds >= 1.5625, "{}", short.seconds);
        // (the default of a minute would be a minute and a half)
        apply(&client, &engine, |w| {
            number(
                w,
                "serverside bandwidth retry wait (seconds): ",
                (1, 2_592_000),
                3,
            );
        });
        let long = fetch(&engine, &format!("{base}/status/429")).await;
        assert!(long.seconds >= 3.0 * 1.5625, "{}", long.seconds);
    }

    // leaf: audit-options-connection-general-max-number-of-simultaneous-active-network-jobs
    // leaf: audit-options-connection-general-max-number-of-simultaneous-active-network-jobs-per-domain
    #[tokio::test]
    async fn only_as_many_jobs_run_at_once_as_the_options_say() {
        let (client, engine) = with_engine();
        let (base, hits) = serve().await;
        let url = format!("{base}/hold");
        let busiest = |hits: &Hits| {
            let seen = hits.max_active.swap(0, Ordering::SeqCst);
            assert_eq!(hits.active.load(Ordering::SeqCst), 0);
            seen
        };
        // two at once overall (and room for more on the domain)
        apply(&client, &engine, |w| {
            number(
                w,
                "max number of simultaneous active network jobs: ",
                (1, 1000),
                2,
            );
            number(
                w,
                "max number of simultaneous active network jobs per domain: ",
                (1, 100),
                5,
            );
        });
        six(&engine, &url).await;
        let seen = busiest(&hits);
        assert!((2..=2).contains(&seen), "{seen}");
        // five overall, but three on the domain
        apply(&client, &engine, |w| {
            number(
                w,
                "max number of simultaneous active network jobs: ",
                (1, 1000),
                5,
            );
            number(
                w,
                "max number of simultaneous active network jobs per domain: ",
                (1, 100),
                3,
            );
        });
        six(&engine, &url).await;
        let seen = busiest(&hits);
        assert!((2..=3).contains(&seen), "{seen}");
    }

    // leaf: audit-options-connection-general-halt-new-jobs-as-long-as-this-many-network-infrastructure-errors-on-their-domain-0-for-never-wait
    #[tokio::test]
    async fn a_domain_with_enough_errors_is_left_alone_for_as_long_as_the_options_say() {
        let (client, engine) = with_engine();
        let (base, _hits) = serve().await;
        let label = "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ";
        let set = |n: i32, minutes: i32| {
            apply(&client, &engine, |w| {
                number(w, "max retries allowed per request: ", (1, 10), 1);
                let (i, _) = row(w, label);
                w.invoke_number_edited(i, n);
                w.invoke_field_edited(i, 0, 0);
                w.invoke_field_edited(i, 1, minutes);
                w.invoke_field_edited(i, 2, 0);
            });
        };
        // two errors in twenty minutes (not the default ten) halt it
        set(2, 20);
        assert_eq!(engine.options().domain_error_window, 20 * 60);
        for _ in 0..2 {
            let fetched = fetch(&engine, &format!("{base}/status/500")).await;
            assert!(fetched.result.is_err());
        }
        assert!(!engine.domain_ok(&base));
        let job = Job::new();
        let request = Request::get(format!("{base}/uri"));
        let waited =
            tokio::time::timeout(Duration::from_millis(400), engine.fetch(&request, &job)).await;
        assert!(waited.is_err(), "it waited");
        assert_eq!(job.state().wait, WaitReason::Domain);
        // three needed: the same two do not halt it, and a request goes
        set(3, 20);
        assert!(engine.domain_ok(&base));
        assert!(fetch(&engine, &format!("{base}/uri")).await.result.is_ok());
        // zero is never wait: even with the errors still counted, and more
        set(0, 20);
        assert!(engine.domain_ok(&base));
        let fetched = fetch(&engine, &format!("{base}/status/500")).await;
        assert!(fetched.result.is_err());
        assert!(engine.domain_ok(&base));
        assert!(fetch(&engine, &format!("{base}/uri")).await.result.is_ok());
    }

    #[tokio::test]
    async fn a_server_that_goes_quiet_is_given_the_timeout_the_option_says() {
        let (client, engine) = with_engine();
        let (base, _hits) = serve().await;
        // one second to connect, six to hear back (the reference's); the
        // default of ten would be a minute
        apply(&client, &engine, |w| {
            number(w, "network timeout (seconds): ", (1, 2_592_000), 1);
            number(w, "max retries allowed per request: ", (1, 10), 1);
        });
        let fetched = fetch(&engine, &format!("{base}/stall")).await;
        assert!(
            matches!(fetched.result, Err(NetError::StreamTimeout(_))),
            "{:?}",
            fetched.result
        );
        assert!(fetched.seconds >= 6.0, "{}", fetched.seconds);
    }

    // leaf: audit-options-connection-proxy-settings-http
    // leaf: audit-options-connection-proxy-settings-no-proxy
    #[tokio::test]
    async fn the_http_proxy_and_the_hosts_it_is_not_used_for_decide_where_requests_go() {
        let (client, engine) = with_engine();
        let (base, hits) = serve().await;
        // the local server is the proxy: a proxied request asks it for the
        // whole URL
        apply(&client, &engine, |w| {
            noneable_text(w, "http: ", Some(&base));
        });
        let proxied = engine
            .fetch(&Request::get("http://booru.invalid/uri"), &Job::new())
            .await
            .unwrap()
            .text();
        assert_eq!(proxied, "http://booru.invalid/uri");
        assert_eq!(hits.count("uri"), 1);
        // the default no_proxy has the local host: asked directly
        let direct = engine
            .fetch(&Request::get(format!("{base}/uri")), &Job::new())
            .await
            .unwrap()
            .text();
        assert_eq!(direct, "/uri");
        assert_eq!(hits.count("uri"), 2);
        // naming the invalid host in no_proxy sends it where it points, not
        // to the proxy
        apply(&client, &engine, |w| {
            noneable_text(w, "no_proxy: ", Some("booru.invalid,127.0.0.1"));
            number(
                w,
                "max connection attempts allowed per request: ",
                (1, 10),
                1,
            );
        });
        let fetched = fetch(&engine, "http://booru.invalid/uri").await;
        assert!(fetched.result.is_err(), "{:?}", fetched.result);
        assert_eq!(hits.count("uri"), 2, "the proxy was not asked");
        // and with the proxy cleared, nothing is asked of it
        apply(&client, &engine, |w| {
            noneable_text(w, "http: ", None);
            noneable_text(w, "no_proxy: ", None);
        });
        let fetched = fetch(&engine, "http://booru.invalid/uri").await;
        assert!(fetched.result.is_err());
        assert_eq!(hits.count("uri"), 2, "the proxy was not asked");
    }

    // leaf: audit-options-connection-proxy-settings-https
    #[tokio::test]
    async fn https_requests_ask_the_https_proxy_to_connect() {
        use std::io::{BufRead as _, BufReader, Write as _};
        let (client, engine) = with_engine();
        // a proxy that notes what it is asked and refuses
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let (sent, asked) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut line = String::new();
                let _ = BufReader::new(stream.try_clone().unwrap()).read_line(&mut line);
                let _ = sent.send(line.trim().to_owned());
                let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n");
            }
        });
        apply(&client, &engine, |w| {
            noneable_text(w, "https: ", Some(&proxy));
            number(
                w,
                "max connection attempts allowed per request: ",
                (1, 10),
                1,
            );
        });
        let fetched = fetch(&engine, "https://booru.invalid/x").await;
        assert!(fetched.result.is_err(), "the proxy refused");
        assert_eq!(
            asked.recv_timeout(Duration::from_secs(5)).unwrap(),
            "CONNECT booru.invalid:443 HTTP/1.1"
        );
        // and with none, it is not asked again
        apply(&client, &engine, |w| {
            noneable_text(w, "https: ", None);
        });
        let fetched = fetch(&engine, "https://booru.invalid/x").await;
        assert!(fetched.result.is_err());
        assert!(asked.try_recv().is_err(), "no proxy, no ask");
    }
}

/// The downloading page's error delays, each changed in the real options
/// window and then waited out by the real downloader that makes it: a URL
/// queue's runner for gallery and watcher network errors, the subscription
/// runner for a subscription's network and other errors.
mod error_delays {
    use std::sync::Arc;
    use std::time::Duration;

    use hydrus_download::{Downloader, QueueRunner};
    use hydrus_import::FileImporter;
    use hydrus_media::MediaTools;
    use hydrus_net::{Job, NetEngine, NetOptions};
    use hydrus_store::network::NetworkSettings;
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedType};

    use super::{Client, client, row};

    const NETWORK_ROW: &str = "Delay time on a gallery/watcher network error:";
    const SUBSCRIPTION_NETWORK_ROW: &str = "Delay time on a subscription network error:";
    const SUBSCRIPTION_OTHER_ROW: &str = "Delay time on a subscription other error:";

    /// Seconds from the epoch.
    fn now() -> i64 {
        i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
        .unwrap()
    }

    /// A client, its downloader (no retries, no halting, no pauses) and a
    /// port nothing listens on.
    fn with_downloader() -> (Client, Arc<Downloader>, tokio::net::TcpSocket, String) {
        let client = client();
        client
            .store
            .write(|ctx| {
                let mut pauses: hydrus_store::settings::Pauses =
                    hydrus_store::settings::get(ctx.conn())?;
                pauses.network_traffic = false;
                pauses.subscriptions = false;
                pauses.paged_importers = false;
                pauses.file_queues = false;
                pauses.gallery_searches = false;
                hydrus_store::settings::set(ctx.conn(), &pauses)?;
                let mut network: NetworkSettings = hydrus_store::settings::get(ctx.conn())?;
                network.max_connection_attempts = 1;
                network.domain_error_number = 0;
                hydrus_store::settings::set(ctx.conn(), &network)?;
                hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
            })
            .unwrap();
        let settings: NetworkSettings = client.get();
        let net = Arc::new(
            NetEngine::new(
                client.store.clone(),
                NetOptions {
                    obey_bandwidth: false,
                    ..NetOptions::from_settings(&settings)
                },
            )
            .unwrap(),
        );
        let importer = FileImporter::new(client.store.clone(), MediaTools::new());
        let downloader = Arc::new(Downloader::new(client.store.clone(), net, importer).unwrap());
        // (a socket bound and never listened on refuses connections)
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = socket.local_addr().unwrap().to_string();
        (client, downloader, socket, address)
    }

    /// The example downloader (key "aa") searching `address`, with a URL
    /// class and a parser for its pages so that it counts as functional.
    fn functional_downloader(client: &Client, address: &str) {
        use hydrus_core::url::strings::StringMatch;
        use hydrus_core::url::{
            AnyGug, DomainMask, Gug, Gugs, UrlClass, UrlClassSettings, UrlType,
        };
        use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
        use hydrus_parse::formula::{
            Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch,
        };
        let search = UrlClass {
            name: "search".into(),
            key: vec![0xce],
            url_type: UrlType::Gallery,
            preferred_scheme: "http".into(),
            domain_mask: DomainMask::new(vec![address.to_owned()], vec![], false, false),
            path_components: [
                StringMatch::fixed("search"),
                StringMatch::any(),
                StringMatch::any(),
            ]
            .into_iter()
            .map(|m| (m, None))
            .collect(),
            ..UrlClass::default()
        };
        let thread = UrlClass {
            name: "thread".into(),
            key: vec![0xcf],
            url_type: UrlType::Watchable,
            preferred_scheme: "http".into(),
            domain_mask: DomainMask::new(vec![address.to_owned()], vec![], false, false),
            path_components: [StringMatch::fixed("thread"), StringMatch::any()]
                .into_iter()
                .map(|m| (m, None))
                .collect(),
            ..UrlClass::default()
        };
        let classes = UrlClassSettings {
            parser_links: vec![
                (hex::encode(&search.key), Some("ac".into())),
                (hex::encode(&thread.key), Some("ac".into())),
            ],
            parser_keys: vec!["ac".into()],
            url_classes: vec![search, thread],
            collapse_leading_slashes: false,
        };
        let downloaders = hydrus_parse::Downloaders {
            parsers: vec![PageParser {
                reference_auxiliary: None,
                name: "search".into(),
                key: "ac".into(),
                converter: hydrus_core::url::StringConverter::default(),
                subsidiary: Vec::new(),
                content_parsers: vec![ContentParser {
                    name: "posts".into(),
                    kind: ContentKind::Url {
                        url_type: 7,
                        priority: 50,
                    },
                    formula: Formula {
                        reference_auxiliary: None,
                        name: String::new(),
                        kind: FormulaKind::Html {
                            rules: vec![HtmlRule {
                                walk: HtmlWalk::Descendants(TagSearch {
                                    attrs: [("class".to_owned(), "thumb".to_owned())]
                                        .into_iter()
                                        .collect(),
                                    index: None,
                                }),
                                tag_name: Some("a".into()),
                                text_match: None,
                            }],
                            content: HtmlContent::Attribute("href".into()),
                        },
                        processor: hydrus_core::url::StringProcessor::default(),
                    },
                }],
                example_urls: Vec::new(),
            }],
            gugs: Gugs {
                gugs: vec![AnyGug::Single(Gug {
                    name: "example tag search".into(),
                    key: "aa".into(),
                    url_template: format!("http://{address}/search/%tags%/1"),
                    replacement_phrase: "%tags%".into(),
                    separator: "+".into(),
                    initial_search_text: "tag".into(),
                    example_search_text: "blue_eyes".into(),
                })],
                keys_to_display: vec!["aa".into()],
            },
            ..hydrus_parse::Downloaders::default()
        };
        client
            .store
            .write_and_refresh(move |ctx| {
                hydrus_store::settings::set(ctx.conn(), &classes)?;
                hydrus_store::settings::set(ctx.conn(), &downloaders)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_core::subscriptions::GalleryDefaults {
                        file_limit: Some(2000),
                        gug: Some(("aa".into(), "example tag search".into())),
                    },
                )
            })
            .unwrap();
    }

    /// Set the duration row to `days` days, `hours` hours, `minutes` minutes
    /// and `seconds` seconds, apply, and return the seconds that make.
    fn set_delay(
        client: &Client,
        row_label: &str,
        (days, hours, minutes, seconds): (i32, i32, i32, i32),
    ) -> i64 {
        let window = client.options("downloading");
        let (i, found) = row(&window, row_label);
        assert_eq!(found.kind, 8, "{row_label}");
        for (field, value) in [days, hours, minutes, seconds].into_iter().enumerate() {
            window.invoke_field_edited(i, i32::try_from(field).unwrap(), value);
        }
        window.invoke_apply();
        i64::from(days) * 86_400
            + i64::from(hours) * 3_600
            + i64::from(minutes) * 60
            + i64::from(seconds)
    }

    // leaf: audit-options-downloading-misc-delay-time-on-a-gallery-watcher-network-error
    #[tokio::test(flavor = "multi_thread")]
    async fn a_search_or_a_watcher_that_meets_a_network_error_waits_as_long_as_the_option_says() {
        use super::new_page;
        let (client, downloader, _held, address) = with_downloader();
        functional_downloader(&client, &address);
        downloader.reload_settings().unwrap();
        let runner = QueueRunner::new(
            Arc::clone(&downloader),
            client
                .get::<NetworkSettings>()
                .downloader_network_error_delay,
        );
        // a gallery page with a search, set going
        let search = |query: &str| {
            new_page(&client.ui, true);
            client.ui.invoke_gallery_queries(query.into());
            client.bound.downloader_updates.force();
            (client.bound.sync)();
            let queue = client
                .bound
                .current
                .borrow()
                .borrow()
                .gallery()
                .unwrap()
                .queries[0]
                .queue;
            client
                .store
                .write(move |ctx| queues::set_paused(ctx.conn(), queue, Some(false), Some(false)))
                .unwrap();
            runner.start_all().unwrap();
            queue
        };
        let waiting = |queue: i64| {
            let runner = Arc::clone(&runner);
            async move {
                for _ in 0..600 {
                    if let Some(until) = runner.status(queue).delayed_until {
                        return until;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                panic!("the search never waited: {:?}", runner.status(queue));
            }
        };
        // a watcher page with a thread, set going
        let watch = |thread: u32| {
            new_page(&client.ui, false);
            client
                .ui
                .invoke_watcher_urls(format!("http://{address}/thread/{thread}").into());
            client.bound.downloader_updates.force();
            (client.bound.sync)();
            let queue = client
                .bound
                .current
                .borrow()
                .borrow()
                .watchers()
                .unwrap()
                .watchers[0]
                .queue;
            runner.start_all().unwrap();
            queue
        };
        // seconds from now it is told to wait, when it is told
        let watcher_waits = |queue: i64| {
            let store = client.store.clone();
            async move {
                for _ in 0..600 {
                    let state = store
                        .read(move |c| {
                            Ok(hydrus_store::watchers::watcher_state(
                                &queues::queue(c, queue)?.unwrap(),
                            ))
                        })
                        .unwrap()
                        .unwrap();
                    if state.no_work_until > 0 {
                        return state.no_work_until - now();
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                panic!("the watcher never waited");
            }
        };
        // the default: ninety minutes, for a search and for a watcher
        let first = search("first");
        let wait = waiting(first).await - now();
        assert!((5_390..=5_400).contains(&wait), "{wait}");
        let wait = watcher_waits(watch(1)).await;
        assert!((5_390..=5_400).contains(&wait), "{wait}");
        // three hours, twenty minutes and five seconds, once the runner has
        // reloaded the options as the app's poll does
        let set = set_delay(&client, NETWORK_ROW, (0, 3, 20, 5));
        runner.reload_settings().unwrap();
        let second = search("second");
        let wait = waiting(second).await - now();
        assert!((set - 10..=set).contains(&wait), "{wait} against {set}");
        let wait = watcher_waits(watch(2)).await;
        assert!((set - 10..=set).contains(&wait), "{wait} against {set}");
    }

    /// A subscription holding one query: due for a search of the example
    /// downloader (at `address`) if `searching`, otherwise already synced and
    /// holding a file to fetch, with no place to import it.
    fn subscription(client: &Client, name: &str, searching: bool) -> i64 {
        use hydrus_core::import_options::{ImportOptionsSlice, LocationOptions};
        use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
        let settings = SubscriptionSettings {
            gug_key: "aa".into(),
            gug_name: "example tag search".into(),
            import_options: ImportOptionsSlice {
                locations: (!searching).then(|| LocationOptions {
                    destinations: Vec::new(),
                    ..LocationOptions::default()
                }),
                ..ImportOptionsSlice::default()
            },
            ..SubscriptionSettings::default()
        };
        let name = name.to_owned();
        client
            .store
            .write(move |ctx| {
                let id =
                    hydrus_store::subscriptions::create_subscription(ctx.conn(), &name, &settings)?
                        .unwrap();
                let at = now();
                let state = if searching {
                    QueryState::new("blue_eyes")
                } else {
                    QueryState {
                        last_check_time: at,
                        next_check_time: at + 86_400,
                        ..QueryState::new("synced")
                    }
                };
                let queue = hydrus_store::subscriptions::add_query(ctx.conn(), id, &state, 0)?;
                if !searching {
                    let url = "http://127.0.0.1:1/file".to_owned();
                    queues::add_file_seeds(
                        ctx.conn(),
                        queue,
                        &[NewFileSeed {
                            seed_type: SeedType::Url,
                            data: url.clone(),
                            data_for_comparison: url,
                            source_time: None,
                            referral_url: None,
                            meta: FileSeedMeta::default(),
                        }],
                        false,
                        0,
                    )?;
                }
                Ok(id)
            })
            .unwrap()
    }

    /// Run a subscription and how long, from now, it was told to wait, and why.
    async fn waits(client: &Client, downloader: &Downloader, id: i64) -> (i64, String) {
        downloader.run_subscription(id, &Job::new()).await.unwrap();
        let sub = client
            .store
            .read(move |c| hydrus_store::subscriptions::subscription(c, id))
            .unwrap()
            .unwrap();
        (
            sub.settings.no_work_until - now(),
            sub.settings.no_work_until_reason,
        )
    }

    // leaf: audit-options-downloading-misc-delay-time-on-a-subscription-network-error
    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscription_that_meets_a_network_error_waits_as_long_as_the_option_says() {
        let (client, downloader, _held, address) = with_downloader();
        functional_downloader(&client, &address);
        downloader.reload_settings().unwrap();
        let first = subscription(&client, "first", true);
        // the default: twelve hours
        let (wait, reason) = waits(&client, &downloader, first).await;
        assert!((43_190..=43_200).contains(&wait), "{wait}");
        assert!(reason.starts_with("network error: "), "{reason}");
        // a day, two hours and a minute, once the downloader has reloaded the
        // options as the app's poll does
        let set = set_delay(&client, SUBSCRIPTION_NETWORK_ROW, (1, 2, 1, 0));
        downloader.reload_settings().unwrap();
        let second = subscription(&client, "second", true);
        let (wait, reason) = waits(&client, &downloader, second).await;
        assert!((set - 10..=set).contains(&wait), "{wait} against {set}");
        assert!(reason.starts_with("network error: "), "{reason}");
    }

    // leaf: audit-options-downloading-misc-delay-time-on-a-subscription-other-error
    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscription_that_meets_another_error_waits_as_long_as_the_option_says() {
        let (client, downloader, _held, _address) = with_downloader();
        let first = subscription(&client, "first", false);
        // the default: thirty-six hours
        let (wait, reason) = waits(&client, &downloader, first).await;
        assert!((129_590..=129_600).contains(&wait), "{wait}");
        assert!(reason.starts_with("error: "), "{reason}");
        // two days and an hour
        let set = set_delay(&client, SUBSCRIPTION_OTHER_ROW, (2, 1, 0, 0));
        downloader.reload_settings().unwrap();
        let second = subscription(&client, "second", false);
        let (wait, reason) = waits(&client, &downloader, second).await;
        assert!((set - 10..=set).contains(&wait), "{wait} against {set}");
        assert!(reason.starts_with("error: "), "{reason}");
    }
}
