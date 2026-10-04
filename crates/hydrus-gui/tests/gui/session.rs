//! The last session's pages over the `basic` fixture: tabs for each
//! notebook on the way to the page shown, each search page as it was left
//! (its search, sort and files, not searched again), and the pages we don't
//! open yet saying so.

use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::pages::{
    DownloaderKind, Page, PageContent, PageKey, PageSort, PageSortBy, Session,
};
use hydrus_gui::{MainWindow, Pages, Tabs, bind, headless};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, SortBy, SortOrder, parse_api_search, search_files,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

/// A store imported from the fixture (whose files it uses in place, so the
/// fixture's directory is kept too).
fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

#[test]
fn the_last_session_opens_as_it_was_left() {
    let (_dirs, store) = store();
    // a search page left showing some of what its search finds, largest first
    let search = FileSearchContext {
        predicates: parse_api_search(&serde_json::json!(["system:everything"])).unwrap(),
        ..FileSearchContext::default()
    };
    let everything: Vec<HashId> = store
        .read(|conn| {
            Ok(search_files(
                conn,
                &store.snapshot(),
                &search,
                FileSort {
                    by: SortBy::FileSize,
                    order: SortOrder::Descending,
                },
                &Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    assert!(everything.len() > 5);
    let shown: Vec<HashId> = everything[..5].to_vec();
    let search_page = page(
        "my search",
        PageContent::Search {
            search: search.clone(),
            synchronised: true,
            sort: Some(PageSort {
                by: PageSortBy::System(0),
                ascending: false,
            }),
            lock: None,
            collect: None,
        },
    );
    let downloader = page(
        "threads",
        PageContent::Downloader {
            kind: DownloaderKind::Watchers,
            queues: vec![1, 2],
            sort: None,
            page: None,
        },
    );
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            page(
                "pages",
                PageContent::Pages(vec![search_page.clone(), downloader.clone()]),
            ),
            page("downloaders", PageContent::Pages(Vec::new())),
        ],
    };
    let (search_key, downloader_key) = (search_page.key, downloader.key);
    let (session_again, shown_again) = (session.clone(), shown.clone());
    let (files, other_files) = (shown.clone(), everything[5..7].to_vec());
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session, 0)?;
            sessions::set_page_files(conn, &search_key, &files)?;
            sessions::set_page_files(conn, &downloader_key, &other_files)
        })
        .unwrap();

    let mut pages = Pages::open(store.clone()).unwrap();
    // the first page of the first notebook, as the reference opens
    assert_eq!(
        pages.tabs(),
        [
            Tabs {
                names: vec!["pages".into(), "downloaders".into()],
                selected: 0
            },
            Tabs {
                names: vec!["my search".into(), "threads".into()],
                selected: 0
            },
        ]
    );
    {
        let opened = pages.current();
        let opened = opened.borrow();
        assert_eq!(opened.predicates(), ["system:everything"]);
        assert_eq!(opened.results(), shown, "as left, not searched again");
        assert_eq!(opened.file_sort().unwrap().by, SortBy::FileSize);
        assert_eq!(opened.file_sort().unwrap().order, SortOrder::Descending);
        assert!(opened.note().is_none());
    }
    // a new sort sorts the files shown, and doesn't search again
    pages
        .current()
        .borrow_mut()
        .set_sort_order(SortOrder::Ascending);
    let mut reversed = shown.clone();
    reversed.reverse();
    assert_eq!(pages.current().borrow().results(), reversed);

    pages.select(1, 1);
    let opened = pages.current();
    assert!(
        opened
            .borrow()
            .note()
            .unwrap()
            .contains("watcher downloader page")
    );
    assert_eq!(opened.borrow().results(), &everything[5..7]);
    assert!(!opened.borrow_mut().add_predicate("system:inbox"));

    pages.select(0, 1);
    assert_eq!(
        pages.tabs().len(),
        1,
        "an empty notebook has no tabs of its own"
    );
    assert!(pages.current().borrow().results().is_empty());

    // back in the notebook: the page it showed last, as a tab widget keeps
    // its tab
    pages.select(0, 0);
    assert!(pages.current().borrow().note().is_some());
    // and a page opened before is as it was left
    pages.select(1, 0);
    assert_eq!(pages.current().borrow().results(), reversed);

    // saved, and opened again: as it was left (the pages not opened too)
    pages.current().borrow_mut().add_predicate("system:inbox");
    let inbox = pages.current().borrow().results().to_vec();
    assert!(!inbox.is_empty() && inbox.len() < everything.len());
    pages.save(10).unwrap();
    let mut again = Pages::open(store.clone()).unwrap();
    assert_eq!(again.tabs(), pages.tabs());
    {
        let opened = again.current();
        let opened = opened.borrow();
        // (system:inbox entered in place of system:everything)
        assert_eq!(opened.predicates(), ["system:inbox"]);
        assert_eq!(opened.file_sort().unwrap().by, SortBy::FileSize);
        assert_eq!(opened.file_sort().unwrap().order, SortOrder::Ascending);
        assert_eq!(opened.results(), inbox);
    }
    again.select(1, 1);
    assert_eq!(again.current().borrow().results(), &everything[5..7]);
    assert!(again.current().borrow().note().is_some());
    // put back as the window below expects it
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session_again, 0)?;
            sessions::set_page_files(conn, &search_key, &shown_again)
        })
        .unwrap();

    // the window: tabs for each notebook on the way to the page shown
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let rows = ui.get_tab_rows();
    assert_eq!(rows.row_count(), 2);
    // (named as the reference names tabs: with the files shown)
    assert_eq!(
        rows.row_data(1).unwrap().names.row_data(0).unwrap(),
        "my search (5)"
    );
    assert!(ui.get_status().starts_with("5 "), "{}", ui.get_status());
    assert_eq!(ui.get_note(), "");
    ui.invoke_tab_chosen(1, 1);
    assert_eq!(ui.get_tab_rows().row_data(1).unwrap().selected, 1);
    assert!(ui.get_status().starts_with("2 "), "{}", ui.get_status());
    assert!(ui.get_note().contains("watcher"));
    assert_eq!(bound.current.borrow().borrow().results().len(), 2);
    ui.invoke_tab_chosen(1, 0);
    ui.show().unwrap();
    let main_window = windows.get(0).unwrap();
    let (width, height) = (1100, 700);
    headless::render(&main_window, width, height);
    headless::render(&main_window, width, height);
    bound.rows.wait();
    let pixels = headless::render(&main_window, width, height);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("session.png"), &pixels, width, height).unwrap();
    // the page's thumbnails were drawn
    let colours: std::collections::HashSet<&[u8]> = pixels.chunks(4).collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());
}

