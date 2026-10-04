//! The media viewer's file shortcuts, as the reference's defaults have
//! them: F7 archives, shift+F7 inboxes, delete sends the file to the trash
//! (asking first; it then leaves the page and the viewer), shift+delete
//! undeletes, and on a trash page delete deletes for good.

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::HashId;
use hydrus_gui::media_actions::{self, Deletion};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::LocationContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// (in the inbox, the local domains it is in by name)
fn state(store: &Store, id: HashId) -> (bool, Vec<String>) {
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
        .unwrap();
    let m = &batch.results[0];
    let mut domains: Vec<String> = m
        .current
        .iter()
        .filter_map(|c| snapshot.services.get(c.service).ok())
        .filter(|s| {
            matches!(
                s.service_type(),
                hydrus_core::ServiceType::LocalFileDomain
                    | hydrus_core::ServiceType::LocalFileTrashDomain
            )
        })
        .map(|s| s.name.clone())
        .collect();
    domains.sort();
    (m.inbox, domains)
}

#[test]
fn the_viewer_s_shortcuts_archive_inbox_and_delete() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();

    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let before = page.borrow().results().to_vec();
    assert!(before.len() > 2);
    // a file only in "my files"
    let index = before
        .iter()
        .position(|&id| state(&store, id).1 == ["my files"])
        .unwrap();
    let file = before[index];
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let caption = viewer.get_caption();

    // F7 and shift+F7
    viewer.invoke_archive();
    assert_eq!(state(&store, file), (false, vec!["my files".into()]));
    viewer.invoke_inbox();
    assert_eq!(state(&store, file), (true, vec!["my files".into()]));

    // delete asks first (from a page on all local files, to the trash; on
    // one domain, from it); no leaves it be
    let my_files = store
        .snapshot()
        .services
        .by_name("my files")
        .unwrap()
        .clone();
    assert_eq!(
        media_actions::deletion(
            &store,
            &LocationContext::single(my_files.key.clone()),
            &[file]
        ),
        Some(Deletion::FromDomain {
            domain: my_files.id,
            name: "my files".into()
        })
    );
    viewer.invoke_delete();
    assert_eq!(viewer.get_question(), "Send this file to the trash?");
    viewer.invoke_answer(false);
    assert_eq!(viewer.get_question(), "");
    assert_eq!(state(&store, file).1, ["my files"]);
    // yes sends it to the trash, and it leaves the page and the viewer
    viewer.invoke_delete();
    viewer.invoke_answer(true);
    assert_eq!(state(&store, file).1, ["trash"]);
    let after = page.borrow().results().to_vec();
    assert_eq!(after.len(), before.len() - 1);
    assert!(!after.contains(&file));
    assert_ne!(viewer.get_caption(), caption);
    assert!(viewer.get_caption().ends_with(&format!("/{}", after.len())));
    let deleted_reason: Option<String> = store
        .read(|c| {
            let snapshot = store.snapshot();
            Ok(
                hydrus_store::media::load(c, &snapshot.services, None, &[file])?.results[0]
                    .deletion_reason
                    .clone(),
            )
        })
        .unwrap();
    assert_eq!(
        deleted_reason.as_deref(),
        Some(media_actions::DELETE_REASON)
    );
    assert!(
        ui.get_status()
            .starts_with(&format!("{} files - totalling ", after.len())),
        "{}",
        ui.get_status()
    );

    // shift+delete restores it
    media_actions::undelete(&store, &[file]).unwrap();
    assert_eq!(state(&store, file).1, ["my files"]);

    // on a trash page, delete deletes for good (unless the delete lock
    // holds the file)
    media_actions::delete(&store, &[file], &Deletion::ToTrash).unwrap();
    let trash = LocationContext::single(hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::TRASH.to_vec(),
    ));
    let deletion = media_actions::deletion(&store, &trash, &[file]).unwrap();
    assert_eq!(deletion, Deletion::Physically);
    assert_eq!(deletion.question(1), "Permanently delete this file?");
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::delete_lock::DeleteLock {
                    archived: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    media_actions::archive(&store, &[file]).unwrap();
    media_actions::delete(&store, &[file], &deletion).unwrap();
    assert_eq!(state(&store, file).1, ["trash"], "the lock holds it");
    media_actions::inbox(&store, &[file]).unwrap();
    media_actions::delete(&store, &[file], &deletion).unwrap();
    assert!(state(&store, file).1.is_empty());
    assert!(media_actions::deletion(&store, &trash, &[file]).is_none());

    viewer.invoke_close_requested();
    assert!(ui.get_thumbnail_rows().row_count() > 0);

    // the thumbnails' shortcuts, once a click gives them the keyboard
    let main_window = windows.get(0).unwrap();
    headless::render(&main_window, 1100, 700);
    let first = slint::LogicalPosition::new(300.0 + 4.0 + 76.0, 4.0 + 63.0);
    for event in [
        slint::platform::WindowEvent::PointerPressed {
            position: first,
            button: slint::platform::PointerEventButton::Left,
        },
        slint::platform::WindowEvent::PointerReleased {
            position: first,
            button: slint::platform::PointerEventButton::Left,
        },
    ] {
        main_window.dispatch_event(event);
    }
    let selected = {
        let page = page.borrow();
        let selected = page.selected_files();
        assert_eq!(selected.len(), 1, "a click selects");
        selected[0]
    };
    let key = |key: slint::platform::Key| {
        let text: slint::SharedString = key.into();
        main_window.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
        main_window.dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
    };
    assert!(state(&store, selected).0);
    key(slint::platform::Key::F7);
    assert!(!state(&store, selected).0, "F7 archives it");
    ui.invoke_inbox_selected();
    assert!(state(&store, selected).0);
    let before = page.borrow().results().len();
    key(slint::platform::Key::Delete);
    assert_eq!(ui.get_question(), "Send this file to the trash?");
    key(slint::platform::Key::Escape);
    assert_eq!(ui.get_question(), "");
    assert_eq!(page.borrow().results().len(), before);
    key(slint::platform::Key::Delete);
    key(slint::platform::Key::Return);
    assert_eq!(ui.get_question(), "");
    assert_eq!(state(&store, selected).1, ["trash"]);
    assert_eq!(page.borrow().results().len(), before - 1);
}

