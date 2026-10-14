//! Closing pages from the tab menu when some of them have importers at work,
//! against `oracle/fixtures/tab_close_objections.json` (recorded by
//! `oracle/record_tab_close_objections.py`): the longer question with each
//! objecting page's reason, its yes that waits a moment, its "no, but show
//! me the pages" and the list that opens, and the single page's "Close
//! <name>?" with the reason; each answered as the recording was.

use std::sync::Arc;
use std::time::Duration;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::pages::{
    DownloaderKind, DownloaderPageState, Page, PageContent, PageKey, Session,
};
use hydrus_gui::reasons_dialog;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedType};
use hydrus_store::sessions::{self, LAST_SESSION};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use crate::common::widgets;
use crate::subscriptions::store;

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: hydrus_search::FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

/// A URL importer page with this many URLs queued, paused.
fn importer(store: &Arc<Store>, name: &str, count: usize) -> Page {
    let key = PageKey::random();
    let items: Vec<NewFileSeed> = (0..count)
        .map(|i| {
            let url = format!("https://files.example/{name}-{i}.jpg");
            NewFileSeed {
                seed_type: SeedType::Url,
                data: url.clone(),
                data_for_comparison: url,
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            }
        })
        .collect();
    let label = name.to_owned();
    let queue = store
        .write(move |ctx| {
            let queue = queues::create_queue(
                ctx.conn(),
                QueueKind::Urls,
                &label,
                Some(&key.0),
                &ImportOptionsSlice::default(),
                1,
            )?;
            queues::set_paused(ctx.conn(), queue, Some(true), Some(true))?;
            queues::add_file_seeds(ctx.conn(), queue, &items, false, 1)?;
            Ok(queue)
        })
        .unwrap();
    Page {
        key,
        name: name.into(),
        content: PageContent::Downloader {
            kind: DownloaderKind::Urls,
            queues: vec![queue],
            sort: None,
            page: Some(Box::new(DownloaderPageState::default())),
        },
    }
}

fn names(ui: &MainWindow) -> Vec<String> {
    ui.get_tab_rows()
        .row_data(0)
        .unwrap()
        .full_names
        .iter()
        .map(|n| n.to_string())
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

fn choose(ui: &MainWindow, pane: usize, label: &str) {
    let at = labels(ui, pane)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label:?} in {:?}", labels(ui, pane)));
    ui.invoke_menu_line_clicked(
        i32::try_from(pane).unwrap(),
        i32::try_from(at).unwrap(),
        200.0,
        100.0,
        10.0,
    );
}

/// The notebook as the recorder built it: a, imp1 (3 URLs), b, imp2 (2), c.
fn open(store: &Arc<Store>) -> (MainWindow, hydrus_gui::Bound) {
    open_layout(store, false)
}

/// Or a, a page of pages `group` holding the importer `nested imp` (4 URLs),
/// b.
fn open_layout(store: &Arc<Store>, nested: bool) -> (MainWindow, hydrus_gui::Bound) {
    let pages = if nested {
        vec![
            search("a"),
            Page {
                key: PageKey::random(),
                name: "group".into(),
                content: PageContent::Pages(vec![importer(store, "nested imp", 4)]),
            },
            search("b"),
        ]
    } else {
        vec![
            search("a"),
            importer(store, "imp1", 3),
            search("b"),
            importer(store, "imp2", 2),
            search("c"),
        ]
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 100))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().select(0, 0);
    (ui, bound)
}

fn run(menu: &str) -> (&'static str, Vec<&'static str>) {
    match menu {
        // (on the first tab there is no left to close: the entry is on the menu itself)
        "other pages" | "nested: other pages" => ("close other pages", vec![]),
        "pages to the left" => ("close", vec!["pages to the left"]),
        "pages to the right" | "pages to the right (nothing objects)" => {
            ("close", vec!["pages to the right"])
        }
        _ => ("close page", vec![]),
    }
}