/// New pages go at the far right of the current notebook and are shown;
/// closing a page shows the one to its right (or left, if it was last);
/// downloader pages stay; the top notebook always has a page.
#[test]
fn pages_open_and_close_as_the_reference_does() {
    let (_dirs, store) = store();
    let search = |name: &str| {
        page(
            name,
            PageContent::Search {
                search: FileSearchContext::default(),
                synchronised: true,
                sort: None,
                lock: None,
                collect: None,
            },
        )
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            page(
                "pages",
                PageContent::Pages(vec![
                    search("b"),
                    page(
                        "threads",
                        PageContent::Downloader {
                            kind: DownloaderKind::Watchers,
                            queues: vec![],
                            sort: None,
                            page: None,
                        },
                    ),
                ]),
            ),
            search("c"),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let names =
        |pages: &Pages| -> Vec<Vec<String>> { pages.tabs().into_iter().map(|t| t.names).collect() };
    let shown = |pages: &Pages| pages.shown().name.clone();

    let mut pages = Pages::open(store.clone()).unwrap();
    pages.select(0, 1);
    assert_eq!(shown(&pages), "b");
    pages.new_search_page();
    assert_eq!(names(&pages)[1], ["b", "threads", "files"]);
    assert_eq!(shown(&pages), "files");
    let files = pages.current();
    assert!(files.borrow().results().is_empty());

    // the last page closed: the one to its left
    pages.close_shown().unwrap();
    assert_eq!(shown(&pages), "threads");
    // a watcher page closes too (its watchers wait), and ctrl+u brings it
    // back where it was
    pages.close_shown().unwrap();
    assert_eq!(names(&pages)[1], ["b"]);
    assert!(pages.unclose());
    assert_eq!(names(&pages)[1], ["b", "threads"]);
    assert_eq!(shown(&pages), "threads");
    pages.select(1, 0);
    pages.close_shown().unwrap();
    assert_eq!(names(&pages)[1], ["threads"]);
    // at the top, the one to its right
    pages.select(0, 0);
    pages.close_shown().unwrap();
    assert_eq!(names(&pages)[0], ["pages", "c"]);
    assert_eq!(shown(&pages), "threads");
    pages.select(0, 1);
    pages.close_shown().unwrap();
    assert_eq!(names(&pages)[0], ["pages"]);
    // closed pages are gone from the saved session too
    pages.save(1).unwrap();
    let again = Pages::open(store.clone()).unwrap();
    assert_eq!(
        names(&again),
        [vec!["pages".to_owned()], vec!["threads".to_owned()]]
    );
    // ctrl+u reopens them, last closed first, where they were and as they
    // were, and shows them
    assert_eq!(pages.closed_count(), 4);
    assert!(pages.unclose());
    assert_eq!(names(&pages)[0], ["pages", "c"]);
    assert_eq!(shown(&pages), "c");
    assert!(pages.unclose());
    assert_eq!(names(&pages)[0], ["a", "pages", "c"]);
    assert_eq!(shown(&pages), "a");
    assert!(pages.unclose());
    assert_eq!(names(&pages)[1], ["b", "threads"]);
    assert_eq!(shown(&pages), "b");
    assert!(pages.unclose());
    assert_eq!(names(&pages)[1], ["b", "threads", "files"]);
    assert_eq!(shown(&pages), "files");
    assert!(Rc::ptr_eq(&pages.current(), &files));
    assert!(!pages.unclose());

    // the window: ctrl+t / F9 and ctrl+w, and middle-clicking a tab
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let top = |ui: &MainWindow| -> Vec<String> {
        let row = ui.get_tab_rows().row_data(0).unwrap();
        (0..row.names.row_count())
            .map(|i| row.names.row_data(i).unwrap().to_string())
            .collect()
    };
    ui.invoke_tab_chosen(0, 0);
    // ctrl+t opens the page chooser; enter twice is "file search", then the
    // first file domain by name, "art"
    ui.invoke_new_page();
    assert_eq!(ui.get_chooser_labels().row_data(7).unwrap(), "file search");
    ui.invoke_chooser_enter();
    assert_eq!(ui.get_chooser_labels().row_data(7).unwrap(), "art");
    ui.invoke_chooser_enter();
    assert_eq!(ui.get_chooser_labels().row_count(), 0, "the chooser closed");
    assert_eq!(ui.get_tab_rows().row_data(1).unwrap().selected, 1);
    assert_eq!(bound.pages.borrow().shown().name, "files");
    ui.invoke_close_page();
    assert_eq!(bound.pages.borrow().shown().name, "threads");
    // a watcher page closes too (its watchers hold no imports here, so
    // without asking), and ctrl+u brings it back
    ui.invoke_close_page();
    assert_eq!(ui.get_question(), "");
    assert_ne!(bound.pages.borrow().shown().name, "threads");
    ui.invoke_unclose_page();
    assert_eq!(bound.pages.borrow().shown().name, "threads");
    // a tab other than the one shown closes without changing what is shown
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(8);
    ui.invoke_chooser_pressed(8);
    ui.invoke_tab_chosen(1, 0);
    ui.invoke_close_tab(1, 1);
    assert_eq!(bound.pages.borrow().shown().name, "threads");
    // (a notebook's tab decorated, as the reference's are)
    assert_eq!(top(&ui), ["pages \u{2193}"]);
    // ctrl+u brings it back, shown
    ui.invoke_unclose_page();
    assert_eq!(bound.pages.borrow().shown().name, "files");
    assert_eq!(ui.get_tab_rows().row_data(1).unwrap().selected, 1);

    // the top notebook is never without a page
    let store = bound.current.borrow().borrow().store().clone();
    let mut pages = Pages::single(hydrus_gui::SearchPage::new(store));
    let first = pages.shown().key;
    pages.close_shown().unwrap();
    assert_eq!(names(&pages), [vec!["files".to_owned()]]);
    assert_ne!(pages.shown().key, first);
}

/// A saved session (one kept from hydrus) loads from the page chooser's
/// "sessions" menu into a page of pages named after it, its pages showing
/// the files they showed; the saved session itself stays as it was.
#[test]
fn a_saved_session_appends_as_a_page_of_pages() {
    use hydrus_gui::page_chooser::{NewPage, PageChooser};

    let (_dirs, store) = store();
    let everything: Vec<HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 4")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    let search = FileSearchContext {
        predicates: parse_api_search(&serde_json::json!(["system:inbox"])).unwrap(),
        ..FileSearchContext::default()
    };
    let saved_page = page(
        "inbox things",
        PageContent::Search {
            search,
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    );
    let saved = Session {
        name: "my session".into(),
        pages: vec![saved_page.clone()],
    };
    let files = everything.clone();
    let saved_again = saved.clone();
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved_again, 0)?;
            sessions::set_page_files(ctx.conn(), &saved_page.key, &files)
        })
        .unwrap();

    let mut chooser = PageChooser::new(&store);
    // file search, download, special, and the sessions
    assert_eq!(chooser.labels()[1], "sessions");
    assert_eq!(chooser.press(2), None);
    let offered: Vec<String> = chooser
        .labels()
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect();
    assert!(offered.contains(&"my session".to_owned()), "{offered:?}");
    assert!(
        !offered.contains(&LAST_SESSION.to_owned()),
        "not the one open"
    );
    let position = chooser
        .labels()
        .iter()
        .position(|l| l == "my session")
        .unwrap();
    let choice = chooser.press(position + 1).unwrap();
    assert_eq!(choice, NewPage::Session("my session".into()));

    let mut pages = Pages::open(store.clone()).unwrap();
    pages.new_page(&choice).unwrap();
    let tabs = pages.tabs();
    assert_eq!(tabs[0].names.last().unwrap(), "my session");
    assert_eq!(tabs[1].names, ["inbox things"]);
    {
        let opened = pages.current();
        let opened = opened.borrow();
        assert_eq!(opened.predicates(), ["system:inbox"]);
        assert_eq!(opened.results(), everything, "the files it showed");
    }
    pages.save(1).unwrap();
    // twice is two copies; the saved one is untouched
    pages.new_page(&choice).unwrap();
    pages.save(2).unwrap();
    let kept = store
        .read(|conn| sessions::load(conn, "my session"))
        .unwrap()
        .unwrap();
    assert_eq!(kept, saved);
    let kept_files = store
        .read(|conn| sessions::page_files(conn, &saved.pages[0].key))
        .unwrap();
    assert_eq!(kept_files, everything);
}

#[test]
fn the_pages_are_kept_for_the_client_api_and_do_what_it_asks() {
    let (_dirs, store) = store();
    let everything = FileSearchContext {
        predicates: parse_api_search(&serde_json::json!(["system:everything"])).unwrap(),
        ..FileSearchContext::default()
    };
    let all: Vec<HashId> = store
        .read(|conn| {
            Ok(search_files(
                conn,
                &store.snapshot(),
                &everything,
                FileSort {
                    by: SortBy::ImportTime,
                    order: SortOrder::Descending,
                },
                &Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    assert!(all.len() > 6);
    let searching = |name: &str| {
        page(
            name,
            PageContent::Search {
                search: everything.clone(),
                synchronised: true,
                sort: None,
                lock: None,
                collect: None,
            },
        )
    };
    let (a, b) = (searching("a"), searching("b"));
    let nb = page("nb", PageContent::Pages(vec![b.clone()]));
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![a.clone(), nb.clone()],
    };
    let (a_files, b_files) = (all[..2].to_vec(), all[2..5].to_vec());
    let (written_a, written_b) = (a_files.clone(), b_files.clone());
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session, 0)?;
            sessions::set_page_files(conn, &a.key, &written_a)?;
            sessions::set_page_files(conn, &b.key, &written_b)?;
            // (shown when the client last closed)
            sessions::set_shown(conn, LAST_SESSION, Some(&b.key))
        })
        .unwrap();

    // it opens on the page shown last
    let pages = Pages::open(store.clone()).unwrap();
    assert_eq!(pages.shown().key, b.key);
    let _windows = headless::init();
    let window = MainWindow::new().unwrap();
    let bound = bind(&window, pages);
    let kept = |key: &PageKey| {
        let key = *key;
        store
            .read(move |conn| {
                Ok((
                    sessions::page_files(conn, &key)?,
                    sessions::page_selected(conn, &key)?,
                    sessions::shown(conn, LAST_SESSION)?,
                ))
            })
            .unwrap()
    };

    // the pages as they are: files, selection, the page shown
    window.invoke_select_all();
    (bound.sync)();
    assert_eq!(
        kept(&b.key),
        (b_files.clone(), b_files.clone(), Some(b.key))
    );
    window.invoke_select_none();
    (bound.sync)();
    assert_eq!(kept(&b.key).1, []);

    // files added to a page not opened yet: at its end, once each
    let added = vec![all[6], all[0], all[5]];
    let asked = added.clone();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::push_command(conn, &a.key, &sessions::PageCommand::AddFiles(asked))?;
            sessions::push_command(conn, &a.key, &sessions::PageCommand::Focus)
        })
        .unwrap();
    (bound.sync)();
    let a_now: Vec<HashId> = a_files.iter().copied().chain([all[6], all[5]]).collect();
    assert_eq!(kept(&a.key), (a_now.clone(), Vec::new(), Some(a.key)));
    // (focused, it is shown, with the files added)
    assert_eq!(window.get_tab_rows().row_count(), 1);
    assert_eq!(bound.current.borrow().borrow().files(), a_now);

    // refreshing a notebook searches its pages again
    let nb_key = nb.key;
    store
        .write(move |ctx| {
            sessions::push_command(ctx.conn(), &nb_key, &sessions::PageCommand::Refresh)
        })
        .unwrap();
    (bound.sync)();
    let (b_now, _, _) = kept(&b.key);
    assert_eq!(b_now.len(), all.len());
    assert_eq!(kept(&a.key).0, a_now);

    // the media viewer, while open, with the file it shows
    window.invoke_thumbnail_activated(0);
    (bound.sync)();
    let viewers = store.read(sessions::media_viewers).unwrap();
    assert_eq!(viewers.len(), 1);
    assert_eq!(viewers[0].canvas_type, 0);
    assert_eq!(viewers[0].file, Some(a_now[0]));
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    viewer.invoke_next();
    (bound.sync)();
    let moved = store.read(sessions::media_viewers).unwrap();
    assert_eq!(moved[0].file, Some(a_now[1]));
    assert_eq!(moved[0].canvas_key, viewers[0].canvas_key);
    viewer.invoke_close_requested();
    (bound.sync)();
    assert!(store.read(sessions::media_viewers).unwrap().is_empty());
}

