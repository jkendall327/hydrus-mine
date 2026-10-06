//! Actual main/viewer launch paths, native staged controls and retained callbacks.
use super::tag_dialog_preferences::fixture;
use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
use hydrus_gui::{MainWindow, ManageTagsWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    manage_tags_sort::{Context, Settings, Sort},
    settings,
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
const LABELS: [&str; 2] = [
    "Default tag sort in search page manage tags dialogs: ",
    "Default tag sort in media viewer manage tags dialogs: ",
];
fn sort(value: &Value) -> Sort {
    Sort {
        order: TagSort {
            sort_type: match value["sort_type"].as_u64().unwrap() {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => panic!("unknown type"),
            },
            ascending: value["sort_order"] == 0,
            group_by: match value["group_by"].as_u64().unwrap() {
                0 => TagGroupBy::Nothing,
                1 => TagGroupBy::NamespaceAz,
                2 => TagGroupBy::NamespaceUser,
                _ => panic!("unknown group"),
            },
        },
        use_siblings: value["use_siblings"].as_bool().unwrap(),
    }
}
fn set(store: &Store, context: Context, value: &Value) {
    let value = sort(value);
    store
        .write(move |ctx| {
            let mut saved: Settings = settings::get(ctx.conn())?;
            match context {
                Context::SearchPage => saved.search_page = value,
                Context::MediaViewer => saved.media_viewer = value,
            }
            settings::set(ctx.conn(), &saved)
        })
        .unwrap();
}
fn rows(window: &ManageTagsWindow, recorded: &Value) -> Value {
    let names = recorded["corpus"].as_array().unwrap();
    json!(
        window
            .get_tags()
            .iter()
            .filter_map(|row| names
                .iter()
                .find(|name| row.text.starts_with(name["tag"].as_str().unwrap()))
                .map(|name| name["tag"].as_str().unwrap().to_owned()))
            .collect::<Vec<_>>()
    )
}
fn assert_sort(window: &ManageTagsWindow, value: &Value) {
    assert_eq!(
        window.get_sort_type(),
        value["sort_type"].as_i64().unwrap() as i32
    );
    assert_eq!(
        window.get_sort_order(),
        if value["sort_type"] == 2 {
            i32::from(value["sort_order"] == 0)
        } else {
            i32::from(value["sort_order"] != 0)
        }
    );
    assert_eq!(
        window.get_sort_group(),
        value["group_by"].as_i64().unwrap() as i32
    );
    assert_eq!(
        window.get_sort_siblings(),
        i32::from(value["use_siblings"] != true)
    );
}
fn open_options(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
fn page(window: &OptionsWindow) {
    let index = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag sort")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
}
fn edit(window: &OptionsWindow, index: usize, value: &Value) {
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABELS[index])
        .unwrap();
    let row = i32::try_from(row).unwrap();
    window.invoke_tag_sort_chosen(row, 0, value["sort_type"].as_i64().unwrap() as i32);
    window.invoke_tag_sort_chosen(
        row,
        1,
        if value["sort_type"] == 2 {
            i32::from(value["sort_order"] == 0)
        } else {
            i32::from(value["sort_order"] != 0)
        },
    );
    window.invoke_tag_sort_chosen(row, 2, value["group_by"].as_i64().unwrap() as i32);
    window.invoke_tag_sort_chosen(row, 3, i32::from(value["use_siblings"] != true));
}
#[test]
fn native_options_stage_cancel_save_reopen_and_reject_retired_sender() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (directory, store, files) = fixture::seed_owned(&recorded);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "sort defaults",
            None,
            files,
        )),
    );
    let initial = store.read::<Settings>(settings::get).unwrap();
    open_options(&ui);
    let old = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&old);
    let labels: Vec<_> = old
        .get_rows()
        .iter()
        .filter(|row| row.kind == 12 || row.kind == 35)
        .map(|row| row.label.to_string())
        .collect();
    assert_eq!(
        labels,
        [
            "Default tag sort in search pages: ",
            LABELS[0],
            "Default tag sort in the media viewer: ",
            LABELS[1]
        ]
    );
    for index in 0..2 {
        edit(&old, index, &recorded["options"]["applied"][index]);
    }
    assert_eq!(store.read::<Settings>(settings::get).unwrap(), initial);
    old.invoke_cancel();
    assert_eq!(store.read::<Settings>(settings::get).unwrap(), initial);
    open_options(&ui);
    let current = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&current);
    old.invoke_tag_sort_chosen(0, 0, 2);
    old.invoke_apply();
    assert_eq!(store.read::<Settings>(settings::get).unwrap(), initial);
    for index in 0..2 {
        edit(&current, index, &recorded["options"]["applied"][index]);
    }
    current.hide().unwrap();
    current.invoke_apply();
    assert_eq!(store.read::<Settings>(settings::get).unwrap(), initial);
    current.show().unwrap();
    current.invoke_apply();
    let saved = store.read::<Settings>(settings::get).unwrap();
    assert_eq!(saved.search_page, sort(&recorded["options"]["applied"][0]));
    assert_eq!(saved.media_viewer, sort(&recorded["options"]["applied"][1]));
    let reopened = Store::open(directory[1].path()).unwrap();
    assert_eq!(reopened.read::<Settings>(settings::get).unwrap(), saved);
    open_options(&ui);
    let current = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&current);
    for index in 0..2 {
        let row = current
            .get_rows()
            .iter()
            .find(|row| row.label == LABELS[index])
            .unwrap();
        assert_eq!(row.kind, 35);
        assert_eq!(
            row.index,
            recorded["options"]["reopened"][index]["sort_type"]
                .as_i64()
                .unwrap() as i32
        );
        assert_eq!(row.sibling_sort_visible, index == 0);
        assert_eq!(row.grouped, index == 1);
    }
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1000, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("manage-tags-sort-options.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
    current.invoke_cancel();
}
#[test]
fn main_selection_and_viewer_current_file_use_distinct_defaults_and_live_dialog_keeps_its_sort() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed_owned(&recorded);
    let current_file = files[0];
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "recorded sort corpus",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|file| *file == current_file)
        .unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    ui.invoke_select_all();
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    // Representative native row replays supplement the exhaustive model matrix.
    for index in [0, 1, 6, 7, 48, 49, 66, 67] {
        let case = &recorded["cases"][index];
        let context = if case["context"] == 1 {
            Context::SearchPage
        } else {
            Context::MediaViewer
        };
        set(&store, context, &case["sort"]);
        match context {
            Context::SearchPage => ui.invoke_manage_tags_selected(),
            Context::MediaViewer => viewer.invoke_manage_tags(),
        }
        let window = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        assert_sort(&window, &case["sort"]);
        assert_eq!(rows(&window, &recorded), case["rows"], "{case}");
        window.invoke_cancel();
    }
    for case in recorded["lifetime"].as_array().unwrap() {
        let context = if case["context"] == 1 {
            Context::SearchPage
        } else {
            Context::MediaViewer
        };
        let launch = || match context {
            Context::SearchPage => ui.invoke_manage_tags_selected(),
            Context::MediaViewer => viewer.invoke_manage_tags(),
        };
        // Other context deliberately has a different policy: each real launcher must choose its own.
        let other = if context == Context::SearchPage {
            Context::MediaViewer
        } else {
            Context::SearchPage
        };
        set(&store, other, &case["reopened"]["sort"]);
        set(&store, context, &case["before"]["sort"]);
        launch();
        let old = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        assert_sort(&old, &case["before"]["sort"]);
        assert_eq!(rows(&old, &recorded), case["before"]["rows"]);
        old.invoke_sort_chosen(0, 1);
        old.invoke_sort_chosen(1, 0);
        old.invoke_sort_chosen(3, 0);
        assert_sort(&old, &case["local"]["sort"]);
        assert_eq!(rows(&old, &recorded), case["local"]["rows"]);
        set(&store, context, &case["reopened"]["sort"]);
        assert_sort(&old, &case["after_options"]["sort"]);
        assert_eq!(rows(&old, &recorded), case["after_options"]["rows"]);
        old.hide().unwrap();
        old.invoke_sort_chosen(0, 2);
        assert_sort(&old, &case["local"]["sort"]);
        old.show().unwrap();
        old.invoke_cancel();
        launch();
        let current = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        old.invoke_sort_chosen(0, 0);
        old.invoke_apply();
        assert_sort(&current, &case["reopened"]["sort"]);
        assert_eq!(rows(&current, &recorded), case["reopened"]["rows"]);
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 720, 680);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
        current.invoke_cancel();
    }
}

