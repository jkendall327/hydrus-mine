//! Tags > sync: the review entry, "sync now" and the two switches.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, settings::BackgroundWork};
use slint::Model as _;

fn sync_lines(ui: &MainWindow) -> Vec<hydrus_gui::MenuLine> {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    if ui.get_menu_open() < 0 {
        ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    }
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let row = pane.lines.iter().position(|r| r.label == "sync").unwrap();
    assert_eq!(pane.lines.row_data(row).unwrap().kind, 3, "a submenu");
    ui.invoke_menu_line_hovered(0, i32::try_from(row).unwrap(), 200.0, 22.0, 0.0);
    ui.get_menu_panes()
        .row_data(1)
        .unwrap()
        .lines
        .iter()
        .collect()
}
fn click(ui: &MainWindow, label: &str) {
    let lines = sync_lines(ui);
    let at = lines.iter().position(|l| l.label == label).unwrap();
    assert!(lines[at].usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
}

// leaf: audit-media-menu-tags-sync-tag-display-during-idle-time, audit-media-menu-tags-sync-tag-display-during-normal-time, audit-media-menu-tags-sync-now, audit-media-menu-tags-review-current-sibling-parent-sync
#[test]
fn the_sync_submenu_has_the_reference_entries_and_they_act() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    // the reference's order: review, ---, sync now, ---, idle, normal
    let lines = sync_lines(&ui);
    let shape: Vec<_> = lines
        .iter()
        .map(|l| (l.label.to_string(), l.kind))
        .collect();
    assert_eq!(
        shape,
        [
            ("review current sibling/parent sync".to_string(), 0),
            (String::new(), 2),
            ("sync now".to_string(), 0),
            (String::new(), 2),
            ("sync tag display during idle time".to_string(), 1),
            ("sync tag display during normal time".to_string(), 1),
        ]
    );
    let work = |store: &Store| {
        store
            .read(hydrus_store::settings::get::<BackgroundWork>)
            .unwrap()
    };
    let before = work(&store);
    assert_eq!(lines[4].checked, before.tag_display_during_idle);
    assert_eq!(lines[5].checked, before.tag_display_during_active);
    // each switch flips its own option only, and the menu shows it next time
    click(&ui, "sync tag display during idle time");
    let after = work(&store);
    assert_eq!(
        after.tag_display_during_idle,
        !before.tag_display_during_idle
    );
    assert_eq!(
        after.tag_display_during_active,
        before.tag_display_during_active
    );
    assert_eq!(sync_lines(&ui)[4].checked, after.tag_display_during_idle);
    click(&ui, "sync tag display during normal time");
    let last = work(&store);
    assert_eq!(last.tag_display_during_idle, after.tag_display_during_idle);
    assert_eq!(
        last.tag_display_during_active,
        !before.tag_display_during_active
    );
    assert_eq!(sync_lines(&ui)[5].checked, last.tag_display_during_active);
    // sync now: nothing is ever outstanding, so the reference's message
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    click(&ui, "sync now");
    let popups = store
        .read(|conn| hydrus_store::popups::all(conn, now))
        .unwrap();
    assert!(popups.iter().any(|j| {
        serde_json::to_string(j)
            .unwrap()
            .contains("Seems like we are all synced already!")
    }));
    // review opens a window
    let windows_before = windows.count();
    click(&ui, "review current sibling/parent sync");
    assert_eq!(windows.count(), windows_before + 1);
}
