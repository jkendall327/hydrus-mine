//! Options > tag presentation > namespace colours' "edit": each selected
//! entry is offered the colour picker in turn (`_EditNamespaceColour`).
use hydrus_core::tag_presentation::NamespaceColours;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind};
use hydrus_store::settings;
use slint::{ComponentHandle as _, Model as _};

// leaf: audit-options-tag-presentation-namespace-colours-edit
// leaf: audit-options-tag-presentation-namespace-colours-namespace-colours-editor
#[test]
fn edit_offers_the_picker_for_each_selected_namespace_and_a_cancel_keeps_the_colour() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = hydrus_gui::headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = store.read::<NamespaceColours>(settings::get).unwrap();
    let open = || {
        ui.invoke_menu_title_pressed(0, 20.0, 22.0);
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let index = lines
            .iter()
            .position(|line| line.label == "options\u{2026}")
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        let page = options
            .get_pages()
            .iter()
            .position(|page| page.text == "tag presentation")
            .unwrap();
        options.invoke_page_chosen(i32::try_from(page).unwrap());
        options
    };
    let rgb = |options: &hydrus_gui::OptionsWindow, label: &str| {
        let row = options
            .get_namespace_colour_rows()
            .iter()
            .find(|row| row.label == label)
            .unwrap();
        [row.colour.red(), row.colour.green(), row.colour.blue()]
    };
    let at = |options: &hydrus_gui::OptionsWindow, label: &str| {
        options
            .get_namespace_colour_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap() as i32
    };
    let options = open();
    // nothing selected: edit does nothing
    options.invoke_namespace_colour_action("edit".into());
    assert!(hydrus_gui::options_namespace_colours::last_picker().is_none());

    // select 'character' and 'series' and edit both: the picker starts on
    // each one's current colour; the first is changed, the second cancelled
    options.invoke_namespace_colour_clicked(at(&options, "'character' tags"), false, false);
    options.invoke_namespace_colour_clicked(at(&options, "'series' tags"), true, false);
    options.invoke_namespace_colour_action("edit".into());
    let first = hydrus_gui::options_namespace_colours::last_picker().unwrap();
    assert!(first.window().is_visible());
    assert_eq!(
        (first.get_red(), first.get_green(), first.get_blue()),
        (0, 170, 0)
    );
    first.invoke_accepted(10, 20, 30);
    assert!(!first.window().is_visible());
    assert_eq!(rgb(&options, "'character' tags"), [10, 20, 30]);
    let second = hydrus_gui::options_namespace_colours::last_picker().unwrap();
    assert!(second.window().is_visible());
    assert_eq!(
        (second.get_red(), second.get_green(), second.get_blue()),
        (170, 0, 170)
    );
    second.invoke_cancelled();
    assert_eq!(rgb(&options, "'series' tags"), [170, 0, 170]);
    // the edited entries leave the selection (the reference removes and re-adds each term)
    assert!(
        options
            .get_namespace_colour_rows()
            .iter()
            .all(|row| !row.selected)
    );
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before,
        "staged until Options is OKed"
    );

    // the protected 'namespaced tags' and 'unnamespaced tags' rows edit too
    options.invoke_namespace_colour_clicked(at(&options, "unnamespaced tags"), false, false);
    options.invoke_namespace_colour_action("edit".into());
    let third = hydrus_gui::options_namespace_colours::last_picker().unwrap();
    assert_eq!(
        (third.get_red(), third.get_green(), third.get_blue()),
        (0, 111, 250)
    );
    third.invoke_accepted(255, 0, 0);

    options.invoke_cancel();
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before
    );
    let options = open();
    assert_eq!(rgb(&options, "'character' tags"), [0, 170, 0]);
    options.invoke_namespace_colour_clicked(at(&options, "'character' tags"), false, false);
    options.invoke_namespace_colour_action("edit".into());
    hydrus_gui::options_namespace_colours::last_picker()
        .unwrap()
        .invoke_accepted(10, 20, 30);
    options.invoke_apply();
    let saved = store.read::<NamespaceColours>(settings::get).unwrap();
    let mut expected = before.clone();
    for (namespace, colour) in &mut expected.colours {
        if namespace.as_deref() == Some("character") {
            *colour = [10, 20, 30];
        }
    }
    // (the saved list is in the editor's order)
    let sorted = |mut colours: NamespaceColours| {
        colours.colours.sort();
        colours
    };
    assert_eq!(sorted(saved), sorted(expected));
    let options = open();
    assert_eq!(rgb(&options, "'character' tags"), [10, 20, 30]);
    options.invoke_cancel();
}

// leaf: audit-options-tag-presentation-namespace-colours-edit
// leaf: audit-options-tag-presentation-namespace-colours-namespace-colours-editor
#[test]
fn edit_replays_the_reference_s_list_the_pickers_it_offers_and_what_is_left_selected() {
    let recorded = hydrus_testkit::fixture_json("namespace_colour_controls.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = hydrus_gui::headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    let rows = |options: &hydrus_gui::OptionsWindow| -> Vec<(String, [u8; 3])> {
        options
            .get_namespace_colour_rows()
            .iter()
            .map(|row| {
                (
                    row.label.to_string(),
                    [row.colour.red(), row.colour.green(), row.colour.blue()],
                )
            })
            .collect()
    };
    let recorded_rows = |value: &serde_json::Value| -> Vec<(String, [u8; 3])> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let rgb: Vec<u8> = row["rgb"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| u8::try_from(c.as_u64().unwrap()).unwrap())
                    .collect();
                (
                    row["label"].as_str().unwrap().to_owned(),
                    [rgb[0], rgb[1], rgb[2]],
                )
            })
            .collect()
    };
    // the list as the reference's panel shows it
    assert_eq!(rows(&options), recorded_rows(&recorded["initial"]));
    // nothing selected: edit offers nothing
    options.invoke_namespace_colour_action("edit".into());
    assert!(hydrus_gui::options_namespace_colours::last_picker().is_none());
    assert_eq!(
        recorded["edit_cases"]["nothing_selected_offered"],
        serde_json::json!([])
    );

    let edit = &recorded["edit_cases"];
    for (n, namespace) in edit["selected"].as_array().unwrap().iter().enumerate() {
        let label = format!("'{}' tags", namespace.as_str().unwrap());
        let at = options
            .get_namespace_colour_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap();
        options.invoke_namespace_colour_clicked(i32::try_from(at).unwrap(), n > 0, false);
    }
    options.invoke_namespace_colour_action("edit".into());
    // each selected entry is offered its own colour; answered by the colour offered
    // (the reference's order here comes from a set, so the recording sorts what was offered)
    let mut offered = Vec::new();
    while let Some(picker) = hydrus_gui::options_namespace_colours::last_picker()
        && picker.window().is_visible()
    {
        let rgb = [picker.get_red(), picker.get_green(), picker.get_blue()];
        offered.push(serde_json::json!({"rgb": rgb}));
        if rgb == [0, 170, 0] {
            picker.invoke_accepted(10, 20, 30);
        } else {
            picker.invoke_cancelled();
        }
    }
    offered.sort_by_key(|o| o["rgb"].to_string());
    let mut theirs = edit["offered"].as_array().unwrap().clone();
    theirs.sort_by_key(|o| o["rgb"].to_string());
    assert_eq!(offered, theirs);
    assert_eq!(rows(&options), recorded_rows(&edit["rows"]));
    // and no entry is left selected
    assert_eq!(edit["selected_after"], serde_json::json!([]));
    assert!(
        options
            .get_namespace_colour_rows()
            .iter()
            .all(|row| !row.selected)
    );
    options.invoke_cancel();
}
