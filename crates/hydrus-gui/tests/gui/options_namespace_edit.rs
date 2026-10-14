//! Options > tag presentation > namespace colours' "edit": each selected
//! entry is offered the colour picker in turn (`_EditNamespaceColour`).
use hydrus_core::tag_presentation::NamespaceColours;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind};
use hydrus_store::settings;
use slint::{ComponentHandle as _, Model as _};

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
    // (the entries edited are replaced, so no longer selected, as in the
    // reference)
    assert_eq!(
        options
            .get_namespace_colour_rows()
            .iter()
            .filter(|row| row.selected)
            .count(),
        0
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

fn namespace_label(namespace: &serde_json::Value) -> String {
    match namespace {
        serde_json::Value::Null => "namespaced tags".to_owned(),
        serde_json::Value::String(text) if text.is_empty() => "unnamespaced tags".to_owned(),
        serde_json::Value::String(text) => format!("'{text}' tags"),
        other => panic!("{other}"),
    }
}

/// "edit" over the selections of `oracle/record_namespace_colour_edit.py`
/// (the real `_EditNamespaceColour` with Qt's colour dialog): the list as
/// opened, the colours each picker starts on, what each answer leaves in
/// the list and its selection, and what OK writes.
// leaf: audit-options-tag-presentation-namespace-colours-edit
// leaf: audit-options-tag-presentation-namespace-colours-namespace-colours-editor
#[test]
fn edit_replays_the_references_pickers_selections_and_saved_colours() {
    let recorded = hydrus_testkit::fixture_json("namespace_colour_edit.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = hydrus_gui::headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = store.read::<NamespaceColours>(settings::get).unwrap();
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

    let listing = |options: &hydrus_gui::OptionsWindow| -> Vec<(String, [u8; 3], bool)> {
        options
            .get_namespace_colour_rows()
            .iter()
            .map(|row| {
                (
                    row.label.to_string(),
                    [row.colour.red(), row.colour.green(), row.colour.blue()],
                    row.selected,
                )
            })
            .collect()
    };
    let recorded_listing =
        |rows: &serde_json::Value, selected: bool| -> Vec<(String, [u8; 3], bool)> {
            rows.as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let colour = row["colour"].as_array().unwrap();
                    (
                        namespace_label(&row["namespace"]),
                        [
                            u8::try_from(colour[0].as_u64().unwrap()).unwrap(),
                            u8::try_from(colour[1].as_u64().unwrap()).unwrap(),
                            u8::try_from(colour[2].as_u64().unwrap()).unwrap(),
                        ],
                        selected && row["selected"].as_bool().unwrap(),
                    )
                })
                .collect()
        };
    // the list as opened: the reference's entries, in its order, none selected
    assert_eq!(
        listing(&options),
        recorded_listing(&recorded["initial"], false)
    );

    for step in recorded["steps"].as_array().unwrap() {
        let label = step["label"].as_str().unwrap();
        // select the recorded namespaces, the first plainly and the rest with ctrl
        let selected: Vec<String> = step["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(namespace_label)
            .collect();
        for (n, wanted) in selected.iter().enumerate() {
            let at = options
                .get_namespace_colour_rows()
                .iter()
                .position(|row| row.label.as_str() == wanted)
                .unwrap();
            options.invoke_namespace_colour_clicked(i32::try_from(at).unwrap(), n > 0, false);
        }
        // answer each picker the way the recording did, noting where it started
        let replies: Vec<([i64; 3], [i64; 3])> = step["replies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reply| {
                let numbers = |v: &serde_json::Value| -> [i64; 3] {
                    let v = v.as_array().unwrap();
                    [
                        v[0].as_i64().unwrap(),
                        v[1].as_i64().unwrap(),
                        v[2].as_i64().unwrap(),
                    ]
                };
                (numbers(&reply[0]), numbers(&reply[1]))
            })
            .collect();
        options.invoke_namespace_colour_action("edit".into());
        let mut started = Vec::new();
        while let Some(picker) = hydrus_gui::options_namespace_colours::last_picker()
            && picker.window().is_visible()
        {
            let start = [
                i64::from(picker.get_red()),
                i64::from(picker.get_green()),
                i64::from(picker.get_blue()),
            ];
            started.push(start);
            match replies.iter().find(|(from, _)| *from == start) {
                Some((_, to)) => picker.invoke_accepted(
                    i32::try_from(to[0]).unwrap(),
                    i32::try_from(to[1]).unwrap(),
                    i32::try_from(to[2]).unwrap(),
                ),
                None => picker.invoke_cancelled(),
            }
        }
        started.sort_unstable();
        let mut theirs: Vec<[i64; 3]> = step["pickers_started_on"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let c = c.as_array().unwrap();
                [
                    c[0].as_i64().unwrap(),
                    c[1].as_i64().unwrap(),
                    c[2].as_i64().unwrap(),
                ]
            })
            .collect();
        theirs.sort_unstable();
        assert_eq!(started, theirs, "{label}: the colours the pickers start on");
        assert_eq!(
            listing(&options),
            recorded_listing(&step["after"], true),
            "{label}: the list afterwards"
        );
    }
    assert_eq!(
        store.read::<NamespaceColours>(settings::get).unwrap(),
        before,
        "staged until Options is OKed"
    );
    options.invoke_apply();
    let saved = store.read::<NamespaceColours>(settings::get).unwrap();
    let expected = recorded["options_after_commit"].as_object().unwrap();
    assert_eq!(saved.colours.len(), expected.len());
    for (namespace, colour) in &saved.colours {
        let key = namespace.clone().unwrap_or_else(|| "None".to_owned());
        let theirs = expected[&key].as_array().unwrap();
        assert_eq!(
            colour.map(i64::from).to_vec(),
            theirs
                .iter()
                .map(|c| c.as_i64().unwrap())
                .collect::<Vec<_>>(),
            "{key}"
        );
    }
}
