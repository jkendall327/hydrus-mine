//! Real thumbnail menus/questions, scoped Store writes and retired owner answers.
use hydrus_core::{HashId, ServiceKey, search::context::FileSearchContext};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_gui_model::selected_deletion_records::Plan;
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{path::Path, sync::Arc};
#[path = "../../../hydrus-gui-model/tests/support/selected_deletion_records_seed.rs"]
mod seed;

fn page(store: Arc<Store>, files: &[HashId]) -> SearchPage {
    let hashes = store
        .read(|conn| hydrus_store::media::load_basic(conn, files))
        .unwrap()
        .into_iter()
        .map(|media| media.hash)
        .collect();
    SearchPage::restored(
        store,
        FileSearchContext {
            location: hydrus_search::LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec(),
            )),
            predicates: vec![hydrus_search::Predicate::System(
                hydrus_search::SystemPredicate::Hash {
                    hashes: hydrus_search::FileHashes::Sha256(hashes),
                    inclusive: true,
                },
            )],
            ..Default::default()
        },
        true,
        None,
        files.to_vec(),
    )
}
fn select(ui: &MainWindow, bound: &Bound, files: &[HashId]) -> Option<(String, i32)> {
    bound.current.borrow().borrow_mut().select_files(files);
    ui.invoke_thumbnail_menu_requested(-1);
    ui.get_thumbnail_menu()
        .trash
        .iter()
        .find(|row| row.label.starts_with("clear deletion record"))
        .map(|row| (row.label.to_string(), row.id))
}

#[test]
fn actual_menu_and_answer_replay_clear_only_captured_records_and_enable_reimport() {
    let fixture = hydrus_testkit::fixture_json("selected_deletion_records.json");
    let (_dirs, store) = seed::store();
    let files = seed::files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page(store.clone(), &files)));
    for case in fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["new_renderer"] == true && case["case"] != "collection")
    {
        seed::reset(&store, &files);
        let indices: &[usize] = match case["case"].as_str().unwrap() {
            "none" => &[],
            "one_deleted" => &[0],
            "two_deleted" => &[0, 1],
            "mixed" => &[0, 2, 3],
            "trash" => &[2],
            "current" => &[3],
            other => panic!("unknown recorded case {other}"),
        };
        let selected: Vec<_> = indices.iter().map(|&i| files[i]).collect();
        let ours = select(&ui, &bound, &selected)
            .map(|(label, _)| label)
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(ours), case["labels"]);
    }
    seed::reset(&store, &files);
    let (label, id) = select(&ui, &bound, &files[..1]).unwrap();
    bound
        .current
        .borrow()
        .borrow_mut()
        .select_files(&files[..2]);
    ui.invoke_menu_chosen(id);
    let dispatch = &fixture["dispatch"][1];
    assert_eq!(label, dispatch["label"]);
    assert_eq!(
        ui.get_question(),
        dispatch["questions"][0]["message"].as_str().unwrap()
    );
    ui.invoke_answer(false);
    assert_eq!(
        seed::state(&store, &files)["status"],
        serde_json::json!([3, 3, 3, 2])
    );
    for case in fixture["actions"].as_array().unwrap() {
        seed::reset(&store, &files);
        let selected: Vec<_> = case["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        let item = select(&ui, &bound, &selected);
        if let Some((_, id)) = item {
            ui.invoke_menu_chosen(id);
        } else {
            assert!(case["questions"].as_array().unwrap().is_empty());
        }
        let questions = case["questions"].as_array().unwrap();
        if let Some(question) = questions.first() {
            assert_eq!(
                ui.get_question().as_str(),
                question["message"].as_str().unwrap()
            );
        }
        let local = store.snapshot().services.by_name("my files").unwrap().id;
        match case["case"].as_str().unwrap() {
            "readd_during_question" => {
                let file = files[0];
                store
                    .write_content(move |writer| {
                        writer.add_files(local, &[(file, Some(1_700_000_000_100))])
                    })
                    .unwrap();
            }
            "new_delete_during_question" => {
                let file = files[3];
                store
                    .write_content(move |writer| {
                        writer.delete_files(writer.roles().combined_local_media, &[file], None)?;
                        writer.delete_files(writer.roles().local_file_storage, &[file], None)
                    })
                    .unwrap();
            }
            _ => {}
        }
        let queued = seed::queue(&store);
        if case["case"] == "mixed" {
            let adapter = windows.get(0).unwrap();
            let pixels = headless::render(&adapter, 1100, 700);
            assert!(ui.get_question_visible());
            headless::save_png(
                &Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("selected_deletion_records_question.png"),
                &pixels,
                1100,
                700,
            )
            .unwrap();
        }
        ui.invoke_answer(case["accepted"].as_bool().unwrap());
        assert_eq!(
            seed::state(&store, &files),
            case["after"],
            "{}",
            case["case"]
        );
        assert_eq!(seed::queue(&store), queued);
        assert!(ui.get_question().is_empty());
    }
    seed::reset(&store, &files);
    let owner = bound.current.borrow().clone();
    owner
        .borrow_mut()
        .set_collect(hydrus_core::pages::PageCollect {
            namespaces: vec!["parity-unmatched-collection".into()],
            collect_unmatched: true,
            ..Default::default()
        });
    assert_eq!(owner.borrow().results().len(), 1);
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_menu_requested(-1);
    let item = ui
        .get_thumbnail_menu()
        .trash
        .iter()
        .find(|row| row.label.starts_with("clear deletion record"))
        .unwrap();
    assert_eq!(item.label, "clear deletion record for selected");
    assert_eq!(owner.borrow().selected_files().len(), 4);
    ui.invoke_menu_chosen(item.id);
    assert_eq!(
        ui.get_question(),
        fixture["collections"][1]["questions"][0]["message"]
            .as_str()
            .unwrap()
    );
    ui.invoke_answer(true);
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    let hash: hydrus_core::Sha256 = fixture["corpus"][0].as_str().unwrap().parse().unwrap();
    assert_eq!(
        importer.known_status(&hash, "").unwrap().0.status,
        hydrus_import::status::ImportStatus::Unknown
    );
    assert_eq!(
        seed::state(&store, &files)["status"],
        serde_json::json!([0, 0, 3, 2])
    );
}

