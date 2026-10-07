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
            assert!(!ui.get_question().is_empty());
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
            hydrus_store::settings::set(ctx.conn(), &gui)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !ui.window().is_visible(),
        "completed exit precedes retained callbacks"
    );
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
    let failure = ui.get_question();
    assert!(failure.contains("scripted selected clear failure"));
    // A retained menu cannot replace the pending failure with another deletion
    // question. Acknowledgment dismisses it without retrying the failed writer.
    ui.invoke_menu_chosen(id);
    assert_eq!(ui.get_question(), failure);
    assert_eq!(seed::state(&store, &files), before);
    assert_eq!(seed::queue(&store), queue);
    ui.invoke_answer(true);
    assert!(ui.get_question().is_empty());
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

// Relevant Store tables are captured in full, including identities/timestamps
// and reasons, rather than only the importer status summary.
fn deletion_record_tables(store: &Store) -> serde_json::Value {
    store.read(|conn| {
        let deleted = conn.prepare("SELECT service_id,hash_id,deleted_ms,original_added_ms FROM file_domain_deleted ORDER BY service_id,hash_id")?
            .query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let current = conn.prepare("SELECT service_id,hash_id,added_ms FROM file_domain_current ORDER BY service_id,hash_id")?
            .query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let reasons = conn.prepare("SELECT hash_id,reason_id FROM file_deletion_reasons ORDER BY hash_id")?
            .query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(serde_json::json!({"deleted":deleted,"current":current,"reasons":reasons}))
    }).unwrap()
}
fn cleared_record_tables(
    store: &Store,
    mut before: serde_json::Value,
    captured: &[HashId],
) -> serde_json::Value {
    let roles = hydrus_store::content::DomainRoles::new(&store.snapshot().services).unwrap();
    let mut local = roles.local;
    local.extend([roles.combined_local_media, roles.local_file_storage]);
    let captured = captured
        .iter()
        .map(|file| i64::from(file.get()))
        .collect::<Vec<_>>();
    before["deleted"].as_array_mut().unwrap().retain(|row| {
        !captured.contains(&row[1].as_i64().unwrap())
            || !local
                .iter()
                .any(|service| i64::from(service.get()) == row[0].as_i64().unwrap())
    });
    before["reasons"]
        .as_array_mut()
        .unwrap()
        .retain(|row| !captured.contains(&row[0].as_i64().unwrap()));
    // This finite fixture has no remaining covered deletion for the captured
    // files; the combined-deleted aggregate must remove only those identities.
    assert!(before["deleted"].as_array().unwrap().iter().all(|row| {
        !captured.contains(&row[1].as_i64().unwrap())
            || !roles
                .covered_by_combined_deleted
                .iter()
                .any(|service| i64::from(service.get()) == row[0].as_i64().unwrap())
    }));
    before["current"].as_array_mut().unwrap().retain(|row| {
        row[0].as_i64().unwrap() != i64::from(roles.combined_deleted.get())
            || !captured.contains(&row[1].as_i64().unwrap())
    });
    before
}

