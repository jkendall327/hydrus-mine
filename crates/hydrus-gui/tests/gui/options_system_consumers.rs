//! The options-system pages against their consumers: each test changes the
//! option in the real options window (the control as the reference shows it),
//! applies, and checks the behaviour the saved value changes.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, OptionRow, OptionsWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
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
struct Client {
    _dirs: [tempfile::TempDir; 2],
    store: Arc<Store>,
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    _windows: hydrus_gui::headless::Windows,
}

fn client() -> Client {
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    Client {
        _dirs: dirs,
        store,
        ui,
        bound,
        _windows: windows,
    }
}

impl Client {
    /// file > options…, on `page`.
    fn options(&self, page: &str) -> OptionsWindow {
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

    fn get<T: hydrus_store::settings::Setting>(&self) -> T {
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

fn row(options: &OptionsWindow, label: &str) -> (i32, OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i as i32, rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

/// A checkbox row, set.
fn check(options: &OptionsWindow, label: &str, on: bool) {
    let (i, found) = row(options, label);
    assert_eq!(found.kind, 1, "{label:?} is a checkbox");
    options.invoke_check_toggled(i, on);
}

/// A number row (kind 2): its limits are the reference's.
fn number(options: &OptionsWindow, label: &str, limits: (i32, i32), n: i32) {
    let (i, found) = row(options, label);
    assert_eq!(
        (found.kind, found.minimum, found.maximum),
        (2, limits.0, limits.1),
        "{label:?}"
    );
    options.invoke_number_edited(i, n);
}

/// A number that may be none (kind 3): its none phrase is the reference's.
fn noneable(options: &OptionsWindow, label: &str, phrase: &str, n: Option<i32>) {
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

// leaf: audit-options-connection-general-max-connection-attempts-allowed-per-request
// leaf: audit-options-connection-general-max-retries-allowed-per-request
// leaf: audit-options-connection-general-debug-do-not-verify-regular-https-traffic
// leaf: audit-options-connection-general-halt-new-jobs-as-long-as-this-many-network-infrastructure-errors-on-their-domain-0-for-never-wait
// leaf: audit-options-connection-proxy-settings-http
// leaf: audit-options-connection-proxy-settings-https
// leaf: audit-options-connection-proxy-settings-no-proxy
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

// leaf: audit-options-files-and-trash-delete-lock-after-archive-delete-filter-ensure-deletees-are-inboxed-before-delete
#[test]
fn archive_delete_filter_reinboxes_deletees_when_the_option_and_lock_are_set() {
    use hydrus_gui_model::archive_delete::ArchiveDeleteFilter;
    use hydrus_search::LocationContext;

    let client = client();
    let snapshot = client.store.snapshot();
    let source = snapshot.services.by_name("art").unwrap().id;
    let files: Vec<HashId> = client
        .store
        .read(move |c| {
            let mut q = c.prepare(
                "SELECT hash_id FROM file_domain_current WHERE service_id = ? ORDER BY hash_id LIMIT 2",
            )?;
            Ok(q.query_map([source], |r| r.get(0))?
                .collect::<Result<_, _>>()?)
        })
        .unwrap();
    assert_eq!(files.len(), 2);
    let location = || LocationContext::single(snapshot.services.get(source).unwrap().key.clone());
    let finish = |files: &[HashId]| {
        let archived = files.to_vec();
        client
            .store
            .write_content(move |w| {
                // (put back where the last pass deleted them from)
                let back: Vec<_> = archived.iter().map(|f| (*f, None)).collect();
                w.add_files(source, &back)?;
                w.inbox(&archived)?;
                w.archive(&archived)
            })
            .unwrap();
        let mut filter =
            ArchiveDeleteFilter::new(client.store.clone(), files.to_vec(), location()).unwrap();
        // (the deletee is the second; the first is kept)
        filter.keep();
        filter.delete();
        filter.commit().unwrap();
        // (the files still in the inbox table, trashed or not)
        let files = files.to_vec();
        let inbox: Vec<HashId> = client
            .store
            .read(move |c| {
                let mut q = c.prepare("SELECT hash_id FROM file_inbox")?;
                let all: Vec<HashId> = q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
                Ok(all.into_iter().filter(|h| files.contains(h)).collect())
            })
            .unwrap();
        inbox
    };
    let deletee = files[1];

    // by default a deletee stays as archived as it was
    let inbox = finish(&files);
    assert!(!inbox.contains(&deletee));

    let reinbox = "After archive/delete filter, ensure deletees are inboxed before delete: ";
    // the reinbox option without the lock does nothing
    let window = client.options("files and trash");
    check(&window, reinbox, true);
    window.invoke_apply();
    let inbox = finish(&files);
    assert!(!inbox.contains(&deletee));

    // with the lock too, the deletee is inboxed so the lock lets it go
    let window = client.options("files and trash");
    check(
        &window,
        "Do not permit archived files to be deleted from the trash: ",
        true,
    );
    window.invoke_apply();
    let inbox = finish(&files);
    assert!(inbox.contains(&deletee), "reinboxed");
    assert!(!inbox.contains(&files[0]), "the kept file is archived");

    // switched off again, it is not
    let window = client.options("files and trash");
    check(&window, reinbox, false);
    window.invoke_apply();
    let inbox = finish(&files);
    assert!(!inbox.contains(&deletee));
}

// leaf: audit-options-files-and-trash-number-of-hours-a-file-will-stay-in-the-trash-before-being-deleted
// leaf: audit-options-files-and-trash-maximum-size-of-trash-mb
#[test]
fn trash_limits_decide_which_trashed_files_maintenance_deletes_for_good() {
    use hydrus_store::trash::{TrashReport, TrashSettings, maintain_trash};

    const HOUR_MS: i64 = 3_600_000;
    let client = client();
    // ten files in the trash, oldest first, each 200,000 bytes, trashed 9.5
    // to 0.5 hours ago
    let files = client
        .store
        .write_content(|w| {
            let roles = w.roles().clone();
            let ids_in = |w: &hydrus_store::content::ContentWriter<'_>, domain| {
                let mut stmt = w.conn().prepare(
                    "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 ORDER BY hash_id",
                )?;
                let rows = stmt.query_map([domain], |r| r.get::<_, HashId>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            };
            let already = ids_in(w, roles.trash)?;
            w.delete_files(roles.trash, &already, None)?;
            let files: Vec<HashId> = ids_in(w, roles.combined_local_media)?
                .into_iter()
                .take(10)
                .collect();
            w.delete_files(roles.combined_local_media, &files, None)?;
            let now = w.now_ms();
            for (i, &id) in files.iter().enumerate() {
                let age = 10 - i64::try_from(i).unwrap();
                w.conn().execute(
                    "UPDATE file_domain_current SET added_ms = ?1 WHERE service_id = ?2 AND hash_id = ?3",
                    rusqlite::params![now - age * HOUR_MS + HOUR_MS / 2, roles.trash, id],
                )?;
                w.conn()
                    .execute("UPDATE files SET size = 200000 WHERE hash_id = ?1", [id])?;
            }
            Ok(files)
        })
        .unwrap();
    assert_eq!(files.len(), 10);
    let trash_role = hydrus_store::content::DomainRoles::new(&client.store.snapshot().services)
        .unwrap()
        .trash;
    let in_trash = || -> Vec<HashId> {
        client
            .store
            .read(move |c| {
                let mut q = c.prepare(
                    "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 ORDER BY hash_id",
                )?;
                Ok(q.query_map([trash_role], |r| r.get(0))?
                    .collect::<Result<_, _>>()?)
            })
            .unwrap()
    };

    let age = "Number of hours a file will stay in the trash before being deleted: ";
    let size = "Maximum size of trash (MB): ";
    let window = client.options("files and trash");
    let (_, shown) = row(&window, age);
    assert_eq!(
        (
            shown.kind,
            shown.number,
            shown.minimum,
            shown.maximum,
            shown.is_none
        ),
        (3, 72, 0, 8640, false)
    );
    assert_eq!(shown.none_phrase, "no age limit");
    let (_, shown) = row(&window, size);
    assert_eq!(
        (
            shown.kind,
            shown.number,
            shown.minimum,
            shown.maximum,
            shown.is_none
        ),
        (3, 2048, 0, 20480, false)
    );
    assert_eq!(shown.none_phrase, "no size limit");
    // the defaults delete nothing from a trash this young and small
    assert_eq!(
        maintain_trash(&client.store, 256).unwrap(),
        TrashReport::default()
    );
    assert_eq!(in_trash(), files);

    // three hours, and no size limit: the seven older go
    noneable(&window, age, "no age limit", Some(3));
    noneable(&window, size, "no size limit", None);
    window.invoke_apply();
    assert_eq!(
        client.get::<TrashSettings>(),
        TrashSettings {
            max_age_hours: Some(3),
            max_size_mb: None
        }
    );
    assert_eq!(
        maintain_trash(&client.store, 256).unwrap(),
        TrashReport {
            over_size: 0,
            over_age: 7
        }
    );
    assert_eq!(in_trash(), files[7..]);

    // no age limit, one megabyte: 600,000 bytes remain, under it
    let window = client.options("files and trash");
    noneable(&window, age, "no age limit", None);
    noneable(&window, size, "no size limit", Some(0));
    window.invoke_apply();
    assert_eq!(
        client.get::<TrashSettings>(),
        TrashSettings {
            max_age_hours: None,
            max_size_mb: Some(0)
        }
    );
    assert_eq!(
        maintain_trash(&client.store, 256).unwrap(),
        TrashReport {
            over_size: 3,
            over_age: 0
        },
        "a trash limited to nothing is emptied"
    );
    assert!(in_trash().is_empty());
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
fn row_in(options: &OptionsWindow, in_box: &str, label: &str) -> (i32, OptionRow) {
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

    /// The GUI's idle state, published as of the clock.
    fn publish(store: &Store, clock: &HeldClock, idle: bool) {
        hydrus_store::idle_state::publish(store.dir(), idle, clock.now_ms()).unwrap();
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
        publish(store, &clock, idle);
        let reference_packet = worked(events);
        let reference_wait = waited(events);
        match pass["worker"].as_str().unwrap() {
            "similar" => {
                // a search packet goes on until its time is up or nothing is
                // left: the first batch takes the recorded share of the
                // packet, and a packet that isn't over finds nothing more
                let calls = std::cell::Cell::new(0);
                let wait = workers::similar_files_step(store, &clock, |_| {
                    calls.set(calls.get() + 1);
                    if calls.get() > 1 {
                        return Ok(0);
                    }
                    clock.advance(reference_packet.unwrap().mul_f64(share));
                    Ok(files)
                });
                if reference_packet.is_none() {
                    assert_eq!(calls.get(), 0, "held: {what}");
                    assert_eq!(reference_wait, Duration::from_secs(30), "{what}");
                    assert_eq!(wait, workers::SIMILAR_FILES_HOLD, "{what}");
                } else {
                    // (the packet's end is where the reference's was)
                    let expected_calls = if more { 1 } else { 2 };
                    assert_eq!(calls.get(), expected_calls, "{what}");
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

    fn kind(weight: u64) -> &'static str {
        match weight {
            100 => "metadata",
            25 => "exif",
            5 => "presence",
            other => panic!("weight {other}"),
        }
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
            publish(store, &clock, idle);
            let wait = throttle.step(store, &clock, |able, used| {
                let report = importer.run_file_maintenance_batch(able, &mut |weight| {
                    trace.push(Line::Job(clock.now_ms() / 1000 - t0, kind(weight)));
                    used(weight);
                })?;
                Ok::<_, hydrus_import::ImportError>(report.total())
            });
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

const IDLE_ENABLED: &str =
    "Run maintenance jobs when the client is idle and the system is not otherwise busy: ";
const CPU_PERCENT: &str = "Consider the system busy if CPU usage is above: ";
const CPU_CORES: &str = "% on ";

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-run-maintenance-jobs-when-the-client-is-idle-and-the-system-is-not-otherwise-busy
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-consider-the-system-busy-if-cpu-usage-is-above
// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-on
#[test]
fn idle_and_cpu_busy_options_decide_whether_background_work_may_run() {
    use hydrus_store::idle_state;
    let client = client();
    let options = client.options("maintenance and processing");
    // The reference's controls and limits.
    let (_, percent) = row_in(&options, "idle", CPU_PERCENT);
    assert_eq!((percent.kind, percent.minimum, percent.maximum), (2, 5, 99));
    let (_, cores) = row_in(&options, "idle", CPU_CORES);
    assert_eq!(
        (
            cores.kind,
            cores.none_phrase.as_str(),
            cores.minimum,
            cores.maximum
        ),
        (3, "ignore cpu usage", 1, 64)
    );
    // Idle on, no activity timeouts, busy at 5% on one core.
    let (at, _) = row_in(&options, "idle", IDLE_ENABLED);
    options.invoke_check_toggled(at, true);
    for label in [
        "Permit idle mode if no general browsing activity has occurred in the past: ",
        "Permit idle mode if your mouse cursor has not been moved in the past: ",
        "Permit idle mode if no Client API requests in the past: ",
    ] {
        let (i, _) = row_in(&options, "idle", label);
        options.invoke_none_toggled(i, true);
    }
    let (i, _) = row_in(&options, "idle", CPU_PERCENT);
    options.invoke_number_edited(i, 5);
    let (i, _) = row_in(&options, "idle", CPU_CORES);
    options.invoke_number_edited(i, 1);
    options.invoke_none_toggled(i, false);
    options.invoke_apply();
    let saved: hydrus_store::settings::GuiIdleSettings = client.get();
    assert!(saved.enabled);
    assert_eq!((saved.busy_cpu_percent, saved.busy_cpu_count), (5, Some(1)));

    // The runtime's first sample has nothing to compare with. The cores are
    // read from a fake `/proc/stat`: this one core ran 60% busy in each
    // minute (60 of 100 jiffies), however long the test takes.
    let jiffies = std::rc::Rc::new(std::cell::Cell::new((0u64, 0u64)));
    client.bound.maintenance.use_cpu_times({
        let jiffies = jiffies.clone();
        move || vec![jiffies.get()]
    });
    let dir = client.store.dir().to_owned();
    let base = hydrus_core::TimestampMs::now().0 + 10_000_000;
    let minute = |n: i64| {
        jiffies.set((60 * n as u64, 100 * n as u64));
        client.bound.maintenance.poll_at(base + 60_000 * n).unwrap();
        base + 60_000 * n
    };
    let set_percent = |percent: i64| {
        let options = client.options("maintenance and processing");
        let (i, _) = row_in(&options, "idle", CPU_PERCENT);
        options.invoke_number_edited(i, percent as _);
        options.invoke_apply();
    };
    jiffies.set((0, 0));
    client.bound.maintenance.poll_at(base).unwrap();
    assert!(idle_state::is_idle(&dir, base));
    // 60% is above 5%: busy, though still idle.
    let at = minute(1);
    assert!(!idle_state::is_idle(&dir, at), "busy system");
    assert_eq!(client.ui.get_status_busy(), "CPU busy");
    assert_eq!(client.ui.get_status_idle(), "idle", "still idle, but busy");
    // (their tooltips are the reference's)
    assert_eq!(
        client.ui.get_status_idle_tip(),
        "client is idle, it can do maintenance work"
    );
    assert_eq!(
        client.ui.get_status_busy_tip(),
        "this computer has been doing work recently, so some hydrus maintenance will not start"
    );

    // Raise the percent above what the cores ran: the same load is not busy,
    // and work may run again.
    set_percent(70);
    let saved: hydrus_store::settings::GuiIdleSettings = client.get();
    assert_eq!(saved.busy_cpu_percent, 70);
    let at = minute(2);
    assert!(idle_state::is_idle(&dir, at), "60% is not above 70%");
    assert_eq!(client.ui.get_status_busy(), "");
    // Exactly the load is not above it either (the reference compares with >).
    set_percent(60);
    let at = minute(3);
    assert!(idle_state::is_idle(&dir, at), "60% is not above 60%");
    set_percent(59);
    let at = minute(4);
    assert!(!idle_state::is_idle(&dir, at), "60% is above 59%");
    assert_eq!(client.ui.get_status_busy(), "CPU busy");

    // "ignore cpu usage" clears it at the next check, and the percent
    // control is disabled while no core count is set (and enabled again
    // once one is).
    let options = client.options("maintenance and processing");
    assert!(row_in(&options, "idle", CPU_PERCENT).1.enabled);
    let (i, _) = row_in(&options, "idle", CPU_CORES);
    options.invoke_none_toggled(i, true);
    options.invoke_apply();
    let options = client.options("maintenance and processing");
    assert!(
        !row_in(&options, "idle", CPU_PERCENT).1.enabled,
        "no core count: the percent is not used"
    );
    let at = minute(5);
    assert!(idle_state::is_idle(&dir, at));
    assert_eq!(client.ui.get_status_busy(), "");
    let (i, _) = row_in(&options, "idle", CPU_CORES);
    options.invoke_none_toggled(i, false);
    options.invoke_apply();
    let options = client.options("maintenance and processing");
    assert!(
        row_in(&options, "idle", CPU_PERCENT).1.enabled,
        "a core count is set again"
    );
    options.invoke_cancel();
    let at = minute(6);
    assert!(
        !idle_state::is_idle(&dir, at),
        "busy again, at 59% on 1 core"
    );

    // Switching the idle option off stops idle work whatever the CPU does.
    let options = client.options("maintenance and processing");
    let (i, _) = row_in(&options, "idle", IDLE_ENABLED);
    options.invoke_check_toggled(i, false);
    options.invoke_apply();
    let at = minute(7);
    assert!(!idle_state::is_idle(&dir, at));
}

// Exiting after editing the shutdown box (File > options…, then the window's close).

fn shutdown_row(options: &OptionsWindow, label: &str) -> (i32, OptionRow) {
    row_in(options, "shutdown", label)
}

fn shutdown_settings(client: &Client) -> hydrus_store::settings::ShutdownWork {
    client.get()
}

fn seconds_now() -> i64 {
    hydrus_core::TimestampMs::now().secs()
}

impl Client {
    /// Bind the window again, as a client started afresh (a closed one does
    /// not take another exit).
    fn restart(&mut self) {
        self.ui.show().unwrap();
        self.bound = bind(&self.ui, Pages::open(self.store.clone()).unwrap());
    }

    /// With the shutdown settings as new and the last shutdown work `ago`
    /// seconds back, the shutdown box edited by `edit` and applied; then the
    /// window is closed.
    fn exit_after(&mut self, ago: i64, edit: impl FnOnce(&OptionsWindow)) {
        self.restart();
        self.store
            .write(move |ctx| {
                ctx.conn()
                    .execute_batch("DROP TABLE IF EXISTS sqlite_stat1")?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ShutdownWork {
                        last_done: seconds_now() - ago,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let options = self.options("maintenance and processing");
        edit(&options);
        options.invoke_apply();
        self.ui
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    }
}

fn choose_shutdown(options: &OptionsWindow, choice: i32) {
    let (i, row) = shutdown_row(options, "Run jobs on shutdown: ");
    assert_eq!(row.items.row_count(), 3);
    options.invoke_choice_chosen(i, choice);
}

/// Whether the database has been analysed (planner statistics exist; empty
/// tables get none, so the list of tables due stays as it was).
fn analysed(client: &Client) -> bool {
    client
        .store
        .read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE name = 'sqlite_stat1'",
                    [],
                    |_| Ok(()),
                )
                .is_ok())
        })
        .unwrap()
}

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-run-jobs-on-shutdown
#[test]
fn run_jobs_on_shutdown_decides_what_the_exit_does() {
    use hydrus_gui_model::shutdown_work::{ACTIONS, ASK_TITLE};
    let mut client = client();
    let options = client.options("maintenance and processing");
    let (_, row) = shutdown_row(&options, "Run jobs on shutdown: ");
    let items: Vec<String> = row.items.iter().map(|s| s.to_string()).collect();
    assert_eq!(items, ACTIONS, "the reference's three choices");
    assert_eq!(row.index, 2, "ask first is the default");
    options.invoke_cancel();

    // do not run jobs: the exit goes straight through, doing nothing.
    client.exit_after(1_000_000, |o| choose_shutdown(o, 0));
    assert_eq!(shutdown_settings(&client).action, 0);
    assert!(hydrus_gui::client_exit::maintenance_question().is_none());
    assert!(!client.ui.window().is_visible(), "exited");
    assert!(!analysed(&client), "no work was done");
    assert!(
        shutdown_settings(&client).last_done < seconds_now() - 900_000,
        "untouched"
    );

    // run if needed: the work is done without asking.
    client.exit_after(1_000_000, |o| choose_shutdown(o, 1));
    assert_eq!(shutdown_settings(&client).action, 1);
    assert!(hydrus_gui::client_exit::maintenance_question().is_none());
    assert!(!client.ui.window().is_visible(), "exited");
    assert!(analysed(&client), "the outstanding analysis was run");
    assert!(shutdown_settings(&client).last_done >= seconds_now() - 5);

    // ask first: the exit waits on the question; no skips the work but is
    // not asked again, yes does it.
    client.exit_after(1_000_000, |o| choose_shutdown(o, 2));
    assert_eq!(shutdown_settings(&client).action, 2);
    let question = hydrus_gui::client_exit::maintenance_question().expect("asked");
    assert_eq!(question.get_window_title(), ASK_TITLE);
    assert!(client.ui.window().is_visible(), "the exit waits");
    question.invoke_answered(false);
    assert!(!client.ui.window().is_visible(), "exited");
    assert!(!analysed(&client), "declined");
    assert!(shutdown_settings(&client).last_done >= seconds_now() - 5);

    client.exit_after(1_000_000, |o| choose_shutdown(o, 2));
    let question = hydrus_gui::client_exit::maintenance_question().expect("asked");
    question.invoke_answered(true);
    assert!(!client.ui.window().is_visible(), "exited");
    assert!(analysed(&client), "accepted: the analysis was run");
}

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-only-run-shutdown-jobs-once-per
#[test]
fn shutdown_jobs_run_only_once_per_the_edited_period() {
    const PERIOD: &str = "Only run shutdown jobs once per: ";
    let mut client = client();
    let options = client.options("maintenance and processing");
    let (_, row) = shutdown_row(&options, PERIOD);
    let units: Vec<String> = row.fields.iter().map(|f| f.label.to_string()).collect();
    assert_eq!(units.len(), 3, "days, hours and minutes: {units:?}");
    assert!(row.enabled);
    assert_eq!(row.fields.row_data(0).unwrap().value, 1, "one day");
    // no use while jobs are not run on shutdown
    choose_shutdown(&options, 0);
    options.invoke_apply();
    let options = client.options("maintenance and processing");
    assert!(!shutdown_row(&options, PERIOD).1.enabled);
    options.invoke_cancel();

    // Ten minutes since the last run, and the default period is a day: no
    // question, though ask-first is the default.
    client.exit_after(600, |_| {});
    assert!(hydrus_gui::client_exit::maintenance_question().is_none());
    assert!(!client.ui.window().is_visible(), "exited at once");
    assert!(!analysed(&client));

    // The same ten minutes against an edited period of five: due, so asked.
    client.exit_after(600, |o| {
        let (i, _) = shutdown_row(o, PERIOD);
        o.invoke_field_edited(i, 0, 0);
        o.invoke_field_edited(i, 1, 0);
        o.invoke_field_edited(i, 2, 5);
    });
    assert_eq!(shutdown_settings(&client).period_seconds, 300);
    let question = hydrus_gui::client_exit::maintenance_question().expect("due, so asked");
    assert!(client.ui.window().is_visible());
    question.invoke_answered(false);
    assert!(!client.ui.window().is_visible());

    // A longer period than the time since the last run: skipped again.
    client.exit_after(600, |o| {
        let (i, _) = shutdown_row(o, PERIOD);
        o.invoke_field_edited(i, 0, 0);
        o.invoke_field_edited(i, 1, 2);
        o.invoke_field_edited(i, 2, 0);
    });
    assert_eq!(shutdown_settings(&client).period_seconds, 7_200);
    assert!(hydrus_gui::client_exit::maintenance_question().is_none());
    assert!(!client.ui.window().is_visible());
}

// leaf: audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-max-number-of-minutes-to-run-shutdown-jobs
#[test]
fn the_edited_shutdown_minutes_are_in_the_question() {
    const MINUTES: &str = "Max number of minutes to run shutdown jobs: ";
    let mut client = client();
    let options = client.options("maintenance and processing");
    let (_, row) = shutdown_row(&options, MINUTES);
    assert_eq!(
        (row.kind, row.minimum, row.maximum, row.number),
        (2, 1, 1440, 5)
    );
    assert!(row.enabled);
    // no use unless jobs are run on shutdown
    choose_shutdown(&options, 0);
    options.invoke_apply();
    let options = client.options("maintenance and processing");
    assert!(!shutdown_row(&options, MINUTES).1.enabled, "not run at all");
    options.invoke_cancel();

    for minutes in [5, 7, 90] {
        client.exit_after(1_000_000, |o| {
            let (i, _) = shutdown_row(o, MINUTES);
            o.invoke_number_edited(i, minutes);
        });
        assert_eq!(shutdown_settings(&client).max_minutes, minutes as u32);
        let question = hydrus_gui::client_exit::maintenance_question().expect("asked");
        let message = question.get_message().to_string();
        assert!(
            message.starts_with(&format!(
                "Is now a good time for the client to do up to {minutes} minutes' maintenance work? (Will auto-no in 15 seconds)\n\nThe outstanding jobs appear to be:\n\nanalyze "
            )),
            "{message}"
        );
        question.invoke_answered(false);
    }
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
