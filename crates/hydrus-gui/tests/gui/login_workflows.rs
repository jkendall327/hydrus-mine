//! Login credential/dialog consumers, exchange staging and stale handle safety.
use hydrus_gui::{
    headless, login_credential_window as credential,
    login_workflows_window::{self as windows, Slots},
};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_parse::login::LoginManager;
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
fn store() -> (tempfile::TempDir, Arc<Store>, LoginManager) {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let manager = legacy::manager(
        &SerialisableObject::from_tuple_str(&fixture["manager"].to_string()).unwrap(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let initial = manager.clone();
    store
        .write_and_refresh(move |ctx| hydrus_store::logins::save(ctx.conn(), &initial))
        .unwrap();
    (dir, store, manager)
}
#[test]
fn credential_entry_masks_password_and_replays_reference_invalid_confirmation() {
    let (_dir, _store, manager) = store();
    let rendered = headless::init();
    let slot = credential::CredentialsSlot::default();
    let accepted = Rc::new(RefCell::new(None));
    let applied: credential::CredentialsApplied = Rc::new({
        let accepted = accepted.clone();
        move |value| {
            *accepted.borrow_mut() = Some(value);
            Ok(())
        }
    });
    let window = credential::open_credentials(
        &manager.scripts[0].credentials,
        &BTreeMap::new(),
        &slot,
        applied.clone(),
    )
    .unwrap();
    assert_eq!(window.get_rows().row_data(0).unwrap().name, "username");
    assert!(!window.get_rows().row_data(0).unwrap().hidden);
    assert!(window.get_rows().row_data(1).unwrap().hidden);
    window.invoke_edited(0, "1?!".into());
    window.invoke_edited(1, "x".into());
    window.invoke_action("apply".into());
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    assert_eq!(
        window.get_question(),
        fixture["credentials"][2]["questions"][0].as_str().unwrap()
    );
    window.invoke_action("back".into());
    assert!(window.get_question().is_empty());
    assert!(accepted.borrow().is_none());
    let pixels = headless::render(&rendered.get(0).unwrap(), 760, 360);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login_credentials.png"),
        &pixels,
        760,
        360,
    )
    .unwrap();
    window.invoke_edited(0, "alice".into());
    window.invoke_edited(1, "dummy-pass".into());
    window.invoke_action("apply".into());
    assert_eq!(accepted.borrow().as_ref().unwrap()["username"], "alice");
    assert!(slot.borrow().is_none());
    *accepted.borrow_mut() = None;
    let fresh = credential::open_credentials(
        &manager.scripts[0].credentials,
        &BTreeMap::new(),
        &slot,
        applied,
    )
    .unwrap();
    fresh.invoke_action("apply".into());
    fresh.invoke_action("confirm".into());
    assert_eq!(accepted.borrow().as_ref().unwrap()["password"], "");
    *accepted.borrow_mut() = None;
    window.invoke_action("confirm".into());
    assert!(accepted.borrow().is_none());
}
#[test]
fn script_definition_matcher_child_stages_until_parent_apply_and_cancel_discards() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_credential_clicked(1, false, false);
    script.invoke_action("edit-credential".into());
    let child = slots.definition.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(child.get_name(), "username");
    child.invoke_action("matcher".into());
    let matcher = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    assert!(child.get_child_open());
    matcher.set_match_type(1);
    matcher.set_fixed("token".into());
    matcher.invoke_changed();
    matcher.invoke_apply();
    assert!(!child.get_child_open());
    child.set_name("account".into());
    child.set_kind(1);
    child.invoke_action("apply".into());
    assert!(slots.definition.borrow().is_none());
    assert_eq!(
        script
            .get_credentials()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "account"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    script.invoke_action("apply".into());
    assert!(!script.get_question().is_empty());
    script.invoke_action("back".into());
    assert!(slots.script.borrow().is_some());
    script.invoke_action("apply".into());
    script.invoke_action("confirm".into());
    list.invoke_action("cancel".into());
    child.invoke_action("apply".into());
    script.invoke_action("confirm".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.set_name("renamed login".into());
    script.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].name, "renamed login");
    assert_eq!(saved.scripts[0].key, original.scripts[0].key);
    assert_eq!(saved.domains, original.domains);
    let list = windows::open_scripts(&store, &Slots::default()).unwrap();
    assert_eq!(
        list.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "renamed login"
    );
    list.invoke_action("cancel".into());
}
#[test]
fn login_exchange_reviews_without_mutating_then_parent_apply_persists_unique_scripts() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_action("import".into());
    let exchange = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    exchange.set_text(fixture["script"].to_string().into());
    exchange.invoke_action("review".into());
    assert!(exchange.get_ready());
    assert_eq!(list.get_rows().row_count(), 1);
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    exchange.invoke_action("apply".into());
    assert_eq!(list.get_rows().row_count(), 2);
    assert!(!list.get_child_open());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[1].name, "synthetic login (1)");
    assert_ne!(saved.scripts[1].key, original.scripts[0].key);
    assert_eq!(saved.domains, original.domains);
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_action("import".into());
    let child = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    list.invoke_action("cancel".into());
    child.set_text(fixture["script"].to_string().into());
    child.invoke_action("review".into());
    child.invoke_action("apply".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}