/// A URL downloader page over its queue, which the daemon works (played
/// here by writing to the queue as it would): made from the page chooser,
/// handing typed URLs over, showing the queue's status and the files it
/// brings as they come, pausing, and closing as the reference asks.
#[test]
fn a_url_downloader_page_shows_and_controls_its_queue() {
    use hydrus_store::live::{self, JobKind, JobLive, QueueLive};
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

    let (_dirs, store) = store();
    let files: Vec<(HashId, hydrus_core::Sha256)> = store
        .read(|conn| {
            let ids: Vec<HashId> = conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 3")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let hashes = hydrus_store::master::hashes(conn, &ids)?;
            Ok(ids.iter().map(|id| (*id, hashes[id])).collect())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then urls
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    assert_eq!(bound.pages.borrow().shown().name, "url import");
    assert!(ui.get_importing());
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let key = bound.pages.borrow().shown().key;
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert_eq!(made.page_key.as_deref(), Some(&key.0[..]));
    assert_eq!(ui.get_import_status(), "");
    assert_eq!(ui.get_import_progress(), "");

    // typed URLs are handed to the daemon, trimmed
    ui.invoke_url_entered("  https://site.example/post/1 \u{feff}".into());
    ui.invoke_url_entered("   ".into());
    let typed = store
        .write(move |ctx| queues::take_url_requests(ctx.conn(), queue))
        .unwrap();
    assert_eq!(typed, ["https://site.example/post/1"]);

    // the daemon at work: a new file, one already in the database, a
    // failure and one to go
    let seed = |n: usize| NewFileSeed {
        seed_type: SeedType::Url,
        data: format!("https://site.example/post/{n}"),
        data_for_comparison: format!("https://site.example/post/{n}"),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    let (first, second) = (files[0].1, files[1].1);
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            queues::add_file_seeds(conn, queue, &[seed(1), seed(2), seed(3), seed(4)], false, 0)?;
            let mut seeds = queues::file_seeds(conn, queue)?;
            for (seed, (status, hash)) in seeds.iter_mut().zip([
                (SeedStatus::SuccessfulAndNew, Some(second)),
                (SeedStatus::SuccessfulButRedundant, Some(first)),
                (SeedStatus::Error, None),
            ]) {
                seed.status = status;
                if let Some(hash) = hash {
                    seed.meta.set_hash("sha256", hash.to_hex());
                }
                queues::update_file_seed(conn, seed)?;
            }
            Ok(())
        })
        .unwrap();
    (bound.sync)();
    assert_eq!(
        ui.get_import_status(),
        "2 successful (1 already in db), 1 failed"
    );
    assert_eq!(ui.get_import_progress(), "3/4");
    assert!((ui.get_import_fraction() - 0.75).abs() < 1e-6);
    // (its files, in the order they came, and its tab's progress)
    assert_eq!(
        bound.current.borrow().borrow().files(),
        [files[1].0, files[0].0]
    );
    let tab = |ui: &MainWindow| -> String {
        let row = ui.get_tab_rows().row_data(0).unwrap();
        let shown = usize::try_from(row.selected).unwrap();
        row.names.row_data(shown).unwrap().to_string()
    };
    assert_eq!(tab(&ui), "url import (2 - 3/4)");

    // what the daemon is downloading: its line under the file log, which
    // the cancel button asks the daemon to stop
    assert_eq!(ui.get_file_download().left, "");
    assert!(!ui.get_file_download().can_cancel);
    let downloading = QueueLive {
        file_job: Some(JobLive {
            url: String::new(),
            status: "downloading\u{2026}".into(),
            speed: 1536,
            bytes_read: 1_048_576,
            bytes_to_read: Some(5_242_880),
            done: false,
            error: false,
        }),
        ..QueueLive::default()
    };
    store
        .write(move |ctx| live::publish(ctx.conn(), &[(queue, Some(downloading))]))
        .unwrap();
    (bound.sync)();
    let line = ui.get_file_download();
    assert_eq!(line.left, "downloading\u{2026}");
    assert_eq!(line.right, "1 MB/5 MB 1.5 KB/s");
    assert!((line.fraction - 0.2).abs() < 1e-6);
    assert!(line.can_cancel);
    assert_eq!(ui.get_search_download().left, "", "no gallery page");
    ui.invoke_cancel_download(false);
    assert_eq!(
        store.write(|ctx| live::take_cancels(ctx.conn())).unwrap(),
        [(queue, JobKind::File)]
    );
    // (the daemon stopping: nothing downloading)
    store.write(|ctx| live::clear(ctx.conn())).unwrap();
    (bound.sync)();
    assert_eq!(ui.get_file_download().right, "");
    assert!(!ui.get_file_download().can_cancel);

    // pausing pauses its files and search, nudging the daemon
    store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
    ui.invoke_pause_play_files();
    assert!(ui.get_import_paused());
    let paused = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert!(paused.files_paused && paused.gallery_paused);
    assert_eq!(
        store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap(),
        [queue]
    );

    // closing it asks, as it holds imports; closed, its queue waits
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"url import\"?\n\nThis is a urls import page holding 4 import objects."
    );
    ui.invoke_answer(true);
    assert_ne!(bound.pages.borrow().shown().key, key);
    let page_closed = |store: &Arc<Store>| {
        store
            .read(move |c| Ok(queues::queue(c, queue)?.map(|q| q.page_closed)))
            .unwrap()
    };
    assert_eq!(page_closed(&store), Some(true));
    // reopened, it runs again
    ui.invoke_unclose_page();
    assert_eq!(bound.pages.borrow().shown().key, key);
    assert_eq!(page_closed(&store), Some(false));
    // resumed, with work left, it is still importing
    ui.invoke_pause_play_files();
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"url import\"?\n\nThis page is still importing."
    );
    ui.invoke_answer(false);
    assert_eq!(bound.pages.borrow().shown().key, key);
    // closed for good as the client closes: the queue goes
    ui.invoke_close_page();
    ui.invoke_answer(true);
    bound.pages.borrow_mut().forget_closed();
    assert_eq!(page_closed(&store), None);
}

