//! Real staged Options and pointer/key/wheel input reach thumbnail preferences.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::settings::{self, ThumbnailNavigation};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, row as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "thumbnails")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    window
}

#[derive(Clone, Debug)]
struct SidebarPair {
    frames: [hydrus_gui::MenuChoiceFrame; 3],
    preferred: [f32; 2],
}
type SidebarFrames = std::rc::Rc<std::cell::RefCell<[Option<SidebarPair>; 2]>>;
fn observe_sidebar_buttons(ui: &MainWindow) -> SidebarFrames {
    let frames = std::rc::Rc::new(std::cell::RefCell::new([None, None]));
    ui.set_measure_sidebar_buttons(true);
    ui.on_sidebar_buttons_measured({
        let frames = frames.clone();
        move |index, pair, first, second, first_width, second_width| {
            frames.borrow_mut()[usize::try_from(index).unwrap()] = Some(SidebarPair {
                frames: [pair, first, second],
                preferred: [first_width, second_width],
            });
        }
    });
    frames
}
fn contained(frame: &hydrus_gui::MenuChoiceFrame, parent: &hydrus_gui::MenuChoiceFrame) -> bool {
    frame.x >= parent.x
        && frame.y >= parent.y
        && frame.x + frame.w <= parent.x + parent.w
        && frame.y + frame.h <= parent.y + parent.h
}
fn settled_sidebar_buttons(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    observed: &SidebarFrames,
    width: u32,
    height: u32,
    stacked: bool,
) -> Vec<u8> {
    use slint::platform::WindowAdapter as _;
    use std::time::{Duration, Instant};
    assert!(std::ptr::eq(native.window(), ui.window()));
    assert!(ui.window().is_visible());
    *observed.borrow_mut() = [None, None];
    let started = Instant::now();
    let mut previous = None;
    let mut scrolled = false;
    loop {
        let pixels = headless::render(native, width, height);
        let measured = observed.borrow().clone();
        if let [Some(include), Some(domains)] = &measured {
            let viewport = ui.get_sidebar_search_frame();
            let bits = [include, domains].map(|pair| {
                pair.frames
                    .iter()
                    .flat_map(|frame| [frame.x, frame.y, frame.w, frame.h])
                    .chain(pair.preferred)
                    .map(f32::to_bits)
                    .collect::<Vec<_>>()
            });
            if started.elapsed() >= Duration::from_millis(35)
                && !ui.window().has_active_animations()
                && previous
                    .as_ref()
                    .is_some_and(|(old, old_pixels)| *old == bits && *old_pixels == pixels)
            {
                let last = &domains.frames[2];
                if !scrolled && last.y + last.h > viewport.y + viewport.h {
                    native.dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            viewport.x + viewport.w / 2.0,
                            viewport.y + viewport.h / 2.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -(last.y + last.h - viewport.y - viewport.h + 8.0),
                    });
                    // Re-observe both actual button rows after the physical scroll.
                    *observed.borrow_mut() = [None, None];
                    scrolled = true;
                    previous = None;
                    continue;
                }
                let sidebar = ui.get_sidebar_frame();
                assert!(
                    [viewport.x, viewport.y, viewport.w, viewport.h]
                        .into_iter()
                        .all(f32::is_finite)
                );
                assert!(viewport.w > 0.0 && viewport.h > 0.0 && contained(&viewport, &sidebar));
                assert!(sidebar.x >= 0.0 && sidebar.y >= 0.0);
                assert!(
                    sidebar.x + sidebar.w <= width as f32 && sidebar.y + sidebar.h <= height as f32
                );
                for pair in [include, domains] {
                    let [allocated, first, second] = &pair.frames;
                    for frame in &pair.frames {
                        assert!(
                            [frame.x, frame.y, frame.w, frame.h]
                                .into_iter()
                                .all(f32::is_finite)
                        );
                        assert!(frame.w > 0.0 && frame.h > 0.0);
                        assert!(
                            contained(frame, &sidebar),
                            "actual sidebar containment: {measured:?}; sidebar={sidebar:?}"
                        );
                    }
                    assert!(contained(first, allocated) && contained(second, allocated));
                    assert!(
                        contained(first, &viewport) && contained(second, &viewport),
                        "buttons must be visible in actual search viewport: {measured:?}; viewport={viewport:?}"
                    );
                    assert!(
                        pair.preferred
                            .into_iter()
                            .all(|width| width.is_finite() && width > 0.0)
                    );
                    assert!(
                        first.w >= pair.preferred[0] && second.w >= pair.preferred[1],
                        "full intrinsic label allocation: {measured:?}"
                    );
                    if stacked {
                        assert!(
                            first.y + first.h < second.y,
                            "narrow buttons must be strictly disjoint: {measured:?}"
                        );
                    } else {
                        assert_eq!(first.y.to_bits(), second.y.to_bits());
                        assert!(
                            first.x + first.w < second.x,
                            "wide buttons must remain paired: {measured:?}"
                        );
                    }
                }
                assert!(include.frames[0].y + include.frames[0].h < domains.frames[0].y);
                eprintln!(
                    "actual thumbnail-navigation sidebar {width}x{height}: {measured:?}; sidebar={sidebar:?}"
                );
                return pixels;
            }
            previous = Some((bits, pixels));
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "sidebar controls did not settle: {measured:?}; sidebar={:?}; animations={}",
            ui.get_sidebar_frame(),
            ui.window().has_active_animations()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn capture_saved_navigation_options(
    windows: &headless::Windows,
    options: &OptionsWindow,
    labels: &[serde_json::Value],
    expected_shift: bool,
    filename: &str,
) {
    use slint::platform::WindowAdapter as _;
    use std::time::{Duration, Instant};
    let native = windows.get(windows.count() - 1).unwrap();
    assert!(std::ptr::eq(native.window(), options.window()));
    assert!(options.window().is_visible());
    let check_index = row(options, labels[0].as_str().unwrap());
    let number_index = row(options, labels[1].as_str().unwrap());
    let check = std::rc::Rc::new(std::cell::RefCell::new(None));
    let number = std::rc::Rc::new(std::cell::RefCell::new(None));
    options.set_measure_check_states(true);
    options.on_check_state_measured({
        let check = check.clone();
        move |index, frame, checked, enabled| {
            if index == check_index {
                *check.borrow_mut() = Some((frame, checked, enabled));
            }
        }
    });
    options.set_measure_speed_memory_layout(true);
    options.on_speed_memory_geometry({
        let number = number.clone();
        move |index, kind, frame| {
            if index == number_index && kind == "number" {
                *number.borrow_mut() = Some(frame);
            }
        }
    });
    headless::render(&native, 1000, 900);
    // The three interaction controls end the actual thumbnails section. Use
    // ordinary ScrollView wheel input rather than changing content position.
    native.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(700.0, 500.0),
        delta_x: 0.0,
        delta_y: -10000.0,
    });
    let started = Instant::now();
    let mut previous = None;
    loop {
        let pixels = headless::render(&native, 1000, 900);
        let actual_check = check.borrow().clone();
        let actual_number = number.borrow().clone();
        if let (Some((check, checked, enabled)), Some(number)) = (&actual_check, &actual_number) {
            let bits =
                [check, number].map(|frame| [frame.x, frame.y, frame.w, frame.h].map(f32::to_bits));
            if started.elapsed() >= Duration::from_millis(35)
                && !options.window().has_active_animations()
                && previous
                    .as_ref()
                    .is_some_and(|(old, old_pixels)| *old == bits && *old_pixels == pixels)
            {
                for frame in [check, number] {
                    assert!(
                        [frame.x, frame.y, frame.w, frame.h]
                            .into_iter()
                            .all(f32::is_finite)
                    );
                    assert!(frame.x >= 200.0 && frame.y > 0.0 && frame.w > 0.0 && frame.h > 0.0);
                    assert!(
                        frame.x + frame.w <= 1000.0 && frame.y + frame.h < 850.0,
                        "actual saved controls must fit above the footer: check={actual_check:?}, number={actual_number:?}"
                    );
                }
                assert_eq!(*checked, expected_shift, "actual reopened CheckBox");
                assert!(*enabled);
                assert!(check.y + check.h < number.y);
                headless::save_png(
                    &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(filename),
                    &pixels,
                    1000,
                    900,
                )
                .unwrap();
                return;
            }
            previous = Some((bits, pixels));
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "saved Options controls did not settle: check={actual_check:?}, number={actual_number:?}, visible={}, animations={}",
            options.window().is_visible(),
            options.window().has_active_animations()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn row(w: &OptionsWindow, label: &str) -> i32 {
    w.get_rows()
        .iter()
        .position(|row| row.label == label)
        .unwrap() as i32
}
fn value(p: &ThumbnailNavigation) -> serde_json::Value {
    serde_json::json!({"shift":p.shift_moves_origin,"percent":p.visibility_percent,"rate":p.scroll_rate})
}
fn settle(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    for _ in 0..8 {
        headless::render(native, 700, 600);
    }
}
fn key(native: &slint::platform::software_renderer::MinimalSoftwareWindow, key: Key) {
    let text: slint::SharedString = key.into();
    native.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    native.dispatch_event(WindowEvent::KeyReleased { text });
}
fn click(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ui: &MainWindow,
    index: usize,
    shift: bool,
) {
    let columns = ui.get_grid_columns() as usize;
    let width = ui.get_thumbnail_width() + 2.0 * ui.get_thumbnail_margin();
    let height = ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin();
    let position = slint::LogicalPosition::new(
        ui.get_grid_origin_x() + (index % columns) as f32 * width + width / 2.0,
        ui.get_grid_origin_y()
            + ui.get_grid_scroll()
            + (index / columns) as f32 * height
            + height / 2.0,
    );
    assert!(
        position.y >= ui.get_grid_origin_y()
            && position.y < ui.get_grid_origin_y() + ui.get_grid_visible_height(),
        "pointer target {index} must be inside the visible thumbnail viewport"
    );
    if shift {
        native.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
    }
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    if shift {
        native.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    }
}
#[test]
fn owned_options_replay_cancel_save_reopen_and_real_keyboard_wheel_consumers() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_navigation.json");
    let (_dirs, store) = crate::subscriptions::store();
    // The recorded pointer targets need two columns in this 700px window.
    // Keep the original replay's 280px sidebar rather than inheriting the
    // newly configurable 400px splitter default and clicking below the view.
    store
        .write(|ctx| {
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hpos = 280;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    native.dispatch_event(WindowEvent::WindowActiveChanged(true));
    settle(&native);
    assert!((ui.get_sidebar_actual_width() - 280.0).abs() < 0.1);
    assert_eq!(ui.get_grid_columns(), 2);
    assert!(bound.current.borrow().borrow().results().len() > 12);
    let labels = fixture["labels"].as_array().unwrap();
    let before = store.read(settings::get::<ThumbnailNavigation>).unwrap();
    let cancelled = options(&ui, &bound);
    cancelled.invoke_check_toggled(row(&cancelled, labels[0].as_str().unwrap()), true);
    cancelled.invoke_number_edited(row(&cancelled, labels[1].as_str().unwrap()), 40);
    cancelled.invoke_text_edited(row(&cancelled, labels[2].as_str().unwrap()), "0.5".into());
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store.read(settings::get::<ThumbnailNavigation>).unwrap(),
        before
    );
    for event in fixture["events"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_check_toggled(
            row(&w, labels[0].as_str().unwrap()),
            event["input"][0].as_bool().unwrap(),
        );
        w.invoke_number_edited(
            row(&w, labels[1].as_str().unwrap()),
            event["input"][1].as_i64().unwrap() as i32,
        );
        w.invoke_text_edited(
            row(&w, labels[2].as_str().unwrap()),
            event["input"][2].as_str().unwrap().into(),
        );
        w.invoke_apply();
        assert_eq!(value(&store.read(settings::get).unwrap()), event["saved"]);
        let reopened = options(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[0].as_str().unwrap()) as usize)
                .unwrap()
                .checked,
            event["reopened"]["shift"].as_bool().unwrap()
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[1].as_str().unwrap()) as usize)
                .unwrap()
                .number,
            event["reopened"]["percent"].as_i64().unwrap() as i32
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(row(&reopened, labels[2].as_str().unwrap()) as usize)
                .unwrap()
                .text,
            event["reopened"]["rate"].as_str().unwrap()
        );
        if event == &fixture["events"][0] || event == &fixture["events"][1] {
            capture_saved_navigation_options(
                &windows,
                &reopened,
                &labels,
                event["reopened"]["shift"].as_bool().unwrap(),
                if event == &fixture["events"][0] {
                    "thumbnail-navigation-options-saved-true.png"
                } else {
                    "thumbnail-navigation-options-saved-false.png"
                },
            );
            assert_eq!(value(&store.read(settings::get).unwrap()), event["saved"]);
        }
        reopened.invoke_cancel();
    }
    for case in fixture["selections"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_check_toggled(
            row(&w, labels[0].as_str().unwrap()),
            case["enabled"].as_bool().unwrap(),
        );
        w.invoke_text_edited(row(&w, labels[2].as_str().unwrap()), "1.0".into());
        w.invoke_apply();
        ui.invoke_select_none();
        ui.set_grid_scroll(0.0);
        settle(&native);
        for step in case["steps"].as_array().unwrap() {
            let action = step["action"].as_str().unwrap();
            if action == "move" {
                key(&native, Key::RightArrow);
            } else {
                click(
                    &native,
                    &ui,
                    step["index"].as_u64().unwrap() as usize,
                    action == "shift",
                );
            }
            settle(&native);
            let page = bound.current.borrow().clone();
            let page = page.borrow();
            assert_eq!(
                serde_json::json!(page.selected_indices()),
                step["after"]["selected"]
            );
            assert_eq!(serde_json::json!(page.focused()), step["after"]["focused"]);
        }
    }
    // Real wheel events use Winit's60px-per-line normalization; Qt's fixture
    // uses angle120. Both represent one tick at the default three wheel lines.
    let span = ui.get_thumbnail_height() + 2.0 * ui.get_thumbnail_margin();
    for rate in fixture["rates"].as_array().unwrap() {
        let w = options(&ui, &bound);
        w.invoke_text_edited(
            row(&w, labels[2].as_str().unwrap()),
            rate["rate"].as_str().unwrap().into(),
        );
        w.invoke_apply();
        assert_eq!(
            i64::from(ui.get_thumbnail_wheel_step()),
            rate["step"].as_i64().unwrap()
        );
        ui.set_grid_scroll(0.0);
        settle(&native);
        let selected = bound.current.borrow().borrow().selected_files();
        native.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(
                ui.get_grid_origin_x() + span / 2.0,
                ui.get_grid_origin_y() + span / 2.0,
            ),
            delta_x: 0.0,
            delta_y: -60.0,
        });
        settle(&native);
        let expected = (3.0 * ui.get_thumbnail_wheel_step() as f32)
            .min(ui.get_grid_visible_height())
            .min((ui.get_grid_content_height() - ui.get_grid_visible_height()).max(0.0));
        assert!(
            (-ui.get_grid_scroll() - expected).abs() < 0.1,
            "actual wheel distance"
        );
        assert_eq!(bound.current.borrow().borrow().selected_files(), selected);
    }
    // Partially visible target rows exercise strict threshold changes through
    // actual Down keys; a fully hidden End target would merely hit every cap.
    for percent in [1, 40, 75, 99] {
        let w = options(&ui, &bound);
        w.invoke_number_edited(row(&w, labels[1].as_str().unwrap()), percent);
        w.invoke_apply();
        let count = bound.current.borrow().borrow().results().len();
        let columns = ui.get_grid_columns() as usize;
        let top = ((count - 1) / columns) as f32 * span + ui.get_thumbnail_margin();
        for extra in [-1.0, 1.0] {
            ui.invoke_select_none();
            let offset =
                (top - ui.get_grid_visible_height() + span * percent as f32 / 100.0 + extra).clamp(
                    0.0,
                    (ui.get_grid_content_height() - ui.get_grid_visible_height()).max(0.0),
                );
            ui.set_grid_scroll(-offset);
            settle(&native);
            click(&native, &ui, count - 1 - columns, false);
            settle(&native);
            let target = hydrus_gui_model::thumbnail_navigation::scroll_target(
                f64::from(top),
                f64::from(span),
                f64::from(-ui.get_grid_scroll()),
                f64::from(ui.get_grid_visible_height()),
                f64::from(ui.get_grid_content_height()),
                percent as u8,
            ) as f32;
            key(&native, Key::DownArrow);
            settle(&native);
            assert_eq!(bound.current.borrow().borrow().focused(), Some(count - 1));
            assert!(
                (-ui.get_grid_scroll() - target).abs() < 0.1,
                "percent{percent}, edge{extra}"
            );
        }
    }
    let sidebar_frames = observe_sidebar_buttons(&ui);
    let layout_before = store.read(hydrus_store::page_layout::load).unwrap();
    let results_before = bound.current.borrow().borrow().results().to_vec();
    // After the original input replays, observe the ordinary wider sidebar
    // and restore the recorded narrow consumer for its defining capture.
    ui.set_sidebar_requested_width(400.0);
    let wide = settled_sidebar_buttons(&native, &ui, &sidebar_frames, 1100, 700, false);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("thumbnail-navigation-sidebar-wide.png"),
        &wide,
        1100,
        700,
    )
    .unwrap();
    ui.set_sidebar_requested_width(280.0);
    let _ = settled_sidebar_buttons(&native, &ui, &sidebar_frames, 700, 600, true);
    assert_eq!(
        store.read(hydrus_store::page_layout::load).unwrap(),
        layout_before
    );
    assert_eq!(bound.current.borrow().borrow().results(), results_before);
    assert_eq!(ui.get_grid_columns(), 2);
    let pixels = headless::render_snapshot(&native, 700, 600);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-navigation.png"),
        &pixels,
        700,
        600,
    )
    .unwrap();
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    let reopened = hydrus_store::Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read(settings::get::<ThumbnailNavigation>)
            .unwrap()
            .visibility_percent,
        99
    );
}
