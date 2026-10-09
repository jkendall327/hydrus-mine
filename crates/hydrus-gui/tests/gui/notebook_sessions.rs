//! Notebook context session saving and append, replayed against the real client.
use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::{Store, sessions};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

fn source() -> Vec<Page> {
    vec![
        Page {
            key: PageKey::random(),
            name: "source notebook".into(),
            content: PageContent::Pages(vec![
                search("inside one"),
                Page {
                    key: PageKey::random(),
                    name: "nested".into(),
                    content: PageContent::Pages(vec![search("inside two")]),
                },
            ]),
        },
        search("outside"),
    ]
}

fn tree(pages: &[Page]) -> serde_json::Value {
    serde_json::json!(
        pages
            .iter()
            .map(|page| match &page.content {
                PageContent::Pages(children) =>
                    serde_json::json!({"name":page.name,"children":tree(children)}),
                _ => serde_json::json!({"name":page.name}),
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

// leaf: audit-options-tabs-context-action-2287-non-reserved-session-name
// leaf: audit-options-tabs-context-action-2290-create-a-new-session
#[test]
fn clicked_notebook_saving_replays_names_overwrite_invalid_and_cancellation() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("notebook_sessions.json");
    let seed = Session {
        name: "existing work".into(),
        pages: vec![search("old entry")],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &seed, 1))
        .unwrap();
    let original = source();
    for step in fixture["steps"].as_array().unwrap() {
        let session = Session {
            name: sessions::LAST_SESSION.into(),
            pages: original.clone(),
        };
        let outside = original[1].key;
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &session, 100)?;
                sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&outside))
            })
            .unwrap();
        let prior = store
            .read(|conn| sessions::load(conn, "existing work"))
            .unwrap();
        let names_before = store
            .read(sessions::names)
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
        choose(&ui, 0, "save this page of pages to a session");
        choose(
            &ui,
            1,
            step["name"].as_str().unwrap_or("create a new session"),
        );
        for question in step["asked"].as_array().unwrap() {
            let dialog = bound
                .session_dialog
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            match question["kind"].as_str().unwrap() {
                "text" => {
                    assert!(dialog.get_asking_name());
                    assert_eq!(dialog.get_message(), question["message"].as_str().unwrap());
                    assert_eq!(dialog.get_text(), question["default"].as_str().unwrap());
                    if let Some(name) = question["answer"].as_str() {
                        dialog.invoke_name_entered(name.into());
                    } else {
                        dialog.invoke_cancelled();
                    }
                }
                "warning" => {
                    assert_eq!(dialog.get_warning(), question["message"].as_str().unwrap());
                }
                _ => {
                    assert!(!dialog.get_asking_name());
                    assert_eq!(dialog.get_message(), question["message"].as_str().unwrap());
                    assert_eq!(
                        dialog.get_window_title(),
                        question["title"].as_str().unwrap()
                    );
                    assert_eq!(dialog.get_yes_label(), question["yes"].as_str().unwrap());
                    assert_eq!(dialog.get_no_label(), question["no"].as_str().unwrap());
                    if let Some(answer) = question["answer"].as_bool() {
                        dialog.invoke_answered(answer);
                    } else {
                        dialog.invoke_cancelled();
                    }
                }
            }
        }
        assert!(bound.session_dialog.borrow().is_none());
        assert_eq!(
            tree(&bound.pages.borrow().session().pages),
            step["open_tree"]
        );
        assert_eq!(
            bound.pages.borrow().shown().name,
            step["shown"].as_str().unwrap()
        );
        if step["saved"].as_array().unwrap().is_empty() {
            assert_eq!(
                store
                    .read(|conn| sessions::load(conn, "existing work"))
                    .unwrap(),
                prior
            );
            assert_eq!(
                store
                    .read(sessions::names)
                    .unwrap()
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
                names_before
            );
        } else {
            for recorded in step["saved"].as_array().unwrap() {
                let name = recorded["name"].as_str().unwrap();
                let saved = store
                    .read(|conn| sessions::load(conn, name))
                    .unwrap()
                    .unwrap();
                assert_eq!(tree(&saved.pages), recorded["children"]);
                let source_keys: Vec<_> = bound
                    .pages
                    .borrow()
                    .session()
                    .all_pages()
                    .iter()
                    .map(|p| p.key)
                    .collect();
                assert!(
                    saved
                        .all_pages()
                        .iter()
                        .all(|p| !source_keys.contains(&p.key))
                );
                assert!(
                    store
                        .read(|conn| hydrus_store::session_backups::latest(conn, name))
                        .unwrap()
                        .is_some()
                );
            }
        }
    }
    let mut reopened = Pages::open(store.clone()).unwrap();
    reopened
        .append_session_to_notebook(Some(original[0].key), "existing work")
        .unwrap();
    assert_eq!(tree(&reopened.session().pages), fixture["appended"]["tree"]);
    assert_eq!(
        reopened.shown().name,
        fixture["appended"]["shown"].as_str().unwrap()
    );
    reopened.select(0, 0);
    let last = reopened.tabs()[1].names.len() - 1;
    assert_eq!(reopened.tabs()[1].selected, last);
    assert_eq!(reopened.shown().name, "inside one");
    reopened.sync(200).unwrap();
    assert_eq!(
        Pages::open(store.clone()).unwrap().session().pages,
        reopened.session().pages
    );
    let previous = reopened.session().pages.clone();
    assert!(
        reopened
            .append_session_to_notebook(Some(PageKey::random()), "existing work")
            .is_err()
    );
    assert!(
        reopened
            .append_session_to_notebook(Some(original[0].key), "missing session")
            .is_err()
    );
    assert_eq!(reopened.session().pages, previous);
}