// leaf: audit-options-tabs-context-action-2074-close-other-page-s
// leaf: audit-options-tabs-context-action-2084-pages-to-the-left
// leaf: audit-options-tabs-context-action-2089-pages-to-the-right
// leaf: audit-options-tabs-context-action-2057-close-page
#[test]
fn closing_pages_with_importers_asks_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("tab_close_objections.json");
    let _windows = headless::init();
    let (_dirs, store) = store();
    let mut with_dialog = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let menu = case["menu"].as_str().unwrap();
        let answer = case["answer"].as_str().unwrap();
        let index = i32::try_from(case["index"].as_u64().unwrap()).unwrap();
        let asked = case["asked"].as_array().unwrap();
        let (ui, bound) = open_layout(&store, menu.starts_with("nested"));
        assert_eq!(names(&ui), strings(&case["before"]), "{menu}");
        let original = bound.pages.borrow().session().pages.clone();
        let (entry, sub) = run(menu);
        // (the dialog's yes waits; the check that it does is below, with
        // a long wait; here it is short so that the answer can be given)
        reasons_dialog::set_yes_delay(Some(Duration::ZERO));
        ui.invoke_tab_menu_requested(0, index, 30.0, 55.0);
        choose(&ui, 0, entry);
        if let Some(sub) = sub.first() {
            choose(&ui, 1, sub);
        }
        let label = format!("{menu} / {answer}");
        if asked.is_empty() {
            // nothing objects and the reference did not ask
            assert_eq!(ui.get_question(), "", "{label}");
        } else if asked[0]["dialog"] == "yes/no" {
            assert_eq!(
                ui.get_question(),
                asked[0]["message"].as_str().unwrap(),
                "{label}"
            );
            ui.invoke_answer(answer == "yes");
        } else {
            with_dialog += 1;
            assert_eq!(ui.get_question(), "", "{label}: not the plain question");
            slint::platform::update_timers_and_animations();
            let dialog = reasons_dialog::dialog().expect("the dialog");
            widgets::lay_out(dialog.window(), 480.0, 300.0);
            assert_eq!(
                dialog.get_message(),
                asked[0]["message"].as_str().unwrap(),
                "{label}"
            );
            assert_eq!(
                dialog.get_window_title(),
                asked[0]["title"].as_str().unwrap()
            );
            let panel = &asked[0]["panel"]["buttons"];
            let buttons: Vec<String> = dialog.get_choices().iter().map(|c| c.to_string()).collect();
            assert_eq!(
                buttons,
                strings(&serde_json::json!(
                    panel
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b["text"].clone())
                        .collect::<Vec<_>>()
                )),
                "{label}"
            );
            match answer {
                "yes" => widgets::click(dialog.window(), "yes"),
                "no" => widgets::click(dialog.window(), "no"),
                _ => {
                    widgets::click(dialog.window(), "no, but show me the pages");
                    let list = reasons_dialog::list().expect("the list of pages");
                    widgets::lay_out(list.window(), 480.0, 300.0);
                    let listed = &case["lists"][0];
                    assert_eq!(list.get_window_title(), listed["title"].as_str().unwrap());
                    let want: Vec<String> = listed["choices"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c[0].as_str().unwrap().to_owned())
                        .collect();
                    let got: Vec<String> =
                        list.get_choices().iter().map(|c| c.to_string()).collect();
                    assert_eq!(got, want, "{label}: the list");
                    // each button goes to its page (the reference's list showed
                    // one page a button), and the list stays
                    assert_eq!(want.len(), case["shown"].as_array().unwrap().len());
                    for (n, text) in want.iter().enumerate() {
                        widgets::click(list.window(), text);
                        let page = text.rsplit(": ").next().unwrap();
                        assert_eq!(bound.pages.borrow().shown().name, page, "{label} #{n}");
                    }
                }
            }
        }
        assert_eq!(names(&ui), strings(&case["after"]), "{label}");
        // closed, each page is undoable (the recorded closed indices), and
        // undone they are back as they were
        assert_eq!(
            bound.pages.borrow_mut().closed_names().len(),
            case["closed_indices"].as_array().unwrap().len(),
            "{label}: closed pages"
        );
        while bound.pages.borrow_mut().unclose() {}
        assert_eq!(
            bound.pages.borrow().session().pages,
            original,
            "{label}: undone"
        );
        reasons_dialog::set_yes_delay(None);
    }
    assert_eq!(with_dialog, 14, "cases with the dialog");
}

// leaf: audit-options-tabs-context-action-2074-close-other-page-s
#[test]
fn the_dialogs_yes_waits_before_it_can_be_pressed() {
    let recorded = hydrus_testkit::fixture_json("tab_close_objections.json");
    let case = recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["menu"] == "other pages" && c["answer"] == "yes")
        .unwrap();
    let wanted: Vec<bool> = case["asked"][0]["panel"]["buttons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["enabled"].as_bool().unwrap())
        .collect();
    let _windows = headless::init();
    let (_dirs, store) = store();
    let (ui, _bound) = open(&store);
    // a minute: the check cannot be late
    reasons_dialog::set_yes_delay(Some(Duration::from_secs(60)));
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "close other pages");
    let dialog = reasons_dialog::dialog().unwrap();
    widgets::lay_out(dialog.window(), 480.0, 300.0);
    let enabled: Vec<bool> = (0..3)
        .map(|i| {
            widgets::enabled(
                dialog.window(),
                ["yes", "no", "no, but show me the pages"][i],
            )
        })
        .collect();
    assert_eq!(enabled, wanted, "yes starts disabled");
    // pressing it does nothing
    widgets::click(dialog.window(), "yes");
    assert_eq!(names(&ui).len(), 5);
    assert!(reasons_dialog::dialog().is_some());
    // after a short wait it can be pressed (a wait only bounds from one side)
    dialog.hide().unwrap();
    reasons_dialog::set_yes_delay(Some(Duration::from_millis(30)));
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "close other pages");
    let dialog = reasons_dialog::dialog().unwrap();
    widgets::lay_out(dialog.window(), 480.0, 300.0);
    let start = std::time::Instant::now();
    while !widgets::enabled(dialog.window(), "yes") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "yes never enabled"
        );
        widgets::settle();
    }
    reasons_dialog::set_yes_delay(None);
}