#[test]
fn hidden_stale_page_rebound_and_accepted_exit_cannot_clear_captured_records() {
    let (_dirs, store) = seed::store();
    let files = seed::files(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page(store.clone(), &files)));
    seed::reset(&store, &files);
    let (_, id) = select(&ui, &bound, &files[..2]).unwrap();
    let before = seed::state(&store, &files);
    ui.hide().unwrap();
    ui.invoke_menu_chosen(id);
    assert!(ui.get_question().is_empty());
    ui.show().unwrap();
    ui.invoke_menu_chosen(id);
    assert!(!ui.get_question().is_empty());
    ui.hide().unwrap();
    ui.invoke_answer(true);
    assert_eq!(seed::state(&store, &files), before);
    ui.show().unwrap();
    let (_, id) = select(&ui, &bound, &files[..2]).unwrap();
    ui.invoke_menu_chosen(id);
    let old = bound.current.borrow().clone();
    let successor = std::rc::Rc::new(std::cell::RefCell::new(page(store.clone(), &files)));
    *bound.current.borrow_mut() = successor;
    ui.invoke_answer(true);
    assert_eq!(seed::state(&store, &files), before);
    ui.invoke_menu_chosen(id);
    assert!(ui.get_question().is_empty());
    *bound.current.borrow_mut() = old;
    let (_, id) = select(&ui, &bound, &files[..2]).unwrap();
    ui.invoke_menu_chosen(id);
    let fresh = bind(&ui, Pages::single(page(store.clone(), &files)));
    ui.invoke_answer(true);
    assert_eq!(seed::state(&store, &files), before);
    let (_, id) = select(&ui, &fresh, &files[..2]).unwrap();
    ui.invoke_menu_chosen(id);
    ui.invoke_answer(false);
    assert_eq!(seed::state(&store, &files), before);
    store
        .write(|ctx| {
            let mut gui: hydrus_store::settings::GuiSettings =
                hydrus_store::settings::get(ctx.conn())?;
            gui.confirm_exit = false;
            hydrus_store::settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.show().unwrap();
    ui.invoke_menu_chosen(id);
    ui.invoke_answer(true);
    assert_eq!(seed::state(&store, &files), before);
    assert_eq!(Plan::capture(&store, &files).unwrap().files, files[..2]);
    let live = bind(&ui, Pages::single(page(store.clone(), &files)));
    let (_, id) = select(&ui, &live, &files[..2]).unwrap();
    ui.invoke_menu_chosen(id);
    ui.invoke_answer(true);
    assert_eq!(
        seed::state(&store, &files)["status"],
        serde_json::json!([0, 0, 3, 2])
    );
}

#[test]
fn tab_roundtrip_permanently_invalidates_menu_and_answer_and_advanced_child_blocks_dispatch() {
    let (_dirs, store) = seed::store();
    let files = seed::files(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page(store.clone(), &files)));
    seed::reset(&store, &files);
    let before = seed::state(&store, &files);
    let (_, old_menu) = select(&ui, &bound, &files[..2]).unwrap();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_menu_chosen(old_menu);
    assert!(ui.get_question().is_empty());
    assert_eq!(seed::state(&store, &files), before);

    let (_, id) = select(&ui, &bound, &files[..2]).unwrap();
    ui.invoke_menu_chosen(id);
    assert!(!ui.get_question().is_empty());
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_answer(true);
    assert_eq!(seed::state(&store, &files), before);

    store
        .write(|ctx| {
            let mut preferences: hydrus_store::settings::DeletionPreferences =
                hydrus_store::settings::get(ctx.conn())?;
            preferences.advanced = true;
            preferences.confirm_trash = true;
            hydrus_store::settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    let (_, id) = select(&ui, &bound, &files[..2]).unwrap();
    bound
        .current
        .borrow()
        .borrow_mut()
        .select_files(&files[3..]);
    ui.invoke_delete_selected();
    let child = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
    assert!(ui.get_question().is_empty());
    // Restore a genuinely eligible selection behind the real owned modal.
    bound
        .current
        .borrow()
        .borrow_mut()
        .select_files(&files[..2]);
    ui.invoke_menu_chosen(id);
    assert!(ui.get_question().is_empty());
    assert!(bound.delete_files.borrow().is_some());
    assert_eq!(seed::state(&store, &files), before);
    child.invoke_cancel();
    ui.invoke_menu_chosen(id);
    assert!(!ui.get_question().is_empty());
    ui.invoke_answer(false);
    assert_eq!(seed::state(&store, &files), before);
    let file = files[0];
    let storage = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .local_file_storage;
    store
        .write(move |ctx| {
            ctx.conn().execute_batch(&format!(
                "CREATE TRIGGER fail_selected_clear BEFORE DELETE ON file_domain_deleted \
                 WHEN OLD.hash_id={} AND OLD.service_id={} \
                 BEGIN SELECT RAISE(ABORT,'scripted selected clear failure'); END;",
                file.get(),
                storage.get()
            ))?;
            Ok(())
        })
        .unwrap();
    let queue = seed::queue(&store);
    ui.invoke_menu_chosen(id);
    ui.invoke_answer(true);
    assert!(ui.get_warning().contains("scripted selected clear failure"));
    assert_eq!(seed::state(&store, &files), before);
    assert_eq!(seed::queue(&store), queue);
    store
        .write(|ctx| {
            ctx.conn()
                .execute_batch("DROP TRIGGER fail_selected_clear")?;
            Ok(())
        })
        .unwrap();
}