#[test]
fn network_login_scripts_menu_reaches_persisted_editor() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let ui = hydrus_gui::MainWindow::new().unwrap();
    let bound = hydrus_gui::bind(&ui, hydrus_gui::Pages::open(store.clone()).unwrap());
    let titles = ui.get_menu_titles();
    let network = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(network).unwrap(), 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let logins = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "logins")
        .unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(logins).unwrap(), 300.0, 100.0, 10.0);
    let lines = ui.get_menu_panes().row_data(1).unwrap().lines;
    let scripts = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "login scripts…")
        .unwrap();
    assert!(lines.row_data(scripts).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(scripts).unwrap(), 0.0, 0.0, 0.0);
    let window = bound
        .login_workflows
        .scripts
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(window.get_rows().row_count(), original.scripts.len());
    window.invoke_action("cancel".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
}

#[test]
fn script_step_content_preview_apply_and_parent_cancel_are_owned_and_restricted() {
    let (_dir, store, original) = store();
    let rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_step_clicked(0);
    script.invoke_action("edit-step".into());
    let step = slots.step.step.borrow().as_ref().unwrap().clone_strong();
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 880, 680);
    assert!(step.get_footer_y() >= 0.0 && step.get_footer_y() + step.get_footer_height() <= 680.0);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login_step.png"),
        &pixels,
        880,
        680,
    )
    .unwrap();
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let cells = step.get_content().row_data(0).unwrap().cells;
    assert_eq!(
        cells.row_data(1).unwrap(),
        fixture["step_states"][0]["state"]["content_rows"][0][1]
            .as_str()
            .unwrap()
    );
    step.invoke_content_clicked(0, false, false);
    step.invoke_action("edit-content".into());
    let content = slots
        .step
        .parsers
        .content
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let choices = content.get_fields().row_data(1).unwrap().options;
    assert_eq!(
        serde_json::to_value(
            (0..choices.row_count())
                .map(|i| choices.row_data(i).unwrap().to_string())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        fixture["permitted_content_types"][0]
    );
    assert!(content.get_document().is_empty());
    content.invoke_action("test".into());
    assert!(content.get_preview().contains("dummy-csrf"));
    content.invoke_text_edited(0, "renamed response".into());
    content.invoke_text_edited(5, "token".into());
    content.invoke_action("apply".into());
    assert_eq!(
        step.get_content()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "renamed response"
    );
    step.set_name("edited request".into());
    step.set_path("signin".into());
    step.set_has_subdomain(true);
    step.set_subdomain("".into());
    step.invoke_action("apply".into());
    assert_eq!(
        script
            .get_steps()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "edited request"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    script.invoke_action("apply".into());
    assert!(!script.get_question().is_empty());
    script.invoke_action("confirm".into());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].steps[0].path, "/signin");
    assert!(saved.scripts[0].steps[0].subdomain.is_none());
    assert_eq!(
        saved.scripts[0].steps[0].static_args,
        original.scripts[0].steps[0].static_args
    );
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_step_clicked(0);
    script.invoke_action("edit-step".into());
    let step = slots.step.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_content_clicked(0, false, false);
    step.invoke_action("edit-content".into());
    let content = slots
        .step
        .parsers
        .content
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    content.invoke_action("formula".into());
    let formula = slots
        .step
        .parsers
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 900, 650);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    list.invoke_action("cancel".into());
    assert!(slots.step.step.borrow().is_none());
    assert!(slots.step.parsers.content.borrow().is_none());
    assert!(slots.step.parsers.formula.formula.borrow().is_none());
    formula.invoke_apply();
    content.invoke_action("apply".into());
    step.invoke_action("apply".into());
    script.invoke_action("apply".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}