#[test]
fn rebind_and_accepted_exit_retire_visible_sort_children_and_old_viewer_launcher() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed_owned(&recorded);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let pages = || {
        Pages::single(SearchPage::fixed(
            store.clone(),
            "sort owner",
            None,
            files.clone(),
        ))
    };
    let first = bind(&ui, pages());
    ui.invoke_select_all();
    ui.invoke_thumbnail_activated(0);
    let old_viewer = first.viewer.borrow().as_ref().unwrap().clone_strong();
    old_viewer.hide().unwrap();
    old_viewer.invoke_manage_tags();
    assert!(first.manage_tags.borrow().is_none());
    old_viewer.show().unwrap();
    old_viewer.invoke_close_requested();
    assert!(first.viewer.borrow().is_none());
    old_viewer.show().unwrap();
    old_viewer.invoke_manage_tags();
    assert!(
        first.manage_tags.borrow().is_none(),
        "closed viewer remains retired after re-show even while main binding lives"
    );
    ui.invoke_manage_tags_selected();
    let old = first.manage_tags.borrow().as_ref().unwrap().clone_strong();
    old.invoke_sort_chosen(0, 1);
    let old_type = old.get_sort_type();
    let next = bind(&ui, pages());
    assert!(!old.window().is_visible());
    assert!(first.manage_tags.borrow().is_none());
    old.show().unwrap();
    old.invoke_sort_chosen(0, 2);
    old.invoke_apply();
    assert_eq!(old.get_sort_type(), old_type);
    old_viewer.invoke_manage_tags();
    assert!(first.manage_tags.borrow().is_none());
    assert!(next.manage_tags.borrow().is_none());
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let current = next.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mut gui: hydrus_store::settings::GuiSettings = store.read(settings::get).unwrap();
    gui.confirm_exit = true;
    store
        .write(move |ctx| {
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(current.window().is_visible());
    current.invoke_sort_chosen(0, 1);
    assert_eq!(current.get_sort_type(), 1);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!current.window().is_visible());
    assert!(next.manage_tags.borrow().is_none());
    ui.show().unwrap();
    current.show().unwrap();
    current.invoke_sort_chosen(0, 2);
    current.invoke_apply();
    assert_eq!(current.get_sort_type(), 1);
    ui.invoke_manage_tags_selected();
    old_viewer.invoke_manage_tags();
    assert!(first.manage_tags.borrow().is_none());
    assert!(next.manage_tags.borrow().is_none());
    assert_eq!(
        store.read::<Settings>(settings::get).unwrap(),
        Settings::default(),
        "local sorts never overwrite saved opening policies"
    );
}

