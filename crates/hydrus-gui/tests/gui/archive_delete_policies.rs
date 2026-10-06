//! Real Options drafts and owned domain buttons, delay and transactional consumers.
use hydrus_core::{HashId, ServiceKey};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::{
    Store,
    archive_delete_preferences::{self, Preferences},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn setup() -> (
    tempfile::TempDir,
    Arc<Store>,
    Vec<HashId>,
    Vec<hydrus_core::ServiceId>,
) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let qt = hydrus_testkit::fixture_json("archive_delete_policies.json");
    let ids: Vec<_> = qt["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let key = ServiceKey::from_hex(d["key"].as_str().unwrap()).unwrap();
            store.snapshot().services.by_key(&key).unwrap().id
        })
        .collect();
    let files=store.read(|conn|{let mut q=conn.prepare("SELECT hash_id FROM file_domain_current WHERE service_id=? ORDER BY hash_id LIMIT 2")?;Ok(q.query_map([ids[0]],|r|r.get::<_,HashId>(0))?.collect::<Result<Vec<_>,_>>()?)}).unwrap();
    assert_eq!(files.len(), 2);
    let written = files.clone();
    let domains = ids.clone();
    store
        .write_content(move |w| {
            for domain in domains {
                w.add_files(
                    domain,
                    &written.iter().map(|file| (*file, None)).collect::<Vec<_>>(),
                )?;
            }
            w.inbox(&written)
        })
        .unwrap();
    (dir, store, files, ids)
}
fn ui(
    store: &Arc<Store>,
    files: &[HashId],
    domain: hydrus_core::ServiceId,
) -> (MainWindow, hydrus_gui::Bound) {
    let context = FileSearchContext {
        location: LocationContext::single(
            store.snapshot().services.get(domain).unwrap().key.clone(),
        ),
        ..Default::default()
    };
    let page = SearchPage::restored(store.clone(), context, false, None, files.to_vec());
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page));
    (ui, bound)
}
fn open_filter(ui: &MainWindow, bound: &hydrus_gui::Bound) -> hydrus_gui::ArchiveDeleteWindow {
    ui.invoke_archive_delete_filter();
    bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
#[test]
fn actual_multiple_choices_refuse_early_commit_then_delete_only_selected_domain() {
    let (_dir, store, files, ids) = setup();
    let windows = headless::init();
    let (ui, bound) = ui(&store, &files, ids[0]);
    let filter = open_filter(&ui, &bound);
    ui.invoke_archive_delete_filter();
    assert!(
        std::ptr::eq(
            bound.archive_delete.borrow().as_ref().unwrap().window(),
            filter.window()
        ),
        "repeated launch retains active filtering owner"
    );
    assert_eq!(filter.get_caption(), "1/2");
    filter.invoke_keep();
    ui.invoke_archive_delete_filter();
    assert_eq!(
        filter.get_caption(),
        "2/2",
        "existing decisions survive repeated launch"
    );
    let started = Instant::now();
    filter.invoke_delete();
    assert_eq!(filter.get_commit_labels().row_count(), 3);
    assert!(!filter.get_commit_ready());
    let labels = filter.get_commit_labels().iter().collect::<Vec<_>>();
    ui.invoke_archive_delete_filter();
    assert!(!filter.get_commit_ready());
    assert_eq!(
        filter.get_commit_labels().iter().collect::<Vec<_>>(),
        labels
    );
    let before = store
        .read(|conn| hydrus_store::media::current_domains(conn, &files))
        .unwrap();
    filter.invoke_commit();
    filter.invoke_commit_choice(1);
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        before
    );
    assert!(bound.archive_delete.borrow().is_some());
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 900, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("archive-delete-multiple-delayed.png"),
        &pixels,
        900,
        700,
    )
    .unwrap();
    filter.hide().unwrap();
    filter.invoke_commit_choice(1);
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        before
    );
    filter.show().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !filter.get_commit_ready() && Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::yield_now();
    }
    assert!(filter.get_commit_ready());
    assert!(started.elapsed() >= Duration::from_millis(1200));
    let expected = store.snapshot().services.get(ids[0]).unwrap().name.clone();
    let index = filter
        .get_commit_labels()
        .iter()
        .position(|label| label.as_str() == format!("delete 1 from {expected}?"))
        .unwrap();
    filter.invoke_commit_choice(i32::try_from(index).unwrap());
    assert!(bound.archive_delete.borrow().is_none());
    let after = store
        .read(|conn| hydrus_store::media::current_domains(conn, &files))
        .unwrap();
    assert!(!after[&files[1]].contains(&ids[0]));
    assert!(after[&files[1]].contains(&ids[1]));
    assert!(after[&files[0]].contains(&ids[0]));
    filter.show().unwrap();
    filter.invoke_commit_choice(0);
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        after
    );
    filter.hide().unwrap();
}
#[test]
fn saved_all_domains_single_choice_and_forget_question_are_owned_across_rebind() {
    let (_dir, store, files, ids) = setup();
    let windows = headless::init();
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &Preferences {
                    all_domains: true,
                    delay_multiple: true,
                },
            )
        })
        .unwrap();
    let (ui, bound) = ui(&store, &files, ids[0]);
    let filter = open_filter(&ui, &bound);
    filter.invoke_keep();
    filter.invoke_delete();
    assert_eq!(filter.get_commit_labels().row_count(), 1);
    assert!(filter.get_commit_ready());
    filter.invoke_forget();
    assert!(filter.get_forget_question());
    ui.invoke_archive_delete_filter();
    assert!(filter.get_forget_question());
    assert!(
        std::ptr::eq(
            bound.archive_delete.borrow().as_ref().unwrap().window(),
            filter.window()
        ),
        "repeated launch must not orphan Forget question"
    );
    let caption = filter.get_caption();
    filter.invoke_resume();
    filter.invoke_delete();
    filter.invoke_commit();
    assert_eq!(filter.get_caption(), caption);
    assert!(bound.archive_delete.borrow().is_some());
    filter.invoke_forget_answered(false);
    assert!(!filter.get_forget_question());
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 900, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive-delete-all-domains.png"),
        &pixels,
        900,
        700,
    )
    .unwrap();
    filter.invoke_commit();
    let after = store
        .read(|conn| hydrus_store::media::current_domains(conn, &files))
        .unwrap();
    assert!(!after[&files[1]].contains(&ids[0]));
    assert!(!after[&files[1]].contains(&ids[1]));
    let old = open_filter(&ui, &bound);
    old.invoke_keep();
    old.invoke_close_requested();
    assert!(old.get_commit_ready());
    let before = store
        .read(|conn| hydrus_store::media::current_domains(conn, &files))
        .unwrap();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    old.invoke_commit();
    old.invoke_forget();
    old.invoke_forget_answered(true);
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        before
    );
    assert!(successor.archive_delete.borrow().is_none());
    old.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    old.show().unwrap();
    old.invoke_commit();
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        before
    );
    old.hide().unwrap();
}
#[test]
fn retained_finish_cannot_write_after_its_main_owner_is_dropped() {
    let (_dir, store, files, ids) = setup();
    let _windows = headless::init();
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &Preferences {
                    all_domains: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let (ui, bound) = ui(&store, &files, ids[0]);
    let weak_main = ui.as_weak();
    let filter = open_filter(&ui, &bound);
    filter.invoke_keep();
    filter.invoke_delete();
    assert!(filter.get_commit_ready());
    let before = store
        .read(|conn| hydrus_store::media::current_domains(conn, &files))
        .unwrap();
    let inbox_count = || {
        store
            .read(|conn| {
                Ok(
                    conn.query_row("SELECT count(*) FROM file_inbox", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                )
            })
            .unwrap()
    };
    let inbox_before = inbox_count();
    ui.hide().unwrap();
    drop(ui);
    assert!(weak_main.upgrade().is_none());
    assert!(!filter.invoke_owner_valid());
    filter.invoke_commit();
    filter.invoke_commit_choice(0);
    assert_eq!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap(),
        before
    );
    assert_eq!(inbox_count(), inbox_before, "kept files must not archive");
    assert!(bound.archive_delete.borrow().is_some());
    filter.invoke_retire();
    assert!(bound.archive_delete.borrow().is_none());
}

#[test]
fn staged_options_cancel_save_hidden_callbacks_and_reopen_reach_next_finish() {
    let (dir, store, _files, _ids) = setup();
    let _windows = headless::init();
    let main = MainWindow::new().unwrap();
    main.show().unwrap();
    let bound = bind(&main, Pages::open(store.clone()).unwrap());
    main.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = main.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    main.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "files and trash")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = |label: &str| {
        i32::try_from(
            options
                .get_rows()
                .iter()
                .position(|row| row.label == label)
                .unwrap(),
        )
        .unwrap()
    };
    let all =
        row("When finishing archive/delete filtering, always delete from all possible domains: ");
    let delay = row(
        "When finishing archive/delete filtering, delay activation of multiple deletion choice buttons: ",
    );
    options.hide().unwrap();
    options.invoke_check_toggled(all, true);
    options.invoke_check_toggled(delay, false);
    options.show().unwrap();
    options.invoke_apply();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences::default()
    );
    options.invoke_check_toggled(all, true);
    options.invoke_check_toggled(delay, false);
    options.invoke_cancel();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences::default()
    );
    // Retained cancelled owner cannot resurrect or save its abandoned policy.
    options.show().unwrap();
    options.invoke_apply();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences::default()
    );
    options.hide().unwrap();
    main.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = main.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    main.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let saved_owner = bound.options.borrow().as_ref().unwrap().clone_strong();
    saved_owner.invoke_page_chosen(i32::try_from(page).unwrap());
    saved_owner.invoke_check_toggled(all, true);
    saved_owner.invoke_check_toggled(delay, false);
    saved_owner.invoke_apply();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences {
            all_domains: true,
            delay_multiple: false
        }
    );
    options.show().unwrap();
    options.invoke_check_toggled(all, false);
    options.invoke_apply();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences {
            all_domains: true,
            delay_multiple: false
        }
    );
    options.hide().unwrap();
    saved_owner.invoke_cancel();
    drop(saved_owner);
    drop(bound);
    drop(main);
    drop(options);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.read(archive_delete_preferences::load).unwrap(),
        Preferences {
            all_domains: true,
            delay_multiple: false
        }
    );
}