/// Pages closed when the client last closed (or crashed) don't come back,
/// nor do their downloads.
#[test]
fn queues_of_pages_left_closed_go_when_the_client_opens() {
    use hydrus_store::queues::{self, QueueKind};

    let (_dirs, store) = store();
    let queue = store
        .write(|ctx| {
            let conn = ctx.conn();
            let options = hydrus_core::import_options::ImportOptionsSlice::default();
            let queue =
                queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
            queues::set_page_closed(conn, queue, true)?;
            Ok(queue)
        })
        .unwrap();
    Pages::open(store.clone()).unwrap();
    assert!(
        store
            .read(move |c| queues::queue(c, queue))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_gallery_downloader_page_shows_and_controls_its_searches() {
    use hydrus_core::url::{AnyGug, Gug, Gugs};
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

    let (_dirs, store) = store();
    // one downloader, the client's default, and another
    let gug = |name: &str, key: &str| {
        AnyGug::Single(Gug {
            name: name.into(),
            key: key.into(),
            url_template: format!("https://{key}.example/search?tags=%tags%"),
            replacement_phrase: "%tags%".into(),
            separator: "+".into(),
            initial_search_text: "enter tags".into(),
            example_search_text: String::new(),
        })
    };
    let downloaders = hydrus_parse::Downloaders {
        gugs: Gugs {
            gugs: vec![gug("site tag search", "aa"), gug("other search", "bb")],
            keys_to_display: vec!["aa".into(), "bb".into()],
        },
        ..hydrus_parse::Downloaders::default()
    };
    let defaults = hydrus_core::subscriptions::GalleryDefaults {
        file_limit: Some(100),
        gug: Some(("aa".into(), "site tag search".into())),
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    let files: Vec<(HashId, hydrus_core::Sha256)> = store
        .read(|conn| {
            let ids: Vec<HashId> = conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 3")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let hashes = hydrus_store::master::hashes(conn, &ids)?;
            Ok(ids.iter().map(|id| (*id, hashes[id])).collect())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // download, then gallery: a page with the client's default downloader
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    assert_eq!(ui.get_chooser_labels().row_data(5).unwrap(), "gallery");
    ui.invoke_chooser_pressed(6);
    assert_eq!(bound.pages.borrow().shown().name, "gallery");
    assert!(ui.get_gallery_page());
    let data = ui.get_gallery_data();
    assert_eq!(data.top_status, "waiting for new queries");
    assert_eq!(
        data.gug_names
            .row_data(usize::try_from(data.gug_index).unwrap())
            .unwrap(),
        "site tag search"
    );
    assert_eq!(data.initial_search_text, "enter tags");
    assert_eq!(data.file_limit, 100);
    assert!(!data.no_limit && !data.highlighted);
    assert_eq!(bound.current.borrow().borrow().files(), []);

    // queries entered become searches, the first shown
    ui.invoke_gallery_queries("red eyes\n blue \n\n".into());
    let data = ui.get_gallery_data();
    assert_eq!(data.top_status, "2 queries - 0/0");
    assert!(data.highlighted);
    assert_eq!(data.highlighted_query, "red eyes");
    let row = |ui: &MainWindow, row: usize| -> Vec<String> {
        let cells = ui.get_gallery_rows().row_data(row).unwrap().cells;
        (0..cells.row_count())
            .map(|i| cells.row_data(i).unwrap().to_string())
            .collect()
    };
    // (sorted by query, the shown one starred)
    assert_eq!(row(&ui, 0)[..2], ["blue", "site tag search"]);
    assert_eq!(row(&ui, 1)[0], "* red eyes");
    assert_eq!(row(&ui, 1)[4], "pending");
    let key = bound.pages.borrow().shown().key;
    let made = store
        .read(move |c| queues::queues_with_page_key(c, &key.0))
        .unwrap();
    assert_eq!(made.len(), 2);
    for queue in &made {
        let search: hydrus_core::gallery::GallerySearch =
            serde_json::from_value(queue.extra.clone()).unwrap();
        assert_eq!(search.file_limit, Some(100));
        assert_eq!(queue.name, "gallery");
    }
    let (red, blue) = (made[0].id, made[1].id);

    // the daemon at work: the shown search's files join the page, the
    // other's don't
    let seed = |n: usize| NewFileSeed {
        seed_type: SeedType::Url,
        data: format!("https://aa.example/post/{n}"),
        data_for_comparison: format!("https://aa.example/post/{n}"),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    let (first, second) = (files[0].1, files[1].1);
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            for (queue, n, hash, status) in [
                (red, 1, Some(first), SeedStatus::SuccessfulAndNew),
                (blue, 2, Some(second), SeedStatus::SuccessfulAndNew),
                (blue, 3, None, SeedStatus::Vetoed),
            ] {
                queues::add_file_seeds(conn, queue, &[seed(n)], false, 0)?;
                let mut seeds = queues::file_seeds(conn, queue)?;
                let seed = seeds.last_mut().unwrap();
                seed.status = status;
                if let Some(hash) = hash {
                    seed.meta.set_hash("sha256", hash.to_hex());
                }
                queues::update_file_seed(conn, seed)?;
            }
            Ok(())
        })
        .unwrap();
    (bound.sync)();
    assert_eq!(bound.current.borrow().borrow().files(), [files[0].0]);
    assert_eq!(ui.get_gallery_data().top_status, "2 queries - 3/3");
    // (its files done, its search's first page not yet read)
    assert_eq!(row(&ui, 0)[2..6], ["\u{23F9}", "", "pending", "2 - 1Ign"]);

    // highlighting another search shows it, its files replacing the page's
    // (the list sorted by query, as the reference's at first)
    assert_eq!(
        (ui.get_gallery_sort_column(), ui.get_gallery_ascending()),
        (0, true)
    );
    assert!(!ui.get_gallery_data().has_selection);
    ui.invoke_gallery_row_clicked(0, false, false);
    assert!(ui.get_gallery_rows().row_data(0).unwrap().selected);
    assert!(ui.get_gallery_data().has_selection);
    assert!(ui.get_gallery_data().can_highlight);
    assert!(ui.get_gallery_data().can_retry_ignored);
    ui.invoke_gallery_highlight();
    let data = ui.get_gallery_data();
    assert_eq!(data.highlighted_query, "blue");
    assert_eq!(row(&ui, 0)[0], "* blue");
    assert_eq!(bound.current.borrow().borrow().files(), [files[1].0]);

    // retrying its ignored file, and pausing its files, from the list
    store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
    ui.invoke_gallery_retry(true);
    let counts = store
        .read(move |c| queues::file_seed_counts(c, blue))
        .unwrap();
    assert_eq!(counts.get(&SeedStatus::Unknown), Some(&1));
    ui.invoke_gallery_pause_play(false, false);
    assert_eq!(row(&ui, 0)[2], "\u{23F8}");
    assert!(
        store
            .read(move |c| queues::queue(c, blue))
            .unwrap()
            .unwrap()
            .files_paused
    );
    assert_eq!(ui.get_gallery_data().files_line, "paused");
    assert_eq!(
        store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap(),
        [blue]
    );

    // another downloader and no file limit for new queries; kept with the
    // page, as is the search shown
    // (offered by name)
    assert_eq!(
        ui.get_gallery_data().gug_names.row_data(0).unwrap(),
        "other search"
    );
    ui.invoke_gallery_gug(0);
    ui.invoke_gallery_limit(true, 0);
    let data = ui.get_gallery_data();
    assert_eq!(
        data.gug_names
            .row_data(usize::try_from(data.gug_index).unwrap())
            .unwrap(),
        "other search"
    );
    assert!(data.no_limit);
    ui.invoke_gallery_queries("green".into());
    let green = store
        .read(move |c| queues::queues_with_page_key(c, &key.0))
        .unwrap()
        .into_iter()
        .find(|q| q.id != red && q.id != blue)
        .unwrap();
    let search: hydrus_core::gallery::GallerySearch =
        serde_json::from_value(green.extra.clone()).unwrap();
    assert_eq!(
        (search.source_name.as_str(), search.file_limit),
        ("other search", None)
    );
    bound.pages.borrow_mut().sync(5).unwrap();
    let saved = store
        .read(|c| sessions::load(c, LAST_SESSION))
        .unwrap()
        .unwrap();
    let kept = saved
        .all_pages()
        .into_iter()
        .find(|p| p.key == key)
        .unwrap()
        .clone();
    let PageContent::Downloader {
        queues: kept_queues,
        page: Some(state),
        ..
    } = kept.content
    else {
        panic!("{kept:?}");
    };
    assert_eq!(kept_queues, [red, blue, green.id]);
    assert_eq!(state.highlighted, Some(blue));
    let own = state.gallery.unwrap();
    assert_eq!(
        (own.gug_name.as_str(), own.file_limit),
        ("other search", None)
    );

    // removing the shown search asks, then clears the page
    ui.invoke_gallery_remove();
    assert_eq!(
        ui.get_question(),
        "Remove the 1 selected queries?\n\nThe currently highlighted query will be removed, \
         and the media panel cleared."
    );
    ui.invoke_answer(true);
    assert_eq!(ui.get_gallery_data().top_status, "2 queries - 1/1");
    assert!(!ui.get_gallery_data().highlighted);
    assert_eq!(bound.current.borrow().borrow().files(), []);
    assert!(
        store
            .read(move |c| queues::queue(c, blue))
            .unwrap()
            .is_none()
    );

    // closing asks while a search still has files to get, else when it
    // holds any; closed, its searches wait
    store
        .write(move |ctx| {
            queues::add_file_seeds(ctx.conn(), red, &[seed(4)], false, 0)?;
            Ok(())
        })
        .unwrap();
    (bound.sync)();
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"gallery\"?\n\n1 queries are still importing."
    );
    ui.invoke_answer(false);
    ui.invoke_gallery_row_clicked(1, false, false);
    ui.invoke_gallery_pause_play(false, false);
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"gallery\"?\n\nThis is a gallery downloader page holding 2 import objects."
    );
    ui.invoke_answer(true);
    assert_ne!(bound.pages.borrow().shown().key, key);
    for queue in [red, green.id] {
        assert!(
            store
                .read(move |c| queues::queue(c, queue))
                .unwrap()
                .unwrap()
                .page_closed
        );
    }
}

#[test]
fn a_watcher_downloader_page_shows_and_controls_its_watchers() {
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedStatus, SeedType};
    use hydrus_store::watchers::watcher_state;

    let (_dirs, store) = store();
    let files: Vec<(HashId, hydrus_core::Sha256)> = store
        .read(|conn| {
            let ids: Vec<HashId> = conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let hashes = hydrus_store::master::hashes(conn, &ids)?;
            Ok(ids.iter().map(|id| (*id, hashes[id])).collect())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // download, then watcher: an empty watcher page
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);
    assert_eq!(bound.pages.borrow().shown().name, "watcher");
    assert!(ui.get_watcher_page() && !ui.get_gallery_page());
    let data = ui.get_watcher_data();
    assert_eq!(data.top_status, "waiting for new watchers");
    assert!(!data.highlighted);
    assert_eq!(ui.get_status(), "no highlighted watcher");

    // threads entered (a line that isn't one dropped) become watchers with
    // the client's checker options, the first shown
    let (one, two) = (
        "https://boards.example/thread/1",
        "https://boards.example/thread/2",
    );
    ui.invoke_watcher_urls(format!("{one}\n not a url \n{two}\n").into());
    let key = bound.pages.borrow().shown().key;
    let made = store
        .read(move |c| queues::queues_with_page_key(c, &key.0))
        .unwrap();
    assert_eq!(made.len(), 2);
    let checkers: hydrus_core::subscriptions::CheckerDefaults =
        store.read(hydrus_store::settings::get).unwrap();
    for queue in &made {
        assert_eq!(
            (queue.kind, queue.name.as_str()),
            (QueueKind::Watcher, "watcher")
        );
        assert_eq!(watcher_state(queue).unwrap().checker, checkers.watchers);
    }
    let (first, second) = (made[0].id, made[1].id);
    assert_eq!(watcher_state(&made[0]).unwrap().url, one);
    let row = |ui: &MainWindow, row: usize| -> Vec<String> {
        let cells = ui.get_watcher_rows().row_data(row).unwrap().cells;
        (0..cells.row_count())
            .map(|i| cells.row_data(i).unwrap().to_string())
            .collect()
    };
    let data = ui.get_watcher_data();
    assert_eq!(data.top_status, "2 watchers - 0/0");
    assert!(data.highlighted);
    assert_eq!(
        (data.subject.as_str(), data.url.as_str()),
        ("no subject", one)
    );
    assert_eq!(row(&ui, 0)[0], "* unknown subject");
    assert_eq!(row(&ui, 0)[3], "just added");
    // a thread the page watches already is not watched again (it says so,
    // once it no longer says it was just added, as in the reference)
    ui.invoke_watcher_urls(one.into());
    assert_eq!(
        store
            .read(move |c| queues::queues_with_page_key(c, &key.0))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(row(&ui, 0)[3], "just added");

    // the daemon at work: the thread's title, and the shown watcher's
    // files join the page
    let file = files[0].1;
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let mut state = watcher_state(&queues::queue(conn, first)?.unwrap()).unwrap();
            "a thread".clone_into(&mut state.subject);
            queues::set_queue_extra(conn, first, &serde_json::to_value(&state).unwrap())?;
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://boards.example/file/1".into(),
                data_for_comparison: "https://boards.example/file/1".into(),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, first, &[seed], false, 0)?;
            let mut seeds = queues::file_seeds(conn, first)?;
            let seed = seeds.last_mut().unwrap();
            seed.status = SeedStatus::SuccessfulAndNew;
            seed.meta.set_hash("sha256", file.to_hex());
            queues::update_file_seed(conn, seed)
        })
        .unwrap();
    (bound.sync)();
    assert_eq!(bound.current.borrow().borrow().files(), [files[0].0]);
    let data = ui.get_watcher_data();
    assert_eq!(data.top_status, "2 watchers - 1/1");
    assert_eq!(data.subject, "a thread");
    assert!(
        data.velocity_line
            .starts_with("at last check, found 1 files"),
        "{}",
        data.velocity_line
    );

    // pausing the shown watcher's checking, from its box, then checking it
    // now (which resumes it), each nudging the daemon
    store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
    ui.invoke_watcher_pause_play(true, true);
    let state = |queue: i64| {
        watcher_state(
            &store
                .read(move |c| queues::queue(c, queue))
                .unwrap()
                .unwrap(),
        )
        .unwrap()
    };
    assert!(state(first).checking_paused);
    assert!(ui.get_watcher_data().checking_paused);
    assert_eq!(ui.get_watcher_data().checker_line, "paused");
    ui.invoke_watcher_check_now(true);
    assert!(state(first).check_now && !state(first).checking_paused);
    assert!(!ui.get_watcher_data().can_check_now);
    assert_eq!(
        store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap(),
        [first]
    );

    // the other's files paused, from the list
    let other = (0..2).find(|&r| !row(&ui, r)[0].starts_with('*')).unwrap();
    ui.invoke_watcher_row_clicked(i32::try_from(other).unwrap(), false, false);
    ui.invoke_watcher_pause_play(false, false);
    assert!(
        store
            .read(move |c| queues::queue(c, second))
            .unwrap()
            .unwrap()
            .files_paused
    );
    assert_eq!(row(&ui, other)[1], "\u{23F8}");
    // a failed file on it: it can be retried, and highlighted
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://boards.example/file/2".into(),
                data_for_comparison: "https://boards.example/file/2".into(),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, second, &[seed], false, 0)?;
            let mut seeds = queues::file_seeds(conn, second)?;
            let seed = seeds.last_mut().unwrap();
            seed.status = SeedStatus::Error;
            queues::update_file_seed(conn, seed)
        })
        .unwrap();
    (bound.sync)();
    let data = ui.get_watcher_data();
    assert!(data.can_retry_failed && !data.can_retry_ignored);
    assert!(data.can_highlight);
    // (sorted by status at first, then as clicked)
    assert_eq!(
        (ui.get_watcher_sort_column(), ui.get_watcher_ascending()),
        (3, true)
    );
    ui.invoke_watcher_sort(0, false);
    assert_eq!(
        (ui.get_watcher_sort_column(), ui.get_watcher_ascending()),
        (0, false)
    );
    let other = (0..2).find(|&r| !row(&ui, r)[0].starts_with('*')).unwrap();
    let other_row = i32::try_from(other).unwrap();

    // kept with the page: its watchers, the one shown, its checker
    bound.pages.borrow_mut().sync(5).unwrap();
    let saved = store
        .read(|c| sessions::load(c, LAST_SESSION))
        .unwrap()
        .unwrap();
    let kept = saved
        .all_pages()
        .into_iter()
        .find(|p| p.key == key)
        .unwrap()
        .clone();
    let PageContent::Downloader {
        queues: kept_queues,
        page: Some(kept_state),
        ..
    } = kept.content
    else {
        panic!("{kept:?}");
    };
    assert_eq!(kept_queues, [first, second]);
    assert_eq!(kept_state.highlighted, Some(first));
    assert_eq!(kept_state.checker, Some(checkers.watchers.clone()));

    // the shown one selected too (ctrl+click): the list's buttons act on
    // both, neither can be highlighted, and removing asks of both
    ui.invoke_watcher_row_clicked(1 - other_row, true, false);
    let data = ui.get_watcher_data();
    assert!(data.has_selection && !data.can_highlight);
    assert!((0..2).all(|r| ui.get_watcher_rows().row_data(r).unwrap().selected));
    let checking = |queue| state(queue).checking_paused;
    let before = (checking(first), checking(second));
    ui.invoke_watcher_pause_play(true, false);
    assert_eq!((checking(first), checking(second)), (!before.0, !before.1));
    ui.invoke_watcher_pause_play(true, false);
    assert_eq!((checking(first), checking(second)), before);
    ui.invoke_watcher_remove();
    assert_eq!(
        ui.get_question(),
        "Remove the 2 selected watchers?\n\n2 are not yet DEAD.\n\nThe currently highlighted \
         watcher will be removed, and the media panel cleared."
    );
    ui.invoke_answer(false);
    // (ctrl+click again takes it away)
    ui.invoke_watcher_row_clicked(1 - other_row, true, false);
    let usize_row = usize::try_from(1 - other_row).unwrap();
    assert!(!ui.get_watcher_rows().row_data(usize_row).unwrap().selected);

    // removing the selected (not shown) watcher asks, as the reference does
    ui.invoke_watcher_remove();
    assert_eq!(
        ui.get_question(),
        "Remove the 1 selected watchers?\n\n1 are not yet DEAD."
    );
    ui.invoke_answer(true);
    assert!(
        store
            .read(move |c| queues::queue(c, second))
            .unwrap()
            .is_none()
    );
    assert_eq!(ui.get_watcher_data().top_status, "1 watchers - 1/1");

    // closing asks while it holds anything; closed, its watcher waits
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"watcher\"?\n\nThis is a watcher page holding 1 import objects."
    );
    ui.invoke_answer(true);
    assert_ne!(bound.pages.borrow().shown().key, key);
    assert!(
        store
            .read(move |c| queues::queue(c, first))
            .unwrap()
            .unwrap()
            .page_closed
    );
}

