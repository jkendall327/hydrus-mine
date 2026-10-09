//! The options search finds the reference's help paragraphs, unit and
//! computed labels as well as its controls' labels, and goes to them: the
//! page shows, and the row (a paragraph is a row of its own) is highlighted.
//! The full list of entries is compared with the reference's in
//! `tests/model/options_dialog.rs`.

use slint::Model as _;

use crate::options_gui_support::{Client, page_names, row};

fn matches(window: &hydrus_gui::OptionsWindow) -> Vec<String> {
    (0..window.get_matches().row_count())
        .map(|i| window.get_matches().row_data(i).unwrap().to_string())
        .collect()
}

fn go_to(window: &hydrus_gui::OptionsWindow, query: &str, wanted: &str) {
    window.set_search_text(query.into());
    window.invoke_search_edited(query.into());
    let found = matches(window);
    let at = found
        .iter()
        .position(|m| m.starts_with(wanted))
        .unwrap_or_else(|| panic!("{wanted:?} in {found:?}"));
    window.invoke_search_chosen(i32::try_from(at).unwrap());
}

// leaf: audit-options-options-search
#[test]
fn the_search_goes_to_help_paragraphs_and_computed_labels() {
    let client = Client::basic();
    let window = client.open_options();

    // a paragraph the reference shows above a box's controls
    go_to(&window, "hit Ctrl+P", "By default, you can hit Ctrl+P");
    assert_eq!(
        page_names(&window)[usize::try_from(window.get_page()).unwrap()],
        "command palette"
    );
    let (_, note) = row(
        &window,
        "By default, you can hit Ctrl+P to bring up a Command Palette. It initially shows your pages for quick navigation, but it can search for more.",
    );
    assert_eq!(note.kind, 41);
    assert!(note.found, "highlighted");

    // a label the reference computes from the machine
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    go_to(
        &window,
        "you appear to have",
        &format!("(you appear to have {cores} cores)"),
    );
    assert_eq!(
        page_names(&window)[usize::try_from(window.get_page()).unwrap()],
        "maintenance and processing"
    );

    // the unit a byte amount is in
    go_to(
        &window,
        "thumbnail cache",
        "Memory reserved for thumbnail cache:",
    );
    assert_eq!(
        page_names(&window)[usize::try_from(window.get_page()).unwrap()],
        "speed and memory"
    );
}
