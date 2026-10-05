//! Real asynchronous Store snapshot/reconstruction through the Help/debug action.
use hydrus_core::{
    HashId,
    pages::{Page, PageContent, PageKey, Session},
};
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless, page_chooser::NewPage};
use hydrus_store::{Store, sessions};
use slint::{ComponentHandle as _, Model as _};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let rows = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = rows.iter().position(|row| row.label == label).unwrap();
    assert!(rows.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}
fn launch(ui: &MainWindow) {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(ui, 0, "debug");
    choose(ui, 1, "gui actions");
    choose(ui, 2, "close and reload current gui session");
}
fn wait(control: &hydrus_gui::debug_session_reload::Control) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while control.pending() != 0 {
        assert!(
            Instant::now() < deadline,
            "actual reload worker did not complete"
        );
        control.poll();
        std::thread::yield_now();
    }
}
fn files(store: &Store, row: &serde_json::Value) -> Vec<HashId> {
    row["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| {
            let hash = hex::decode(hash.as_str().unwrap()).unwrap();
            store
                .read(|conn| {
                    Ok(HashId(conn.query_row(
                        "SELECT hash_id FROM hashes WHERE hash = ?",
                        [hash],
                        |row| row.get(0),
                    )?))
                })
                .unwrap()
        })
        .collect()
}
fn seed(store: &Store) -> Vec<PageKey> {
    fn pages(
        store: &Store,
        rows: &[serde_json::Value],
        media: &mut Vec<(PageKey, Vec<HashId>, Vec<HashId>)>,
    ) -> Vec<Page> {
        rows.iter()
            .map(|row| {
                let key = PageKey::random();
                let content = if let Some(children) = row["children"].as_array() {
                    PageContent::Pages(pages(store, children, media))
                } else {
                    let files = files(store, row);
                    let selected = if row["selected_hashes"].as_array().unwrap().is_empty() {
                        Vec::new()
                    } else {
                        files.clone()
                    };
                    media.push((key, files, selected));
                    PageContent::Search {
                        search: hydrus_search::FileSearchContext::default(),
                        synchronised: false,
                        sort: None,
                        lock: None,
                        collect: None,
                    }
                };
                Page {
                    key,
                    name: row["name"].as_str().unwrap().into(),
                    content,
                }
            })
            .collect()
    }
    let fixture = hydrus_testkit::fixture_json("debug_session_reload.json");
    let mut media = Vec::new();
    let tree = pages(store, fixture["before"].as_array().unwrap(), &mut media);
    let last = media.last().unwrap().0;
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: tree,
    };
    let keys = session.all_pages().iter().map(|page| page.key).collect();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&last))?;
            for (key, files, selected) in media {
                sessions::set_page_files(ctx.conn(), &key, &files)?;
                sessions::set_page_selected(ctx.conn(), &key, &selected)?;
            }
            Ok(())
        })
        .unwrap();
    keys
}
fn assert_tree(bound: &Bound, store: &Store) {
    fn compare(pages: &mut Pages, tree: &[Page], rows: &[serde_json::Value], store: &Store) {
        assert_eq!(tree.len(), rows.len());
        for (page, row) in tree.iter().zip(rows) {
            assert_eq!(page.name, row["name"].as_str().unwrap());
            match &page.content {
                PageContent::Pages(children) => {
                    compare(pages, children, row["children"].as_array().unwrap(), store)
                }
                _ => {
                    let opened = pages.page(&page.key).unwrap();
                    assert_eq!(opened.borrow().files(), files(store, row));
                    assert!(opened.borrow().selected_files().is_empty());
                }
            }
        }
    }
    let fixture = hydrus_testkit::fixture_json("debug_session_reload.json");
    let mut pages = bound.pages.borrow_mut();
    let tree = pages.session().pages.clone();
    compare(
        &mut pages,
        &tree,
        fixture["after"].as_array().unwrap(),
        store,
    );
    assert_eq!(pages.shown().name, fixture["shown"].as_str().unwrap());
    assert_eq!(pages.tabs()[0].selected, 0);
    pages.select(0, 1);
    assert_eq!(
        pages.tabs()[1].selected,
        0,
        "fresh nested notebook also selects first child"
    );
    pages.select(0, 0);
}
fn no_slots(store: &Store) {
    assert_eq!(store.read(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM session_snapshots WHERE name LIKE 'temp_session_slot_for_reload_%'", [], |row| row.get::<_, i64>(0))?)).unwrap(), 0);
}

#[test]
fn actual_menu_snapshot_restores_order_resets_selection_and_keeps_closed_originals() {
    let fixture = hydrus_testkit::fixture_json("debug_session_reload.json");
    assert!(fixture["questions"].as_array().unwrap().is_empty());
    assert_eq!(fixture["visible_at_delivery"], false);
    assert_eq!(fixture["temporary_slot_present"], false);
    assert_eq!(
        fixture["before"][1]["children"][1]["selected_hashes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (_dirs, store) = super::subscriptions::store();
    let keys = seed(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let old = bound.current.borrow().clone();
    assert_eq!(old.borrow().selected_files().len(), 2);
    let old_files = old.borrow().files();
    launch(&ui);
    assert_eq!(bound.debug_session_reload.pending(), 1);
    // Actual queued work freezes before this later edit/page creation and delivers
    // while hidden, without asking permission to close the changed current tree.
    old.borrow_mut().add_files(&[HashId(20)]);
    bound.pages.borrow_mut().new_page(&NewPage::Pages).unwrap();
    ui.hide().unwrap();
    wait(&bound.debug_session_reload);
    assert!(!ui.window().is_visible());
    assert!(ui.get_question().is_empty());
    assert_tree(&bound, &store);
    assert!(
        bound
            .pages
            .borrow()
            .session()
            .all_pages()
            .iter()
            .all(|page| !keys.contains(&page.key))
    );
    assert!(!Rc::ptr_eq(&old, &bound.current.borrow()));
    assert!(bound.pages.borrow().closed_count() >= keys.len());
    assert!(
        old.borrow().files().len() > old_files.len(),
        "old closed objects remain independent"
    );
    no_slots(&store);
    bound.pages.borrow_mut().save(2).unwrap();
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.shown().name, fixture["shown"].as_str().unwrap());
    assert!(
        reopened
            .session()
            .all_pages()
            .iter()
            .all(|page| !keys.contains(&page.key))
    );
    // A real retained native grid shows the restored file list.
    ui.show().unwrap();
    let drawn = windows.get(0).unwrap();
    headless::render(&drawn, 1000, 700);
    bound.rows.wait();
    let pixels = headless::render(&drawn, 1000, 700);
    assert!(bound.rows.cached() > 0, "restored real thumbnail decoding");
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("debug-session-reload.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
}

#[test]
fn importer_logs_restore_with_new_queue_identity_and_paused_closed_owner() {
    use hydrus_store::queues::{self, NewFileSeed, SeedType};
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().new_page(&NewPage::Urls).unwrap();
    let old_key = bound.pages.borrow().shown().key;
    let old_queue = match &bound.pages.borrow().shown().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("URL page"),
    };
    store
        .write(move |ctx| {
            queues::set_paused(ctx.conn(), old_queue, Some(true), Some(true))?;
            queues::add_file_seeds(
                ctx.conn(),
                old_queue,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: "https://reload.example/file.jpg".into(),
                    data_for_comparison: "https://reload.example/file.jpg".into(),
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta::default(),
                }],
            )?;
            Ok(())
        })
        .unwrap();
    bound.debug_session_reload.start();
    wait(&bound.debug_session_reload);
    let restored = bound
        .pages
        .borrow()
        .session()
        .all_pages()
        .into_iter()
        .find_map(|page| match &page.content {
            PageContent::Downloader { queues, .. } => Some((page.key, queues[0])),
            _ => None,
        })
        .unwrap();
    assert_ne!(restored.0, old_key);
    assert_ne!(restored.1, old_queue);
    let original = store
        .read(|conn| queues::queue(conn, old_queue))
        .unwrap()
        .unwrap();
    let current = store
        .read(|conn| queues::queue(conn, restored.1))
        .unwrap()
        .unwrap();
    assert!(original.page_closed);
    assert!(!current.page_closed);
    assert!(current.files_paused && current.gallery_paused);
    assert_eq!(
        store
            .read(|conn| queues::file_seeds(conn, restored.1))
            .unwrap()[0]
            .data,
        "https://reload.example/file.jpg"
    );
    assert!(bound.pages.borrow().closed_count() > 0);
    no_slots(&store);
    headless::render(&windows.get(0).unwrap(), 1000, 700);
}

#[test]
fn hidden_new_launch_cancelled_exit_rebind_and_shared_final_drop_ownership() {
    let (_dirs, store) = super::subscriptions::store();
    seed(&store);
    store
        .write(|ctx| {
            let mut gui: hydrus_store::settings::GuiSettings =
                hydrus_store::settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            hydrus_store::settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let control = bound.debug_session_reload.clone();
    ui.hide().unwrap();
    control.start();
    assert_eq!(control.pending(), 0);
    ui.show().unwrap();
    control.start();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    wait(&control);
    assert_eq!(bound.pages.borrow().shown().name, "reload first");
    control.start();
    let old_key = bound.pages.borrow().shown().key;
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(control.pending(), 0);
    control.poll();
    control.start();
    assert_eq!(control.pending(), 0);
    assert_eq!(bound.pages.borrow().shown().key, old_key);
    let successor_control = successor.debug_session_reload.clone();
    let clone = successor.clone();
    drop(successor);
    successor_control.start();
    assert_eq!(successor_control.pending(), 1);
    wait(&successor_control);
    successor_control.start();
    drop(clone);
    assert_eq!(successor_control.pending(), 0);
    successor_control.poll();
    successor_control.start();
    assert_eq!(successor_control.pending(), 0);
    drop(bound);
    let last = bind(&ui, Pages::open(store.clone()).unwrap());
    last.debug_session_reload.start();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    let final_key = last.pages.borrow().shown().key;
    ui.show().unwrap();
    last.debug_session_reload.poll();
    last.debug_session_reload.start();
    assert_eq!(last.debug_session_reload.pending(), 0);
    assert_eq!(last.pages.borrow().shown().key, final_key);
    let fresh = MainWindow::new().unwrap();
    fresh.show().unwrap();
    let dropped_main = bind(&fresh, Pages::open(store).unwrap());
    dropped_main.debug_session_reload.start();
    drop(fresh);
    dropped_main.debug_session_reload.poll();
    assert_eq!(dropped_main.debug_session_reload.pending(), 0);
}