#[test]
fn a_double_click_on_a_tab_rows_empty_space_chooses_a_page_for_it() {
    let (_dirs, store) = store();
    let search = |name: &str| {
        page(
            name,
            PageContent::Search {
                search: FileSearchContext::default(),
                synchronised: true,
                sort: None,
                lock: None,
                collect: None,
            },
        )
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            page("pages", PageContent::Pages(vec![search("b")])),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let row = |level: usize| -> Vec<String> {
        let row = ui.get_tab_rows().row_data(level).unwrap();
        (0..row.names.row_count())
            .map(|i| row.names.row_data(i).unwrap().to_string())
            .collect()
    };
    // (a page shown in the "pages" notebook, under the top one)
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(ui.get_tab_rows().row_count(), 2);
    let (top, inner) = (row(0), row(1));
    let chooser_open = || ui.get_chooser_labels().row_count() > 0;

    // one press is nothing; two on the top row's space: the chooser, and
    // the page chosen goes in the top notebook
    ui.invoke_tab_space_pressed(0, false);
    assert!(!chooser_open());
    ui.invoke_tab_space_pressed(0, false);
    assert!(chooser_open());
    ui.invoke_chooser_enter();
    ui.invoke_chooser_enter();
    assert!(!chooser_open());
    let mut expected = top.clone();
    expected.push("files".into());
    assert_eq!(row(0), expected);
    assert_eq!(bound.pages.borrow().shown().name, "files");

    // a middle double click on the inner row's space: a page in it
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_space_pressed(1, true);
    ui.invoke_tab_space_pressed(1, true);
    assert!(chooser_open());
    ui.invoke_chooser_enter();
    ui.invoke_chooser_enter();
    let mut expected = inner.clone();
    expected.push("files".into());
    assert_eq!(row(1), expected);
    // (a press of each button doesn't make a double click)
    ui.invoke_tab_space_pressed(1, false);
    ui.invoke_tab_space_pressed(1, true);
    assert!(!chooser_open());

    // a chooser dismissed leaves new pages where they go by default
    // (the presses made by the pointer, on the top row's empty space)
    ui.show().unwrap();
    headless::render(&windows.get(0).unwrap(), 1100, 700);
    let press = |x: f32, y: f32| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let position = slint::LogicalPosition::new(x, y);
        for event in [
            WindowEvent::PointerMoved { position },
            WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            },
        ] {
            ui.window().dispatch_event(event);
        }
    };
    // (under the menu bar)
    press(600.0, 22.0 + 14.0);
    press(600.0, 22.0 + 14.0);
    assert!(chooser_open(), "the top row's empty space double-clicked");
    ui.invoke_chooser_cancel();
    ui.invoke_new_page();
    ui.invoke_chooser_enter();
    ui.invoke_chooser_enter();
    expected.push("files".into());
    assert_eq!(row(1), expected);
}

/// A local import page (the reference's "import" page) over its import,
/// which the daemon works (played here by writing to the queue as it
/// would): made with its files, named "import", its "imports" box showing
/// what the import is doing, its file log's status and progress and the
/// files it brings as they come, pausing, and closing as the reference
/// asks.
#[test]
fn a_local_import_page_shows_and_controls_its_import() {
    use hydrus_gui::page_chooser::NewPage;
    use hydrus_store::live::{self, QueueLive};
    use hydrus_store::queues::{self, LocalImport, QueueKind, SeedStatus};

    let (_dirs, store) = store();
    let files: Vec<(HashId, hydrus_core::Sha256)> = store
        .read(|conn| {
            let ids: Vec<HashId> = conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let hashes = hydrus_store::master::hashes(conn, &ids)?;
            Ok(ids.iter().map(|id| (*id, hashes[id])).collect())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
    let paths = vec![
        ("/imports/a.png".to_owned(), Some(1_600_000_000)),
        ("/imports/b.jpg".to_owned(), Some(1_600_000_001)),
        ("/imports/c.gif".to_owned(), None),
    ];
    // (tags for one of them, as the "filename tagging" dialog gives)
    let mut tags = queues::PathTags::new();
    tags.insert(
        "/imports/b.jpg".to_owned(),
        vec![("6d79".to_owned(), ["blue".to_owned()].into_iter().collect())],
    );
    (bound.open_page)(&NewPage::LocalImport {
        paths: paths.clone(),
        tags,
        routers: Vec::new(),
        delete_after_success: true,
    });
    assert_eq!(bound.pages.borrow().shown().name, "import");
    assert!(ui.get_importing() && ui.get_local_import());
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let key = bound.pages.borrow().shown().key;
    // its import, for the daemon: its files in order, and deleting them
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert_eq!(made.kind, QueueKind::LocalImport);
    assert_eq!(made.page_key.as_deref(), Some(&key.0[..]));
    assert_eq!(
        LocalImport::of(&made),
        Some(LocalImport {
            delete_after_success: true,
            routers: Vec::new(),
        })
    );
    let made_seeds = store.read(move |c| queues::file_seeds(c, queue)).unwrap();
    assert_eq!(
        made_seeds[1].meta.external_additional_tags,
        [("6d79".to_owned(), ["blue".to_owned()].into_iter().collect())]
    );
    assert!(made_seeds[0].meta.external_additional_tags.is_empty());
    let seeds: Vec<(String, Option<i64>)> = made_seeds
        .into_iter()
        .map(|s| (s.data, s.source_time))
        .collect();
    assert_eq!(seeds, paths);
    assert_eq!(
        store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap(),
        [queue]
    );
    assert_eq!(ui.get_import_status(), "");
    assert_eq!(ui.get_import_progress(), "0/3");

    // the daemon at work: a new file, one already in the database, one to
    // go, and what it is doing
    let (first, second) = (files[0].1, files[1].1);
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let mut seeds = queues::file_seeds(conn, queue)?;
            for (seed, (status, hash)) in seeds.iter_mut().zip([
                (SeedStatus::SuccessfulAndNew, second),
                (SeedStatus::SuccessfulButRedundant, first),
            ]) {
                seed.status = status;
                seed.meta.set_hash("sha256", hash.to_hex());
                queues::update_file_seed(conn, seed)?;
            }
            let importing = QueueLive {
                files_status: "importing".into(),
                ..QueueLive::default()
            };
            live::publish(conn, &[(queue, Some(importing))])
        })
        .unwrap();
    (bound.sync)();
    assert_eq!(ui.get_import_action(), "importing");
    assert_eq!(ui.get_import_status(), "2 successful (1 already in db)");
    assert_eq!(ui.get_import_progress(), "2/3");
    assert_eq!(
        bound.current.borrow().borrow().files(),
        [files[1].0, files[0].0]
    );
    let row = ui.get_tab_rows().row_data(0).unwrap();
    let shown = usize::try_from(row.selected).unwrap();
    assert_eq!(row.names.row_data(shown).unwrap(), "import (2 - 2/3)");

    // pausing pauses it, nudging the daemon
    ui.invoke_pause_play_files();
    assert!(ui.get_import_paused());
    let paused = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert!(paused.files_paused);
    assert_eq!(
        store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap(),
        [queue]
    );
    // closing asks, as it holds imports
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"import\"?\n\nThis is a local import page holding 3 import objects."
    );
    ui.invoke_answer(false);
    // resumed, with work left, it is still importing
    ui.invoke_pause_play_files();
    ui.invoke_close_page();
    assert_eq!(
        ui.get_question(),
        "Close \"import\"?\n\nThis page is still importing."
    );
    ui.invoke_answer(false);
    assert_eq!(bound.pages.borrow().shown().key, key);
}

