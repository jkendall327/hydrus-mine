//! Saved GUI policy reaching every actual native radio-list and parent default.
use hydrus_core::HashId;
use hydrus_gui::{
    Bound, MainWindow, OptionsWindow, Pages, PredicateEditorWindow, SearchPage, bind, headless,
};
use hydrus_store::{
    Store,
    radio_return::{self, RadioReturn},
    settings,
};
use slint::{
    ComponentHandle as _, Model as _,
    platform::{Key, PointerEventButton, WindowEvent},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
const LABEL: &str = "Force that hitting Enter/Return on radio button lists triggers a dialog ok: ";
fn save(store: &Store, force: bool) {
    store
        .write(move |c| {
            settings::set(
                c.conn(),
                &RadioReturn {
                    force_dialog_ok: force,
                },
            )
        })
        .unwrap();
}
fn key(window: &slint::Window, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}
fn click(window: &slint::Window, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}
fn predicate(ui: &MainWindow, bound: &Bound, hash: bool) -> PredicateEditorWindow {
    ui.invoke_search_edited("".into());
    let at = ui
        .get_suggestions()
        .iter()
        .position(|p| {
            p.text
                == if hash {
                    "system:hash"
                } else {
                    "system:filesize"
                }
        })
        .unwrap();
    ui.invoke_suggestion_chosen(at as i32);
    bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}
fn options(ui: &MainWindow, bound: &Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let at = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = w.get_pages().iter().position(|p| p.text == "gui").unwrap();
    w.invoke_page_chosen(page as i32);
    let row = w.get_rows().iter().position(|r| r.label == LABEL).unwrap();
    assert_eq!(w.get_rows().row_data(row).unwrap().kind, 1);
    (w, row as i32)
}

#[test]
fn gui_options_cancel_hidden_stale_apply_save_reopen_and_live_existing_editor() {
    let fixture = hydrus_testkit::fixture_json("radio_return.json");
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let (cancelled, row) = options(&ui, &bound);
    assert_eq!(
        cancelled.get_rows().row_data(row as usize).unwrap().checked,
        fixture["options"][0]["checked"].as_bool().unwrap()
    );
    cancelled.invoke_check_toggled(row, false);
    assert!(store.read(radio_return::load).unwrap().force_dialog_ok);
    cancelled.invoke_cancel();
    let (hidden, row) = options(&ui, &bound);
    cancelled.invoke_check_toggled(row, false);
    cancelled.invoke_apply();
    assert!(store.read(radio_return::load).unwrap().force_dialog_ok);
    hidden.invoke_check_toggled(row, false);
    hidden.hide().unwrap();
    hidden.invoke_apply();
    assert!(store.read(radio_return::load).unwrap().force_dialog_ok);
    hidden.invoke_cancel();
    let (accepted, row) = options(&ui, &bound);
    accepted.invoke_check_toggled(row, false);
    accepted.invoke_apply();
    assert!(!store.read(radio_return::load).unwrap().force_dialog_ok);
    let (reopened, row) = options(&ui, &bound);
    assert!(!reopened.get_rows().row_data(row as usize).unwrap().checked);
    reopened.invoke_cancel();
    let editor = predicate(&ui, &bound, false);
    assert!(!editor.invoke_force_radio_ok(0));
    // Saved edits are consulted on every key event, not cached at opening.
    save(&store, true);
    assert!(editor.invoke_force_radio_ok(0));
    save(&store, false);
    assert!(!editor.invoke_force_radio_ok(0));
    key(editor.window(), Key::Return.into());
    assert!(
        bound.predicate_editor.borrow().is_none(),
        "false delegates to the real parent default button"
    );
    assert!(!bound.current.borrow().borrow().predicates().is_empty());
    assert!(
        !Store::open(store.dir())
            .unwrap()
            .read(radio_return::load)
            .unwrap()
            .force_dialog_ok
    );
    ui.hide().unwrap();
}

#[test]
fn all_recorded_filesize_hash_groups_accept_with_live_policy_and_preserve_multiline_consumption() {
    let fixture = hydrus_testkit::fixture_json("radio_return.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["kind"].as_str().unwrap().starts_with("delete"))
    {
        let kind = case["kind"].as_str().unwrap();
        let hash = kind.starts_with("hash");
        save(&store, case["at_open"].as_bool().unwrap());
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let w = predicate(&ui, &bound, hash);
        let geometry = Rc::new(RefCell::new(BTreeMap::<i32, (f32, f32, f32, f32)>::new()));
        w.on_hash_field_placed({
            let geometry = geometry.clone();
            move |field, x, y, width, height| {
                geometry.borrow_mut().insert(field, (x, y, width, height));
            }
        });
        if hash {
            w.invoke_text_edited(0, 2, "ab".repeat(32).into());
        }
        let native = windows.get(windows.count() - 1).unwrap();
        let pixels = headless::render(&native, 1000, 500);
        if kind == "hash_type" {
            let (x, y, width, height) = geometry.borrow()[&5];
            assert!(width > 30.0 && height > 80.0);
            click(w.window(), x + 12.0, y + 16.0);
        }
        let field = if kind == "hash_type" { 5 } else { 1 };
        let before = w
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(field)
            .unwrap()
            .chosen;
        let count = w
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(field)
            .unwrap()
            .options
            .row_count() as i32;
        assert!(count > 1);
        let down = before + 1 < count;
        key(
            w.window(),
            if down { Key::DownArrow } else { Key::UpArrow }.into(),
        );
        assert_eq!(
            w.get_panels()
                .row_data(0)
                .unwrap()
                .fields
                .row_data(field)
                .unwrap()
                .chosen,
            before + if down { 1 } else { -1 },
            "real {kind} group must have keyboard focus"
        );
        key(
            w.window(),
            if down { Key::UpArrow } else { Key::DownArrow }.into(),
        );
        assert_eq!(
            w.get_panels()
                .row_data(0)
                .unwrap()
                .fields
                .row_data(field)
                .unwrap()
                .chosen,
            before
        );
        save(&store, case["saved_at_key"].as_bool().unwrap());
        assert_eq!(
            w.invoke_force_radio_ok(0),
            case["saved_at_key"].as_bool().unwrap()
        );
        assert!(
            !w.invoke_force_radio_ok(99),
            "invalid panel cannot arm the default route"
        );
        // The physical radio event reestablishes its real panel after the rejected index.
        key(w.window(), Key::Return.into());
        assert_eq!(case["dialog_result"], 1);
        assert!(bound.predicate_editor.borrow().is_none(), "{case}");
        assert!(!w.window().is_visible());
        assert_eq!(
            w.get_panels()
                .row_data(0)
                .unwrap()
                .fields
                .row_data(field)
                .unwrap()
                .chosen,
            before
        );
        assert!(!bound.current.borrow().borrow().predicates().is_empty());
        if kind == "filesize"
            && case["at_open"] == false
            && case["saved_at_key"] == true
            && case["key"] == "Return"
        {
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("radio-return-native.png"),
                &pixels,
                1000,
                500,
            )
            .unwrap();
        }
    }
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let w = predicate(&ui, &bound, true);
    let geometry = Rc::new(RefCell::new(BTreeMap::<i32, (f32, f32, f32, f32)>::new()));
    w.on_hash_field_placed({
        let geometry = geometry.clone();
        move |field, x, y, width, height| {
            geometry.borrow_mut().insert(field, (x, y, width, height));
        }
    });
    w.invoke_text_edited(0, 2, "ab".repeat(32).into());
    headless::render(&windows.get(windows.count() - 1).unwrap(), 1000, 500);
    let (x, y, width, height) = geometry.borrow()[&2];
    assert!(width >= 420.0 && height >= 220.0);
    click(w.window(), x + 30.0, y + 30.0);
    save(&store, true);
    key(w.window(), Key::Return.into());
    assert!(w.window().is_visible() && bound.predicate_editor.borrow().is_some());
    assert!(
        w.get_hash_text().contains('\n'),
        "multiline text consumes Return before parent defaults"
    );
    w.invoke_cancel();
    assert!(!w.invoke_force_radio_ok(0));
    ui.hide().unwrap();
}

fn current(store: &Store, file: HashId, domain: hydrus_core::ServiceId) -> bool {
    store
        .read(|c| {
            c.query_row(
                "SELECT EXISTS(SELECT 1 FROM file_domain_current WHERE hash_id=? AND service_id=?)",
                rusqlite::params![file, domain],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
}
#[test]
fn actual_advanced_delete_action_reason_defaults_and_retained_owners_follow_live_policy() {
    let fixture = hydrus_testkit::fixture_json("radio_return.json");
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|c| {
            let mut p: settings::DeletionPreferences = settings::get(c.conn())?;
            p.advanced = true;
            p.confirm_trash = true;
            p.remember_action = false;
            p.remember_reason = false;
            settings::set(c.conn(), &p)
        })
        .unwrap();
    let primary = store.snapshot().services.by_name("my files").unwrap().id;
    let file=store.read(|c|c.query_row("SELECT hash_id FROM file_domain_current WHERE service_id=? ORDER BY hash_id LIMIT 1",[primary],|r|r.get::<_,HashId>(0)).map_err(Into::into)).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "Synthetic radio-key file",
            None,
            vec![file],
        )),
    );
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"].as_str().unwrap().starts_with("delete"))
    {
        save(&store, case["at_open"].as_bool().unwrap());
        bound.current.borrow().borrow_mut().select_files(&[file]);
        ui.invoke_delete_selected();
        let w = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
        let draft = hydrus_gui_model::delete_files::Draft::load(
            &store,
            &[file],
            None,
            "Synthetic radio-key probe",
        )
        .unwrap();
        let action = w.get_selected_action() as usize;
        let settings::DeletionAction::Domain(key) = &draft.choices[action].action else {
            panic!("only logical local-domain deletion is replayed")
        };
        let domain = store.snapshot().services.by_key(key).unwrap().id;
        assert!(current(&store, file, domain));
        if case["kind"] == "delete_reason" {
            assert!(w.get_reason_enabled());
            w.set_radio_focus(1);
        } else {
            w.set_radio_focus(0);
        }
        let native = windows.get(windows.count() - 1).unwrap();
        headless::render(&native, 720, 640);
        let reason = case["kind"] == "delete_reason";
        let before = if reason {
            w.get_selected_reason()
        } else {
            w.get_selected_action()
        };
        let count = if reason {
            w.get_reasons().row_count()
        } else {
            w.get_actions().row_count()
        } as i32;
        assert!(count > 1);
        key(w.window(), Key::DownArrow.into());
        assert_eq!(
            if reason {
                w.get_selected_reason()
            } else {
                w.get_selected_action()
            },
            (before + 1) % count,
            "real deletion radio group must consume arrows"
        );
        key(w.window(), Key::UpArrow.into());
        assert_eq!(
            if reason {
                w.get_selected_reason()
            } else {
                w.get_selected_action()
            },
            before
        );
        save(&store, case["saved_at_key"].as_bool().unwrap());
        assert_eq!(
            w.invoke_force_radio_ok(),
            case["saved_at_key"].as_bool().unwrap()
        );
        key(w.window(), Key::Return.into());
        assert_eq!(case["dialog_result"], 1);
        assert!(bound.delete_files.borrow().is_none(), "{case}");
        assert!(!w.window().is_visible());
        assert!(
            !current(&store, file, domain),
            "real accepted default must apply the captured domain deletion"
        );
        store
            .write_content(move |writer| writer.add_files(domain, &[(file, None)]))
            .unwrap();
    }
    bound.current.borrow().borrow_mut().select_files(&[file]);
    ui.invoke_delete_selected();
    let hidden = bound.delete_files.borrow().as_ref().unwrap().clone_strong();
    hidden.hide().unwrap();
    save(&store, true);
    assert!(!hidden.invoke_force_radio_ok());
    hidden.invoke_accept_deletion();
    assert!(current(&store, file, primary));
    hidden.show().unwrap();
    let successor = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "Successor",
            None,
            vec![file],
        )),
    );
    assert!(!hidden.invoke_force_radio_ok());
    key(hidden.window(), Key::Return.into());
    assert!(
        current(&store, file, primary),
        "retired main cannot delete through an old default handler"
    );
    assert!(bound.delete_files.borrow().is_none());
    assert!(successor.delete_files.borrow().is_none());
    ui.hide().unwrap();
}
