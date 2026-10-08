//! The actual Options owner, staged children and importer consumers after Apply.
use hydrus_core::{
    import_options::{CallerType, ImportOptionsManager},
    url::{UrlClass, UrlClassSettings, UrlType},
};
use hydrus_gui::{
    ImportOptionsPanelWindow, ImportOptionsWindow, MainWindow, OptionsWindow, Pages, bind,
    headless, import_options_panel_window as panel,
};
use hydrus_store::settings::{self, ImportOptionsUiSettings};
use slint::{ComponentHandle as _, Model as _};

fn open_options(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&index| lines.row_data(index).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
fn page(options: &OptionsWindow) {
    let index = (0..options.get_pages().row_count())
        .find(|&index| options.get_pages().row_data(index).unwrap().text == "import options")
        .unwrap();
    options.set_page(i32::try_from(index).unwrap());
    options.invoke_page_chosen(i32::try_from(index).unwrap());
    assert!(options.get_rows().iter().any(|row| row.kind == 18));
    options.invoke_import_options_clicked();
}
fn rows(window: &ImportOptionsPanelWindow, list: i32) -> Vec<Vec<String>> {
    let rows = match list {
        0 => window.get_defaults(),
        1 => window.get_urls(),
        _ => window.get_profiles(),
    };
    rows.iter()
        .map(|row| row.cells.iter().map(|cell| cell.to_string()).collect())
        .collect()
}
fn select(window: &ImportOptionsPanelWindow, list: i32, name: &str) {
    let index = rows(window, list)
        .iter()
        .position(|row| row[0] == name)
        .unwrap();
    window.invoke_clicked(list, i32::try_from(index).unwrap(), false, false);
}
fn kind(window: &ImportOptionsWindow, name: &str) {
    let index = window
        .get_labels()
        .iter()
        .position(|label| label.contains(name))
        .unwrap();
    window.invoke_kind_clicked(i32::try_from(index).unwrap());
}
fn synthetic_classes() -> Vec<UrlClass> {
    [
        (1, "alpha post", UrlType::Post),
        (2, "beta watch", UrlType::Watchable),
        (3, "gamma gallery", UrlType::Gallery),
        (4, "excluded file", UrlType::File),
    ]
    .into_iter()
    .map(|(key, name, url_type)| UrlClass {
        key: vec![key; 32],
        name: name.into(),
        url_type,
        ..UrlClass::default()
    })
    .collect()
}

#[test]
fn options_drafts_replay_clear_reset_simple_mode_and_cancel_without_store_changes() {
    let (_dirs, store) = crate::subscriptions::store();
    let mut initial = ImportOptionsManager::default();
    initial.favourites.clear();
    let seeded = initial.clone();
    let classes = synthetic_classes();
    store
        .write(move |tx| {
            let mut registry: UrlClassSettings = settings::get(tx.conn())?;
            registry.url_classes = classes;
            settings::set(tx.conn(), &registry)?;
            settings::set(tx.conn(), &seeded)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("import_options_panel.json");
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let window = panel::last_opened().unwrap();
    let launched = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |url| launched.borrow_mut().push(url.to_owned())
    });
    window.invoke_action(-1, "help".into());
    assert_eq!(
        &*launched.borrow(),
        &[format!(
            "https://hydrusnetwork.github.io/hydrus/{}",
            fixture["documentation"][0].as_str().unwrap()
        )]
    );
    assert_eq!(
        serde_json::json!(rows(&window, 0)),
        fixture["initial"]["defaults"]
    );
    assert_eq!(
        serde_json::json!(rows(&window, 1)),
        fixture["initial"]["urls"]
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 980, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("import_options_panel.png"),
        &pixels,
        980,
        850,
    )
    .unwrap();
    for step in fixture["steps"].as_array().unwrap().iter().take(10) {
        let list = i32::from(step["action"] != "_SeeDefaultStack");
        let names = step["selection"][if list == 0 { "defaults" } else { "urls" }]
            .as_array()
            .unwrap();
        select(&window, list, names[0].as_str().unwrap());
        window.invoke_action(list, "stack".into());
        assert_eq!(
            window.get_information(),
            step["calls"][0]["information"].as_str().unwrap()
        );
        window.invoke_action(-1, "dismiss-information".into());
    }
    window.invoke_action(-1, "tldr".into());
    assert_eq!(
        window.get_information(),
        fixture["steps"][10]["calls"][0]["information"]
            .as_str()
            .unwrap()
    );
    window.invoke_action(-1, "dismiss-information".into());
    select(&window, 0, "global");
    window.invoke_action(0, "clear".into());
    assert_eq!(
        window.get_information(),
        fixture["steps"][11]["calls"][0]["information"]
            .as_str()
            .unwrap()
    );
    assert!(window.get_question().is_empty());
    window.invoke_action(-1, "dismiss-information".into());
    window.invoke_action(0, "edit".into());
    let editor = panel::editing_window().unwrap();
    assert!(!editor.get_default_choice_enabled());
    editor.set_custom_index(0);
    editor.invoke_changed();
    assert_eq!(editor.get_custom_index(), 1);
    editor.invoke_cancel();
    select(&window, 0, "gallery/post urls");
    window.invoke_action(0, "edit".into());
    let editor = panel::editing_window().unwrap();
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(false);
    editor.invoke_changed();
    editor.invoke_cancel();
    assert_eq!(
        serde_json::json!(rows(&window, 0)),
        fixture["initial"]["defaults"]
    );
    window.invoke_action(0, "clear".into());
    assert_eq!(
        window.get_question(),
        fixture["steps"][14]["calls"][0]["question"]
            .as_str()
            .unwrap()
    );
    window.invoke_answered(false);
    assert_eq!(
        serde_json::json!(rows(&window, 0)),
        fixture["initial"]["defaults"]
    );
    window.invoke_action(0, "clear".into());
    window.invoke_clicked(0, 6, false, false);
    window.invoke_sort(0, 0, false);
    window.invoke_answered(true);
    assert_eq!(
        rows(&window, 0)
            .iter()
            .find(|row| row[0] == "gallery/post urls")
            .unwrap()[1],
        ""
    );
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        initial
    );
    window.invoke_action(0, "reset".into());
    assert!(window.get_resetting());
    window.invoke_reset_chosen(0);
    assert_eq!(
        window.get_question(),
        fixture["steps"][27]["calls"][1]["question"]
            .as_str()
            .unwrap()
    );
    assert_eq!(window.get_yes_label(), "let's do it");
    window.invoke_answered(false);
    assert_eq!(
        rows(&window, 0)
            .iter()
            .find(|row| row[0] == "gallery/post urls")
            .unwrap()[1],
        ""
    );
    window.invoke_action(0, "reset".into());
    window.invoke_reset_chosen(0);
    window.invoke_answered(true);
    assert!(
        !rows(&window, 0)
            .iter()
            .find(|row| row[0] == "gallery/post urls")
            .unwrap()[1]
            .is_empty()
    );
    window.invoke_simple_changed(false);
    select(&window, 0, "subscription");
    window.invoke_action(0, "edit".into());
    let retained = panel::editing_window().unwrap();
    assert_eq!(retained.get_labels().row_count(), 8);
    options.invoke_apply();
    assert!(bound.options.borrow().is_some());
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        initial
    );
    options.invoke_cancel();
    assert!(bound.options.borrow().is_none());
    assert!(panel::last_opened().is_none());
    retained.invoke_apply();
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        initial
    );
    assert!(
        store
            .read(settings::get::<ImportOptionsUiSettings>)
            .unwrap()
            .simple
    );
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let window = panel::last_opened().unwrap();
    assert!(window.get_simple());
    select(&window, 0, "subscription");
    window.invoke_action(0, "edit".into());
    assert_eq!(panel::editing_window().unwrap().get_labels().row_count(), 2);
    options.invoke_cancel();
}