#[test]
fn tab_reordering_keeps_nested_notebooks_and_shown_leaf() {
    use hydrus_gui::tab_context::{Move, Sort};
    let (_dirs, store) = store();
    let search = || PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    };
    let left = page("same", search());
    let right = page("same", search());
    let nested = page(
        "z nested",
        PageContent::Pages(vec![left.clone(), right.clone()]),
    );
    let other = page("a", search());
    let saved = Session {
        name: LAST_SESSION.into(),
        pages: vec![nested.clone(), other.clone()],
    };
    let (left_key, right_key) = (left.key, right.key);
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 100)?;
            sessions::set_page_files(ctx.conn(), &left_key, &[HashId(1)])?;
            sessions::set_page_files(ctx.conn(), &right_key, &[HashId(2), HashId(3)])
        })
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.select(1, 1);
    assert_eq!(pages.shown().key, right.key);
    pages.sort_tabs(0, Sort::Name, true).unwrap();
    assert_eq!(pages.session().pages[0].key, other.key);
    assert_eq!(pages.shown().key, right.key);
    pages.sort_tabs(1, Sort::Name, false).unwrap();
    assert_eq!(pages.tabs()[1].selected, 0); // equal names: most files first
    assert_eq!(pages.shown().key, right.key);
    pages.move_tab(0, 1, Move::First);
    assert_eq!(pages.session().pages[0].key, nested.key);
    assert_eq!(pages.shown().key, right.key);
    pages.move_tab(1, 1, Move::First); // move the unselected sibling
    assert_eq!(pages.shown().key, right.key);
    assert_eq!(pages.tabs()[1].selected, 1);
    pages.sync(200).unwrap();
    let reopened = Pages::open(store).unwrap();
    assert_eq!(reopened.shown().key, right.key);
    assert_eq!(reopened.session().pages, pages.session().pages);
}

#[test]
fn historical_session_append_restores_independent_tree_files_and_selection() {
    let (_dirs, store) = store();
    let leaf = page(
        "original",
        PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    );
    let notebook = page("nested", PageContent::Pages(vec![leaf.clone()]));
    let saved = Session {
        name: LAST_SESSION.into(),
        pages: vec![notebook],
    };
    let leaf_key = leaf.key;
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 1)?;
            sessions::set_page_files(ctx.conn(), &leaf_key, &[HashId(1), HashId(2)])?;
            sessions::set_page_selected(ctx.conn(), &leaf_key, &[HashId(2)])
        })
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    assert_eq!(pages.current().borrow().selected_files(), [HashId(2)]);
    pages.save_session("work", 100).unwrap();
    pages.rename_shown("changed");
    pages.current().borrow_mut().select_none();
    pages.save_session("work", 200).unwrap();
    let snapshot = store
        .read(|conn| hydrus_store::session_backups::load(conn, "work", 100_000))
        .unwrap()
        .unwrap();
    let old = snapshot.session.all_pages().last().unwrap().key;
    assert!(
        store
            .read(|conn| sessions::page_files(conn, &old))
            .unwrap()
            .is_empty()
    );
    pages.append_session_backup("work", 100_000).unwrap();
    assert_eq!(pages.session().pages.len(), 2); // append at top, even while nested
    assert_eq!(pages.session().pages[1].name, "work");
    assert_eq!(pages.shown().name, "original");
    let restored = pages.shown().key;
    assert_ne!(restored, old);
    assert_ne!(restored, leaf.key);
    assert_eq!(pages.current().borrow().files(), [HashId(1), HashId(2)]);
    assert_eq!(pages.current().borrow().selected_files(), [HashId(2)]);
    store
        .write(|ctx| sessions::delete(ctx.conn(), "work"))
        .unwrap();
    assert_eq!(pages.current().borrow().files(), [HashId(1), HashId(2)]);
    assert!(pages.append_session_backup("work", 100_000).is_err());
    pages.sync(300).unwrap();
    let mut reopened = Pages::open(store).unwrap();
    assert_eq!(reopened.shown().key, restored);
    assert_eq!(reopened.current().borrow().selected_files(), [HashId(2)]);
}

#[test]
fn tab_size_sort_sums_open_nested_media_and_keeps_stable_ties() {
    use hydrus_gui::tab_context::Sort;
    let (_dirs, store) = store();
    let context = FileSearchContext {
        predicates: parse_api_search(&serde_json::json!(["system:everything"])).unwrap(),
        ..FileSearchContext::default()
    };
    let files = store
        .read(|conn| {
            Ok(search_files(
                conn,
                &store.snapshot(),
                &context,
                FileSort {
                    by: SortBy::FileSize,
                    order: SortOrder::Descending,
                },
                &Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    let largest = files[0];
    let smallest = *files.last().unwrap();
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[largest, smallest]))
        .unwrap();
    assert!(info[0].info.as_ref().unwrap().size > info[1].info.as_ref().unwrap().size);
    let search = || PageContent::Search {
        search: context.clone(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    };
    let big = page("big", search());
    let small = page("small", search());
    let tie = page("same as small", search());
    let notebook = page("nested", PageContent::Pages(vec![big.clone()]));
    let saved = Session {
        name: LAST_SESSION.into(),
        pages: vec![small.clone(), notebook.clone(), tie.clone()],
    };
    let (big_key, small_key, tie_key) = (big.key, small.key, tie.key);
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 100)?;
            sessions::set_page_files(ctx.conn(), &big_key, &[largest])?;
            sessions::set_page_files(ctx.conn(), &small_key, &[smallest])?;
            sessions::set_page_files(ctx.conn(), &tie_key, &[smallest])
        })
        .unwrap();
    let mut pages = Pages::open(store).unwrap();
    for key in [small.key, big.key, tie.key] {
        pages.page(&key).unwrap();
    }
    pages.show(&small.key);
    pages.sort_tabs(0, Sort::Size, false).unwrap();
    assert_eq!(
        pages
            .session()
            .pages
            .iter()
            .map(|p| p.key)
            .collect::<Vec<_>>(),
        [notebook.key, small.key, tie.key]
    );
    assert_eq!(pages.shown().key, small.key);
    pages.sort_tabs(0, Sort::Size, true).unwrap();
    assert_eq!(
        pages
            .session()
            .pages
            .iter()
            .map(|p| p.key)
            .collect::<Vec<_>>(),
        [small.key, tie.key, notebook.key]
    );
    assert_eq!(pages.shown().key, small.key);
}

#[test]
fn bulk_tab_close_matches_reference_and_undo_restores_nested_positions() {
    use hydrus_gui::tab_context::Close;
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let search = || PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    };
    let original = vec![
        page("a", search()),
        page(
            "nested",
            PageContent::Pages(vec![page("child 0", search()), page("child 1", search())]),
        ),
        page("c", search()),
        page("d", search()),
    ];
    for step in fixture["close"].as_array().unwrap() {
        let saved = Session {
            name: LAST_SESSION.into(),
            pages: original.clone(),
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
            .unwrap();
        let mut pages = Pages::open(store.clone()).unwrap();
        pages.select(0, 2);
        let side = match step["side"].as_str().unwrap() {
            "left" => Close::Left,
            "right" => Close::Right,
            _ => Close::Other,
        };
        let (keys, question) = pages
            .close_tabs_question(0, step["index"].as_u64().unwrap() as usize, side)
            .unwrap();
        assert_eq!(question, step["asked"][0].as_str().unwrap());
        if step["accepted"].as_bool().unwrap() {
            pages.close_tab_keys(&keys).unwrap();
        }
        assert_eq!(
            serde_json::json!(
                pages
                    .session()
                    .pages
                    .iter()
                    .map(|p| &p.name)
                    .collect::<Vec<_>>()
            ),
            step["pages"]
        );
        assert_eq!(
            pages.closed_names().len(),
            step["closed_indices"].as_array().unwrap().len()
        );
        while pages.unclose() {}
        assert_eq!(pages.session().pages, original);
    }
}

#[test]
fn tab_close_focus_settings_and_context_navigation_reach_consumers() {
    use hydrus_gui::tab_context::{Move, NotebookSettings};
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let search = || PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    };
    let original: Vec<_> = (0..4)
        .map(|i| page(&format!("page {i}"), search()))
        .collect();
    for step in fixture["navigation"].as_array().unwrap() {
        let saved = Session {
            name: LAST_SESSION.into(),
            pages: original.clone(),
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
            .unwrap();
        let mut pages = Pages::open(store.clone()).unwrap();
        pages.select(0, 2);
        let movement = match step["movement"].as_str().unwrap() {
            "first" => Move::First,
            "left" => Move::Left,
            "right" => Move::Right,
            _ => Move::Last,
        };
        pages.navigate_tabs(0, movement, std::time::Instant::now());
        assert_eq!(
            pages.tabs()[0].selected,
            step["selected"].as_u64().unwrap() as usize
        );
    }
    for left in [false, true] {
        let saved = Session {
            name: LAST_SESSION.into(),
            pages: original.clone(),
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &saved, 100)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &NotebookSettings {
                        close_focus_left: left,
                        ..NotebookSettings::default()
                    },
                )
            })
            .unwrap();
        let mut pages = Pages::open(store.clone()).unwrap();
        pages.select(0, 2);
        pages.close_shown().unwrap();
        assert_eq!(pages.shown().name, if left { "page 1" } else { "page 3" });
    }
}

