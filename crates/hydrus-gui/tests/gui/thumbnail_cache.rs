//! Real staged byte/time widgets, immediate cache consumers and owned debug Clear.
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, ThumbnailCacheSettings},
};
use slint::{ComponentHandle as _, Model as _};
fn store() -> ([tempfile::TempDir; 2], std::sync::Arc<Store>) {
    let source = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    ([source, dir], store)
}
fn menu(ui: &MainWindow, title: &str) {
    let index = ui
        .get_menu_titles()
        .iter()
        .position(|t| t.label == title)
        .unwrap();
    ui.invoke_menu_title_pressed(index as i32, 100., 22.);
}
fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = ui.get_menu_panes();
    let p = panes.row_count() - 1;
    let i = panes
        .row_data(p)
        .unwrap()
        .lines
        .iter()
        .position(|l| l.label == label)
        .unwrap();
    (p as i32, i as i32)
}
fn choose(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_clicked(p, i, 100., 100., 100.);
}
fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_hovered(p, i, 100., 100., 100.);
}
fn clear(ui: &MainWindow) {
    menu(ui, "help");
    hover(ui, "debug");
    hover(ui, "memory actions");
    choose(ui, "clear thumbnail cache");
}
// Navigate the production menu key route after drawing each parent. Its
// placement observer supplies the real submenu anchors, unlike dummy hover
// coordinates that are sufficient to dispatch an action but not to picture it.
fn highlight_menu_line(ui: &MainWindow, label: &str) {
    let (pane, target) = line(ui, label);
    let pane = usize::try_from(pane).unwrap();
    let count = ui
        .get_menu_panes()
        .row_data(pane)
        .unwrap()
        .lines
        .row_count();
    for _ in 0..count {
        if ui.get_menu_panes().row_data(pane).unwrap().current == target {
            return;
        }
        assert!(ui.invoke_menu_key(slint::platform::Key::DownArrow.into(), false));
    }
    assert_eq!(ui.get_menu_panes().row_data(pane).unwrap().current, target);
}
fn capture_clear_menu(ui: &MainWindow, windows: &headless::Windows) {
    let native = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&native, 1100, 850);
    }
    assert!(ui.invoke_menu_key("h".into(), true));
    assert_eq!(ui.get_menu_panes().row_count(), 1);
    for label in ["debug", "memory actions"] {
        for _ in 0..3 {
            headless::render(&native, 1100, 850);
        }
        highlight_menu_line(ui, label);
        let (pane, index) = line(ui, label);
        let entry = ui
            .get_menu_panes()
            .row_data(usize::try_from(pane).unwrap())
            .unwrap()
            .lines
            .row_data(usize::try_from(index).unwrap())
            .unwrap();
        assert_eq!(entry.kind, 3, "{label} is the real submenu");
        assert!(entry.usable);
        assert!(ui.invoke_menu_key(slint::platform::Key::RightArrow.into(), false));
    }
    assert_eq!(ui.get_menu_panes().row_count(), 3);
    highlight_menu_line(ui, "clear thumbnail cache");
    let (pane, index) = line(ui, "clear thumbnail cache");
    let entry = ui
        .get_menu_panes()
        .row_data(usize::try_from(pane).unwrap())
        .unwrap()
        .lines
        .row_data(usize::try_from(index).unwrap())
        .unwrap();
    assert_eq!(entry.kind, 0);
    assert!(entry.usable);
    let pixels = headless::render(&native, 1100, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-cache-clear-menu.png"),
        &pixels,
        1100,
        850,
    )
    .unwrap();
}
fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    menu(ui, "file");
    choose(ui, "options…");
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "speed and memory")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    window
        .get_rows()
        .iter()
        .position(|r| r.label == label)
        .unwrap() as i32
}
fn saved(store: &Store) -> ThumbnailCacheSettings {
    store.read(settings::get).unwrap()
}
fn fill(bound: &Bound) {
    for i in 0..bound.rows.row_count().min(5) {
        bound.rows.row_data(i).unwrap();
    }
    bound.rows.wait();
}
fn bitmap(bound: &Bound) -> Option<hydrus_gui::Thumbnail> {
    bound
        .rows
        .iter()
        .take(5)
        .flat_map(|row| row.thumbnails.iter().collect::<Vec<_>>())
        .find(|thumb| thumb.image.size().width > 0)
}
// leaf: audit-options-help-debug-action-clear-thumbnail-cache
// leaf: audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout
#[test]
fn staged_exact_byte_units_timeout_policy_clear_reopen_and_incarnation_retirement() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_cache.json");
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    fill(&bound);
    assert!(bound.rows.cached_bytes() > 0);
    let first = bitmap(&bound).expect("real retained-source thumbnail pixels were decoded");
    assert!(
        first.image.size().width > 0,
        "real retained-source thumbnail pixels were decoded"
    );
    assert!(
        bound.rows.cached_bytes()
            >= u64::from(first.image.size().width) * u64::from(first.image.size().height) * 3
    );
    assert_eq!(bound.rows.cache_policy(), ThumbnailCacheSettings::default());
    assert_eq!(
        saved(&store).bytes,
        fixture["initial"]["bytes"].as_u64().unwrap()
    );
    assert_eq!(
        saved(&store).timeout,
        fixture["initial"]["timeout"].as_u64().unwrap()
    );
    let window = options(&ui, &bound);
    let bytes = row(&window, "Memory reserved for thumbnail cache:");
    let timeout = row(&window, "Thumbnail cache timeout:");
    assert_eq!(window.get_rows().row_data(bytes as usize).unwrap().kind, 33);
    assert_eq!(
        window.get_rows().row_data(bytes as usize).unwrap().number,
        32
    );
    assert_eq!(window.get_rows().row_data(bytes as usize).unwrap().index, 2);
    window.invoke_number_edited(bytes, 1);
    window.invoke_choice_chosen(bytes, 1);
    window.invoke_field_edited(timeout, 0, 0);
    window.invoke_field_edited(timeout, 2, 5);
    assert!(
        window
            .get_rows()
            .row_data(bytes as usize)
            .unwrap()
            .text
            .contains("about 0 thumbnails")
    );
    assert_eq!(saved(&store), ThumbnailCacheSettings::default());
    assert_eq!(bound.rows.cache_policy(), ThumbnailCacheSettings::default());
    window.invoke_cancel();
    window.invoke_apply();
    assert_eq!(saved(&store), ThumbnailCacheSettings::default());
    let window = options(&ui, &bound);
    let bytes = row(&window, "Memory reserved for thumbnail cache:");
    let timeout = row(&window, "Thumbnail cache timeout:");
    window.invoke_number_edited(bytes, 0);
    window.invoke_choice_chosen(bytes, 0);
    window.invoke_field_edited(timeout, 0, 0);
    window.invoke_field_edited(timeout, 2, 10);
    window.invoke_apply();
    assert_eq!(
        saved(&store),
        ThumbnailCacheSettings {
            bytes: 0,
            timeout: 600
        }
    );
    assert_eq!(bound.rows.cache_policy(), saved(&store));
    assert_eq!(bound.rows.cached_bytes(), 0);
    let window = options(&ui, &bound);
    let bytes = row(&window, "Memory reserved for thumbnail cache:");
    let timeout = row(&window, "Thumbnail cache timeout:");
    // Zero decomposes to TB in the actual BytesControl, and timeout reopens as ten minutes.
    assert_eq!(window.get_rows().row_data(bytes as usize).unwrap().index, 4);
    assert_eq!(
        window
            .get_rows()
            .row_data(timeout as usize)
            .unwrap()
            .fields
            .row_data(2)
            .unwrap()
            .value,
        10
    );
    window.invoke_number_edited(bytes, 32);
    window.invoke_choice_chosen(bytes, 2);
    window.invoke_field_edited(timeout, 2, 0);
    window.invoke_apply();
    assert_eq!(
        saved(&store),
        ThumbnailCacheSettings {
            bytes: 32 * 1024 * 1024,
            timeout: 300
        }
    );
    assert_eq!(bound.rows.cache_policy(), saved(&store));
    fill(&bound);
    assert!(bound.rows.cached() > 0);
    // Clear is the real nested menu action, without a question or a settings write.
    let before = saved(&store);
    assert_eq!(fixture["debug"]["label"], "clear thumbnail cache");
    capture_clear_menu(&ui, &windows);
    choose(&ui, "clear thumbnail cache");
    assert_eq!(ui.get_menu_panes().row_count(), 0);
    assert_eq!(bound.rows.cached(), 0);
    assert!(ui.get_question().is_empty());
    assert_eq!(saved(&store), before);
    fill(&bound);
    assert!(bound.rows.cached() > 0);
    // Keep the immediate zero/no-question/unchanged-settings assertions above
    // any draw: a real thumbnail paint may itself request and refill the cache.
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 850);
    bound.rows.wait();
    let pixels = headless::render_snapshot(&native, 1100, 850);
    assert!(bitmap(&bound).is_some());
    assert!(bound.rows.cached() > 0);
    assert!(ui.get_question().is_empty());
    assert_eq!(saved(&store), before);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-cache-reloaded.png"),
        &pixels,
        1100,
        850,
    )
    .unwrap();
    let decoded = bound.rows.cached_bytes();
    ui.hide().unwrap();
    clear(&ui);
    assert_eq!(bound.rows.cached_bytes(), decoded);
    ui.show().unwrap();
    clear(&ui);
    assert_eq!(bound.rows.cached(), 0);
    fill(&bound);
    // Page switching reuses an already-decoded bitmap immediately, without another wait/decode.
    let shared_bytes = bound.rows.cached_bytes();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    assert!(bitmap(&bound).is_some());
    assert_eq!(bound.rows.cached_bytes(), shared_bytes);
    ui.invoke_tab_chosen(0, 0);
    assert!(bitmap(&bound).is_some());
    assert_eq!(bound.rows.cached_bytes(), shared_bytes);
    // One cache is shared across pages in this MainWindow, while a successor bind retires it.
    let retired_options = options(&ui, &bound);
    retired_options.invoke_number_edited(
        row(&retired_options, "Memory reserved for thumbnail cache:"),
        1,
    );
    retired_options.invoke_choice_chosen(
        row(&retired_options, "Memory reserved for thumbnail cache:"),
        1,
    );
    let old_rows = bound.rows.clone();
    let successor = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    assert!(!retired_options.window().is_visible());
    retired_options.invoke_apply();
    assert_eq!(saved(&store), before);
    assert_eq!(successor.rows.cache_policy(), before);
    assert_eq!(old_rows.cached(), 0);
    old_rows.row_data(0);
    old_rows.wait();
    assert_eq!(old_rows.cached(), 0);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    fill(&successor);
    assert!(successor.rows.cached() > 0);
    let close_options = options(&ui, &successor);
    close_options.invoke_number_edited(
        row(&close_options, "Memory reserved for thumbnail cache:"),
        16,
    );
    close_options.invoke_choice_chosen(
        row(&close_options, "Memory reserved for thumbnail cache:"),
        1,
    );
    let mut gui = saved_gui(&store);
    gui.confirm_exit = true;
    store
        .write(move |w| {
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(w.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(w.conn(), &shutdown)?;
            settings::set(w.conn(), &gui)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert!(close_options.window().is_visible());
    assert_eq!(saved(&store), before);
    assert!(successor.rows.cached() > 0);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!close_options.window().is_visible());
    close_options.invoke_apply();
    assert_eq!(saved(&store), before);
    assert_eq!(successor.rows.cached(), 0);
    ui.show().unwrap();
    successor.rows.row_data(0);
    successor.rows.wait();
    assert_eq!(successor.rows.cached(), 0);
    assert_eq!(
        Store::open(dirs[1].path())
            .unwrap()
            .read(settings::get::<ThumbnailCacheSettings>)
            .unwrap(),
        before
    );
    // Rendered native Options evidence is produced by hosted GUI CI, not locally executed.
    let live = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let window = options(&ui, &live);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1100, 850);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("thumbnail-cache-options.png"),
        &pixels,
        1100,
        850,
    )
    .unwrap();
    window.invoke_cancel();
}
fn saved_gui(store: &Store) -> settings::GuiSettings {
    store.read(settings::get).unwrap()
}
