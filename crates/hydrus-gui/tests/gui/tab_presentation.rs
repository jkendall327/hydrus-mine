//! Staged options reach actual nested layout, measured labels and page access.
use hydrus_core::pages::{FileCountDisplay, Page, PageContent, PageKey, PageNameSettings, Session};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store, sessions,
    settings::{self, TabAlignment, TabPresentationSettings},
};
use slint::{ComponentHandle as _, Model as _};

const ALIGN: &str = "Notebook tab alignment: ";
const TREE: &str = "EXPERIMENTAL: Show tab tree view: ";
const HIDE: &str = "EXPERIMENTAL: Hide main page navigation tabs: ";
const ELIDE: &str = "When there are too many tabs to fit, '...' elide their names so they fit: ";
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
fn notebook(name: &str, children: Vec<Page>) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Pages(children),
    }
}
fn source() -> Session {
    let f = hydrus_testkit::fixture_json("tab_presentation.json");
    Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![
            notebook(f["names"][0].as_str().unwrap(), vec![]),
            notebook(
                f["names"][1].as_str().unwrap(),
                vec![
                    search(f["nested_names"][0].as_str().unwrap()),
                    search(f["nested_names"][1].as_str().unwrap()),
                ],
            ),
            notebook(f["names"][2].as_str().unwrap(), vec![]),
        ],
    }
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let index = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui pages")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    let rows = window.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().label == label)
            .unwrap(),
    )
    .unwrap()
}
fn choose(window: &OptionsWindow, settings: TabPresentationSettings) {
    window.invoke_choice_chosen(row(window, ALIGN), settings.alignment.code());
    window.invoke_choice_chosen(row(window, TREE), settings.tree_side());
    window.invoke_check_toggled(row(window, HIDE), settings.hide_navigation_tabs);
    window.invoke_check_toggled(row(window, ELIDE), settings.elide_names);
}
fn seed(store: &Store, session: &Session) {
    let session = session.clone();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            settings::set(
                ctx.conn(),
                &PageNameSettings {
                    max_chars: 256,
                    file_counts: FileCountDisplay::None,
                    ..PageNameSettings::default()
                },
            )
        })
        .unwrap();
}
fn settle(window: &slint::platform::software_renderer::MinimalSoftwareWindow) -> Vec<u8> {
    for _ in 0..20 {
        headless::render(window, 900, 600);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    headless::render_snapshot(window, 900, 600)
}
fn screenshot(name: &str, pixels: &[u8]) {
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        pixels,
        900,
        600,
    )
    .unwrap();
}
fn dimension(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.1,
        "actual {actual}, expected {expected}"
    );
}
fn click(window: &slint::platform::software_renderer::MinimalSoftwareWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}
fn selection(ui: &MainWindow, bound: &hydrus_gui::Bound, key: PageKey, original: &Session) {
    assert_eq!(bound.pages.borrow().shown().key, key);
    assert_eq!(bound.pages.borrow().session(), original);
    let rows = ui.get_tab_rows();
    assert_eq!(rows.row_count(), 2);
    assert_eq!(rows.row_data(0).unwrap().selected, 1);
    assert_eq!(rows.row_data(1).unwrap().selected, 1);
}

