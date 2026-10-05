//! Actual Options owners and the opening consumer replay recorded rescue geometry.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless, windows::OpeningRescue};
use hydrus_gui_model::window_rescue::{Rect, Screen};
use hydrus_store::{
    Store,
    settings::{self, WindowRescueSettings},
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
const LABELS: [&str; 3] = [
    "BUGFIX: Disable off-screen window rescue: ",
    "When rescuing, add top-left safety padding:",
    "DEBUG: top-left padding to use (px): ",
];
fn value(p: &WindowRescueSettings) -> Value {
    json!({"disabled":p.disabled,"add_padding":p.add_padding,"padding":p.padding})
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "gui")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn edit(window: &OptionsWindow, event: &Value) {
    window.invoke_check_toggled(row(window, LABELS[0]), event["input"][0].as_bool().unwrap());
    window.invoke_check_toggled(row(window, LABELS[1]), event["input"][1].as_bool().unwrap());
    window.invoke_number_edited(
        row(window, LABELS[2]),
        i32::try_from(event["input"][2].as_i64().unwrap()).unwrap(),
    );
}
#[test]
fn staged_options_cancel_reopen_and_live_owned_opening_consumer_match_qt() {
    let fixture = hydrus_testkit::fixture_json("window_rescue.json");
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    for event in fixture["stages"].as_array().unwrap() {
        let before = store.read(settings::get::<WindowRescueSettings>).unwrap();
        let abandoned = options(&ui, &bound);
        edit(&abandoned, event);
        assert_eq!(
            store.read(settings::get::<WindowRescueSettings>).unwrap(),
            before
        );
        abandoned.invoke_cancel();
        let current = options(&ui, &bound);
        abandoned.invoke_apply();
        assert_eq!(
            store.read(settings::get::<WindowRescueSettings>).unwrap(),
            before
        );
        edit(&current, event);
        assert_eq!(
            store.read(settings::get::<WindowRescueSettings>).unwrap(),
            before
        );
        current.invoke_apply();
        assert_eq!(value(&store.read(settings::get).unwrap()), event["saved"]);
        let reopened = options(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, LABELS[0])).unwrap())
                .unwrap()
                .checked,
            event["reopened"]["disabled"].as_bool().unwrap()
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, LABELS[1])).unwrap())
                .unwrap()
                .checked,
            event["reopened"]["add_padding"].as_bool().unwrap()
        );
        assert_eq!(
            i64::from(
                reopened
                    .get_rows()
                    .row_data(usize::try_from(row(&reopened, LABELS[2])).unwrap())
                    .unwrap()
                    .number
            ),
            event["reopened"]["padding"].as_i64().unwrap()
        );
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 980, 850);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("window_rescue_options.png"),
            &pixels,
            980,
            850,
        )
        .unwrap();
        reopened.invoke_cancel();
    }
    let monitors: Vec<_> = fixture["topology"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let g = &s["geometry"];
            let a = &s["available"];
            Screen {
                geometry: Rect {
                    x: g[0].as_i64().unwrap(),
                    y: g[1].as_i64().unwrap(),
                    width: g[2].as_i64().unwrap(),
                    height: g[3].as_i64().unwrap(),
                },
                available_top_left: (a[0].as_i64().unwrap(), a[1].as_i64().unwrap()),
            }
        })
        .collect();
    for case in fixture["cases"].as_array().unwrap().iter().filter(|case| {
        case["environment"] == "synthetic_topology" && case["size"] == json!([100, 100])
    }) {
        let preferences: WindowRescueSettings =
            serde_json::from_value(case["settings"].clone()).unwrap();
        let copy = preferences.clone();
        store
            .write(move |ctx| settings::set(ctx.conn(), &copy))
            .unwrap();
        let child = OptionsWindow::new().unwrap();
        child
            .window()
            .set_size(slint::LogicalSize::new(100.0, 100.0));
        let size = hydrus_gui::windows::state(child.window()).size;
        assert_eq!(size, (100, 100));
        let desired = (
            case["position"][0].as_i64().unwrap(),
            case["position"][1].as_i64().unwrap(),
        );
        let mut opening = OpeningRescue::new(
            desired,
            "manage_options_dialog",
            store.read(settings::get).unwrap(),
        );
        assert_eq!(
            opening.observe(child.window(), &monitors),
            None,
            "hidden owner cannot consume opening"
        );
        child.show().unwrap();
        let result = opening.observe(child.window(), &monitors);
        assert_eq!(json!(result.map(|(x, y)| [x, y])), case["result"]);
        assert_eq!(
            opening.observe(child.window(), &monitors),
            None,
            "an opening is not repeated by subsequent native events"
        );
        child.hide().unwrap();
    }
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(native.path()).unwrap();
    assert_eq!(
        value(&reopened.read(settings::get).unwrap()),
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|case| case["size"] == json!([100, 100]))
            .unwrap()["settings"]
    );
}