#[test]
fn tab_popup_bulk_close_cancellation_and_acceptance_are_wired() {
    fn choose(ui: &MainWindow, pane: i32, label: &str) {
        let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
        let index = (0..lines.row_count())
            .find(|&i| lines.row_data(i).unwrap().label == label)
            .unwrap();
        ui.invoke_menu_line_clicked(pane, index as i32, 200.0, 100.0, 10.0);
    }
    let windows = headless::init();
    let (_dirs, store) = store();
    let original: Vec<_> = (0..3)
        .map(|i| {
            page(
                &format!("page {i}"),
                PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            )
        })
        .collect();
    let saved = Session {
        name: LAST_SESSION.into(),
        pages: original,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "close other pages");
    assert_eq!(ui.get_question(), "Close 2 other pages?");
    ui.invoke_answer(false);
    assert_eq!(bound.pages.borrow().session().pages.len(), 3);
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "close other pages");
    ui.invoke_answer(true);
    assert_eq!(bound.pages.borrow().session().pages.len(), 1);
    assert_eq!(bound.pages.borrow_mut().closed_names().len(), 2);
    assert!(bound.pages.borrow_mut().unclose());
    assert!(bound.pages.borrow_mut().unclose());
    ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
    let screenshot = headless::render(&windows.get(0).unwrap(), 900, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tab-context.png"),
        &screenshot,
        900,
        650,
    )
    .unwrap();
}

#[test]
fn send_down_and_rename_prompts_match_reference_including_cancellation() {
    fn tree(pages: &[Page]) -> serde_json::Value {
        serde_json::json!(
            pages
                .iter()
                .map(|page| match &page.content {
                    PageContent::Pages(children) =>
                        serde_json::json!({"name": page.name, "children": tree(children)}),
                    _ => serde_json::json!({"name": page.name}),
                })
                .collect::<Vec<_>>()
        )
    }
    fn choose(ui: &MainWindow, pane: i32, label: &str) {
        let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
        let index = (0..lines.row_count())
            .find(|&i| lines.row_data(i).unwrap().label == label)
            .unwrap();
        ui.invoke_menu_line_clicked(pane, index as i32, 200.0, 100.0, 10.0);
    }
    use hydrus_gui::tab_context::NotebookSettings;
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let original: Vec<_> = (0..4)
        .map(|i| {
            page(
                &format!("page {i}"),
                PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            )
        })
        .collect();
    for step in fixture["send"].as_array().unwrap() {
        let rename_sent_notebooks = step["rename"].as_bool().unwrap();
        let saved = Session {
            name: LAST_SESSION.into(),
            pages: original.clone(),
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &saved, 100)?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &NotebookSettings {
                        rename_sent_notebooks,
                        ..NotebookSettings::default()
                    },
                )
            })
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        ui.invoke_tab_chosen(0, 2);
        let scope = step["scope"].as_str().unwrap();
        let clicked = if scope == "right" {
            1
        } else {
            step["index"].as_i64().unwrap() as i32
        };
        ui.invoke_tab_menu_requested(0, clicked, 30.0, 55.0);
        choose(&ui, 0, "send down to a new page of pages");
        choose(
            &ui,
            1,
            match scope {
                "this" => "this page",
                "from_here" => "pages from here to the right",
                _ => "pages to the right",
            },
        );
        if scope != "this" {
            assert_eq!(ui.get_question(), step["asked"][0].as_str().unwrap());
            ui.invoke_answer(step["accepted"].as_bool().unwrap());
        }
        if step["rename"].as_bool().unwrap() {
            let dialog = bound
                .tab_name_dialog
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            assert_eq!(
                dialog.get_message(),
                step["text"][0]["message"].as_str().unwrap()
            );
            assert_eq!(
                dialog.get_text(),
                step["text"][0]["default"].as_str().unwrap()
            );
            if let Some(name) = step["name"].as_str() {
                dialog.invoke_name_entered(name.into());
            } else {
                dialog.invoke_cancelled();
            }
            assert!(bound.tab_name_dialog.borrow().is_none());
        }
        assert_eq!(tree(&bound.pages.borrow().session().pages), step["tree"]);
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["shown"].as_str().unwrap()
        );
        assert!(bound.pages.borrow_mut().closed_names().is_empty());
        let moved: Vec<_> = bound
            .pages
            .borrow()
            .session()
            .all_pages()
            .into_iter()
            .filter(|p| !matches!(p.content, PageContent::Pages(_)))
            .map(|p| p.key)
            .collect();
        assert_eq!(moved, original.iter().map(|p| p.key).collect::<Vec<_>>());
    }
}

#[test]
fn rename_tab_uses_frozen_key_and_cancellation_preserves_name() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let original: Vec<_> = (0..4)
        .map(|i| {
            page(
                &format!("page {i}"),
                PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            )
        })
        .collect();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    for step in fixture["rename"].as_array().unwrap() {
        let saved = Session {
            name: LAST_SESSION.into(),
            pages: original.clone(),
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        ui.invoke_tab_chosen(0, 2);
        ui.invoke_tab_rename_requested(0, 1);
        let dialog = bound
            .tab_name_dialog
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(
            dialog.get_message(),
            step["text"][0]["message"].as_str().unwrap()
        );
        assert_eq!(
            dialog.get_text(),
            step["text"][0]["default"].as_str().unwrap()
        );
        ui.invoke_tab_chosen(0, 3); // switching while text entry is open retains target
        if let Some(name) = step["name"].as_str() {
            dialog.invoke_name_entered(name.into());
        } else {
            dialog.invoke_cancelled();
        }
        assert_eq!(
            serde_json::json!(
                bound
                    .pages
                    .borrow()
                    .session()
                    .pages
                    .iter()
                    .map(|p| &p.name)
                    .collect::<Vec<_>>()
            ),
            step["pages"]
        );
        assert_eq!(bound.pages.borrow().shown().key, original[3].key);
        bound.pages.borrow_mut().sync(200).unwrap();
        assert_eq!(
            Pages::open(store.clone()).unwrap().session().pages[1].name,
            step["pages"][1].as_str().unwrap()
        );
    }
}

#[test]
fn send_down_keeps_nested_selection_and_open_search_objects() {
    use hydrus_gui::tab_context::Send;
    let (_dirs, store) = store();
    let search = || PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    };
    let first = page("first", search());
    let selected = page("selected", search());
    let nested = page("nested", PageContent::Pages(vec![first, selected.clone()]));
    let other = page("other", search());
    let saved = Session {
        name: LAST_SESSION.into(),
        pages: vec![nested.clone(), other.clone()],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.select(1, 1);
    let opened = pages.current();
    pages.select(0, 1);
    let targets = pages.send_tab_targets(0, 0, Send::This);
    let grouped = pages.send_tab_keys(&targets, true).unwrap();
    assert_eq!(pages.session().pages[0].key, grouped);
    assert_eq!(pages.shown().key, selected.key);
    assert!(Rc::ptr_eq(&opened, &pages.current()));
    assert!(pages.closed_names().is_empty());
    pages.sync(200).unwrap();
    let reopened = Pages::open(store).unwrap();
    assert_eq!(reopened.shown().key, selected.key);
    assert_eq!(reopened.session().pages, pages.session().pages);
}

#[test]
fn downloader_backup_and_freshest_load_survive_source_queue_deletion() {
    use hydrus_gui::page_chooser::NewPage;
    use hydrus_store::queues::{self, NewFileSeed, SeedStatus, SeedType};
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_importers.json");
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.new_page(&NewPage::Urls).unwrap();
    let original = match &pages.shown().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("URL importer"),
    };
    let initial_urls: Vec<_> = fixture["initial"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|recorded| recorded["url"].as_str().unwrap().to_owned())
        .collect();
    store
        .write(move |ctx| {
            queues::set_paused(ctx.conn(), original, Some(true), Some(true))?;
            for url in initial_urls {
                queues::add_file_seeds(
                    ctx.conn(),
                    original,
                    &[NewFileSeed {
                        seed_type: SeedType::Url,
                        data: url.clone(),
                        data_for_comparison: url,
                        source_time: None,
                        referral_url: None,
                        meta: queues::FileSeedMeta::default(),
                    }],
                    false,
                    100,
                )?;
            }
            Ok(())
        })
        .unwrap();
    pages.save_session("history", 100).unwrap();
    let later = fixture["source_changed"]["files"][2]["url"]
        .as_str()
        .unwrap()
        .to_owned();
    store
        .write(move |ctx| {
            queues::add_file_seeds(
                ctx.conn(),
                original,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: later.clone(),
                    data_for_comparison: later,
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta::default(),
                }],
                false,
                110,
            )?;
            let mut first = queues::file_seeds(ctx.conn(), original)?[0].clone();
            first.status = SeedStatus::SuccessfulButRedundant;
            queues::update_file_seed(ctx.conn(), &first)
        })
        .unwrap();
    pages.save_session("history", 200).unwrap();
    pages.close_shown().unwrap();
    pages.forget_closed();
    assert!(
        store
            .read(|conn| queues::queue(conn, original))
            .unwrap()
            .is_none()
    );
    pages.append_session_backup("history", 100_000).unwrap();
    let backup = match &pages.shown().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("restored URL importer"),
    };
    assert_eq!(
        store
            .read(|conn| queues::file_seeds(conn, backup))
            .unwrap()
            .len(),
        2
    );
    pages.append_session("history").unwrap();
    let freshest = match &pages.shown().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("freshest URL importer"),
    };
    assert_ne!(backup, freshest);
    assert_eq!(
        store
            .read(|conn| queues::file_seeds(conn, freshest))
            .unwrap()
            .len(),
        3
    );
    assert!(
        store
            .read(|conn| queues::queue(conn, freshest))
            .unwrap()
            .unwrap()
            .files_paused
    );
    pages.clear_and_load("history").unwrap();
    let loaded = match &pages.shown().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("loaded URL importer"),
    };
    assert_ne!(loaded, freshest);
    assert!(
        store
            .read(|conn| queues::queue(conn, backup))
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .read(|conn| queues::queue(conn, freshest))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .read(|conn| queues::file_seeds(conn, loaded))
            .unwrap()
            .len(),
        3
    );
    pages.sync(300).unwrap();
    let mut reopened = Pages::open(store).unwrap();
    assert_eq!(reopened.shown().key, pages.shown().key);
    assert_eq!(reopened.current().borrow().import_progress(), (1, 3));
}