#[test]
fn apply_cancel_reopen_and_four_sides_preserve_real_nested_selection_and_full_names() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = source();
    seed(&store, &original);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let tab_frames = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
    // This is the separate measurement observer. Production drag/pointer hit
    // tracking remains installed on tab_geometry and receives the same frames.
    ui.on_tab_geometry_measured({
        let tab_frames = tab_frames.clone();
        move |key, _, _, _, x, y, width, height| {
            if !key.is_empty() {
                tab_frames
                    .borrow_mut()
                    .insert(key.to_string(), (x, y, width, height));
            }
        }
    });
    ui.show().unwrap();
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_chosen(1, 1);
    let key = bound.pages.borrow().shown().key;
    let defaults = TabPresentationSettings::default();
    let cancelled = open(&ui, &bound);
    choose(
        &cancelled,
        TabPresentationSettings {
            alignment: TabAlignment::Bottom,
            tree_alignment: Some(TabAlignment::Right),
            hide_navigation_tabs: true,
            elide_names: false,
        },
    );
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<TabPresentationSettings>)
            .unwrap(),
        defaults,
        "retained cancelled owner cannot apply"
    );
    assert_eq!(ui.get_tab_alignment(), 0);
    assert!(!ui.get_navigation_tabs_hidden());
    selection(&ui, &bound, key, &original);
    let invalid = open(&ui, &bound);
    invalid.invoke_choice_chosen(row(&invalid, ALIGN), 999);
    invalid.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<TabPresentationSettings>)
            .unwrap(),
        defaults
    );
    let fixture = hydrus_testkit::fixture_json("tab_presentation.json");
    let mut previous = None;
    for step in fixture["alignments"].as_array().unwrap() {
        let alignment =
            TabAlignment::from_code(step["settings"]["alignment"].as_i64().unwrap()).unwrap();
        let settings = TabPresentationSettings {
            alignment,
            ..defaults
        };
        let options = open(&ui, &bound);
        choose(&options, settings);
        options.invoke_apply();
        assert_eq!(ui.get_tab_alignment(), alignment.code());
        let pixels = settle(&windows.get(0).unwrap());
        selection(&ui, &bound, key, &original);
        let x = ui.get_tab_navigation_x();
        let y = ui.get_tab_navigation_y();
        let width = ui.get_tab_navigation_width();
        let height = ui.get_tab_navigation_height();
        match alignment {
            TabAlignment::Top => {
                dimension(height, 56.0);
                dimension(ui.get_page_content_y(), y + height);
            }
            TabAlignment::Bottom => {
                dimension(height, 56.0);
                dimension(y, ui.get_page_content_y() + ui.get_page_content_height());
            }
            TabAlignment::Left => {
                dimension(width, 56.0);
                dimension(ui.get_page_content_x(), x + width);
            }
            TabAlignment::Right => {
                dimension(width, 56.0);
                dimension(x, ui.get_page_content_x() + ui.get_page_content_width());
            }
        }
        if matches!(alignment, TabAlignment::Left | TabAlignment::Right) {
            let (x, y, width, height) = tab_frames.borrow()[&original.pages[0].key.to_hex()];
            // Long Qt reference names must paint glyphs along the vertical
            // axis, not merely a horizontal fragment in the middle of a tab.
            // Exclude the tab border and sample actual rendered interior ink.
            let interior_rows: Vec<_> = (0_u16..600)
                .filter(|&row| f32::from(row) >= y + 4.0 && f32::from(row) < y + height - 4.0)
                .collect();
            let ink_rows = interior_rows
                .iter()
                .filter(|&&row| {
                    (0_u16..900).any(|column| {
                        if f32::from(column) < x + 4.0 || f32::from(column) >= x + width - 4.0 {
                            return false;
                        }
                        let pixel =
                            &pixels[(usize::from(row) * 900 + usize::from(column)) * 4..][..3];
                        pixel.iter().all(|&channel| channel < 150)
                    })
                })
                .count();
            assert!(
                ink_rows > interior_rows.len() / 3,
                "long vertical tab label must paint along its length: {ink_rows} ink rows"
            );
        }
        if let Some(previous) = previous {
            assert_ne!(pixels, previous, "actual rendered layout changes sides");
        }
        screenshot(
            &format!(
                "notebook_tabs_{}.png",
                step["choice"]["label"].as_str().unwrap()
            ),
            &pixels,
        );
        previous = Some(pixels);
        assert_eq!(
            Store::open(dir.path())
                .unwrap()
                .read(settings::get::<TabPresentationSettings>)
                .unwrap(),
            settings
        );
        let reopened = open(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, ALIGN)).unwrap())
                .unwrap()
                .index,
            alignment.code()
        );
        reopened.invoke_cancel();
        let rows = ui.get_tab_rows();
        for (level, area) in ["root", "nested"].into_iter().enumerate() {
            let names = rows.row_data(level).unwrap();
            for (index, tab) in step[area]["tabs"].as_array().unwrap().iter().enumerate() {
                assert_eq!(
                    names.names.row_data(index).unwrap(),
                    tab["stored"].as_str().unwrap()
                );
            }
        }
        assert_eq!(
            rows.row_data(0).unwrap().full_names.row_data(0).unwrap(),
            fixture["names"][0].as_str().unwrap()
        );
    }
    (bound.sync)();
    let reopened = Pages::open(Store::open(dir.path()).unwrap()).unwrap();
    assert_eq!(reopened.session(), &original);
    assert_eq!(reopened.shown().key, key);
}

