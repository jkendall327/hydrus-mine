//! Help > debug > data actions that read the world: "scan file storage
//! folders" and "show env", clicked in the real menu on the trees and
//! environments the reference's were recorded on
//! (`oracle/fixtures/debug_scan_storage.json`, `debug_show_env.json`).
use super::debug_menu_actions::start;
use regex::Regex;
use std::path::Path;

fn build(root: &Path, shape: &str, granularity: usize) {
    let dirs = |parent: &Path, n: usize, prefix: &str| {
        for i in 0..n {
            std::fs::create_dir_all(parent.join(format!("{prefix}{i:02}"))).unwrap();
        }
    };
    let files = |parent: &Path, n: usize| {
        for i in 0..n {
            std::fs::write(parent.join(format!("f{i}")), "x").unwrap();
        }
    };
    match shape {
        "three prefixes" => {
            for p in ["fa0", "fa1", "fa2"] {
                let mut path = root.join(p);
                if granularity == 3 {
                    path = path.join(format!("{p}0"));
                }
                std::fs::create_dir_all(path).unwrap();
            }
        }
        "empty" => {}
        "sixteen files" => files(root, 16),
        "seventeen files" => files(root, 17),
        "six hundred dirs" => dirs(root, 600, "d"),
        "six hundred and one dirs" => dirs(root, 601, "d"),
        "twenty second level dirs" => dirs(&root.join("aa"), 20, "b"),
        "twenty-one second level dirs" => dirs(&root.join("aa"), 21, "b"),
        "seventeen second level files" => {
            std::fs::create_dir_all(root.join("aa")).unwrap();
            files(&root.join("aa"), 17);
        }
        other => panic!("no shape {other:?}"),
    }
}