/// Reconstruct a reference harvest tree with its exact ordered media.
fn harvest_source(store: &Store, rows: &serde_json::Value) -> Vec<Page> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            if row.get("children").is_some() {
                page(
                    row["name"].as_str().unwrap(),
                    PageContent::Pages(harvest_source(store, &row["children"])),
                )
            } else {
                let result = page(
                    row["name"].as_str().unwrap(),
                    PageContent::Search {
                        search: FileSearchContext::default(),
                        synchronised: false,
                        sort: None,
                        lock: None,
                        collect: None,
                    },
                );
                let key = result.key;
                let hashes: Vec<_> = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().parse::<hydrus_core::Sha256>().unwrap())
                    .collect();
                store
                    .write(move |ctx| {
                        let ids = hydrus_store::master::hash_ids(ctx.conn(), &hashes)?;
                        let files: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
                        sessions::set_page_files(ctx.conn(), &key, &files)
                    })
                    .unwrap();
                result
            }
        })
        .collect()
}

fn harvest_tree(store: &Store, rows: &[Page]) -> serde_json::Value {
    serde_json::json!(
        rows.iter()
            .map(|page| {
                if let PageContent::Pages(children) = &page.content {
                    serde_json::json!({"name":page.name,"children":harvest_tree(store, children)})
                } else {
                    let hashes = store
                        .read(|conn| {
                            let files = sessions::page_files(conn, &page.key)?;
                            let hashes = hydrus_store::master::hashes(conn, &files)?;
                            Ok(files
                                .iter()
                                .map(|id| hashes[id].to_hex())
                                .collect::<Vec<_>>())
                        })
                        .unwrap();
                    serde_json::json!({"name":page.name,"hashes":hashes})
                }
            })
            .collect::<Vec<_>>()
    )
}

#[test]
fn duplicate_and_collapse_menus_replay_real_ordered_media_and_cancellation() {
    fn choose(ui: &MainWindow, pane: i32, label: &str) {
        let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
        let index = (0..lines.row_count())
            .find(|&i| lines.row_data(i).unwrap().label == label)
            .unwrap();
        ui.invoke_menu_line_clicked(pane, index as i32, 200.0, 100.0, 10.0);
    }
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_harvest.json");
    for (action, steps) in [
        ("collapse", &fixture["collapse"]),
        ("duplicate", &fixture["duplicate"]),
    ] {
        for step in steps.as_array().unwrap() {
            let original = harvest_source(&store, &step["before"]["tree"]);
            let session = Session {
                name: LAST_SESSION.into(),
                pages: original.clone(),
            };
            store
                .write(move |ctx| sessions::save(ctx.conn(), &session, 100))
                .unwrap();
            let ui = MainWindow::new().unwrap();
            let bound = bind(&ui, Pages::open(store.clone()).unwrap());
            ui.invoke_tab_chosen(0, 0);
            ui.invoke_tab_chosen(1, 1);
            let index = step["index"].as_i64().unwrap() as i32;
            let scope = step["scope"].as_str().unwrap_or("this");
            let clicked = if scope == "right" { index - 1 } else { index };
            ui.invoke_tab_menu_requested(0, clicked, 30.0, 55.0);
            if action == "duplicate" {
                choose(&ui, 0, "duplicate page");
            } else {
                choose(&ui, 0, "collapse to a single page");
                choose(
                    &ui,
                    1,
                    match scope {
                        "from_here" => "pages from here to the right",
                        "right" => "pages to the right",
                        _ => "this page",
                    },
                );
                assert_eq!(ui.get_question(), step["asked"][0].as_str().unwrap());
                ui.invoke_answer(step["accepted"].as_bool().unwrap());
            }
            bound.pages.borrow_mut().sync(200).unwrap();
            assert_eq!(
                harvest_tree(&store, &bound.pages.borrow().session().pages),
                step["after"]["tree"]
            );
            assert_eq!(
                bound.pages.borrow().shown().name,
                step["after"]["shown"].as_str().unwrap()
            );
            assert_eq!(
                bound.pages.borrow_mut().closed_names().len(),
                step["after"]["closed_indices"].as_array().unwrap().len()
            );
            if action == "duplicate" {
                let cloned = bound.pages.borrow().session().pages[index as usize + 1].clone();
                assert_ne!(cloned.key, original[index as usize].key);
                let copied_media = bound.pages.borrow().shown().key;
                let original_media = if index == 0 {
                    let PageContent::Pages(children) = &original[0].content else {
                        unreachable!()
                    };
                    children[0].key
                } else {
                    original[1].key
                };
                bound.pages.borrow_mut().close(0, index as usize).unwrap();
                bound.pages.borrow_mut().forget_closed();
                assert!(
                    store
                        .read(|conn| sessions::page_files(conn, &original_media))
                        .unwrap()
                        .is_empty()
                );
                assert!(
                    !store
                        .read(|conn| sessions::page_files(conn, &copied_media))
                        .unwrap()
                        .is_empty()
                );
            } else if step["accepted"] == true {
                let replacement = bound.pages.borrow().shown().key;
                assert!(matches!(
                    bound.pages.borrow().shown().content,
                    PageContent::Search { lock: Some(_), .. }
                ));
                while bound.pages.borrow_mut().unclose() {}
                for source in &original {
                    assert!(
                        bound
                            .pages
                            .borrow()
                            .session()
                            .all_pages()
                            .iter()
                            .any(|p| p.key == source.key)
                    );
                }
                assert!(bound.pages.borrow_mut().show(&replacement));
            } else {
                assert_eq!(bound.pages.borrow().session().pages, original);
            }
        }
    }
}

#[test]
fn collapse_empty_nested_notebook_has_no_hash_lock_and_one_replacement() {
    use hydrus_gui::tab_context::Send;
    let (_dirs, store) = store();
    let empty = page("empty", PageContent::Pages(Vec::new()));
    let parent = page("parent", PageContent::Pages(vec![empty]));
    store
        .write(move |ctx| {
            sessions::save(
                ctx.conn(),
                &Session {
                    name: LAST_SESSION.into(),
                    pages: vec![parent],
                },
                100,
            )
        })
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    let (keys, files, _) = pages
        .collapse_tabs_question(1, 0, Send::This)
        .unwrap()
        .unwrap();
    assert!(files.is_empty());
    pages.collapse_tab_keys(&keys, &files).unwrap();
    assert_eq!(pages.tabs()[1].names, ["files"]);
    assert!(matches!(
        pages.shown().content,
        PageContent::Search { lock: None, .. }
    ));
    pages.sync(200).unwrap();
    assert_eq!(Pages::open(store).unwrap().shown().key, pages.shown().key);
}

#[test]
fn chooser_new_notebook_name_preference_matches_reference_and_persists() {
    use hydrus_store::settings::NotebookCreationSettings;
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("notebook_creation.json");
    assert_eq!(
        NotebookCreationSettings::default().rename_new_notebooks,
        fixture["default_prompt"].as_bool().unwrap()
    );
    for step in fixture["steps"].as_array().unwrap() {
        let original = page(
            "original",
            PageContent::Search {
                search: FileSearchContext::default(),
                synchronised: false,
                sort: None,
                lock: None,
                collect: None,
            },
        );
        let key = original.key;
        let session = Session {
            name: LAST_SESSION.into(),
            pages: vec![original],
        };
        let settings = NotebookCreationSettings {
            rename_new_notebooks: step["prompt"].as_bool().unwrap(),
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &session, 100)?;
                sessions::set_shown(ctx.conn(), LAST_SESSION, Some(&key))?;
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        ui.invoke_new_page();
        ui.invoke_chooser_pressed(6); // special
        ui.invoke_chooser_pressed(8); // page of pages
        assert_eq!(ui.get_chooser_labels().row_count(), 0);
        if step["prompt"] == true {
            let dialog = bound
                .tab_name_dialog
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            assert_eq!(
                dialog.get_message(),
                step["asked"][0]["message"].as_str().unwrap()
            );
            assert_eq!(
                dialog.get_text(),
                step["asked"][0]["default"].as_str().unwrap()
            );
            // Naming remains tied to the created notebook if another page
            // is selected while the text window is open.
            ui.invoke_tab_chosen(0, 0);
            if let Some(name) = step["answer"].as_str() {
                dialog.invoke_name_entered(name.into());
            } else {
                dialog.invoke_cancelled();
            }
            assert!(bound.tab_name_dialog.borrow().is_none());
            ui.invoke_tab_chosen(0, 1);
        } else {
            assert!(bound.tab_name_dialog.borrow().is_none());
        }
        let created = bound.pages.borrow().session().pages[1].clone();
        assert_eq!(created.name, step["name"].as_str().unwrap());
        let PageContent::Pages(children) = &created.content else {
            panic!("created notebook")
        };
        assert_eq!(
            serde_json::json!(children.iter().map(|p| &p.name).collect::<Vec<_>>()),
            step["children"]
        );
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["shown"].as_str().unwrap()
        );
        assert_eq!(bound.pages.borrow().session().pages[0].name, "original");
        bound.pages.borrow_mut().sync(200).unwrap();
        let reopened = Pages::open(store.clone()).unwrap();
        assert_eq!(reopened.session().pages[1], created);
        assert_eq!(reopened.shown().name, "files");
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<NotebookCreationSettings>)
                .unwrap(),
            settings
        );
    }
}
