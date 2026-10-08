//! The "network > pause" menu items, clicked in the real menu bar: each flips only its own
//! global pause switch in the store and shows it ticked, and flips it back. (That each switch
//! stops its workers is tested where the workers are, in hydrus-net and hydrus-download.)

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
    hover(ui, "pause");
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
fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = index(ui, label);
    ui.invoke_menu_line_hovered(
        p,
        i,
        300.0 + 150.0 * p as f32,
        40.0 + 22.0 * i as f32,
        150.0 * p as f32,
    );
}
fn choose(ui: &MainWindow, label: &str) {
    let (p, i) = index(ui, label);
    ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
}

// leaf: audit-network-pause-network_traffic
// leaf: audit-network-pause-file_queues
// leaf: audit-network-pause-watcher_checkers
// leaf: audit-network-pause-subscriptions
#[test]
fn each_pause_item_flips_only_its_own_switch_and_shows_it_ticked() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let items: [(&str, Field); 4] = [
        ("all new network traffic", |p| p.network_traffic),
        ("paged file importing", |p| p.file_queues),
        ("paged watcher checking", |p| p.watcher_checkers),
        ("subscriptions", |p| p.subscriptions),
    ];
    let all = |p: &Pauses| items.map(|(_, f)| f(p));
    assert_eq!(
        all(&store.read(settings::get::<Pauses>).unwrap()),
        [false; 4]
    );
    for (n, (label, field)) in items.iter().enumerate() {
        open_pause_submenu(&ui);
        assert!(
            !lines(&ui).iter().find(|(l, _)| l == label).unwrap().1,
            "{label} starts unticked"
        );
        choose(&ui, label);
        let pauses = store.read(settings::get::<Pauses>).unwrap();
        let mut expected = [false; 4];
        expected[n] = true;
        assert_eq!(all(&pauses), expected, "{label} alone");
        assert!(field(&pauses));
        open_pause_submenu(&ui);
        assert!(
            lines(&ui).iter().find(|(l, _)| l == label).unwrap().1,
            "{label} ticked"
        );
        choose(&ui, label);
        assert_eq!(
            all(&store.read(settings::get::<Pauses>).unwrap()),
            [false; 4],
            "{label} off again"
        );
    }
}

// leaf: audit-network-pause-nudge
#[test]
fn nudge_subscriptions_awake_is_advanced_and_leaves_a_nudge_for_the_daemon() {
    use hydrus_store::queues;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let nudge = "nudge subscriptions awake";
    // (not offered outside advanced mode)
    open_pause_submenu(&ui);
    assert!(!lines(&ui).iter().any(|(l, _)| l == nudge));
    ui.invoke_menu_dismissed();
    store
        .write(|ctx| settings::set(ctx.conn(), &settings::AdvancedMode(true)))
        .unwrap();
    open_pause_submenu(&ui);
    assert!(lines(&ui).iter().any(|(l, _)| l == nudge));
    assert!(!store.read(queues::any_nudged).unwrap());
    choose(&ui, nudge);
    // the daemon (the subscription runner's owner) finds exactly this nudge
    assert!(store.read(queues::any_nudged).unwrap());
    let taken = store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
    assert_eq!(taken, [queues::SUBSCRIPTIONS_NUDGE]);
}