type QuestionFrames = std::rc::Rc<std::cell::RefCell<Option<[hydrus_gui::MenuChoiceFrame; 4]>>>;
fn question_observer(ui: &MainWindow) -> QuestionFrames {
    let frames = std::rc::Rc::new(std::cell::RefCell::new(None));
    ui.set_measure_question_controls(true);
    ui.on_question_controls_measured({
        let frames = frames.clone();
        move |panel, prompt, yes, no| {
            *frames.borrow_mut() = Some([panel, prompt, yes, no]);
        }
    });
    frames
}
fn settled_question(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    frames: &QuestionFrames,
) -> ([hydrus_gui::MenuChoiceFrame; 4], Vec<u8>) {
    use slint::platform::WindowAdapter as _;
    use std::time::{Duration, Instant};
    assert!(std::ptr::eq(native.window(), ui.window()));
    assert!(ui.window().is_visible());
    assert!(!ui.get_question().is_empty());
    // Require new real Timer measurements for this current question instance.
    frames.borrow_mut().take();
    let started = Instant::now();
    let mut previous = None;
    loop {
        let pixels = headless::render(native, 1100, 700);
        let measured = frames.borrow().clone();
        if let Some(measured) = measured.as_ref() {
            let bits = measured
                .clone()
                .map(|frame| [frame.x, frame.y, frame.w, frame.h].map(f32::to_bits));
            if started.elapsed() >= Duration::from_millis(35)
                && !ui.window().has_active_animations()
                && previous.as_ref().is_some_and(|(old_frames, old_pixels)| {
                    *old_frames == bits && *old_pixels == pixels
                })
            {
                for frame in measured {
                    assert!(
                        [frame.x, frame.y, frame.w, frame.h]
                            .into_iter()
                            .all(f32::is_finite)
                    );
                    assert!(frame.x >= 0.0 && frame.y >= 0.0 && frame.w > 0.0 && frame.h > 0.0);
                    assert!(frame.x + frame.w <= 1100.0 && frame.y + frame.h <= 700.0);
                }
                let [panel, prompt, yes, no] = measured;
                for frame in [prompt, yes, no] {
                    assert!(frame.x >= panel.x && frame.y >= panel.y);
                    assert!(
                        frame.x + frame.w <= panel.x + panel.w
                            && frame.y + frame.h <= panel.y + panel.h
                    );
                }
                assert!(prompt.y + prompt.h < yes.y && prompt.y + prompt.h < no.y);
                assert!(
                    yes.x + yes.w < no.x,
                    "measured yes/no buttons must be disjoint"
                );
                eprintln!(
                    "actual selected-record question controls: {measured:?}; question={}",
                    ui.get_question()
                );
                return (measured.clone(), pixels);
            }
            previous = Some((bits, pixels));
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "question controls did not settle: measured={measured:?}, question={}, visible={}, animations={}",
            ui.get_question(),
            ui.window().is_visible(),
            ui.window().has_active_animations()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn question_pointer(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    frame: &hydrus_gui::MenuChoiceFrame,
    pressed: bool,
) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    if pressed {
        native.dispatch_event(WindowEvent::PointerMoved { position });
        native.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
    } else {
        native.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    }
}
fn question_key(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    key: slint::platform::Key,
) {
    let text: slint::SharedString = key.into();
    native.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    native.dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

#[test]
fn actual_question_buttons_keys_and_retired_press_clear_only_captured_records() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    let fixture = hydrus_testkit::fixture_json("selected_deletion_records.json");
    let (_dirs, store) = seed::store();
    let files = seed::files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page(store.clone(), &files)));
    let native = windows.get(0).unwrap();
    let frames = question_observer(&ui);
    for route in ["pointer-no", "pointer-yes", "escape", "return"] {
        seed::reset(&store, &files);
        headless::render(&native, 1100, 700);
        // Real grid input arms Main's enclosing key-capture path. Do not
        // substitute invoke_answer for either physical keyboard outcome.
        let position = slint::LogicalPosition::new(
            ui.get_grid_origin_x() + ui.get_thumbnail_margin() + ui.get_thumbnail_width() / 2.0,
            ui.get_grid_origin_y() + ui.get_thumbnail_margin() + ui.get_thumbnail_height() / 2.0,
        );
        native.dispatch_event(WindowEvent::PointerMoved { position });
        native.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        native.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        assert_eq!(bound.current.borrow().borrow().selected_files().len(), 1);
        let (_, id) = select(&ui, &bound, &files).unwrap();
        ui.invoke_menu_chosen(id);
        assert_eq!(
            ui.get_question(),
            fixture["actions"][3]["questions"][0]["message"]
                .as_str()
                .unwrap()
        );
        assert_eq!(Plan::capture(&store, &files).unwrap().files, files[..2]);
        // A new deletion and selection after the question are not retargeted
        // into the captured pair. The physical delete queue must stay intact.
        let file = files[3];
        store
            .write_content(move |writer| {
                writer.delete_files(writer.roles().combined_local_media, &[file], None)?;
                writer.delete_files(writer.roles().local_file_storage, &[file], None)
            })
            .unwrap();
        bound
            .current
            .borrow()
            .borrow_mut()
            .select_files(&files[3..]);
        let selected = bound.current.borrow().borrow().selected_files();
        let results = bound.current.borrow().borrow().results().to_vec();
        let query = bound
            .current
            .borrow()
            .borrow()
            .favourite_to_save()
            .unwrap()
            .search;
        let before = deletion_record_tables(&store);
        let queue = seed::queue(&store);
        let (measured, pixels) = settled_question(&native, &ui, &frames);
        if route == "pointer-no" {
            headless::save_png(
                &Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("selected-deletion-records-pointer-question.png"),
                &pixels,
                1100,
                700,
            )
            .unwrap();
        }
        match route {
            "pointer-no" | "pointer-yes" => {
                let frame = &measured[if route == "pointer-yes" { 2 } else { 3 }];
                question_pointer(&native, frame, true);
                question_pointer(&native, frame, false);
            }
            "escape" => question_key(&native, Key::Escape),
            "return" => question_key(&native, Key::Return),
            _ => unreachable!(),
        }
        assert!(
            ui.get_question().is_empty(),
            "physical {route} must answer the real question"
        );
        let accepted = route == "pointer-yes" || route == "return";
        assert_eq!(
            deletion_record_tables(&store),
            if accepted {
                cleared_record_tables(&store, before, &files[..2])
            } else {
                before
            },
            "{route}"
        );
        assert_eq!(
            seed::state(&store, &files)["status"],
            if accepted {
                serde_json::json!([0, 0, 3, 3])
            } else {
                serde_json::json!([3, 3, 3, 3])
            }
        );
        assert_eq!(seed::queue(&store), queue);
        assert_eq!(bound.current.borrow().borrow().selected_files(), selected);
        assert_eq!(bound.current.borrow().borrow().results(), results);
        assert_eq!(
            bound
                .current
                .borrow()
                .borrow()
                .favourite_to_save()
                .unwrap()
                .search,
            query
        );
    }

    seed::reset(&store, &files);
    let (_, old_id) = select(&ui, &bound, &files[..1]).unwrap();
    ui.invoke_menu_chosen(old_id);
    let (old_frames, _) = settled_question(&native, &ui, &frames);
    question_pointer(&native, &old_frames[2], true);
    // Process actual Escape press/release, with no artificial empty-panel
    // render. Its normal input turns retire the old conditional controls.
    question_key(&native, Key::Escape);
    assert!(ui.get_question().is_empty());
    let before = deletion_record_tables(&store);
    let queue = seed::queue(&store);
    let old_selected = bound.current.borrow().borrow().selected_files();
    let successor = bind(&ui, Pages::single(page(store.clone(), &files)));
    let (_, id) = select(&ui, &successor, &files[1..2]).unwrap();
    ui.invoke_menu_chosen(id);
    let pending = ui.get_question();
    assert_eq!(
        pending,
        Plan {
            files: files[1..2].to_vec()
        }
        .question()
    );
    let selected = successor.current.borrow().borrow().selected_files();
    let results = successor.current.borrow().borrow().results().to_vec();
    let current = successor.current.borrow().clone();
    let (fresh_frames, _) = settled_question(&native, &ui, &frames);
    question_pointer(&native, &old_frames[2], false);
    assert_eq!(
        ui.get_question(),
        pending,
        "old held release cannot answer the successor question"
    );
    assert_eq!(deletion_record_tables(&store), before);
    assert_eq!(seed::queue(&store), queue);
    assert!(std::rc::Rc::ptr_eq(&successor.current.borrow(), &current));
    assert_eq!(
        successor.current.borrow().borrow().selected_files(),
        selected
    );
    assert_eq!(successor.current.borrow().borrow().results(), results);
    assert_eq!(
        bound.current.borrow().borrow().selected_files(),
        old_selected
    );
    assert!(ui.window().is_visible());
    // A hidden actual button dispatch cannot run the captured Plan. Reopen a
    // fresh pending question after the denied answer to demonstrate recovery.
    ui.hide().unwrap();
    question_pointer(&native, &fresh_frames[2], true);
    question_pointer(&native, &fresh_frames[2], false);
    assert_eq!(deletion_record_tables(&store), before);
    assert_eq!(seed::queue(&store), queue);
    ui.show().unwrap();
    if !ui.get_question().is_empty() {
        question_key(&native, Key::Escape);
    }
    let (_, id) = select(&ui, &successor, &files[1..2]).unwrap();
    ui.invoke_menu_chosen(id);
    let (fresh_frames, _) = settled_question(&native, &ui, &frames);
    question_pointer(&native, &fresh_frames[2], true);
    question_pointer(&native, &fresh_frames[2], false);
    assert!(ui.get_question().is_empty());
    assert_eq!(
        deletion_record_tables(&store),
        cleared_record_tables(&store, before, &files[1..2])
    );
    assert_eq!(
        seed::state(&store, &files)["status"],
        serde_json::json!([3, 0, 3, 2])
    );
    assert_eq!(seed::queue(&store), queue);
}
