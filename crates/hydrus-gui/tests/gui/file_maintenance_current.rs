//! Real menu, queue writes/runners/toaster and named URL destination ownership.
use hydrus_core::HashId;
use hydrus_gui::{Bound, FileMaintenanceWindow, MainWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    file_maintenance::{self, JobType},
    popups,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn pump(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() && Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready(), "owned asynchronous work did not settle");
}
fn seed(store: &Arc<Store>) -> Vec<HashId> {
    let files = store
        .read(|conn| {
            let mut query = conn.prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 3")?;
            Ok(query
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<HashId>>>()?)
        })
        .unwrap();
    let captured = files.clone();
    store
        .write(move |ctx| {
            file_maintenance::cancel_jobs(ctx.conn(), &JobType::ALL)?;
            file_maintenance::add_jobs(ctx.conn(), &captured[..2], JobType::HasExif, 0)?;
            file_maintenance::add_jobs(
                ctx.conn(),
                &captured[2..],
                JobType::HasIccProfile,
                i64::MAX,
            )?;
            ctx.conn().execute("DELETE FROM popups", [])?;
            Ok(())
        })
        .unwrap();
    files
}
fn open(ui: &MainWindow, bound: &Bound) -> FileMaintenanceWindow {
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let maintenance = lines
        .iter()
        .position(|row| row.label == "file maintenance")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(maintenance).unwrap(), 0.0, 0.0, 0.0);
    let lines = ui.get_menu_panes().row_data(1).unwrap().lines;
    let current = lines
        .iter()
        .position(|row| row.label.starts_with("manage scheduled jobs"))
        .unwrap();
    assert!(lines.row_data(current).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(current).unwrap(), 0.0, 0.0, 0.0);
    bound
        .file_maintenance
        .as_ref()
        .unwrap()
        .slot()
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn counts(store: &Store) -> std::collections::BTreeMap<JobType, (u64, u64)> {
    store
        .read(|conn| file_maintenance::job_counts(conn, super::subscriptions::now()))
        .unwrap()
}
fn index(window: &FileMaintenanceWindow, job: JobType) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.cells.row_data(0).unwrap() == job.description())
            .unwrap(),
    )
    .unwrap()
}

// leaf: audit-media-database-maintenance-current
#[test]
fn real_menu_selected_work_future_clear_and_owned_stale_callbacks() {
    let (_dirs, store) = super::subscriptions::store();
    seed(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    pump(|| window.get_rows().row_count() == 2);
    window.invoke_clicked(index(&window, JobType::HasIccProfile), false, false);
    assert!(!window.get_can_work());
    assert!(window.get_can_all());
    window.invoke_work_clicked(false);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    window.invoke_clicked(index(&window, JobType::HasExif), false, false);
    window.invoke_clear_clicked();
    assert_eq!(
        window.get_question(),
        "Clear all the selected scheduled work?"
    );
    window.invoke_answered(false);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    window.invoke_work_clicked(false);
    pump(|| !counts(&store).contains_key(&JobType::HasExif));
    pump(|| window.get_rows().row_count() == 1);
    assert_eq!(counts(&store)[&JobType::HasIccProfile], (0, 1));
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 950, 480);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("file-maintenance-current.png"),
        &pixels,
        950,
        480,
    )
    .unwrap();
    window.invoke_clicked(index(&window, JobType::HasIccProfile), false, false);
    window.invoke_clear_clicked();
    ui.hide().unwrap();
    window.invoke_answered(true);
    assert_eq!(counts(&store)[&JobType::HasIccProfile], (0, 1));
    ui.show().unwrap();
    window.invoke_answered(true);
    pump(|| counts(&store).is_empty());
    window.invoke_close_clicked();
    let successor = open(&ui, &bound);
    window.show().unwrap();
    window.invoke_refresh_clicked();
    window.invoke_close_clicked();
    assert!(
        bound
            .file_maintenance
            .as_ref()
            .unwrap()
            .slot()
            .borrow()
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.window(), successor.window()))
    );
    seed(&store);
    successor.invoke_refresh_clicked();
    pump(|| successor.get_can_all());
    successor.invoke_clicked(index(&successor, JobType::HasExif), false, false);
    successor.invoke_clear_clicked();
    assert!(!successor.get_question().is_empty());
    let replacement = bind(&ui, Pages::open(store.clone()).unwrap());
    successor.show().unwrap();
    successor.invoke_answered(true);
    successor.invoke_work_clicked(true);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    assert!(
        bound
            .file_maintenance
            .as_ref()
            .unwrap()
            .slot()
            .borrow()
            .is_none()
    );
    assert!(
        replacement
            .file_maintenance
            .as_ref()
            .unwrap()
            .open()
            .is_ok()
    );
}