#[test]
fn hidden_main_cannot_launch_or_reshow_child_but_visible_viewer_keeps_its_own_route() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed_owned(&recorded);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "emitter ownership",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    assert!(
        bound.manage_tags.borrow().is_none(),
        "hidden main cannot create its selection dialog"
    );
    assert!(
        bound.current.borrow().borrow().selected_files().is_empty(),
        "hidden main cannot dispatch its selection action either"
    );
    ui.show().unwrap();
    ui.invoke_select_all();
    assert_eq!(
        bound.current.borrow().borrow().selected_files().len(),
        bound.current.borrow().borrow().results().len(),
        "visible main selects the actual files before opening its child"
    );
    ui.invoke_manage_tags_selected();
    let child = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    child.invoke_sort_chosen(0, 1);
    child.hide().unwrap();
    ui.hide().unwrap();
    ui.invoke_manage_tags_selected();
    assert!(
        !child.window().is_visible(),
        "hidden main cannot re-show its retained child"
    );
    assert_eq!(child.get_sort_type(), 1);
    assert!(bound.manage_tags.borrow().is_some());
    ui.show().unwrap();
    ui.invoke_manage_tags_selected();
    assert!(
        child.window().is_visible(),
        "visible live main reuses its existing child"
    );
    assert_eq!(child.get_sort_type(), 1);
    child.invoke_cancel();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(viewer.window().is_visible());
    ui.hide().unwrap();
    viewer.invoke_manage_tags();
    let child = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert!(
        child.window().is_visible(),
        "visible viewer owns its route independently of hidden main"
    );
    assert_eq!(child.get_window_title(), "manage tags");
    child.hide().unwrap();
    ui.invoke_manage_tags_selected();
    assert!(
        !child.window().is_visible(),
        "main emitter guard applies before shared existing-slot show"
    );
    viewer.invoke_manage_tags();
    assert!(child.window().is_visible());
    child.invoke_cancel();
    assert_eq!(
        store.read::<Settings>(settings::get).unwrap(),
        Settings::default()
    );
}