// leaf: audit-options-tabs-context-action-2287-non-reserved-session-name
#[test]
fn notebook_session_copies_preserve_media_and_independent_importer_state() {
    use hydrus_gui::page_chooser::NewPage;
    use hydrus_store::queues::{self, NewFileSeed, SeedType};
    let (_dirs, store) = store();
    let original = source();
    let source_key = original[0].key;
    let leaf = match &original[0].content {
        PageContent::Pages(children) => children[0].key,
        _ => unreachable!(),
    };
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: original,
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_page_files(
                ctx.conn(),
                &leaf,
                &[hydrus_core::HashId(2), hydrus_core::HashId(1)],
            )?;
            sessions::set_page_selected(ctx.conn(), &leaf, &[hydrus_core::HashId(1)])
        })
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.select(0, 0);
    pages.new_page_in(Some(1));
    pages.new_page(&NewPage::Urls).unwrap();
    let importer = match &pages.shown().content {
        PageContent::Downloader {
            kind: DownloaderKind::Urls,
            queues,
            ..
        } => queues[0],
        _ => panic!("URL importer"),
    };
    store
        .write(move |ctx| {
            queues::set_paused(ctx.conn(), importer, Some(true), Some(true))?;
            queues::add_file_seeds(
                ctx.conn(),
                importer,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: "https://files.example/history.jpg".into(),
                    data_for_comparison: "https://files.example/history.jpg".into(),
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta::default(),
                }],
                false,
                100,
            )
            .map(|_| ())
        })
        .unwrap();
    pages
        .save_notebook_session_at_ms(source_key, "notebook copy", 100_000)
        .unwrap();
    let saved = store
        .read(|conn| hydrus_store::session_backups::latest(conn, "notebook copy"))
        .unwrap()
        .unwrap();
    assert_eq!(saved.queues.len(), 1);
    pages.close(0, 0).unwrap();
    pages.forget_closed();
    pages.sync(101).unwrap();
    assert!(
        store
            .read(|conn| queues::queue(conn, importer))
            .unwrap()
            .is_none()
    );
    pages
        .append_session_to_notebook(None, "notebook copy")
        .unwrap();
    assert_eq!(
        pages.current().borrow().files(),
        [hydrus_core::HashId(2), hydrus_core::HashId(1)]
    );
    assert_eq!(
        pages.current().borrow().selected_files(),
        [hydrus_core::HashId(1)]
    );
    let PageContent::Pages(children) = &pages.session().pages.last().unwrap().content else {
        panic!("copy notebook")
    };
    let copied_key = children.last().unwrap().key;
    let copy = match &children.last().unwrap().content {
        PageContent::Downloader { queues, .. } => queues[0],
        _ => panic!("copy importer"),
    };
    assert_ne!(copied_key, leaf);
    assert_eq!(
        store
            .read(|conn| queues::file_seeds(conn, copy))
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .read(|conn| queues::queue(conn, copy))
            .unwrap()
            .unwrap()
            .files_paused
    );
    let before = pages.session().pages.clone();
    assert!(
        pages
            .save_notebook_session_at_ms(PageKey::random(), "vanished", 110_000)
            .is_err()
    );
    assert_eq!(pages.session().pages, before);
    assert!(
        store
            .read(|conn| sessions::load(conn, "vanished"))
            .unwrap()
            .is_none()
    );
}

// leaf: audit-options-tabs-context-action-2270-saved-session-name
// "append session > name" chosen from the notebook tab's menu, as the
// reference's was (notebook_sessions.json `appended`): the saved session's
// pages land inside the notebook and the page shown is the recorded one.
#[test]
fn append_session_from_the_tab_menu_lands_where_the_reference_put_it() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("notebook_sessions.json");
    let original = source();
    // the saved session the recording appended: the source notebook's contents
    let PageContent::Pages(contents) = &original[0].content else {
        panic!("the source notebook")
    };
    let saved = Session {
        name: "existing work".into(),
        pages: contents.clone(),
    };
    let last = Session {
        name: sessions::LAST_SESSION.into(),
        pages: original.clone(),
    };
    let outside = original[1].key;
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 1)?;
            sessions::save(ctx.conn(), &last, 100)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&outside))
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // (a tab's \"append session\" adds to the notebook that holds the tab: the
    // source notebook's own tabs, shown once it is selected)
    bound.pages.borrow_mut().select(0, 0);
    ui.invoke_tab_menu_requested(1, 0, 30.0, 55.0);
    choose(&ui, 0, "append session");
    choose(&ui, 1, "existing work");
    assert_eq!(
        tree(&bound.pages.borrow().session().pages),
        fixture["appended"]["tree"]
    );
    // (the recording's shown page, "outside", is the top-level page the
    // recorder left current; the menu is reached only from inside the source
    // notebook, so that state is not reachable here and is compared where the
    // recording's direct call is replayed, through
    // Pages::append_session_to_notebook, above)
}
