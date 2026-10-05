//! Real main-window popup cards consuming owned staged PopupPanel width settings.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    popup_width::PopupWidth,
    popups::{self, Job},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::{sync::Arc, time::Duration};

const WIDTH: &str = "Approximate max width of popup messages (in characters): ";
const FIXED: &str = "BUGFIX: Force this width as the fixed width for all popup messages: ";
fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}
fn add(store: &Store, text: &str, gauge: bool) -> Job {
    let mut job = Job::text(text, 0.0);
    if gauge {
        job.popup_gauge_1 = Some((1, 4));
    }
    let saved = job.clone();
    store
        .write(move |ctx| popups::add(ctx.conn(), &saved, now()))
        .unwrap();
    job
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|r| r.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "popup notifications")
        .unwrap() as i32;
    window.set_page(page);
    window.invoke_page_chosen(page);
    window
}
fn row(w: &OptionsWindow, label: &str) -> i32 {
    w.get_rows().iter().position(|r| r.label == label).unwrap() as i32
}
fn sizes(ui: &MainWindow, i: usize) -> (f32, f32) {
    (
        ui.get_popup_card_widths().row_data(i).unwrap(),
        ui.get_popup_card_caps().row_data(i).unwrap(),
    )
}

fn same_sizes(ui: &MainWindow, i: usize, expected: (f32, f32)) {
    let actual = sizes(ui, i);
    assert!((actual.0 - expected.0).abs() < 0.001);
    assert!((actual.1 - expected.1).abs() < 0.001);
}

// Advance actual item-owned measurement timers until every current card has
// reported its laid-out frame. No fixed sleep or fabricated dimensions.
pub(crate) fn render_settled(
    ui: &MainWindow,
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        let pixels = headless::render(native, width, height);
        if (0..ui.get_popups().row_count()).all(|index| {
            let (width, cap) = sizes(ui, index);
            width > 0.0 && cap > 0.0
        }) {
            return pixels;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "current popup cards must report their actual initial frames"
        );
        std::thread::yield_now();
    }
}

#[test]
fn options_cancel_apply_successor_stale_callbacks_and_real_popup_geometry() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let native = windows.get(0).unwrap();
    let render = || render_settled(&ui, &native, 2400, 1200);
    let tick = || {
        std::thread::sleep(Duration::from_millis(300));
        slint::platform::update_timers_and_animations();
    };
    let first = add(&store, "short", false);
    tick();
    render();
    let short = sizes(&ui, 0);
    assert!(
        short.0 > 0.0 && short.0 < short.1,
        "short nonfixed card {short:?}"
    );
    let before = store.read(settings::get::<PopupWidth>).unwrap();
    let cancelled = open(&ui, &bound);
    cancelled.invoke_number_edited(row(&cancelled, WIDTH), 100);
    cancelled.invoke_check_toggled(row(&cancelled, FIXED), true);
    tick();
    render();
    same_sizes(&ui, 0, short);
    assert_eq!(store.read(settings::get::<PopupWidth>).unwrap(), before);
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(store.read(settings::get::<PopupWidth>).unwrap(), before);
    let hidden = open(&ui, &bound);
    hidden.invoke_number_edited(row(&hidden, WIDTH), 256);
    hidden.invoke_check_toggled(row(&hidden, FIXED), true);
    hidden.hide().unwrap();
    hidden.invoke_apply();
    assert_eq!(store.read(settings::get::<PopupWidth>).unwrap(), before);
    assert!(bound.options.borrow().is_some());
    hidden.invoke_cancel();
    let accepted = open(&ui, &bound);
    accepted.invoke_number_edited(row(&accepted, WIDTH), 100);
    accepted.invoke_check_toggled(row(&accepted, FIXED), true);
    accepted.invoke_apply();
    tick();
    render();
    same_sizes(&ui, 0, short);
    let _next = add(&store, "short", false);
    tick();
    render();
    let fixed = sizes(&ui, 1);
    assert!(fixed.1 > short.1);
    assert!(
        (fixed.0 - fixed.1).abs() <= 1.0,
        "fixed successor {fixed:?}"
    );
    assert_eq!(ui.get_popups().row_data(0).unwrap().width_characters, 56);
    assert_eq!(ui.get_popups().row_data(1).unwrap().width_characters, 100);
    let key = first.key;
    store
        .write(move |ctx| {
            popups::update(ctx.conn(), &key, now(), |j| {
                j.status_text_1 = Some("synthetic words ".repeat(40));
            })
        })
        .unwrap();
    tick();
    render();
    let updated = sizes(&ui, 0);
    assert!((updated.1 - short.1).abs() < 1.0);
    assert!(updated.0 > short.0 && updated.0 <= updated.1 + 1.0);
    let reopened = open(&ui, &bound);
    assert_eq!(
        reopened
            .get_rows()
            .row_data(row(&reopened, WIDTH) as usize)
            .unwrap()
            .number,
        100
    );
    assert!(
        reopened
            .get_rows()
            .row_data(row(&reopened, FIXED) as usize)
            .unwrap()
            .checked
    );
    // Retired owners cannot commit into the new Options session.
    cancelled.invoke_number_edited(row(&cancelled, WIDTH), 256);
    cancelled.invoke_apply();
    accepted.invoke_apply();
    assert!(bound.options.borrow().is_some());
    reopened.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<PopupWidth>).unwrap(),
        PopupWidth {
            characters: 100,
            fixed: true
        }
    );
    let frame = render();
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("popup_width_native.png"),
        &frame,
        2400,
        1200,
    )
    .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    drop(store);
    let reopened_store = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened_store.read(settings::get::<PopupWidth>).unwrap(),
        PopupWidth {
            characters: 100,
            fixed: true
        }
    );
}

