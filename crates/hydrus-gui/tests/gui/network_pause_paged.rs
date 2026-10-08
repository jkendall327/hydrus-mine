//! The "network > pause" items for all paged importer work and paged gallery
//! searching, clicked in the real menu bar: each flips only its own global
//! switch in the store, shows it ticked, and flips back. That the switches
//! stop the queues' workers is tested where the workers are, in
//! `hydrus-download`'s `url_queue.rs` and `watchers.rs`.
use slint::Model as _;

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, Pauses},
};

type Field = fn(&Pauses) -> bool;

fn open_pause_submenu(ui: &MainWindow) {
    let titles = ui.get_menu_titles();
    let network = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(network).unwrap(), 200.0, 22.0);
    let (p, i) = index(ui, "pause");
    ui.invoke_menu_line_hovered(p, i, 300.0, 40.0 + 22.0 * i as f32, 0.0);
}
fn lines(ui: &MainWindow) -> Vec<(String, bool)> {
    let panes = ui.get_menu_panes();
    let last = panes.row_data(panes.row_count() - 1).unwrap().lines;
    (0..last.row_count())
        .map(|i| {
            let l = last.row_data(i).unwrap();
            (l.label.to_string(), l.checked)
        })
        .collect()
}
fn index(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = ui.get_menu_panes();
    let p = panes.row_count() - 1;
    let i = lines(ui)
        .iter()
        .position(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", lines(ui)));
    (i32::try_from(p).unwrap(), i32::try_from(i).unwrap())
}

// leaf: audit-network-pause-paged_importers
// leaf: audit-network-pause-gallery_searches
#[test]
fn the_paged_importer_and_gallery_search_items_flip_only_their_own_switch() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let items: [(&str, Field); 2] = [
        ("all paged importer work", |p| p.paged_importers),
        ("paged gallery searching", |p| p.gallery_searches),
    ];
    let state = |p: &Pauses| items.map(|(_, f)| f(p));
    for (n, (label, _)) in items.iter().enumerate() {
        open_pause_submenu(&ui);
        assert!(!lines(&ui).iter().find(|(l, _)| l == label).unwrap().1);
        let (p, i) = index(&ui, label);
        ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
        let pauses = store.read(settings::get::<Pauses>).unwrap();
        let mut expected = [false; 2];
        expected[n] = true;
        assert_eq!(state(&pauses), expected, "{label} alone");
        assert_eq!(
            pauses,
            Pauses {
                paged_importers: n == 0,
                gallery_searches: n == 1,
                ..Pauses::default()
            }
        );
        open_pause_submenu(&ui);
        assert!(lines(&ui).iter().find(|(l, _)| l == label).unwrap().1);
        let (p, i) = index(&ui, label);
        ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
        assert_eq!(
            store.read(settings::get::<Pauses>).unwrap(),
            Pauses::default()
        );
    }
}
