//! Real staged Options and stable owning-page filter/trash/move consumers.
use hydrus_core::{HashId, Sha256};
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    content::DomainRoles,
    settings::{self, FileViewRemoval},
};
use slint::{ComponentHandle as _, Model as _};
use std::rc::Rc;
const LABELS: [&str; 4] = [
    "Remove files from view when they are archive/delete filtered: ",
    "--even skipped files: ",
    "Remove files from view when they are sent to the trash: ",
    "Remove files from view when they are moved to another local file domain: ",
];
fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|tx| {
            settings::set(
                tx.conn(),
                &hydrus_store::archive_delete_preferences::Preferences {
                    delay_multiple: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    (dir, store)
}
fn ids(store: &Store) -> Vec<HashId> {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    fixture["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let hash: Sha256 = v.as_str().unwrap().parse().unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}
fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines.iter().position(|r| r.label == "options…").unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let at = window
        .get_pages()
        .iter()
        .position(|p| p.text == "files and trash")
        .unwrap();
    window.invoke_page_chosen(at as i32);
    window
}
fn row(window: &OptionsWindow, i: usize) -> i32 {
    window
        .get_rows()
        .iter()
        .position(|r| r.label == LABELS[i])
        .unwrap() as i32
}
fn edit(window: &OptionsWindow, values: &serde_json::Value) {
    window.invoke_check_toggled(row(window, 0), true);
    window.invoke_check_toggled(row(window, 1), values[1].as_bool().unwrap());
    for i in [0, 2, 3] {
        window.invoke_check_toggled(row(window, i), values[i].as_bool().unwrap());
    }
}
fn saved(store: &Store) -> serde_json::Value {
    let p: FileViewRemoval = store.read(settings::get).unwrap();
    serde_json::json!([p.filtered, p.skipped, p.trashed, p.moved])
}
fn reset(store: &Store, files: &[HashId]) {
    let files = files.to_vec();
    store
        .write_content(move |w| {
            let source = w.snapshot().services.by_name("art").unwrap().id;
            let dest = w.snapshot().services.by_name("my files").unwrap().id;
            w.add_files(
                source,
                &files
                    .iter()
                    .map(|&id| (id, Some(1_234_567_890_000)))
                    .collect::<Vec<_>>(),
            )?;
            w.delete_files(dest, &files, None)?;
            w.inbox(&files)
        })
        .unwrap();
}
fn icons(bound: &Bound, file: HashId) -> Vec<i32> {
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|&id| id == file)
        .unwrap();
    bound
        .rows
        .iter()
        .flat_map(|row| row.thumbnails.iter().collect::<Vec<_>>())
        .nth(index)
        .unwrap()
        .icons
        .iter()
        .map(|icon| icon.kind)
        .collect()
}
#[test]
fn options_match_actual_qt_dependency_apply_cancel_reopen_and_retired_callbacks() {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    let (dir, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    assert_eq!(saved(&store), fixture["initial"]);
    for case in fixture["settings"].as_array().unwrap() {
        let before = saved(&store);
        let cancelled = options(&ui, &bound);
        edit(&cancelled, &case["values"]);
        assert_eq!(saved(&store), before);
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        assert_eq!(saved(&store), before);
        let accepted = options(&ui, &bound);
        edit(&accepted, &case["values"]);
        assert_eq!(
            accepted
                .get_rows()
                .row_data(row(&accepted, 1) as usize)
                .unwrap()
                .enabled,
            case["skipped_enabled"].as_bool().unwrap()
        );
        accepted.invoke_apply();
        assert_eq!(saved(&store), case["persisted"]);
        let reopened = options(&ui, &bound);
        for (i, label) in LABELS.iter().enumerate() {
            assert_eq!(
                reopened
                    .get_rows()
                    .iter()
                    .find(|r| r.label == *label)
                    .unwrap()
                    .checked,
                case["values"][i].as_bool().unwrap()
            );
        }
        reopened.invoke_cancel();
    }
    let window = options(&ui, &bound);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1100, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("files-view-removal.png"),
        &pixels,
        1100,
        850,
    )
    .unwrap();
    window.invoke_cancel();
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(saved(&reopened), saved(&store));
}
#[test]
fn filter_accept_forget_resume_switch_and_closed_source_never_target_successor() {
    let fixture = hydrus_testkit::fixture_json("files_view_removal.json");
    let (_dir, store) = store();
    let files = ids(&store);
    let _windows = headless::init();
    for case in fixture["filters"].as_array().unwrap() {
        reset(&store, &files);
        let prefs = FileViewRemoval {
            filtered: case["remove"].as_bool().unwrap(),
            skipped: case["skipped"].as_bool().unwrap(),
            ..Default::default()
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &prefs))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(
            &ui,
            Pages::single(super::common::all_local_page(store.clone())),
        );
        ui.show().unwrap();
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        let owner = bound.current.borrow().clone();
        owner.borrow_mut().select_files(&files);
        let ordered = owner.borrow().selected_files();
        assert!(
            icons(&bound, ordered[0]).contains(&hydrus_gui::thumbnail_icons::Icon::Inbox.code())
        );
        ui.invoke_archive_delete_filter();
        let filter = bound
            .archive_delete
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        filter.invoke_keep();
        filter.invoke_delete();
        filter.invoke_skip();
        let before = owner.borrow().files();
        match case["outcome"].as_str().unwrap() {
            "accept" => {
                filter.invoke_commit();
                let mut expected = before.clone();
                if prefs.filtered {
                    expected.retain(|f| {
                        !ordered[..2].contains(f) && (!prefs.skipped || *f != ordered[2])
                    });
                }
                assert_eq!(owner.borrow().files(), expected);
                if !prefs.filtered {
                    assert!(
                        !icons(&bound, ordered[0])
                            .contains(&hydrus_gui::thumbnail_icons::Icon::Inbox.code()),
                        "retained archived icon refreshes without rerunning query"
                    );
                }
                if prefs.filtered && !prefs.skipped {
                    assert_eq!(owner.borrow().selected_files(), [ordered[2]]);
                }
            }
            "forget" => {
                filter.invoke_close_requested();
                filter.invoke_forget();
                filter.invoke_forget_answered(true);
                assert_eq!(owner.borrow().files(), before);
            }
            "cancel" => {
                filter.invoke_resume();
                assert!(filter.get_question().is_empty());
                assert_eq!(owner.borrow().files(), before);
                filter.invoke_close_requested();
                filter.invoke_forget();
                filter.invoke_forget_answered(true);
            }
            _ => panic!(),
        }
        let after = owner.borrow().files();
        filter.invoke_commit();
        filter.invoke_keep();
        assert_eq!(owner.borrow().files(), after);
        ui.hide().unwrap();
    }
    reset(&store, &files);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewRemoval {
                    filtered: true,
                    skipped: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let owner = bound.current.borrow().clone();
    owner.borrow_mut().select_files(&files);
    let kept = owner.borrow().selected_files()[0];
    let owner_before = owner.borrow().files();
    ui.invoke_archive_delete_filter();
    let filter = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    filter.invoke_keep();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    let successor = bound.current.borrow().clone();
    assert!(!Rc::ptr_eq(&owner, &successor));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let before = successor.borrow().files();
    filter.invoke_close_requested();
    filter.hide().unwrap();
    filter.invoke_commit();
    assert_eq!(
        owner.borrow().files(),
        owner_before,
        "hidden filter cannot commit"
    );
    filter.show().unwrap();
    filter.invoke_commit();
    assert_eq!(successor.borrow().files(), before);
    assert!(!owner.borrow().files().contains(&kept));
    assert_eq!(owner.borrow().files().len(), owner_before.len() - 1);
    ui.invoke_tab_chosen(0, 0);
    owner.borrow_mut().select_files(&files[1..]);
    ui.invoke_archive_delete_filter();
    let closed = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    closed.invoke_keep();
    bound.pages.borrow_mut().close(0, 0).unwrap();
    ui.invoke_tab_chosen(0, 0);
    let before = successor.borrow().files();
    closed.invoke_commit();
    assert_eq!(successor.borrow().files(), before);
    closed.invoke_forget();
}
#[test]
fn live_trash_policy_uses_actual_membership_and_move_menu_only_removes_source() {
    let (_dir, store) = store();
    let files = ids(&store);
    let _windows = headless::init();
    reset(&store, &files);
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let owner = bound.current.borrow().clone();
    // Prime the actual native grid cache before the successful retained-row write.
    assert!(!icons(&bound, files[0]).contains(&hydrus_gui::thumbnail_icons::Icon::Trash.code()));
    // The saved default keeps a trashed row and selection. Enabling the option updates this live page.
    owner.borrow_mut().select_files(&files[..1]);
    ui.invoke_delete_selected();
    ui.invoke_answer(true);
    assert!(owner.borrow().files().contains(&files[0]));
    assert_eq!(owner.borrow().selected_files(), [files[0]]);
    assert!(
        icons(&bound, files[0]).contains(&hydrus_gui::thumbnail_icons::Icon::Trash.code()),
        "retained trash icon refreshes without querying again"
    );
    let options = options(&ui, &bound);
    options.invoke_check_toggled(row(&options, 2), true);
    options.invoke_check_toggled(row(&options, 3), true);
    options.invoke_apply();
    owner.borrow_mut().select_files(&files[1..2]);
    ui.invoke_delete_selected();
    ui.invoke_answer(true);
    assert!(!owner.borrow().files().contains(&files[1]));
    assert!(owner.borrow().files().contains(&files[0]));
    // One remaining local membership keeps the file visible even though a domain delete occurred.
    let roles = DomainRoles::new(&store.snapshot().services).unwrap();
    let source = store.snapshot().services.by_name("art").unwrap().id;
    let dest = store.snapshot().services.by_name("my files").unwrap().id;
    let file = files[2];
    store
        .write_content(move |w| w.add_files(dest, &[(file, Some(1_234_567_890_000))]))
        .unwrap();
    let location = hydrus_search::LocationContext::single(
        store.snapshot().services.get(source).unwrap().key.clone(),
    );
    owner.borrow_mut().choose_location(location);
    owner.borrow_mut().select_files(&[file]);
    ui.invoke_delete_selected();
    ui.invoke_answer(true);
    assert!(owner.borrow().files().contains(&file));
    let current = store
        .read(|c| hydrus_store::media::current_domains(c, &[file]))
        .unwrap();
    assert!(current[&file].contains(&dest));
    assert!(!current[&file].contains(&roles.trash));
    // Restore one source-only file and use actual strict move menu/owned question.
    reset(&store, &[file]);
    owner.borrow_mut().select_files(&[file]);
    ui.invoke_thumbnail_menu_requested(-1);
    let copy = ui
        .get_thumbnail_menu()
        .locations_copy
        .iter()
        .find(|r| r.label.starts_with("my files"))
        .unwrap()
        .id;
    ui.invoke_menu_chosen(copy);
    let question = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    question.invoke_answer(true);
    assert!(
        owner.borrow().files().contains(&file),
        "copy never applies the move-removal preference"
    );
    reset(&store, &[file]);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    let at = menu
        .locations_move
        .iter()
        .find(|r| r.label.starts_with("from art to my files"))
        .unwrap()
        .id;
    ui.invoke_menu_chosen(at);
    let question = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    question.invoke_answer(false);
    assert!(owner.borrow().files().contains(&file));
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    let at = menu
        .locations_move
        .iter()
        .find(|r| r.label.starts_with("from art to my files"))
        .unwrap()
        .id;
    ui.invoke_menu_chosen(at);
    let question = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    store
        .write_content(move |w| w.add_files(dest, &[(file, Some(1234))]))
        .unwrap();
    let before = owner.borrow().files();
    let selected = owner.borrow().selected_files();
    question.invoke_answer(true);
    assert_eq!(
        owner.borrow().files(),
        before,
        "strict destination race is a successful no-op"
    );
    assert_eq!(owner.borrow().selected_files(), selected);
    reset(&store, &[file]);
    ui.invoke_thumbnail_menu_requested(-1);
    let action = ui
        .get_thumbnail_menu()
        .locations_move
        .iter()
        .find(|r| r.label.starts_with("from art to my files"))
        .unwrap()
        .id;
    ui.invoke_menu_chosen(action);
    let accepted = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    accepted.invoke_answer(true);
    assert!(!owner.borrow().files().contains(&file));
    let current = store
        .read(|c| hydrus_store::media::current_domains(c, &[file]))
        .unwrap();
    assert!(current[&file].contains(&dest));
    assert!(!current[&file].contains(&source));
    assert!(!current[&file].contains(&roles.trash));
    reset(&store, &[file]);
    owner.borrow_mut().refresh();
    owner.borrow_mut().select_files(&[file]);
    ui.invoke_thumbnail_menu_requested(-1);
    let action = ui
        .get_thumbnail_menu()
        .locations_move
        .iter()
        .find(|r| r.label.starts_with("from art to my files"))
        .unwrap()
        .id;
    ui.invoke_menu_chosen(action);
    let failed = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let before = owner.borrow().files();
    let selected = owner.borrow().selected_files();
    store
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "DELETE FROM file_domain_current WHERE service_id=?1",
                [dest],
            )?;
            ctx.conn().execute(
                "DELETE FROM file_domain_deleted WHERE service_id=?1",
                [dest],
            )?;
            hydrus_store::services::delete(ctx.conn(), dest)?;
            hydrus_store::services::insert_with_id(
                ctx.conn(),
                dest,
                &hydrus_core::ServiceKey::new(vec![249; 32]),
                "replacement local domain",
                &hydrus_store::services::ServiceKind::LocalFiles,
            )
        })
        .unwrap();
    failed.invoke_answer(true);
    assert!(!failed.get_error().is_empty());
    assert_eq!(owner.borrow().files(), before);
    assert_eq!(owner.borrow().selected_files(), selected);
    failed.invoke_cancel();
}
#[test]
fn mixed_already_trash_advanced_choice_and_locked_physical_noop_retain_unaffected_rows() {
    let (_dir, store) = store();
    let files = ids(&store);
    reset(&store, &files);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    page.borrow_mut().select_files(&files[..1]);
    ui.invoke_delete_selected();
    ui.invoke_answer(true);
    assert!(page.borrow().files().contains(&files[0]));
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewRemoval {
                    trashed: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    page.borrow_mut().select_files(&files[..2]);
    ui.invoke_delete_selected();
    ui.invoke_answer(true);
    assert!(
        page.borrow().files().contains(&files[0]),
        "previously trashed no-op row is untouched"
    );
    assert!(!page.borrow().files().contains(&files[1]));
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &settings::DeletionPreferences {
                    advanced: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    page.borrow_mut().select_files(&[files[0], files[2]]);
    ui.invoke_delete_selected();
    let child = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
    let action = child
        .get_actions()
        .iter()
        .position(|label| label.contains("from art"))
        .unwrap();
    child.invoke_action_selected(action as i32);
    child.invoke_accept_deletion();
    assert!(
        page.borrow().files().contains(&files[0]),
        "accepted domain choice excludes unrelated existing-trash row"
    );
    assert!(!page.borrow().files().contains(&files[2]));
    // Physical deletion's archive lock may turn an accepted pending action into a no-op.
    store
        .write(|ctx| settings::set(ctx.conn(), &settings::DeletionPreferences::default()))
        .unwrap();
    let roles = DomainRoles::new(&store.snapshot().services).unwrap();
    let location = hydrus_search::LocationContext::single(
        store
            .snapshot()
            .services
            .get(roles.trash)
            .unwrap()
            .key
            .clone(),
    );
    page.borrow_mut().choose_location(location);
    page.borrow_mut().select_files(&files[..1]);
    ui.invoke_delete_selected();
    let file = files[0];
    store.write_content(move |w| w.archive(&[file])).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_store::delete_lock::DeleteLock {
                    archived: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let before = page.borrow().files();
    let selected = page.borrow().selected_files();
    ui.invoke_answer(true);
    assert_eq!(page.borrow().files(), before);
    assert_eq!(page.borrow().selected_files(), selected);
    assert!(
        store
            .read(|c| hydrus_store::media::current_domains(c, &[file]))
            .unwrap()[&file]
            .contains(&roles.local_file_storage)
    );
}

#[test]
fn retained_delete_and_transfer_children_cannot_mutate_a_rebound_main_window() {
    let _windows = headless::init();
    for transfer in [false, true] {
        let (_dir, store) = store();
        let files = ids(&store);
        reset(&store, &files);
        if !transfer {
            store
                .write(|ctx| {
                    settings::set(
                        ctx.conn(),
                        &settings::DeletionPreferences {
                            advanced: true,
                            ..Default::default()
                        },
                    )
                })
                .unwrap();
        }
        let ui = MainWindow::new().unwrap();
        let original = bind(
            &ui,
            Pages::single(super::common::all_local_page(store.clone())),
        );
        ui.show().unwrap();
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        let owner = original.current.borrow().clone();
        owner.borrow_mut().select_files(&files[..1]);
        let stale_accept: Rc<dyn Fn()> = if transfer {
            ui.invoke_thumbnail_menu_requested(-1);
            let action = ui
                .get_thumbnail_menu()
                .locations_move
                .iter()
                .find(|row| row.label.starts_with("from art to my files"))
                .unwrap()
                .id;
            ui.invoke_menu_chosen(action);
            let child = original
                .local_transfer
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            Rc::new(move || child.invoke_answer(true))
        } else {
            ui.invoke_delete_selected();
            let child = original
                .delete_files
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            let action = child
                .get_actions()
                .iter()
                .position(|label| label.contains("from art"))
                .unwrap();
            child.invoke_action_selected(action as i32);
            Rc::new(move || child.invoke_accept_deletion())
        };
        let domains = store
            .read(|conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap();
        let owner_files = owner.borrow().files();
        let successor = bind(
            &ui,
            Pages::single(super::common::all_local_page(store.clone())),
        );
        ui.invoke_search_edited("successor query draft".into());
        let successor_files = successor.current.borrow().borrow().files();
        stale_accept();
        assert_eq!(
            store
                .read(|conn| hydrus_store::media::current_domains(conn, &files))
                .unwrap(),
            domains,
            "retired child must not delete or transfer content"
        );
        assert_eq!(owner.borrow().files(), owner_files);
        assert_eq!(successor.current.borrow().borrow().files(), successor_files);
        assert_eq!(ui.get_search_text(), "successor query draft");
        ui.hide().unwrap();
    }
}

#[test]
fn viewer_deletion_never_prunes_an_externally_retained_forgotten_source_or_successor() {
    let _windows = headless::init();
    let (_dir, store) = store();
    let files = ids(&store);
    reset(&store, &files);
    super::common::remove_trashed_from_view(&store);
    store
        .write(|ctx| {
            let mut preferences: settings::DeletionPreferences = settings::get(ctx.conn())?;
            preferences.advanced = false;
            preferences.confirm_trash = true;
            settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let owner = bound.current.borrow().clone();
    let before_owner = owner.borrow().files();
    let file = files[0];
    let index = owner
        .borrow()
        .results()
        .iter()
        .position(|&id| id == file)
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_delete();
    assert!(!viewer.get_question().is_empty());
    bound
        .pages
        .borrow_mut()
        .save_session("retained deletion source", 1_700_000_000)
        .unwrap();
    bound
        .pages
        .borrow_mut()
        .clear_and_load("retained deletion source")
        .unwrap();
    ui.invoke_tab_chosen(0, 0);
    let successor = bound.current.borrow().clone();
    assert!(!Rc::ptr_eq(&owner, &successor));
    let before_successor = successor.borrow().files();
    viewer.invoke_answer(true);
    assert_eq!(owner.borrow().files(), before_owner);
    assert_eq!(successor.borrow().files(), before_successor);
    let roles = DomainRoles::new(&store.snapshot().services).unwrap();
    assert!(
        store
            .read(|conn| hydrus_store::media::current_domains(conn, &[file]))
            .unwrap()[&file]
            .contains(&roles.trash),
        "the live viewer deleted its file while the forgotten page and successor retained rows"
    );
    viewer.invoke_close_requested();
}
