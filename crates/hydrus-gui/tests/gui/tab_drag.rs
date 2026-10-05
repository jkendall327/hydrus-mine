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
    let rects = geometry(&ui);
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
    // A captured Move/Up can be delivered beyond the bar. The laid-out last tab
    // exists outside the clipping viewport, but must not be navigated or dropped.
    let visible = tab_rect(&rects, original.pages[0].key);
    let clipped = tab_rect(&rects, original.pages[7].key);
    assert!(clipped.x > ui.get_tab_navigation_x() + ui.get_tab_navigation_width());
    pointer(&native, visible, 2);
    assert_eq!(ui.get_tab_tooltip(), original.pages[0].name.as_str());
    pointer(&native, visible, 0);
    std::thread::sleep(std::time::Duration::from_millis(110));
    pointer(&native, clipped, 2);
    assert!(ui.get_tab_drag_active());
    assert_eq!(bound.pages.borrow().shown().key, original.pages[0].key);
    pointer(&native, clipped, 1);
    assert!(!ui.get_tab_drag_active());
    assert_eq!(bound.pages.borrow().session(), &original);
    assert_eq!(bound.pages.borrow().shown().key, original.pages[0].key);
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
}

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: hydrus_search::FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}
fn source() -> Session {
    let mut pages = ["alpha", "beta", "gamma", "omega"]
        .iter()
        .map(|name| search(name))
        .collect::<Vec<_>>();
    pages.push(Page {
        key: PageKey::random(),
        name: "nested".into(),
        content: PageContent::Pages(vec![search("inner one"), search("inner two")]),
    });
    Session {
        name: sessions::LAST_SESSION.into(),
        pages,
    }
}
fn named(session: &Session, name: &str) -> PageKey {
    session
        .all_pages()
        .iter()
        .find(|page| page.name == name)
        .unwrap()
        .key
}
fn set_drag(ui: &MainWindow, bound: &hydrus_gui::Bound, prefs: TabDragSettings) {
    let w = options(ui, bound);
    let start = w
        .get_rows()
        .iter()
        .position(|r| r.label == "Selection chases dropped page after drag and drop: ")
        .unwrap();
    for (offset, value) in [
        prefs.chase,
        prefs.chase_shift,
        prefs.navigate,
        prefs.navigate_shift,
        prefs.wheel_scroll,
        prefs.disabled,
    ]
    .into_iter()
    .enumerate()
    {
        w.invoke_check_toggled(i32::try_from(start + offset).unwrap(), value);
    }
    w.invoke_apply();
}
#[derive(Clone, Copy)]
struct TabRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
type Rects = Rc<RefCell<std::collections::HashMap<String, TabRect>>>;
fn geometry(ui: &MainWindow) -> Rects {
    let rects = Rects::default();
    ui.on_tab_geometry_measured({
        let rects = rects.clone();
        move |key, _, _, _, x, y, w, h| {
            if !key.is_empty() {
                rects
                    .borrow_mut()
                    .insert(key.to_string(), TabRect { x, y, w, h });
            }
        }
    });
    rects
}
fn tab_rect(rects: &Rects, key: PageKey) -> TabRect {
    rects.borrow()[&key.to_hex()]
}
fn pointer(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    rect: TabRect,
    action: i32,
) {
    let position = slint::LogicalPosition::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let event = match action {
        0 => slint::platform::WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        },
        1 => slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        },
        _ => slint::platform::WindowEvent::PointerMoved { position },
    };
    native.dispatch_event(event);
}
fn shift(native: &slint::platform::software_renderer::MinimalSoftwareWindow, down: bool) {
    let event = if down {
        slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Shift.into(),
        }
    } else {
        slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Shift.into(),
        }
    };
    native.dispatch_event(event);
}
fn settle(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    for _ in 0..12 {
        headless::render(native, 900, 600);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn snapshot(bound: &hydrus_gui::Bound) -> serde_json::Value {
    let pages = bound.pages.borrow();
    let session = pages.session();
    let nested = session
        .all_pages()
        .into_iter()
        .find(|p| p.name == "nested")
        .unwrap();
    let PageContent::Pages(children) = &nested.content else {
        panic!("notebook")
    };
    serde_json::json!({"order":session.pages.iter().map(|p|p.name.clone()).collect::<Vec<_>>(),"shown":pages.shown().name,"nested":children.iter().map(|p|p.name.clone()).collect::<Vec<_>>()})
}
fn expected(bound: &hydrus_gui::Bound, step: &serde_json::Value) {
    let value = snapshot(bound);
    for key in ["order", "shown", "nested"] {
        assert_eq!(value[key], step[key], "{key}");
    }
}
#[test]
fn real_pointer_drag_replays_shift_chase_hover_transfer_disable_and_cancel_with_ordered_media() {
    use hydrus_gui_model::tab_drag::Edge;
    let fixture = hydrus_testkit::fixture_json("tab_drag.json");
    let (_dirs, store) = crate::subscriptions::store();
    let session = source();
    let gamma = named(&session, "gamma");
    let nested = named(&session, "nested");
    let alpha = named(&session, "alpha");
    let files = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 3")?
                .query_map([], |row| row.get::<_, hydrus_core::HashId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    assert_eq!(files.len(), 3);
    let ordered = vec![files[2], files[0], files[1]];
    let saved = session.clone();
    let save_files = ordered.clone();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 1)?;
            sessions::set_page_files(ctx.conn(), &gamma, &save_files)?;
            sessions::set_page_selected(ctx.conn(), &gamma, &save_files[1..2])?;
            settings::set(
                ctx.conn(),
                &hydrus_core::pages::PageNameSettings {
                    max_chars: 256,
                    file_counts: hydrus_core::pages::FileCountDisplay::None,
                    ..hydrus_core::pages::PageNameSettings::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let rects = geometry(&ui);
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    native.dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    settle(&native);
    // Real page drags hover a destination without dropping (cancel consumer).
    for step in fixture["navigation"].as_array().unwrap() {
        let held = step["shift"].as_bool().unwrap();
        let enabled = step["enabled"].as_bool().unwrap();
        let prefs = TabDragSettings {
            navigate: if held { !enabled } else { enabled },
            navigate_shift: if held { enabled } else { !enabled },
            ..TabDragSettings::default()
        };
        set_drag(&ui, &bound, prefs);
        ui.invoke_tab_chosen(0, 0);
        settle(&native);
        shift(&native, held);
        pointer(&native, tab_rect(&rects, alpha), 0);
        // Dispatch through Slint: pointer-event(Move) runs before moved().
        // An early Move must preserve the original press for the next Move.
        pointer(&native, tab_rect(&rects, alpha), 2);
        std::thread::sleep(std::time::Duration::from_millis(110));
        pointer(&native, tab_rect(&rects, named(&session, "omega")), 2);
        assert!(
            ui.get_tab_drag_active(),
            "real Move retains the pressed tab"
        );
        settle(&native);
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["after"]["shown"].as_str().unwrap()
        );
        native.dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
        native.dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Escape.into(),
        });
        assert!(!ui.get_tab_drag_active());
        shift(&native, false);
        assert_eq!(bound.pages.borrow().session(), &session);
    }
    for step in fixture["drops"].as_array().unwrap() {
        let held = step["shift"].as_bool().unwrap();
        let chase = step["chase"].as_bool().unwrap();
        set_drag(
            &ui,
            &bound,
            TabDragSettings {
                chase: if held { !chase } else { chase },
                chase_shift: if held { chase } else { !chase },
                navigate: false,
                navigate_shift: false,
                ..TabDragSettings::default()
            },
        );
        ui.invoke_page_tree_chosen(gamma.to_hex().into());
        settle(&native);
        expected(&bound, &step["before"]);
        shift(&native, held);
        pointer(&native, tab_rect(&rects, gamma), 0);
        std::thread::sleep(std::time::Duration::from_millis(110));
        pointer(&native, tab_rect(&rects, alpha), 2);
        settle(&native);
        pointer(&native, tab_rect(&rects, alpha), 1);
        shift(&native, false);
        settle(&native);
        expected(&bound, &step["after"]);
        assert_eq!(
            bound.current.borrow().borrow().files().len(),
            if bound.pages.borrow().shown().key == gamma {
                3
            } else {
                0
            }
        );
    }
    for step in fixture["transfers"].as_array().unwrap() {
        if bound.pages.borrow().tab_parent(gamma) != Some(None) {
            let target = named(&session, "beta");
            assert!(bound.pages.borrow_mut().drop_tab(
                gamma,
                None,
                Some(target),
                Edge::Right,
                true
            ));
            ui.invoke_tab_chosen(0, 2);
        }
        set_drag(
            &ui,
            &bound,
            TabDragSettings {
                chase: step["chase"].as_bool().unwrap(),
                ..TabDragSettings::default()
            },
        );
        settle(&native);
        pointer(&native, tab_rect(&rects, gamma), 0);
        std::thread::sleep(std::time::Duration::from_millis(110));
        pointer(&native, tab_rect(&rects, nested), 2);
        assert!(ui.get_tab_drag_active(), "hover keeps capture across rows");
        settle(&native);
        expected(&bound, &step["before"]);
        let child = named(&session, "inner one");
        pointer(&native, tab_rect(&rects, child), 2);
        assert!(ui.get_tab_drag_active(), "new child Move keeps capture");
        settle(&native);
        pointer(&native, tab_rect(&rects, child), 1);
        assert!(!ui.get_tab_drag_active(), "release ends exactly one drag");
        settle(&native);
        expected(&bound, &step["after"]);
    }
    assert_eq!(bound.pages.borrow().tab_parent(gamma), Some(Some(nested)));
    assert_eq!(bound.pages.borrow().shown().key, gamma);
    assert_eq!(bound.current.borrow().borrow().files(), ordered.as_slice());
    let before = bound.pages.borrow().session().clone();
    set_drag(
        &ui,
        &bound,
        TabDragSettings {
            disabled: true,
            ..TabDragSettings::default()
        },
    );
    settle(&native);
    pointer(&native, tab_rect(&rects, gamma), 0);
    std::thread::sleep(std::time::Duration::from_millis(110));
    pointer(&native, tab_rect(&rects, alpha), 2);
    assert!(!ui.get_tab_drag_active());
    pointer(&native, tab_rect(&rects, alpha), 1);
    assert_eq!(bound.pages.borrow().session(), &before);
    assert!(!bound.pages.borrow_mut().drop_tab(
        nested,
        Some(nested),
        Some(gamma),
        Edge::Body,
        true
    ));
    assert!(!bound.pages.borrow_mut().drop_tab(
        PageKey::random(),
        None,
        Some(alpha),
        Edge::Body,
        true
    ));
    assert_eq!(bound.pages.borrow().session(), &before);
    bound.pages.borrow_mut().sync(42).unwrap();
    assert_eq!(
        store
            .read(|conn| sessions::page_files(conn, &gamma))
            .unwrap(),
        ordered
    );
    assert_eq!(
        store
            .read(|conn| sessions::page_selected(conn, &gamma))
            .unwrap(),
        vec![files[0]]
    );
    let pixels = headless::render_snapshot(&native, 900, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tab-drag-transfer.png"),
        &pixels,
        900,
        600,
    )
    .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.tab_parent(gamma), Some(Some(nested)));
    assert_eq!(reopened.session(), &before);
}
