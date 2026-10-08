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
    // the selection and order are kept
    assert!(
        options
            .get_namespace_colour_rows()
            .iter()
            .filter(|row| row.selected)
            .count()
            == 2
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