#[test]
fn reopened_review_refetches_counts_without_inheriting_the_closed_selection() {
    let (_dirs, store) = super::subscriptions::store();
    seed(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let old = open(&ui, &bound);
    pump(|| old.get_rows().row_count() == 2);
    old.invoke_clicked(index(&old, JobType::HasIccProfile), false, false);
    assert!(old.get_can_clear());
    old.invoke_sorted(1, false);
    old.invoke_close_clicked();
    let fresh = open(&ui, &bound);
    pump(|| fresh.get_rows().row_count() == 2);
    assert!(!fresh.get_can_clear());
    assert!(!fresh.get_can_work());
    assert!(fresh.get_can_all());
    assert!(fresh.get_rows().iter().all(|row| !row.selected));
    old.show().unwrap();
    old.invoke_clicked(index(&old, JobType::HasExif), false, false);
    old.invoke_clear_clicked();
    assert!(fresh.get_question().is_empty());
    assert!(!fresh.get_can_clear());
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
}

#[test]
fn real_popup_cancel_while_pass_waits_for_shared_lease_preserves_every_queued_file() {
    let (_dirs, store) = super::subscriptions::store();
    let files = seed(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    pump(|| window.get_can_all());
    let lease = hydrus_store::store::lock_file_maintenance(store.dir())
        .unwrap()
        .unwrap();
    window.invoke_work_clicked(true);
    pump(|| {
        ui.get_popups()
            .iter()
            .any(|row| row.title == "file maintenance")
    });
    let popup = ui
        .get_popups()
        .iter()
        .position(|row| row.title == "file maintenance")
        .unwrap();
    ui.invoke_popup_cancel(i32::try_from(popup).unwrap());
    drop(lease);
    pump(|| {
        store
            .read(|conn| popups::all(conn, super::subscriptions::now()))
            .unwrap()
            .iter()
            .any(|job| {
                job.status_title.as_deref() == Some("file maintenance")
                    && job.done
                    && job.status_text_1.as_deref() == Some("done!")
            })
    });
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    for file in files[..2].iter().copied() {
        assert!(
            store
                .read(move |conn| file_maintenance::jobs_for(conn, file))
                .unwrap()
                .contains(&JobType::HasExif)
        );
    }
    drop(bound);
    window.show().unwrap();
    window.invoke_work_clicked(true);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
}

#[test]
fn captured_clear_while_force_waits_is_serviced_before_the_next_physical_batch() {
    use hydrus_store::media::FileFlags;
    let (_dirs, store) = super::subscriptions::store();
    let files = seed(&store);
    let first = files[0];
    let original = store
        .read(move |conn| hydrus_store::media::load_basic(conn, &[first]))
        .unwrap();
    assert!(
        !original[0]
            .info
            .as_ref()
            .unwrap()
            .flags
            .has(FileFlags::EXIF)
    );
    store
        .write(move |ctx| {
            ctx.conn().execute(
                "UPDATE files SET flags = flags | ?1 WHERE hash_id = ?2",
                rusqlite::params![FileFlags::EXIF, first],
            )?;
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    pump(|| window.get_can_all());
    window.invoke_clicked(index(&window, JobType::HasExif), false, false);
    let lease = hydrus_store::store::lock_file_maintenance(store.dir())
        .unwrap()
        .unwrap();
    window.invoke_work_clicked(true);
    pump(|| {
        ui.get_popups()
            .iter()
            .any(|row| row.title == "file maintenance")
    });
    window.invoke_clear_clicked();
    assert_eq!(
        window.get_question(),
        "Clear all the selected scheduled work?"
    );
    window.invoke_answered(true);
    window.invoke_refresh_clicked();
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    drop(lease);
    pump(|| !counts(&store).contains_key(&JobType::HasExif));
    pump(|| window.get_rows().row_count() == 1);
    assert_eq!(counts(&store)[&JobType::HasIccProfile], (0, 1));
    // A post-pass Clear would let the HasExif physical runner overwrite this.
    assert!(
        store
            .read(move |conn| hydrus_store::media::load_basic(conn, &[first]))
            .unwrap()[0]
            .info
            .as_ref()
            .unwrap()
            .flags
            .has(FileFlags::EXIF)
    );
}

#[test]
fn pending_exit_decline_and_accepted_exit_are_owned_boundaries_for_review_callbacks() {
    let (_dirs, store) = super::subscriptions::store();
    seed(&store);
    store
        .write(|ctx| {
            let mut settings: hydrus_store::settings::GuiSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    pump(|| window.get_can_all());
    window.invoke_clicked(index(&window, JobType::HasExif), false, false);
    window.invoke_clear_clicked();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    window.invoke_answered(true);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
    ui.invoke_answer(false);
    assert!(!window.get_question().is_empty());
    window.invoke_answered(false);
    assert!(window.get_question().is_empty());
    window.invoke_clear_clicked();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert!(
        bound
            .file_maintenance
            .as_ref()
            .unwrap()
            .slot()
            .borrow()
            .is_none()
    );
    ui.show().unwrap();
    window.show().unwrap();
    window.invoke_answered(true);
    window.invoke_work_clicked(true);
    assert_eq!(counts(&store)[&JobType::HasExif], (2, 0));
}

#[test]
fn named_redownload_preserves_current_and_reuses_named_page_instead_of_unrelated_url_importer() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages
        .new_page(&hydrus_gui_model::page_chooser::NewPage::Urls)
        .unwrap();
    let unrelated = pages.shown().key;
    let fixture = hydrus_testkit::fixture_json("file_maintenance_current.json");
    let url = fixture["redownload"]["url"].as_str().unwrap().to_owned();
    pages
        .import_maintenance_urls(std::slice::from_ref(&url))
        .unwrap();
    assert_eq!(pages.shown().key, unrelated);
    let targets: Vec<_> = pages
        .session()
        .pages
        .iter()
        .filter(|page| page.name == "missing files redownloader")
        .collect();
    assert_eq!(targets.len(), 1);
    let key = targets[0].key;
    let queue = pages.page(&key).unwrap().borrow().importer().unwrap().queue;
    assert_eq!(
        store
            .read(|conn| hydrus_store::queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .name,
        "missing files redownloader"
    );
    pages.import_maintenance_urls(&[url]).unwrap();
    assert_eq!(
        pages
            .session()
            .pages
            .iter()
            .filter(|page| page.name == "missing files redownloader")
            .map(|page| page.key)
            .collect::<Vec<_>>(),
        [key]
    );
    assert_eq!(pages.shown().key, unrelated);
}

#[test]
fn actual_missing_file_integrity_runner_hands_useful_url_to_the_owned_named_import_queue() {
    use hydrus_core::url::{UrlClassSettings, UrlType};
    use hydrus_legacy::{objects::domain, serialisable::SerialisableObject};
    let (_dirs, store) = super::subscriptions::store();
    let file = seed(&store)[0];
    let fixture = hydrus_testkit::fixture_json("clipboard_urls.json");
    let class = fixture["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tuple| {
            domain::url_class(&SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap())
                .unwrap()
        })
        .find(|class| class.url_type == UrlType::File)
        .unwrap();
    let url = class.example_url.clone();
    // The basic fixture already associates three valid URLs with its first file.
    // With only the File class installed these are unclassified, which Qt also
    // admits for repair. Read the associations before adding this case's URLs.
    let mut expected = store
        .read(move |conn| {
            let mut query = conn.prepare(
                "SELECT u.url FROM file_urls f JOIN urls u USING(url_id) WHERE f.hash_id=?1 ORDER BY u.url",
            )?;
            Ok(query
                .query_map([file], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    assert_eq!(
        expected,
        [
            "https://danbooru.donmai.us/posts/1000",
            "https://example.com/post/0",
            "https://gelbooru.com/index.php?page=post&s=view&id=2000",
        ]
    );
    assert!(expected.iter().all(|url| !class.matches(url, false)));
    expected.push(url.clone());
    expected.sort();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &UrlClassSettings {
                    url_classes: vec![class],
                    ..Default::default()
                },
            )?;
            let invalid_id = hydrus_store::master::intern_url(ctx.conn(), "0missing-scheme")?;
            ctx.conn().execute(
                "INSERT OR IGNORE INTO file_urls(hash_id,url_id) VALUES(?1,?2)",
                rusqlite::params![file, invalid_id],
            )?;
            let url_id = hydrus_store::master::intern_url(ctx.conn(), &url)?;
            ctx.conn().execute(
                "INSERT OR IGNORE INTO file_urls(hash_id,url_id) VALUES(?1,?2)",
                rusqlite::params![file, url_id],
            )?;
            file_maintenance::cancel_jobs(ctx.conn(), &JobType::ALL)?;
            file_maintenance::add_jobs(ctx.conn(), &[file], JobType::IntegrityPresenceTryUrl, 0)
        })
        .unwrap();
    let media = store
        .read(move |conn| hydrus_store::media::load_basic(conn, &[file]))
        .unwrap()
        .remove(0);
    let path = store
        .snapshot()
        .storage
        .file_path(&media.hash, media.info.as_ref().unwrap().mime)
        .unwrap();
    assert!(
        path.is_file(),
        "the guarded fixture must contain actual physical media"
    );
    std::fs::remove_file(&path).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let previous = bound.pages.borrow().shown().key;
    let window = open(&ui, &bound);
    pump(|| window.get_can_all());
    window.invoke_work_clicked(true);
    pump(|| {
        bound
            .pages
            .borrow()
            .session()
            .pages
            .iter()
            .any(|page| page.name == "missing files redownloader")
    });
    assert_eq!(bound.pages.borrow().shown().key, previous);
    let pages = bound.pages.borrow();
    let destination = pages
        .session()
        .pages
        .iter()
        .find(|page| page.name == "missing files redownloader")
        .unwrap();
    let hydrus_core::pages::PageContent::Downloader { queues, .. } = &destination.content else {
        panic!("named destination must be a real importer")
    };
    let queue = queues[0];
    let received = store
        .write(move |ctx| hydrus_store::queues::take_url_requests(ctx.conn(), queue))
        .unwrap();
    assert_eq!(
        received, expected,
        "every valid unknown/File URL is handed over once"
    );
    assert!(
        store
            .write(move |ctx| hydrus_store::queues::take_url_requests(ctx.conn(), queue))
            .unwrap()
            .is_empty(),
        "the named queue handoff is consumed once"
    );
    let recorded = hydrus_testkit::fixture_json("file_maintenance_current.json");
    let invalid_error = recorded["redownload"]["errors"][0].as_str().unwrap();
    assert!(
        store
            .read(|conn| popups::all(conn, super::subscriptions::now()))
            .unwrap()
            .iter()
            .any(|job| job.status_text_1.as_deref() == Some(invalid_error))
    );
    assert!(
        store
            .read(move |conn| file_maintenance::jobs_for(conn, file))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .read(move |conn| hydrus_store::media::load_basic(conn, &[file]))
            .unwrap()
            .iter()
            .any(|media| media.hash_id == file)
    );
}
