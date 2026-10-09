//! The main window's menu bar driven as a user drives it: a title pressed,
//! each submenu on the path pointed at so it opens, and the last entry
//! clicked, by the labels the user reads.

#![allow(dead_code)]

use slint::Model as _;

use hydrus_gui::MainWindow;

/// The open menus' lines: (label, kind) (kind 2 a separator, 3 a submenu).
pub fn panes(ui: &MainWindow) -> Vec<Vec<(String, i32, bool)>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| {
                    let line = lines.row_data(i).unwrap();
                    (line.label.to_string(), line.kind, line.usable)
                })
                .collect()
        })
        .collect()
}

/// Open the menu at `path` (a title, then submenus): its lines.
pub fn open(ui: &MainWindow, path: &[&str]) -> Vec<(String, i32, bool)> {
    let titles = ui.get_menu_titles();
    let top = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == path[0])
        .unwrap_or_else(|| panic!("no menu titled {:?}", path[0]));
    if ui.get_menu_open() != i32::try_from(top).unwrap() {
        ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 40.0, 22.0);
    }
    for (depth, label) in path[1..].iter().enumerate() {
        let lines = &panes(ui)[depth];
        let i = lines
            .iter()
            .position(|l| l.0 == *label)
            .unwrap_or_else(|| panic!("no {label:?} in {lines:?}"));
        assert_eq!(lines[i].1, 3, "{label:?} is a submenu");
        let (p, i) = (i32::try_from(depth).unwrap(), i32::try_from(i).unwrap());
        ui.invoke_menu_line_hovered(
            p,
            i,
            300.0 + 150.0 * p as f32,
            40.0 + 22.0 * i as f32,
            150.0 * p as f32,
        );
    }
    panes(ui)
        .get(path.len() - 1)
        .cloned()
        .unwrap_or_else(|| panic!("{path:?} did not open"))
}

/// Click the entry `label` of the menu at `path`; it must be usable.
pub fn choose(ui: &MainWindow, path: &[&str], label: &str) {
    let lines = open(ui, path);
    let i = lines
        .iter()
        .position(|l| l.0 == label)
        .unwrap_or_else(|| panic!("no {label:?} in {lines:?}"));
    assert!(lines[i].2, "{label:?} is usable");
    ui.invoke_menu_line_clicked(
        i32::try_from(path.len() - 1).unwrap(),
        i32::try_from(i).unwrap(),
        0.0,
        0.0,
        0.0,
    );
}
