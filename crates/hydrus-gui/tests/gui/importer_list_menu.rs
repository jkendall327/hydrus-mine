//! A gallery and a watcher page's list menus, opened by a right press on
//! a row (selecting it): copying queries and URLs, showing the selected's
//! files, pausing, and opening a log. Their entries are tested against
//! the reference's in hydrus-gui-model's tests.

use std::cell::RefCell;
use std::rc::Rc;

use slint::Model as _;

use hydrus_core::HashId;
use hydrus_gui::{Bound, Clip, MainWindow, Pages, bind, headless, set_clipper};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

use crate::subscriptions::store;

/// The open menus' lines, by pane.
fn menu(ui: &MainWindow) -> Vec<Vec<String>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|l| lines.row_data(l).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

/// Choose the entry labelled `label` in pane `pane` (hovering it opens a
/// submenu).
fn choose(ui: &MainWindow, pane: usize, label: &str) {
    let line = menu(ui)[pane]
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", menu(ui)));
    let (p, l) = (i32::try_from(pane).unwrap(), i32::try_from(line).unwrap());
    ui.invoke_menu_line_hovered(p, l, 300.0, 100.0, 10.0);
    ui.invoke_menu_line_clicked(p, l, 300.0, 100.0, 10.0);
}

/// The gallery list's queues, in its order.
fn queries(bound: &Bound) -> Vec<(i64, String)> {
    let page = bound.current.borrow();
    let page = page.borrow();
    page.gallery()
        .unwrap()
        .queries
        .iter()
        .map(|q| (q.queue, q.query.clone()))
        .collect()
}

/// Give the store a downloader, a gallery page's default.
pub(crate) fn with_downloader(store: &hydrus_store::Store) {
    let gug = hydrus_core::url::AnyGug::Single(hydrus_core::url::Gug {
        name: "example tag search".into(),
        key: "aa".into(),
        url_template: "https://booru.example/search/%tags%/1".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "blue_eyes".into(),
    });
    let downloaders = hydrus_parse::Downloaders {
        gugs: hydrus_core::url::Gugs {
            gugs: vec![gug],
            keys_to_display: vec!["aa".into()],
        },
        ..hydrus_parse::Downloaders::default()
    };
    let defaults = hydrus_core::subscriptions::GalleryDefaults {
        file_limit: None,
        gug: Some(("aa".into(), "example tag search".into())),
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
}

#[test]
fn a_gallery_lists_menu_acts_on_the_pressed_search() {
    let (_dirs, store) = store();
    let first: (HashId, hydrus_core::Sha256) = store
        .read(|conn| {
            let id: HashId = conn.query_row(
                "SELECT hash_id FROM files ORDER BY hash_id LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            Ok((id, hydrus_store::master::hash(conn, id)?.unwrap()))
        })
        .unwrap();
    with_downloader(&store);
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then gallery
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    ui.invoke_gallery_queries("blue\ngreen".into());
    let rows = queries(&bound);
    let (blue_row, blue) = rows
        .iter()
        .enumerate()
        .find(|(_, q)| q.1 == "blue")
        .map(|(i, q)| (i32::try_from(i).unwrap(), q.0))
        .unwrap();
    let hash = first.1;
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let mut meta = FileSeedMeta::default();
            meta.set_hash("sha256", hash.to_hex());
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://booru.example/post/1".into(),
                data_for_comparison: "https://booru.example/post/1".into(),
                source_time: None,
                referral_url: None,
                meta,
            };
            queues::add_file_seeds(conn, blue, &[seed], false, 0)?;
            let mut seeds = queues::file_seeds(conn, blue)?;
            seeds[0].status = SeedStatus::SuccessfulButRedundant;
            queues::update_file_seed(conn, &seeds[0])
        })
        .unwrap();

    // a right press selects the row pressed, and opens the menu
    ui.invoke_importer_list_menu(blue_row, 20.0, 20.0);
    {
        let page = bound.current.borrow();
        assert_eq!(page.borrow().gallery().unwrap().selected(), [blue]);
    }
    assert_eq!(menu(&ui)[0][0], "copy queries");
    choose(&ui, 0, "copy queries");
    assert!(ui.get_menu_panes().row_count() == 0, "it closes");
    assert_eq!(copied.borrow().last(), Some(&Clip::Text("blue".into())));

    // "show files": an already-in-db file isn't new, but is all files
    ui.invoke_importer_list_menu(blue_row, 20.0, 20.0);
    choose(&ui, 0, "show files");
    choose(&ui, 1, "presenting new files");
    assert_eq!(ui.get_error(), "No presented files for that selection!");
    ui.invoke_importer_list_menu(blue_row, 20.0, 20.0);
    choose(&ui, 0, "show files");
    choose(&ui, 1, "presenting all files");
    let shown = bound.current.borrow().borrow().files().clone();
    assert_eq!(shown, [first.0]);

    // pause/play files
    ui.invoke_importer_list_menu(blue_row, 20.0, 20.0);
    choose(&ui, 0, "pause/play files");
    let paused = store
        .read(move |c| queues::queue(c, blue))
        .unwrap()
        .unwrap()
        .files_paused;
    assert!(paused);

    // the search log, from its submenu
    ui.invoke_importer_list_menu(blue_row, 20.0, 20.0);
    choose(&ui, 0, "search log");
    choose(&ui, 1, "show search log");
    let log = bound.file_log.borrow();
    assert_eq!(log.as_ref().unwrap().get_window_title(), "search log");
}

#[test]
fn a_watcher_lists_menu_copies_the_pressed_watchers_url() {
    let (_dirs, store) = store();
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(store).unwrap());
    // download, then watcher
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);
    ui.invoke_watcher_urls("https://boards.example/thread/1".into());
    ui.invoke_importer_list_menu(0, 20.0, 20.0);
    assert_eq!(
        menu(&ui)[0][..4],
        ["copy urls", "open urls", "", "copy subjects"]
    );
    choose(&ui, 0, "copy urls");
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text("https://boards.example/thread/1".into()))
    );
}