#[test]
fn recorded_hide_gate_retains_live_hierarchy_access_and_elision_changes_actual_paint() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = source();
    seed(&store, &original);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_chosen(1, 1);
    let gamma = bound.pages.borrow().shown().key;
    let PageContent::Pages(children) = &original.pages[1].content else {
        panic!("nested notebook")
    };
    let beta = children[0].key;
    let fixture = hydrus_testkit::fixture_json("tab_presentation.json");
    for step in fixture["hide"].as_array().unwrap() {
        let settings = TabPresentationSettings {
            tree_alignment: step["settings"]["tree"]
                .as_i64()
                .and_then(TabAlignment::from_code),
            hide_navigation_tabs: step["settings"]["hide"].as_bool().unwrap(),
            ..TabPresentationSettings::default()
        };
        let options = open(&ui, &bound);
        choose(&options, settings);
        options.invoke_apply();
        assert_eq!(
            ui.get_navigation_tabs_hidden(),
            step["root"]["hidden"].as_bool().unwrap()
        );
        assert_eq!(ui.get_page_tree_side(), settings.tree_side());
        let pixels = settle(&windows.get(0).unwrap());
        if settings.tabs_hidden() {
            dimension(ui.get_tab_navigation_height(), 0.0);
            let tree = ui.get_page_tree();
            assert_eq!(tree.row_count(), 5);
            assert_eq!(tree.row_data(2).unwrap().key, beta.to_hex());
            assert_eq!(tree.row_data(2).unwrap().depth, 1);
            assert_eq!(tree.row_data(3).unwrap().key, gamma.to_hex());
            assert!(tree.row_data(3).unwrap().selected);
            ui.invoke_page_tree_chosen(beta.to_hex().into());
            assert_eq!(bound.pages.borrow().shown().key, beta);
            ui.invoke_page_tree_chosen(original.pages[1].key.to_hex().into());
            assert_eq!(
                bound.pages.borrow().shown().key,
                beta,
                "notebook remembers its shown child"
            );
            ui.invoke_page_tree_chosen(gamma.to_hex().into());
            ui.invoke_page_tree_chosen(PageKey::random().to_hex().into());
            ui.invoke_page_tree_chosen("invalid".into());
            selection(&ui, &bound, gamma, &original);
            screenshot(
                &format!("notebook_tabs_hidden_{}.png", settings.tree_side()),
                &pixels,
            );
        } else {
            dimension(ui.get_tab_navigation_height(), 56.0);
        }
        selection(&ui, &bound, gamma, &original);
        assert_eq!(
            Store::open(dir.path())
                .unwrap()
                .read(settings::get::<TabPresentationSettings>)
                .unwrap(),
            settings
        );
    }
    let mut paints = Vec::new();
    for elide in [false, true] {
        let options = open(&ui, &bound);
        choose(
            &options,
            TabPresentationSettings {
                elide_names: elide,
                ..TabPresentationSettings::default()
            },
        );
        options.invoke_apply();
        assert_eq!(ui.get_elide_tab_names(), elide);
        let native = windows.get(0).unwrap();
        // A narrower real native window forces fitting; original labels stay intact.
        for _ in 0..20 {
            headless::render(&native, 600, 600);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let pixels = headless::render_snapshot(&native, 600, 600);
        let y = usize::try_from(ui.get_tab_navigation_y() as u32).unwrap();
        let strip = &pixels[y * 600 * 4..(y + 56) * 600 * 4];
        paints.push(strip.to_vec());
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("notebook_tabs_elide_{elide}.png")),
            &pixels,
            600,
            600,
        )
        .unwrap();
        assert_eq!(
            ui.get_tab_rows()
                .row_data(0)
                .unwrap()
                .names
                .row_data(0)
                .unwrap(),
            fixture["alignments"][0]["root"]["tabs"][0]["stored"]
                .as_str()
                .unwrap()
        );
        selection(&ui, &bound, gamma, &original);
        if !elide {
            let x = ui.get_tab_navigation_x() + ui.get_tab_navigation_width() - 10.0;
            let y = ui.get_tab_navigation_y() + 13.0;
            click(&native, x, y);
            let scrolled = headless::render(&native, 600, 600);
            assert_ne!(
                pixels, scrolled,
                "native overflow arrow moves full unelided labels without clipping their height"
            );
            selection(&ui, &bound, gamma, &original);
            for _ in 0..20 {
                click(&native, x, y);
                headless::render(&native, 600, 600);
            }
            native.dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x - 40.0, y),
            });
            headless::render(&native, 600, 600);
            assert_eq!(
                ui.get_tab_tooltip(),
                fixture["names"][2].as_str().unwrap(),
                "last tab remains exposed before the reserved arrow area at maximum scroll"
            );
            selection(&ui, &bound, gamma, &original);
        }
        assert_eq!(
            store
                .read(settings::get::<TabPresentationSettings>)
                .unwrap()
                .elide_names,
            elide
        );
        let reopened = open(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, ELIDE)).unwrap())
                .unwrap()
                .checked,
            elide
        );
        reopened.invoke_cancel();
    }
    assert_ne!(
        paints[0], paints[1],
        "actual tab glyph fitting changes paint, preserving the full labels"
    );
}

