//! Actual staged queue prompts, saved tag consumers and permanently retired owners.
use hydrus_core::{
    Tag,
    tag_presentation::TagPresentation,
    tag_sort::{TagGroupBy, TagSort, TagSortType},
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SessionDialog, bind, headless};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

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
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let index = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag sort")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    assert_eq!(
        window
            .get_rows()
            .iter()
            .filter(|row| row.kind == 37)
            .count(),
        1
    );
    assert_eq!(
        window
            .get_rows()
            .iter()
            .filter(|row| row.kind == 12 || row.kind == 35)
            .count(),
        4
    );
    window
}
fn saved(store: &Store) -> TagPresentation {
    store.read(settings::get).unwrap()
}
fn labels(window: &OptionsWindow) -> Value {
    json!(
        window
            .get_tag_namespace_rows()
            .iter()
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect::<Vec<_>>()
    )
}
fn child(window: &OptionsWindow, action: &str) -> SessionDialog {
    window.invoke_tag_namespace_action(action.into());
    let child = hydrus_gui::options_tag_namespace_order::last_opened().unwrap();
    assert!(child.window().is_visible());
    assert!(window.get_tag_namespace_child_open());
    child
}
fn add(window: &OptionsWindow, raw: &str) {
    let child = child(window, "add");
    assert_eq!(child.get_text(), "namespace");
    assert_eq!(child.get_name_ok_label(), "apply");
    child.invoke_name_entered(raw.into());
    assert!(!window.get_tag_namespace_child_open());
}
#[test]
fn real_queue_matches_all_reference_prompts_and_parent_transactions() {
    let recorded = hydrus_testkit::fixture_json("tag_namespace_order.json");
    let windows = headless::init();
    let (dirs, store) = super::namespace_sorts::store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = saved(&store);
    let options = open(&ui, &bound);
    assert_eq!(labels(&options), recorded["initial"]["labels"]);
    for step in recorded["steps"].as_array().unwrap() {
        if let Some(indices) = step["selection"].as_array() {
            for (n, index) in indices.iter().enumerate() {
                options.invoke_tag_namespace_clicked(
                    i32::try_from(index.as_u64().unwrap()).unwrap(),
                    n != 0,
                    false,
                );
            }
        }
        let action = match step["action"].as_str().unwrap() {
            "_Add" => "add",
            "_Edit" => "edit",
            "_Up" => "up",
            "_Down" => "down",
            "_Delete" => "delete",
            _ => panic!("action"),
        };
        if action == "up" || action == "down" {
            options.invoke_tag_namespace_action(action.into());
        } else {
            let dialog = child(&options, action);
            assert_eq!(
                dialog.get_message(),
                step["calls"][0]["message"].as_str().unwrap()
            );
            let original_page = options.get_page();
            options.invoke_page_chosen(0);
            assert_eq!(
                options.get_page(),
                original_page,
                "owned child blocks page switching"
            );
            options.invoke_apply();
            assert!(options.window().is_visible());
            if action == "delete" {
                dialog.invoke_answered(step["answer"].as_bool().unwrap());
            } else {
                assert_eq!(dialog.get_window_title(), "Enter Text");
                assert_eq!(
                    dialog.get_text(),
                    step["calls"][0]["default"].as_str().unwrap()
                );
                if let Some(raw) = step["answer"].as_str() {
                    dialog.set_text(raw.into());
                    dialog.invoke_name_entered(dialog.get_text());
                } else {
                    dialog.invoke_cancelled();
                }
            }
            assert!(!dialog.window().is_visible());
            assert!(!options.get_tag_namespace_child_open());
        }
        assert_eq!(labels(&options), step["state"]["labels"]);
        assert_eq!(
            json!(
                options
                    .get_tag_namespace_rows()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| row.selected.then_some(i))
                    .collect::<Vec<_>>()
            ),
            step["state"]["selected"]
        );
        assert_eq!(
            saved(&store),
            before,
            "every child edit is staged until parent Apply"
        );
    }
    options.invoke_cancel();
    assert_eq!(saved(&store), before);
    let options = open(&ui, &bound);
    assert_eq!(labels(&options), recorded["initial"]["labels"]);
    for case in recorded["real_text"].as_array().unwrap() {
        let dialog = child(&options, "add");
        assert_eq!(dialog.get_window_title(), case["title"].as_str().unwrap());
        assert_eq!(dialog.get_text(), case["default"].as_str().unwrap());
        assert_eq!(
            dialog.get_name_ok_label(),
            case["buttons"][0].as_str().unwrap()
        );
        dialog.set_text(case["typed"].as_str().unwrap().into());
        if case["cancelled"] == true {
            dialog.invoke_cancelled();
        } else {
            dialog.invoke_name_entered(dialog.get_text());
        }
        assert!(!dialog.window().is_visible());
        assert_eq!(saved(&store), before);
    }
    options.invoke_cancel();
    let options = open(&ui, &bound);
    add(&options, "");
    add(&options, ":");
    add(&options, " CREATOR ");
    add(&options, "creator");
    add(&options, "creator");
    options.invoke_tag_namespace_clicked(0, false, false);
    let editing = child(&options, "edit");
    assert_eq!(editing.get_text(), "creator");
    options.invoke_tag_namespace_clicked(1, false, false);
    editing.invoke_name_entered("studio".into());
    assert_eq!(
        options
            .get_tag_namespace_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "studio",
        "captured stable key is edited"
    );
    store
        .write(|writer| {
            let mut value: TagPresentation = settings::get(writer.conn())?;
            value.namespace_connector = " :: ".into();
            settings::set(writer.conn(), &value)
        })
        .unwrap();
    options.invoke_apply();
    let mut expected = before;
    expected.user_namespaces[0] = "studio".into();
    expected
        .user_namespaces
        .extend(["", ":", " CREATOR ", "creator", "creator"].map(str::to_owned));
    expected.namespace_connector = " :: ".into();
    assert_eq!(saved(&store), expected);
    assert_eq!(
        Store::open(dirs[1].path())
            .unwrap()
            .read(settings::get::<TagPresentation>)
            .unwrap(),
        expected
    );
    let options = open(&ui, &bound);
    let adapter = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&adapter, 1000, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag-namespace-order-options.png"),
        &pixels,
        1000,
        850,
    )
    .unwrap();
    let dialog = child(&options, "add");
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 520, 220);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("tag-namespace-order-enter-text.png"),
        &pixels,
        520,
        220,
    )
    .unwrap();
    dialog.invoke_cancelled();
    options.invoke_cancel();
}
#[test]
fn saved_order_reaches_the_real_sidebar_tag_sort() {
    let recorded = hydrus_testkit::fixture_json("tag_namespace_order.json");
    let seed = hydrus_testkit::fixture_json("tag_dialog_preferences.json");
    let (_dirs, store, files) = super::tag_dialog_preferences::fixture::seed_owned(&seed);
    assert!(files.len() >= 3);
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = recorded.clone();
    let selected = files.clone();
    store
        .write_content(move |writer| {
            for raw in corpus["tags"].as_array().unwrap() {
                let raw = raw.as_str().unwrap();
                let tag = hydrus_store::master::intern_tag(writer.conn(), &Tag::new(raw).unwrap())?;
                let count = usize::try_from(corpus["counts"][raw].as_u64().unwrap()).unwrap();
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &selected[..count],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::fixed(
            store.clone(),
            "recorded namespace corpus",
            None,
            files,
        )),
    );
    let options = open(&ui, &bound);
    for i in 0..options.get_tag_namespace_rows().row_count() {
        options.invoke_tag_namespace_clicked(i32::try_from(i).unwrap(), i != 0, false);
    }
    child(&options, "delete").invoke_answered(true);
    for raw in recorded["applied"].as_array().unwrap() {
        add(&options, raw.as_str().unwrap());
    }
    options.invoke_apply();
    assert_eq!(json!(saved(&store).user_namespaces), recorded["applied"]);
    for case in recorded["sorts"].as_array().unwrap() {
        let sort = TagSort {
            sort_type: match case["type"].as_u64().unwrap() {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => panic!("type"),
            },
            ascending: case["ascending"].as_bool().unwrap(),
            group_by: TagGroupBy::NamespaceUser,
        };
        store
            .write(move |writer| {
                let mut value: TagPresentation = settings::get(writer.conn())?;
                value.search_page_sort = sort;
                settings::set(writer.conn(), &value)
            })
            .unwrap();
        ui.invoke_select_all();
        bound.current.borrow().borrow_mut().refresh_tags();
        let page = bound.current.borrow().clone();
        let page = page.borrow();
        let actual = page
            .tag_rows()
            .iter()
            .filter_map(|row| {
                recorded["tags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|tag| row.starts_with(&format!("{} (", tag.as_str().unwrap())))
                    .map(|tag| tag.as_str().unwrap().to_owned())
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(actual), case["tags"]);
    }
    drop(windows);
}
#[test]
fn hidden_stale_rebound_and_accepted_close_children_cannot_stage_or_save() {
    let _windows = headless::init();
    let (_dirs, store) = super::namespace_sorts::store();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = saved(&store);
    let options = open(&ui, &bound);
    let old = child(&options, "add");
    old.hide().unwrap();
    old.invoke_name_entered("hidden child".into());
    assert!(options.get_tag_namespace_child_open());
    old.show().unwrap();
    options.hide().unwrap();
    old.invoke_name_entered("hidden parent".into());
    options.invoke_tag_namespace_action("up".into());
    assert_eq!(
        labels(&options),
        json!(
            before
                .user_namespaces
                .iter()
                .map(|v| hydrus_gui_model::tag_namespace_order::Editor::label(v))
                .collect::<Vec<_>>()
        )
    );
    options.show().unwrap();
    old.invoke_cancelled();
    let current = child(&options, "add");
    old.invoke_name_entered("stale child".into());
    old.invoke_cancelled();
    assert!(current.window().is_visible());
    current.invoke_name_entered("live raw".into());
    options.invoke_cancel();
    assert_eq!(saved(&store), before);
    let options = open(&ui, &bound);
    let old = child(&options, "add");
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert!(!options.window().is_visible());
    assert!(!old.window().is_visible());
    options.show().unwrap();
    old.show().unwrap();
    old.invoke_name_entered("rebound".into());
    options.invoke_apply();
    assert_eq!(saved(&store), before);
    old.hide().unwrap();
    options.hide().unwrap();
    let options = open(&ui, &successor);
    let current = child(&options, "add");
    store
        .write(|writer| {
            let mut gui: settings::GuiSettings = settings::get(writer.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(writer.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(writer.conn(), &shutdown)?;
            settings::set(writer.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(current.window().is_visible());
    current.invoke_name_entered("declined close stays live".into());
    let retained = child(&options, "add");
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!options.window().is_visible());
    assert!(!retained.window().is_visible());
    ui.show().unwrap();
    options.show().unwrap();
    retained.show().unwrap();
    retained.invoke_name_entered("closed owner".into());
    options.invoke_apply();
    assert_eq!(saved(&store), before);
}