#[test]
fn real_cards_follow_qt_min_max_fixed_gauge_and_wrapped_text_boundaries() {
    let f = hydrus_testkit::fixture_json("popup_width.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    let native = windows.get(0).unwrap();
    for event in f["events"].as_array().unwrap() {
        let p: PopupWidth = serde_json::from_value(event["saved"].clone()).unwrap();
        store
            .write(move |ctx| {
                popups::dismiss_all_done(ctx.conn(), now())?;
                settings::set(ctx.conn(), &p)
            })
            .unwrap();
        // Retire the previous cards before admitting this cohort.
        std::thread::sleep(Duration::from_millis(300));
        slint::platform::update_timers_and_animations();
        for sample in event["samples"].as_array().unwrap() {
            add(
                &store,
                sample["text"].as_str().unwrap(),
                sample["gauge"].as_bool().unwrap(),
            );
        }
        std::thread::sleep(Duration::from_millis(300));
        slint::platform::update_timers_and_animations();
        render_settled(&ui, &native, 2400, 1600);
        for (i, sample) in event["samples"].as_array().unwrap().iter().enumerate() {
            let (width, cap) = sizes(&ui, i);
            assert!(
                cap > 0.0 && width > 0.0 && width <= cap + 1.0,
                "{width}/{cap}: {sample}"
            );
            if event["saved"]["fixed"].as_bool().unwrap() {
                assert!((width - cap).abs() <= 1.0);
            } else if sample["gauge"].as_bool().unwrap() {
                assert!(width >= cap * 0.9);
            }
        }
    }
}

#[test]
fn pending_eleventh_card_snapshots_on_admission_and_row_removal_keeps_old_policies() {
    let fixture = hydrus_testkit::fixture_json("popup_width.json");
    let expected = &fixture["pending"];
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for i in 0..11 {
        add(&store, &format!("synthetic popup {i}"), false);
    }
    std::thread::sleep(Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    assert_eq!(ui.get_popups().row_count(), 10);
    assert!(
        ui.get_popups()
            .iter()
            .all(|p| p.width_characters == 56 && !p.fixed_width)
    );
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &PopupWidth {
                    characters: 80,
                    fixed: true,
                },
            )
        })
        .unwrap();
    let displaced = ui.get_popups().row_data(0).unwrap();
    ui.invoke_popup_dismiss(0);
    let rows: Vec<_> = ui.get_popups().iter().collect();
    assert_eq!(rows.len(), 10);
    for (shown, recorded) in rows.iter().zip(expected["after"].as_array().unwrap()) {
        assert_eq!(shown.text_1, recorded["text"].as_str().unwrap());
        assert_eq!(
            shown.fixed_width,
            recorded["minimum"] == recorded["maximum"]
        );
    }
    assert_eq!(rows[0].text_1, "synthetic popup 1");
    assert!(
        rows[..9]
            .iter()
            .all(|p| p.width_characters == 56 && !p.fixed_width)
    );
    assert_eq!(rows[9].text_1, "synthetic popup 10");
    assert_eq!(rows[9].width_characters, 80);
    assert!(
        rows[9].fixed_width,
        "queued job snapshots at first display, not creation"
    );
    let native = windows.get(0).unwrap();
    render_settled(&ui, &native, 2400, 1200);
    let (width, cap) = sizes(&ui, 9);
    assert!((width - cap).abs() <= 1.0);
    let oldcap = sizes(&ui, 0).1;
    assert!(
        oldcap < cap,
        "mixed-policy row geometry followed its surviving job"
    );
    let measurements = || {
        (0..ui.get_popups().row_count())
            .map(|index| sizes(&ui, index))
            .collect::<Vec<_>>()
    };
    let settled = measurements();
    ui.invoke_popup_card_measured(0, displaced.key, displaced.gui_owner, 1.0, 2.0);
    assert_eq!(
        measurements(),
        settled,
        "displaced row cannot report into its successor"
    );
    let current = ui.get_popups().row_data(0).unwrap();
    drop(bound);
    ui.invoke_popup_card_measured(0, current.key.clone(), current.gui_owner.clone(), 1.0, 2.0);
    assert_eq!(
        measurements(),
        settled,
        "retired binding cannot report measurements"
    );
    let _successor = bind(&ui, Pages::open(store.clone()).unwrap());
    render_settled(&ui, &native, 2400, 1200);
    let successor = measurements();
    ui.invoke_popup_card_measured(0, current.key, current.gui_owner, 1.0, 2.0);
    assert_eq!(
        measurements(),
        successor,
        "prior GUI owner cannot report into a rebound card"
    );
}