#[test]
fn selected_glyph_measurement_triggers_elision_at_the_actual_near_fit_boundary() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![
            search("i"),
            search(&format!("{} distinct end", "M".repeat(42))),
        ],
    };
    seed(&store, &original);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // Stateless layout notifications report the same selected glyph probe that
    // the actual NotebookStrip uses, with all observations owned by this window.
    let measurement = std::rc::Rc::new(std::cell::RefCell::new(None));
    ui.on_tab_measured({
        let measurement = measurement.clone();
        move |level, needed, available, cap| {
            if level == 0 {
                *measurement.borrow_mut() = Some((needed, available, cap));
            }
        }
    });
    ui.show().unwrap();
    settle(&windows.get(0).unwrap());
    let narrow = *measurement.borrow();
    let narrow = narrow.unwrap().0;
    ui.invoke_tab_chosen(0, 1);
    settle(&windows.get(0).unwrap());
    let wide = *measurement.borrow();
    let wide = wide.unwrap().0;
    assert!(
        wide > narrow + 2.0,
        "selected bold glyph advance is included in the actual strip probe: {wide} > {narrow}"
    );
    let width = f32::midpoint(narrow, wide).floor() as u32;
    assert!(width as f32 > narrow && (width as f32) < wide);
    let native = windows.get(0).unwrap();
    for _ in 0..20 {
        headless::render(&native, width, 600);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let pixels = headless::render_snapshot(&native, width, 600);
    let (needed, available, cap) = measurement.borrow().unwrap();
    dimension(needed, wide);
    dimension(available, width as f32);
    assert!(
        cap > 0.0,
        "a viewport that fits unselected text still elides the wider selected glyphs"
    );
    assert_eq!(ui.get_tab_rows().row_data(0).unwrap().selected, 1);
    assert_eq!(bound.pages.borrow().session(), &original);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("notebook_tabs_selected_near_fit.png"),
        &pixels,
        width,
        600,
    )
    .unwrap();
}
