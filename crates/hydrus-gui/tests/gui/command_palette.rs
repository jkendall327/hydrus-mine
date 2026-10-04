//! Real shortcut, async results and launch dispatch under a retained main owner.
use hydrus_core::pages::FavouriteSearch;
use hydrus_gui::{CommandPaletteWindow, MainWindow, Pages, bind, headless};
use hydrus_store::command_palette::{CommandPaletteSettings, Provider};
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};

fn names(window: &CommandPaletteWindow) -> Vec<String> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .filter_map(|i| {
            let row = rows.row_data(i).unwrap();
            (!row.heading).then(|| row.primary.to_string())
        })
        .collect()
}
fn wait(window: &CommandPaletteWindow, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        window.invoke_poll();
        if names(window).iter().any(|s| s.contains(expected)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no {expected:?} in {:?}",
            names(window)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn activate(window: &CommandPaletteWindow, name: &str) {
    let rows = window.get_rows();
    let index = (0..rows.row_count())
        .find(|&i| {
            let row = rows.row_data(i).unwrap();
            !row.heading && row.primary.as_str() == name
        })
        .unwrap_or_else(|| panic!("missing {name:?} in {:?}", names(window)));
    window.invoke_activate(i32::try_from(index).unwrap());
}

#[test]
fn ctrl_p_async_palette_launches_real_pages_favourites_and_main_menu_actions() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &CommandPaletteSettings {
                    show_main_menu: true,
                    show_media_menu: true,
                    initially_show_favourites: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().rename_shown("Palette Alpha");
    let alpha = bound.pages.borrow().shown().key;
    bound.pages.borrow_mut().new_search_page();
    bound.pages.borrow_mut().rename_shown("Palette Beta");
    ui.invoke_tab_chosen(0, 1);
    let favourite = FavouriteSearch {
        folder: Some("Palette Folder".into()),
        name: "Favourite Alpha".into(),
        search: Default::default(),
        synchronised: false,
        sort: None,
        collect: None,
    };
    let written = favourite.clone();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::FavouriteSearches(vec![written]),
            )
        })
        .unwrap();
    ui.show().unwrap();
    let _image = headless::render(&windows.get(0).unwrap(), 1000, 700);
    use slint::platform::{Key, WindowEvent};
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: "p".into() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: "p".into() });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    let palette = bound
        .command_palette
        .borrow()
        .as_ref()
        .expect("Ctrl+P route")
        .clone_strong();
    wait(&palette, "Palette Alpha");
    let image = headless::render(&windows.get(windows.count() - 1).unwrap(), 800, 400);
    assert!(
        image.chunks_exact(4).any(|pixel| pixel != &image[..4]),
        "the palette draws its real input, provider headings and results"
    );
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("command_palette.png"),
        &image,
        800,
        400,
    )
    .unwrap();
    palette.invoke_query_edited("Palette Alpha".into());
    wait(&palette, "Palette Alpha");
    activate(&palette, "Palette Alpha");
    assert!(bound.command_palette.borrow().is_none());
    assert_eq!(bound.pages.borrow().shown().key, alpha);
    ui.invoke_command_palette_requested();
    let palette = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    palette.invoke_query_edited("Palette Folder".into());
    wait(&palette, "Favourite Alpha");
    let before = bound.pages.borrow().page_count();
    activate(&palette, "Favourite Alpha");
    assert_eq!(bound.pages.borrow().page_count(), before + 1);
    assert_eq!(bound.pages.borrow().shown().name, "Favourite Alpha");
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .favourite_to_save()
            .unwrap()
            .search,
        favourite.search
    );
    assert!(!bound.current.borrow().borrow().synchronised());
    ui.invoke_command_palette_requested();
    let palette = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    palette.invoke_query_edited("options".into());
    wait(&palette, "options");
    activate(&palette, "options…");
    assert!(
        bound.options.borrow().is_some(),
        "palette reuses the real main-menu Options handler"
    );
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    options.invoke_cancel();
}
use hydrus_store::Store;

#[test]
fn palette_reopen_rejects_stale_callbacks_and_removed_providers_do_not_return() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &CommandPaletteSettings {
                    provider_order: vec![Provider::Pages],
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().rename_shown("Palette Current");
    ui.invoke_command_palette_requested();
    let old = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    wait(&old, "Palette Current");
    old.invoke_query_edited("Palette".into());
    old.invoke_cancel();
    assert!(bound.command_palette.borrow().is_none());
    ui.invoke_command_palette_requested();
    let successor = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    old.invoke_cancel();
    old.invoke_query_edited("Palette Current".into());
    old.invoke_poll();
    old.invoke_activate(1);
    assert!(bound.command_palette.borrow().is_some());
    wait(&successor, "Palette Current");
    let rows = successor.get_rows();
    assert_eq!(rows.row_count(), 2);
    assert_eq!(rows.row_data(0).unwrap().primary, "Pages");
    successor.invoke_move_selection("down".into());
    assert_eq!(successor.get_selected(), 1);
    successor.invoke_move_selection("up".into());
    assert_eq!(successor.get_selected(), -1);
    successor.invoke_move_selection("up".into());
    assert_eq!(successor.get_selected(), 1);
    successor.invoke_native_focus(false);
    old.invoke_activate(1);
    assert!(bound.command_palette.borrow().is_none());
}

#[test]
fn media_provider_uses_the_actual_thumbnail_dispatcher_and_rejects_a_changed_page() {
    let source = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &CommandPaletteSettings {
                    show_media_menu: true,
                    provider_order: vec![Provider::MediaMenu],
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_tab_chosen(0, 0);
    let original = bound.current.borrow().clone();
    assert!(!original.borrow().results().is_empty());
    original.borrow_mut().select_all();
    assert!(!original.borrow().selected_files().is_empty());
    ui.invoke_command_palette_requested();
    let palette = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    palette.invoke_query_edited("none".into());
    wait(&palette, "none");
    let rows = palette.get_rows();
    let choice = (0..rows.row_count())
        .find_map(|index| {
            let row = rows.row_data(index).unwrap();
            (!row.heading && row.primary.starts_with("none (")).then_some(row.primary.to_string())
        })
        .unwrap();
    activate(&palette, &choice);
    assert!(
        original.borrow().selected_files().is_empty(),
        "real native select-none action must run"
    );
    original.borrow_mut().select_all();
    ui.invoke_command_palette_requested();
    let stale = bound
        .command_palette
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    stale.invoke_query_edited("none".into());
    wait(&stale, "none");
    let rows = stale.get_rows();
    let choice = (0..rows.row_count())
        .find_map(|index| {
            let row = rows.row_data(index).unwrap();
            (!row.heading && row.primary.starts_with("none (")).then_some(row.primary.to_string())
        })
        .unwrap();
    let selected = original.borrow().selected_files();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    activate(&stale, &choice);
    assert_eq!(
        original.borrow().selected_files(),
        selected,
        "a frozen media result must not mutate the old page after its owner changes"
    );
}