// leaf: audit-options-import-options-favourites-profiles-delete
#[test]
fn applied_defaults_url_overrides_profiles_and_simple_preference_reach_consumers() {
    let (_dirs, store) = crate::subscriptions::store();
    let seeded = synthetic_classes();
    store
        .write(move |tx| {
            let mut registry: UrlClassSettings = settings::get(tx.conn())?;
            registry.url_classes = seeded;
            settings::set(tx.conn(), &registry)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = store.read(settings::get::<ImportOptionsManager>).unwrap();
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let window = panel::last_opened().unwrap();
    select(&window, 0, "gallery/post urls");
    window.invoke_action(0, "edit".into());
    let editor = panel::editing_window().unwrap();
    assert!(
        editor
            .get_description()
            .contains("You are editing \"gallery/post urls\".")
    );
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(false);
    editor.invoke_changed();
    editor.invoke_apply();
    select(&window, 1, "alpha post");
    window.invoke_action(1, "edit".into());
    let editor = panel::editing_window().unwrap();
    assert!(
        editor
            .get_description()
            .starts_with("You are editing URL Class \"alpha post\".")
    );
    kind(&editor, "notes");
    assert!(!editor.get_get_notes());
    assert!(
        editor
            .get_custom_choices()
            .row_data(0)
            .unwrap()
            .contains("gallery/post urls")
    );
    editor.set_custom_index(1);
    editor.set_get_notes(true);
    editor.invoke_changed();
    editor.invoke_apply();
    select(&window, 1, "beta watch");
    window.invoke_action(1, "edit".into());
    let editor = panel::editing_window().unwrap();
    kind(&editor, "tags");
    assert!(
        editor
            .get_custom_choices()
            .row_data(0)
            .unwrap()
            .contains("watchable urls")
    );
    editor.invoke_cancel();
    window.invoke_action(2, "add".into());
    let editor = panel::editing_window().unwrap();
    assert_eq!(
        editor.get_favourite_name(),
        "new import options favourite/profile"
    );
    editor.set_favourite_name("new profile".into());
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(false);
    editor.invoke_changed();
    editor.invoke_apply();
    window.invoke_action(2, "add".into());
    let editor = panel::editing_window().unwrap();
    editor.set_favourite_name("new profile".into());
    editor.invoke_apply();
    assert!(
        rows(&window, 2)
            .iter()
            .any(|row| row[0] == "new profile (1)")
    );
    select(&window, 2, "new profile (1)");
    window.invoke_action(2, "delete".into());
    assert_eq!(
        window.get_question(),
        "Delete the favourite/profile named \"new profile (1)\"?"
    );
    window.invoke_answered(false);
    assert!(
        rows(&window, 2)
            .iter()
            .any(|row| row[0] == "new profile (1)")
    );
    window.invoke_action(2, "delete".into());
    window.invoke_answered(true);
    assert!(
        !rows(&window, 2)
            .iter()
            .any(|row| row[0] == "new profile (1)")
    );
    window.invoke_simple_changed(false);
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        before
    );
    assert!(
        store
            .read(settings::get::<ImportOptionsUiSettings>)
            .unwrap()
            .simple
    );
    options.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        before
    );
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let window = panel::last_opened().unwrap();
    select(&window, 0, "gallery/post urls");
    window.invoke_action(0, "edit".into());
    let editor = panel::editing_window().unwrap();
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(false);
    editor.invoke_changed();
    editor.invoke_apply();
    select(&window, 1, "alpha post");
    window.invoke_action(1, "edit".into());
    let editor = panel::editing_window().unwrap();
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(true);
    editor.invoke_changed();
    editor.invoke_apply();
    window.invoke_action(2, "add".into());
    let editor = panel::editing_window().unwrap();
    editor.set_favourite_name("new profile".into());
    kind(&editor, "notes");
    editor.set_custom_index(1);
    editor.set_get_notes(false);
    editor.invoke_changed();
    editor.invoke_apply();
    select(&window, 2, "new profile");
    window.invoke_action(2, "edit".into());
    let editor = panel::editing_window().unwrap();
    editor.set_favourite_name("renamed profile".into());
    editor.invoke_apply();
    assert!(
        rows(&window, 2)
            .iter()
            .any(|row| row[0] == "renamed profile")
    );
    window.invoke_simple_changed(false);
    window.invoke_apply();
    options.invoke_apply();
    let saved = store.read(settings::get::<ImportOptionsManager>).unwrap();
    assert!(!saved.full(CallerType::PostUrls, None, &[]).notes.get_notes);
    assert!(
        saved
            .full(
                CallerType::PostUrls,
                None,
                &[(
                    hex::encode(vec![1; 32]),
                    hydrus_core::import_options::UrlClassKind::Other
                )]
            )
            .notes
            .get_notes
    );
    assert!(
        !saved
            .favourites
            .iter()
            .find(|(name, _)| name == "renamed profile")
            .unwrap()
            .1
            .notes
            .as_ref()
            .unwrap()
            .get_notes
    );
    assert!(
        !store
            .read(settings::get::<ImportOptionsUiSettings>)
            .unwrap()
            .simple
    );
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let reopened = panel::last_opened().unwrap();
    assert!(!reopened.get_simple());
    assert!(
        rows(&reopened, 2)
            .iter()
            .any(|row| row[0] == "renamed profile")
    );
    select(&reopened, 0, "gallery/post urls");
    hydrus_gui::set_paster(|| "not an import-options container".into());
    reopened.invoke_action(0, "paste-merge".into());
    assert!(!reopened.get_error().is_empty());
    assert_eq!(
        store.read(settings::get::<ImportOptionsManager>).unwrap(),
        saved
    );
    options.invoke_cancel();
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    ui.invoke_importer_import_options();
    let consumer = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    kind(&consumer, "notes");
    assert!(!consumer.get_get_notes());
    assert!(
        consumer
            .get_custom_choices()
            .row_data(0)
            .unwrap()
            .contains("gallery/post urls")
    );
    consumer.invoke_cancel();
    crate::folders::open(&ui, "manage import folders…");
    let folders = crate::folders::import_list(&bound);
    folders.invoke_add();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.invoke_edit_import_options();
    let local = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(local.get_labels().row_count(), 8);
    assert!(
        local
            .get_labels()
            .iter()
            .any(|label| label.contains("notes"))
    );
    local.invoke_cancel();
    folder.invoke_cancel();
    folders.invoke_cancel();
}