#[test]
fn confirmation_policy_matches_recorded_actionable_domains_and_counts() {
    use hydrus_store::settings::DeletionPreferences;
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let recording = hydrus_testkit::fixture_json("files_trash.json");
    let mut page = super::common::all_local_page(store.clone());
    page.enter();
    let files = page.results().to_vec();
    let snapshot = store.snapshot();
    let roles = hydrus_store::content::DomainRoles::new(&snapshot.services).unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &files))
        .unwrap();
    for case in recording["deletion"].as_array().unwrap() {
        let count = usize::try_from(case["domains"].as_u64().unwrap()).unwrap();
        let file = batch
            .results
            .iter()
            .find(|m| {
                m.current
                    .iter()
                    .filter(|c| roles.local.contains(&c.service))
                    .count()
                    == count
            })
            .unwrap()
            .hash_id;
        let preferences = DeletionPreferences {
            confirm_trash: case["confirm"].as_bool().unwrap(),
            ..DeletionPreferences::default()
        };
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &preferences))
            .unwrap();
        assert_eq!(
            !media_actions::confirm_deletion(&store, &[file], &Deletion::ToTrash),
            case["resolved"].as_bool().unwrap()
        );
        assert!(media_actions::confirm_deletion(
            &store,
            &[file],
            &Deletion::Physically
        ));
    }
    for case in recording["archive"].as_array().unwrap() {
        let preferences = DeletionPreferences {
            confirm_archive: case["confirm"].as_bool().unwrap(),
            ..DeletionPreferences::default()
        };
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &preferences))
            .unwrap();
        assert_eq!(
            media_actions::confirm_archive(
                &store,
                usize::try_from(case["count"].as_u64().unwrap()).unwrap()
            ),
            !case["questions"].as_array().unwrap().is_empty()
        );
    }
}

#[test]
fn closing_viewer_discards_advanced_deletion_and_reopening_recalls_applied_choice() {
    use hydrus_gui::delete_files_window;
    use hydrus_store::settings::DeletionPreferences;
    use slint::ComponentHandle as _;
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &DeletionPreferences {
                    advanced: true,
                    remember_action: true,
                    ..DeletionPreferences::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let index = page
        .borrow()
        .results()
        .iter()
        .position(|&file| state(&store, file).1 == ["my files"])
        .unwrap();
    let file = page.borrow().results()[index];
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_delete();
    let child = delete_files_window::last_opened().unwrap();
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 640);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    let before: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
    viewer.invoke_close_requested();
    assert!(!child.window().is_visible());
    child.set_custom("closed owner".into());
    child.invoke_accept_deletion();
    assert_eq!(state(&store, file).1, ["my files"]);
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<DeletionPreferences>)
            .unwrap(),
        before
    );
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_delete();
    let child = delete_files_window::last_opened().unwrap();
    let index = (0..child.get_reasons().row_count())
        .find(|&i| child.get_reasons().row_data(i).unwrap() == "custom")
        .unwrap();
    child.invoke_reason_selected(i32::try_from(index).unwrap());
    child.set_custom("accepted viewer reason".into());
    child.invoke_accept_deletion();
    assert_eq!(state(&store, file).1, ["trash"]);
    let prefs: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(prefs.last_reason.as_deref(), Some("accepted viewer reason"));
    let slot = delete_files_window::Slot::default();
    let window = delete_files_window::open(
        &slot,
        &store,
        &[file],
        None,
        media_actions::DELETE_REASON,
        std::rc::Rc::new(|| true),
        std::rc::Rc::new(|| {}),
    )
    .unwrap()
    .unwrap();
    assert!(
        window
            .get_reasons()
            .row_data(usize::try_from(window.get_selected_reason()).unwrap())
            .unwrap()
            .starts_with("keep existing reason:")
    );
    assert!(
        window
            .get_actions()
            .row_data(usize::try_from(window.get_selected_action()).unwrap())
            .unwrap()
            .starts_with("Permanently delete")
    );
    window.invoke_cancel();
    viewer.invoke_close_requested();
}
