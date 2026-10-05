//! The recorded File submenu exposes a real command, without a dialog action.
use hydrus_gui_model::main_menu::{Command, Facts, OpenMenus, menubar};
#[test]
fn actual_file_submenu_enables_and_selects_quick_export_directory() {
    let fixture = hydrus_testkit::fixture_json("quick_export_directory.json");
    let mut open = OpenMenus::default();
    open.open(menubar(&Facts::default()), 0, 0.0, 22.0);
    open.click(0, 4, 150.0, 40.0, 0.0);
    let panes = open.view();
    let labels: Vec<_> = panes[1].lines.iter().map(|row| row.0.as_str()).collect();
    let expected: Vec<_> = fixture["open_labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(labels, expected);
    assert_eq!(panes[1].lines[2].2, fixture["enabled"].as_bool().unwrap());
    assert_eq!(
        open.click(1, 2, 0.0, 0.0, 0.0),
        Some(Command::OpenQuickExportDirectory)
    );
    assert!(!open.is_open());
}