// leaf: audit-options-import-options-favourites-profiles-delete
// (the reference's delete of a single selected profile raises before it asks,
// a known difference in DIFFERENCES.md; the recorded multi-profile delete is
// replayed here)
#[test]
fn deleting_selected_profiles_asks_as_the_reference_does_and_cancel_keeps_them() {
    let fixture = hydrus_testkit::fixture_json("import_options_panel.json");
    let recorded: Vec<&serde_json::Value> = fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["action"] == "_DeleteFavourite" && step["error"].is_null())
        .collect();
    let question = recorded[0]["calls"][0]["question"].as_str().unwrap();
    assert_eq!(recorded[0]["calls"][0]["answer"], false);
    assert_eq!(recorded[1]["calls"][0]["answer"], true);
    assert_eq!(recorded[1]["calls"][0]["question"], question);

    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    open_options(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&options);
    let window = panel::last_opened().unwrap();
    let profiles = |window: &ImportOptionsPanelWindow| -> Vec<String> {
        rows(window, 2)
            .into_iter()
            .map(|row| row[0].clone())
            .collect()
    };
    let before = profiles(&window);
    for _ in 0..2 {
        window.invoke_action(2, "add".into());
        let editor = panel::editing_window().unwrap();
        editor.set_favourite_name("new profile".into());
        kind(&editor, "notes");
        editor.invoke_apply();
    }
    let added: Vec<String> = profiles(&window)
        .into_iter()
        .filter(|name| !before.contains(name))
        .collect();
    assert_eq!(added, ["new profile", "new profile (1)"]);

    // both selected: the recorded question; no keeps them, yes deletes them
    let rows_of = |window: &ImportOptionsPanelWindow| profiles(window);
    let at = |name: &str| rows_of(&window).iter().position(|n| n == name).unwrap() as i32;
    window.invoke_clicked(2, at("new profile"), false, false);
    window.invoke_clicked(2, at("new profile (1)"), true, false);
    window.invoke_action(2, "delete".into());
    assert_eq!(window.get_question(), question);
    window.invoke_answered(false);
    assert_eq!(profiles(&window).len(), before.len() + 2, "cancelled");
    window.invoke_action(2, "delete".into());
    assert_eq!(window.get_question(), question);
    window.invoke_answered(true);
    assert_eq!(profiles(&window), before, "deleted");
    options.invoke_cancel();
}