// leaf: audit-options-help-debug-action-scan-file-storage-folders
#[test]
fn scan_file_storage_folders_says_what_the_reference_says_of_each_tree() {
    let d = start();
    let recording = hydrus_testkit::fixture_json("debug_scan_storage.json");
    let granularity = d.store.snapshot().storage.granularity();
    assert!(granularity == 2 || granularity == 3);
    let took = Regex::new(r" and took .*$").unwrap();
    let mut done = 0;
    for case in recording["cases"].as_array().unwrap() {
        if case["granularity"].as_u64().unwrap() != granularity as u64 {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        build(dir.path(), case["shape"].as_str().unwrap(), granularity);
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        hydrus_gui::set_picker({
            let asked = asked.clone();
            let path = dir.path().to_owned();
            move |kind, title| {
                asked.borrow_mut().push((kind, title.to_owned()));
                vec![path.clone()]
            }
        });
        let before = d.jobs().len();
        d.click(&["data actions", "scan file storage folders"]);
        let recorded_pick = &case["picks"][0];
        assert_eq!(
            *asked.borrow(),
            [(
                hydrus_gui::Pick::Folder,
                recorded_pick["message"].as_str().unwrap().to_owned()
            )]
        );
        assert!(recorded_pick["default_path_is_db_dir"].as_bool().unwrap());
        assert_eq!(
            hydrus_gui::last_pick_directory().as_deref(),
            Some(d.store.dir())
        );
        let jobs = d.jobs();
        assert_eq!(jobs.len(), before + 1, "{}", case["shape"]);
        let said = jobs.last().unwrap().status_text_1.clone().unwrap();
        let recorded = case["messages"][0].as_str().unwrap();
        assert_eq!(
            took.replace(&said, ""),
            took.replace(recorded, ""),
            "{} at granularity {granularity}",
            case["shape"]
        );
        done += 1;
    }
    assert_eq!(done, 9);
    // a cancelled picker says nothing, as recorded
    assert!(
        recording["cancelled_messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    hydrus_gui::set_picker(|_, _| Vec::new());
    let before = d.jobs().len();
    d.click(&["data actions", "scan file storage folders"]);
    assert_eq!(d.jobs().len(), before);
}

// leaf: audit-options-help-debug-action-show-env
#[test]
fn show_env_dumps_each_recorded_environment_as_the_reference_does() {
    let d = start();
    let recording = hydrus_testkit::fixture_json("debug_show_env.json");
    for (name, case) in recording["cases"].as_object().unwrap() {
        let vars: Vec<(String, String)> = case["env"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        hydrus_gui::set_environment(Some(vars));
        let before = d.jobs().len();
        d.click(&["data actions", "show env"]);
        let jobs = d.jobs();
        assert_eq!(jobs.len(), before + 1, "{name}");
        assert_eq!(
            jobs.last().unwrap().status_text_1.as_deref(),
            case["messages"][0].as_str(),
            "{name}"
        );
        assert!(
            hydrus_gui::debug_printed().contains(&case["messages"][0].as_str().unwrap().to_owned()),
            "{name} is also printed"
        );
    }
    hydrus_gui::set_environment(None);
    assert_eq!(recording["pathsep"], ":");
}

// leaf: audit-options-help-debug-action-save-last-session-gui-session
#[test]
fn save_last_session_writes_the_pages_that_are_open_now() {
    use hydrus_core::pages::{Page, PageContent, PageKey, Session};
    use hydrus_gui::{MainWindow, Pages, bind, headless};
    use hydrus_store::sessions::{self, LAST_SESSION};
    use slint::ComponentHandle as _;
    let (dirs, store) = super::subscriptions::store();
    let page = |name: &str, content| Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    };
    let search = |name: &str| {
        page(
            name,
            PageContent::Search {
                search: hydrus_search::FileSearchContext::default(),
                synchronised: false,
                sort: None,
                lock: None,
                collect: None,
            },
        )
    };
    // two pages and a notebook with one, as the pages are open
    let open = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("first"),
            page("a notebook", PageContent::Pages(vec![search("inside")])),
            search("last"),
        ],
    };
    store
        .write({
            let open = open.clone();
            move |ctx| sessions::save(ctx.conn(), &open, 1_700_000_000)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // what the reference's "last session" is saved as at exit is lost (a
    // crash): clear it, and the action puts the open pages back at once
    store
        .write(|ctx| sessions::delete(ctx.conn(), LAST_SESSION))
        .unwrap();
    assert!(
        store
            .read(|c| sessions::load(c, LAST_SESSION))
            .unwrap()
            .is_none()
    );
    let d = super::debug_menu_actions::Debug::from_parts(dirs, store.clone(), ui, bound, windows);
    d.click(&["gui actions", "save 'last session' gui session"]);
    let saved = store
        .read(|c| sessions::load(c, LAST_SESSION))
        .unwrap()
        .expect("saved at once");
    let names = |s: &Session| -> Vec<(usize, String)> {
        s.all_pages().iter().map(|p| (0, p.name.clone())).collect()
    };
    assert_eq!(names(&saved), names(&open));
    assert_eq!(saved.pages.len(), 3);
    assert_eq!(
        serde_json::to_value(&saved.pages).unwrap(),
        serde_json::to_value(&open.pages).unwrap(),
        "every page as it was, keys and searches too"
    );
}

// leaf: audit-options-help-debug-action-reset-multi-column-list-settings-to-default
#[test]
fn reset_multi_column_list_settings_asks_the_question_in_the_reference_s_source() {
    let d = start();
    // the question as ClientGUI._DebugResetColumnListManager has it
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../hydrus/client/gui/ClientGUI.py"),
    )
    .unwrap();
    let body = &source[source.find("def _DebugResetColumnListManager").unwrap()..];
    let literal = Regex::new(r"message = '([^']*)'")
        .unwrap()
        .captures(body)
        .unwrap()[1]
        .to_owned();
    d.click(&["gui actions", "reset multi-column list settings to default"]);
    assert_eq!(d.ui.get_question(), literal);
    // yes has nothing to reset: hydrus-rs keeps no column widths
    d.ui.invoke_answer(true);
    assert_eq!(d.ui.get_question(), "");
    d.click(&["gui actions", "reset multi-column list settings to default"]);
    d.ui.invoke_answer(false);
    assert_eq!(d.ui.get_question(), "");
}

// leaf: audit-options-help-debug-action-review-current-network-jobs
#[test]
fn review_current_network_jobs_opens_the_reference_s_jobs_window() {
    use slint::{ComponentHandle as _, Model as _};
    let d = start();
    // the frame the reference titles 'review network jobs' (ClientGUI._ReviewNetworkJobs)
    // and the columns its list registers (recorded in network_data.json)
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../hydrus/client/gui/ClientGUI.py"),
    )
    .unwrap();
    let body = &source[source.find("def _ReviewNetworkJobs").unwrap()..];
    let title = Regex::new(r"FrameThatTakesScrollablePanel\( self, '([^']*)' \)")
        .unwrap()
        .captures(body)
        .unwrap()[1]
        .to_owned();
    let recorded: Vec<String> = hydrus_testkit::fixture_json("network_data.json")["job_columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert!(hydrus_gui::network_data_window::last_jobs().is_none());
    d.click(&["network actions", "review current network jobs"]);
    let window = hydrus_gui::network_data_window::last_jobs().expect("the window opens");
    assert!(window.window().is_visible());
    assert!(
        window.get_composed_title().starts_with(&title),
        "{}",
        window.get_composed_title()
    );
    let columns: Vec<String> = window
        .get_columns()
        .iter()
        .map(|c| c.title.to_string())
        .collect();
    assert_eq!(columns, recorded);
    assert_eq!(window.get_rows().row_count(), 0, "no jobs are running");
}
