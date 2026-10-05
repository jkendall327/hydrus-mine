//! Real wheel input reaches selection or tab scrolling after staged Options.
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store, sessions,
    settings::{self, TabDragSettings, TabPresentationSettings},
};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = lines.iter().position(|l| l.label == "options…").unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = w
        .get_pages()
        .iter()
        .position(|p| p.text == "gui pages")
        .unwrap();
    w.invoke_page_chosen(i32::try_from(page).unwrap());
    w
}
fn toggle(w: &OptionsWindow, label: &str, value: bool) {
    let row = w.get_rows().iter().position(|r| r.label == label).unwrap();
    w.invoke_check_toggled(i32::try_from(row).unwrap(), value);
}
fn seed() -> Session {
    Session {
        name: sessions::LAST_SESSION.into(),
        pages: ["alpha", "beta", "gamma", "omega", "nested"]
            .iter()
            .map(|name| Page {
                key: PageKey::random(),
                name: (*name).into(),
                content: PageContent::Pages(vec![]),
            })
            .collect(),
    }
}
fn wheel(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    delta: f32,
) {
    native.dispatch_event(slint::platform::WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(
            ui.get_tab_navigation_x() + 30.0,
            ui.get_tab_navigation_y() + 14.0,
        ),
        delta_x: 0.0,
        delta_y: delta,
    });
}
#[test]
fn real_wheel_replays_qt_selection_and_staged_scroll_overflow_without_changing_session() {
    const LABEL: &str = "EXPERIMENTAL: Mouse wheel scrolls tab bar, not page selection: ";
    let fixture = hydrus_testkit::fixture_json("tab_drag.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = seed();
    let saved = session.clone();
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 1))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    headless::render(&native, 700, 500);
    for scroll in [false, true] {
        let cancelled = options(&ui, &bound);
        toggle(&cancelled, LABEL, !scroll);
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        assert!(
            !store
                .read(settings::get::<TabDragSettings>)
                .unwrap()
                .wheel_scroll
        );
        let accepted = options(&ui, &bound);
        toggle(&accepted, LABEL, scroll);
        accepted.invoke_apply();
        assert_eq!(
            store
                .read(settings::get::<TabDragSettings>)
                .unwrap()
                .wheel_scroll,
            scroll
        );
        let reopened = options(&ui, &bound);
        let row = reopened
            .get_rows()
            .iter()
            .find(|r| r.label == LABEL)
            .unwrap();
        assert_eq!(row.checked, scroll);
        reopened.invoke_cancel();
        ui.invoke_tab_chosen(0, 2);
        headless::render(&native, 700, 500);
        for step in fixture["wheels"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["scroll"].as_bool() == Some(scroll))
        {
            wheel(&native, &ui, step["delta"].as_f64().unwrap() as f32);
            headless::render(&native, 700, 500);
            assert_eq!(
                bound.pages.borrow().shown().name,
                step["after"]["shown"].as_str().unwrap()
            );
            assert_eq!(bound.pages.borrow().session(), &session);
        }
    }
    // Long real labels overflow; wheel moves viewport while selection stays put.
    let long = Session {
        name: sessions::LAST_SESSION.into(),
        pages: (0..8)
            .map(|i| Page {
                key: PageKey::random(),
                name: format!("long notebook title {i} with distinct ending"),
                content: PageContent::Pages(vec![]),
            })
            .collect(),
    };
    let original = long.clone();
    let long_saved = long.clone();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &long_saved, 2)?;
            settings::set(
                ctx.conn(),
                &TabPresentationSettings {
                    elide_names: false,
                    ..TabPresentationSettings::default()
                },
            )
        })
        .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    let native = windows.get(windows.count() - 1).unwrap();
    let offsets = Rc::new(RefCell::new(Vec::new()));
    ui.on_tab_scrolled({
        let offsets = offsets.clone();
        move |_, offset| offsets.borrow_mut().push(offset)
    });
    headless::render(&native, 500, 500);
    wheel(&native, &ui, -120.0);
    headless::render(&native, 500, 500);
    let first = *offsets.borrow().last().unwrap();
    assert!(first < 0.0);
    assert_eq!(bound.pages.borrow().shown().key, original.pages[0].key);
    wheel(&native, &ui, 120.0);
    headless::render(&native, 500, 500);
    assert!(*offsets.borrow().last().unwrap() > first);
    assert_eq!(bound.pages.borrow().session(), &original);
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
}
