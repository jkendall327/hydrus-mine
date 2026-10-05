//! Physical wheel and native ComboBox input, saved consumers and owner retirement.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{Store, menu_choice_wheel::MenuChoiceWheel, settings};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};
fn settle(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    for _ in 0..8 {
        headless::render(native, 1100, 800);
    }
}
fn wheel(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    frame: &hydrus_gui::MenuChoiceFrame,
    dx: f32,
    dy: f32,
) {
    assert!(frame.w > 0.0 && frame.h > 0.0);
    native.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(frame.x + frame.w / 2.0, frame.y + frame.h / 2.0),
        delta_x: dx,
        delta_y: dy,
    });
    settle(native);
}
fn click(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    frame: &hydrus_gui::MenuChoiceFrame,
) {
    let position = slint::LogicalPosition::new(frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    click_at(native, position);
}
fn click_at(
    native: &slint::platform::software_renderer::MinimalSoftwareWindow,
    position: slint::LogicalPosition,
) {
    native.dispatch_event(WindowEvent::PointerMoved { position });
    native.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    native.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    settle(native);
}
fn key(native: &slint::platform::software_renderer::MinimalSoftwareWindow, key: Key) {
    let text: slint::SharedString = key.into();
    native.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    native.dispatch_event(WindowEvent::KeyReleased { text });
    settle(native);
}
fn save(store: &Store, enabled: bool) {
    store
        .write(move |writer| settings::set(writer.conn(), &MenuChoiceWheel { enabled }))
        .unwrap();
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let row = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
    bound.options.borrow().as_ref().unwrap().clone_strong()
}
fn page(window: &OptionsWindow, name: &str) {
    let index = window
        .get_pages()
        .iter()
        .position(|page| page.text == name)
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
}
fn setting(window: &OptionsWindow) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == "Mouse wheel can \"scroll\" through menu buttons: ")
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn physical_media_order_changes_results_and_native_pointer_still_opens_and_chooses() {
    let windows = headless::init();
    let (_directories, store) = super::namespace_sorts::store();
    let page = super::common::all_local_page(store.clone());
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(page));
    let native = windows.get(0).unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    settle(&native);
    let size = ui
        .get_sort_names()
        .iter()
        .position(|name| name.to_string().contains("filesize"))
        .unwrap();
    ui.invoke_sort_chosen(i32::try_from(size).unwrap());
    ui.invoke_order_chosen(0);
    settle(&native);
    let ascending = bound.current.borrow().borrow().results().to_vec();
    assert!(ascending.len() > 1);
    assert_eq!(ui.get_order_index(), 0);
    wheel(&native, &ui.get_sort_order_frame(), 0.0, 120.0);
    assert_eq!(ui.get_order_index(), 1);
    let descending = bound.current.borrow().borrow().results().to_vec();
    assert_ne!(
        ascending, descending,
        "wheel changes real media results, not just text"
    );
    assert!(!bound.current.borrow().borrow().sort().ascending);
    save(&store, false);
    wheel(&native, &ui.get_sort_order_frame(), 0.0, -120.0);
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        descending
    );
    save(&store, true);
    wheel(&native, &ui.get_sort_order_frame(), 0.0, -120.0);
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        ascending
    );
    // A real pointer opens the native ComboBox; its focused native popup chooses
    // by Down/Return. A wheel-only overlay stealing the press cannot pass this.
    save(&store, false);
    click(&native, &ui.get_sort_order_frame());
    key(&native, Key::DownArrow);
    key(&native, Key::Return);
    assert_eq!(ui.get_order_index(), 1);
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        descending
    );
    assert!(!ui.get_sort_order_popup_open());
    // The ignored focused wheel must neither select nor leak a disabled child.
    wheel(&native, &ui.get_sort_order_frame(), 0.0, 120.0);
    assert_eq!(ui.get_order_index(), 1);
    key(&native, Key::UpArrow);
    assert_eq!(ui.get_order_index(), 0);
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        ascending
    );
    // The native styles put the popup above or below its button. Hit its
    // bounded two-row band using real pointer events, never selected callbacks.
    let frame = ui.get_sort_order_frame();
    for offset in 0..40 {
        click(&native, &frame);
        assert!(ui.get_sort_order_popup_open(), "pointer opened native menu");
        click_at(
            &native,
            slint::LogicalPosition::new(frame.x + frame.w / 2.0, frame.y + offset as f32 * 4.0),
        );
        if ui.get_order_index() == 1 && !ui.get_sort_order_popup_open() {
            break;
        }
        key(&native, Key::Escape);
    }
    assert_eq!(
        ui.get_order_index(),
        1,
        "pointer chose native second menu item"
    );
    assert!(!ui.get_sort_order_popup_open());
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        descending
    );
    save(&store, true);
    // Pointer selection must not detach the wrapper from later owned updates.
    wheel(&native, &ui.get_sort_order_frame(), 120.0, 0.0);
    assert_eq!(ui.get_order_index(), 0);
    assert_eq!(
        bound.current.borrow().borrow().results().to_vec(),
        ascending
    );
    let pixels = headless::render(&native, 1100, 800);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("menu-choice-wheel-media.png"),
        &pixels,
        1100,
        800,
    )
    .unwrap();
    ui.hide().unwrap();
    wheel(&native, &ui.get_sort_order_frame(), 0.0, 120.0);
    assert_eq!(ui.get_order_index(), 0);
    ui.show().unwrap();
    wheel(&native, &ui.get_sort_order_frame(), 0.0, 120.0);
    assert_eq!(ui.get_order_index(), 1);
    store
        .write(|writer| {
            let mut value: settings::GuiSettings = settings::get(writer.conn())?;
            value.confirm_exit = true;
            settings::set(writer.conn(), &value)
        })
        .unwrap();
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    wheel(&native, &ui.get_sort_order_frame(), 0.0, -120.0);
    assert_eq!(ui.get_order_index(), 1);
    ui.invoke_answer(false);
    wheel(&native, &ui.get_sort_order_frame(), 0.0, -120.0);
    assert_eq!(ui.get_order_index(), 0);
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    ui.show().unwrap();
    wheel(&native, &ui.get_sort_order_frame(), 0.0, 120.0);
    assert_eq!(
        ui.get_order_index(),
        0,
        "accepted-close owner cannot reactivate"
    );
}
#[test]
fn wheel_protocol_emits_single_choice_and_empty_consumes_without_selection() {
    let windows = headless::init();
    let (_directories, store) = super::namespace_sorts::store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let native = windows.get(0).unwrap();
    // Observe the actual widget signal separately from the real consumer test.
    let changes = Rc::new(RefCell::new(Vec::new()));
    ui.on_order_chosen({
        let changes = changes.clone();
        let weak = ui.as_weak();
        move |index| {
            changes.borrow_mut().push(index);
            if let Some(ui) = weak.upgrade() {
                ui.set_order_index(index);
            }
        }
    });
    let fixture = hydrus_testkit::fixture_json("menu_choice_wheel.json");
    for case in fixture["cases"].as_array().unwrap() {
        save(&store, case["enabled"].as_bool().unwrap());
        let choices = case["choices"].as_array().unwrap();
        let names = choices
            .iter()
            .map(|choice| slint::SharedString::from(choice[0].as_str().unwrap()))
            .collect::<Vec<_>>();
        ui.set_order_names(slint::ModelRc::new(slint::VecModel::from(names)));
        let current = choices
            .iter()
            .position(|choice| choice[1] == case["before"])
            .unwrap_or(0);
        ui.set_order_index(i32::try_from(current).unwrap());
        settle(&native);
        changes.borrow_mut().clear();
        wheel(
            &native,
            &ui.get_sort_order_frame(),
            case["dx"].as_f64().unwrap() as f32,
            case["dy"].as_f64().unwrap() as f32,
        );
        assert_eq!(
            changes.borrow().len(),
            usize::try_from(case["emissions"].as_u64().unwrap()).unwrap(),
            "{case}"
        );
        if !choices.is_empty() {
            assert_eq!(
                choices[usize::try_from(ui.get_order_index()).unwrap()][1],
                case["after"]
            );
        }
    }
}
#[test]
fn options_staging_saved_policy_bubbling_and_retired_roots_are_owned() {
    let windows = headless::init();
    let (_directories, store) = super::namespace_sorts::store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    page(&options, "gui");
    assert!(
        options
            .get_rows()
            .row_data(usize::try_from(setting(&options)).unwrap())
            .unwrap()
            .checked
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 800);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("menu-choice-wheel-options.png"),
        &pixels,
        1100,
        800,
    )
    .unwrap();
    options.invoke_check_toggled(setting(&options), false);
    assert!(
        store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    options.invoke_cancel();
    assert!(
        store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    let options = open(&ui, &bound);
    page(&options, "gui");
    options.invoke_check_toggled(setting(&options), false);
    options.invoke_apply();
    assert!(
        !store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
    let options = open(&ui, &bound);
    page(&options, "tag sort");
    let native = windows.get(windows.count() - 1).unwrap();
    let frames = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
    options.on_menu_choice_geometry({
        let frames = frames.clone();
        move |row, part, frame| {
            frames.borrow_mut().insert((row, part), frame);
        }
    });
    settle(&native);
    let row = i32::try_from(
        options
            .get_rows()
            .iter()
            .position(|row| row.label == "Default tag sort in search pages: ")
            .unwrap(),
    )
    .unwrap();
    let frame = frames.borrow().get(&(row, 0)).unwrap().clone();
    let original = options
        .get_rows()
        .row_data(usize::try_from(row).unwrap())
        .unwrap()
        .index;
    let scroll = options.get_options_scroll_y();
    wheel(&native, &frame, 0.0, -120.0);
    assert_eq!(
        options
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .index,
        original
    );
    assert!(
        options.get_options_scroll_y() < scroll,
        "disabled event reaches actual Options ScrollView"
    );
    // Reopen at the top so the same physical row is visible again.
    options.invoke_cancel();
    save(&store, true);
    let options = open(&ui, &bound);
    page(&options, "tag sort");
    let native = windows.get(windows.count() - 1).unwrap();
    let frames = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
    options.on_menu_choice_geometry({
        let frames = frames.clone();
        move |row, part, frame| {
            frames.borrow_mut().insert((row, part), frame);
        }
    });
    settle(&native);
    let frame = frames.borrow().get(&(row, 0)).unwrap().clone();
    options.hide().unwrap();
    wheel(&native, &frame, 0.0, 120.0);
    assert_eq!(
        options
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .index,
        original
    );
    options.show().unwrap();
    wheel(&native, &frame, 0.0, 120.0);
    assert_eq!(
        options
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .index,
        2
    );
    let before = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    options.show().unwrap();
    wheel(&native, &frame, 0.0, -120.0);
    options.invoke_apply();
    assert_eq!(
        store
            .read(hydrus_gui_model::options::Settings::load)
            .unwrap(),
        before
    );
    options.hide().unwrap();
    let options = open(&ui, &successor);
    page(&options, "gui");
    options.invoke_check_toggled(setting(&options), false);
    store
        .write(|writer| {
            let mut value: settings::GuiSettings = settings::get(writer.conn())?;
            value.confirm_exit = true;
            settings::set(writer.conn(), &value)
        })
        .unwrap();
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(options.window().is_visible());
    ui.window().dispatch_event(WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    options.show().unwrap();
    options.invoke_apply();
    assert!(
        store
            .read(hydrus_store::menu_choice_wheel::load)
            .unwrap()
            .enabled
    );
}

#[test]
fn open_manage_tags_reads_live_policy_and_keeps_recorded_type_orders_and_real_rows() {
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
    use hydrus_store::manage_tags_sort::{Settings as Sorts, Sort};
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let wheel_recorded = hydrus_testkit::fixture_json("menu_choice_wheel.json");
    let (_directories, store, files) =
        super::tag_dialog_preferences::fixture::seed_owned(&recorded);
    store
        .write(|writer| {
            let mut sorts: Sorts = settings::get(writer.conn())?;
            sorts.search_page = Sort {
                order: TagSort {
                    sort_type: TagSortType::Tag,
                    ascending: true,
                    group_by: TagGroupBy::NamespaceUser,
                },
                use_siblings: true,
            };
            settings::set(writer.conn(), &sorts)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::fixed(
            store.clone(),
            "wheel sort corpus",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let dialog = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let native = windows.get(windows.count() - 1).unwrap();
    let frames = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
    dialog.on_sort_menu_geometry({
        let frames = frames.clone();
        move |part, frame| {
            frames.borrow_mut().insert(part, frame);
        }
    });
    settle(&native);
    let names = recorded["corpus"].as_array().unwrap();
    for step in wheel_recorded["tags"].as_array().unwrap() {
        let part = match step["field"].as_str().unwrap() {
            "_sort_type" => 0,
            "_sort_order_text" | "_sort_order_count" => 1,
            "_group_by" => 2,
            "_use_siblings" => 3,
            _ => panic!("recorded field"),
        };
        save(&store, step["enabled"].as_bool().unwrap());
        let frame = match part {
            0 => dialog.get_sort_type_frame(),
            1 => dialog.get_sort_order_frame(),
            _ => frames.borrow().get(&part).unwrap().clone(),
        };
        wheel(&native, &frame, 0.0, step["dy"].as_f64().unwrap() as f32);
        let after = &step["after"];
        assert_eq!(
            dialog.get_sort_type(),
            after["type"].as_i64().unwrap() as i32
        );
        assert_eq!(
            dialog.get_sort_order(),
            i32::from(if after["type"] == 2 {
                after["order"] == 0
            } else {
                after["order"] != 0
            })
        );
        assert_eq!(
            dialog.get_sort_group(),
            after["group"].as_i64().unwrap() as i32
        );
        assert_eq!(
            dialog.get_sort_siblings(),
            i32::from(after["siblings"] != true)
        );
        let expected = recorded["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| {
                case["context"] == 1
                    && case["sort"]["sort_type"] == after["type"]
                    && case["sort"]["sort_order"] == after["order"]
                    && case["sort"]["group_by"] == after["group"]
                    && case["sort"]["use_siblings"] == after["siblings"]
            })
            .unwrap();
        let actual = serde_json::json!(
            dialog
                .get_tags()
                .iter()
                .filter_map(|row| {
                    names
                        .iter()
                        .find(|name| row.text.starts_with(name["tag"].as_str().unwrap()))
                        .map(|name| name["tag"].as_str().unwrap().to_owned())
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            actual, expected["rows"],
            "wheel updates real sorted tag rows: {step}"
        );
    }
    let pixels = headless::render(&native, 1100, 800);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("menu-choice-wheel-tags.png"),
        &pixels,
        1100,
        800,
    )
    .unwrap();
    // The final text order is descending; hidden input cannot replace it.
    let before = dialog.get_sort_type();
    dialog.hide().unwrap();
    wheel(&native, &dialog.get_sort_type_frame(), 0.0, 120.0);
    assert_eq!(dialog.get_sort_type(), before);
    dialog.show().unwrap();
    save(&store, false);
    wheel(&native, &dialog.get_sort_type_frame(), 0.0, 120.0);
    assert_eq!(dialog.get_sort_type(), before);
    save(&store, true);
    wheel(&native, &dialog.get_sort_type_frame(), 0.0, 120.0);
    assert_eq!(dialog.get_sort_type(), 2);
    dialog.invoke_cancel();
    dialog.show().unwrap();
    wheel(&native, &dialog.get_sort_type_frame(), 0.0, -120.0);
    assert_eq!(
        dialog.get_sort_type(),
        2,
        "closed owner stays retired after re-show"
    );
    let saved = store.read(settings::get::<Sorts>).unwrap();
    assert_eq!(saved.search_page.order.sort_type, TagSortType::Tag);
    assert!(
        saved.search_page.order.ascending,
        "local wheel order was not saved by Cancel"
    );
}

#[test]
fn manual_export_tag_choices_read_live_policy_and_publish_real_sidebar_rows() {
    use hydrus_core::HashId;
    use hydrus_gui::export_files_window::{self, Slots};
    let (_directories, store) = super::namespace_sorts::store();
    // This test sorts the real sidebar without creating/exporting any files.
    let destination = tempfile::tempdir().unwrap();
    let path = destination.path().to_string_lossy().into_owned();
    store
        .write(move |writer| {
            let mut prefs: settings::ExportSettings = settings::get(writer.conn())?;
            prefs.default_directory = Some(path);
            settings::set(writer.conn(), &prefs)?;
            let mut presentation: hydrus_core::tag_presentation::TagPresentation =
                settings::get(writer.conn())?;
            presentation.search_page_sort = hydrus_core::tag_sort::TagSort {
                sort_type: hydrus_core::tag_sort::TagSortType::Tag,
                ascending: true,
                group_by: hydrus_core::tag_sort::TagGroupBy::Nothing,
            };
            settings::set(writer.conn(), &presentation)
        })
        .unwrap();
    let windows = headless::init();
    let slots = Slots::default();
    let window = export_files_window::open(
        &store,
        vec![HashId(1), HashId(8), HashId(3)],
        &slots,
        Rc::new(|| {}),
    )
    .unwrap();
    let native = windows.get(0).unwrap();
    let frames = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
    window.on_tag_sort_geometry({
        let frames = frames.clone();
        move |part, frame| {
            frames.borrow_mut().insert(part, frame);
        }
    });
    settle(&native);
    window.invoke_tag_sort_chosen(0, 0);
    window.invoke_tag_sort_chosen(1, 0);
    window.invoke_tag_sort_chosen(2, 0);
    settle(&native);
    let rows = || {
        window
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect::<Vec<_>>()
    };
    let ascending = rows();
    assert!(ascending.len() > 1);
    let order = frames.borrow().get(&1).unwrap().clone();
    wheel(&native, &order, 0.0, -120.0);
    assert_eq!(window.get_tag_sort_order(), 1);
    let descending = rows();
    assert_ne!(ascending, descending);
    save(&store, false);
    wheel(&native, &order, 0.0, -120.0);
    assert_eq!(rows(), descending);
    save(&store, true);
    wheel(&native, &order, 0.0, -120.0);
    assert_eq!(rows(), ascending);
    let group = frames.borrow().get(&2).unwrap().clone();
    wheel(&native, &group, 0.0, 120.0);
    assert_eq!(window.get_tag_sort_group(), 2);
    let grouped = rows();
    let kind = frames.borrow().get(&0).unwrap().clone();
    wheel(&native, &kind, 0.0, 120.0);
    assert_eq!(window.get_tag_sort_type(), 2);
    assert_eq!(
        window.get_tag_sort_order(),
        0,
        "separate count default most first"
    );
    window.hide().unwrap();
    wheel(&native, &kind, 0.0, -120.0);
    assert_eq!(window.get_tag_sort_type(), 2);
    window.show().unwrap();
    window.set_trash(true);
    window.invoke_export(false);
    assert!(window.get_asking());
    assert_eq!(
        window.get_question(),
        hydrus_gui_model::export_files::TRASH_WARNING
    );
    wheel(&native, &kind, 0.0, -120.0);
    assert_eq!(window.get_tag_sort_type(), 2);
    window.invoke_answer(1);
    assert!(!window.get_asking() && !window.get_working());
    wheel(&native, &kind, 0.0, -120.0);
    assert_eq!(window.get_tag_sort_type(), 0);
    assert_eq!(
        rows(),
        grouped,
        "type roundtrip restores text order and real grouping"
    );
    window.invoke_dismissed();
    window.show().unwrap();
    wheel(&native, &kind, 0.0, 120.0);
    assert_eq!(
        window.get_tag_sort_type(),
        0,
        "retired export cannot select after re-show"
    );
    window.hide().unwrap();
    let successor = export_files_window::open(
        &store,
        vec![HashId(1), HashId(8), HashId(3)],
        &slots,
        Rc::new(|| {}),
    )
    .unwrap();
    window.show().unwrap();
    wheel(&native, &kind, 0.0, 120.0);
    assert_eq!(
        successor.get_tag_sort_type(),
        0,
        "retained old root cannot affect successor"
    );
    successor.invoke_dismissed();
}

fn media_type(value: &serde_json::Value) -> hydrus_core::pages::PageSortBy {
    use hydrus_core::pages::PageSortBy;
    match value["type"].as_str().unwrap() {
        "system" => PageSortBy::System(value["data"].as_i64().unwrap()),
        "rating" => PageSortBy::Rating(
            hydrus_core::ServiceKey::from_hex(value["data"].as_str().unwrap()).unwrap(),
        ),
        "namespaces" => PageSortBy::Namespaces {
            namespaces: value["data"]["namespaces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
            tag_display_type: value["data"]["tag_display_type"].as_i64().unwrap(),
        },
        _ => panic!("recorded sort type"),
    }
}
#[test]
fn real_media_type_wheels_reach_main_and_staged_options_including_unoffered_current() {
    use hydrus_core::pages::{PageSort, SortSettings};
    use hydrus_search::SortOrder;
    let windows = headless::init();
    let (_directories, store) = super::namespace_sorts::store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let native = windows.get(0).unwrap();
    let recorded = hydrus_testkit::fixture_json("menu_choice_wheel.json");
    for case in recorded["media_types"]["cases"].as_array().unwrap() {
        let current = bound.current.borrow().clone();
        current
            .borrow_mut()
            .set_sort_type(media_type(&case["before"]));
        current
            .borrow_mut()
            .set_sort_order(if case["before_order"] == 0 {
                SortOrder::Ascending
            } else {
                SortOrder::Descending
            });
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        save(&store, case["enabled"].as_bool().unwrap());
        settle(&native);
        let mut files = current.borrow().results().to_vec();
        files.sort();
        assert!(files.len() > 1);
        wheel(
            &native,
            &ui.get_sort_type_frame(),
            0.0,
            case["dy"].as_f64().unwrap() as f32,
        );
        assert_eq!(
            current.borrow().sort().by,
            media_type(&case["after"]),
            "{case}"
        );
        assert_eq!(
            current.borrow().sort().ascending,
            case["after_order"] == 0,
            "{case}"
        );
        let mut after = current.borrow().results().to_vec();
        after.sort();
        assert_eq!(after, files);
        // The extra current item remains offered for ordinary native selection,
        // but the actual Qt flat wheel traversal never includes it.
        if case["name"] == "unoffered-current-no-change" {
            assert_eq!(ui.get_sort_index(), ui.get_sort_wheel_count());
            assert_eq!(
                ui.get_sort_names().row_count(),
                usize::try_from(ui.get_sort_wheel_count() + 1).unwrap()
            );
        }
    }
    for case in recorded["media_types"]["cases"].as_array().unwrap() {
        let value = PageSort {
            by: media_type(&case["before"]),
            ascending: case["before_order"] == 0,
            tag_context: Default::default(),
        };
        store
            .write(move |writer| {
                let mut prefs: SortSettings = settings::get(writer.conn())?;
                prefs.default_sort = value;
                settings::set(writer.conn(), &prefs)
            })
            .unwrap();
        save(&store, case["enabled"].as_bool().unwrap());
        let options = open(&ui, &bound);
        page(&options, "file sort/collect");
        let native = windows.get(windows.count() - 1).unwrap();
        let frames = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
        options.on_menu_choice_geometry({
            let frames = frames.clone();
            move |row, part, frame| {
                frames.borrow_mut().insert((row, part), frame);
            }
        });
        settle(&native);
        let row = options
            .get_rows()
            .iter()
            .position(|row| row.kind == 10)
            .unwrap();
        let frame = frames
            .borrow()
            .get(&(i32::try_from(row).unwrap(), 0))
            .unwrap()
            .clone();
        wheel(&native, &frame, 0.0, case["dy"].as_f64().unwrap() as f32);
        let actual = options.get_rows().row_data(row).unwrap();
        let choices = hydrus_gui_model::sort::page_choices(&store, &media_type(&case["before"]));
        assert_eq!(
            choices[usize::try_from(actual.index).unwrap()].by,
            media_type(&case["after"]),
            "{case}"
        );
        assert_eq!(
            actual.order_index,
            if case["after_order"] == 0 { 0 } else { 1 },
            "{case}"
        );
        assert_eq!(
            store
                .read(settings::get::<SortSettings>)
                .unwrap()
                .default_sort
                .by,
            media_type(&case["before"]),
            "native wheel is only an Options draft"
        );
        options.invoke_cancel();
    }
}
