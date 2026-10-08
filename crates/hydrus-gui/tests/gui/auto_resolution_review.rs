//! A duplicates auto-resolution rule's "review actions" window, opened
//! from the duplicates page's rules, on the database the reference's run
//! left: approving and denying the pairs waiting on a semi-automatic rule,
//! undoing a denial and undoing an action taken, each step's tabs as the
//! reference's (`oracle/record_auto_resolution_review.py`; what the tabs
//! list as the window opens is tested in hydrus-gui-model).

use std::collections::HashMap;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::{AutoResolutionReviewWindow, Bound, MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::duplicates::auto;
use hydrus_store::sessions::{self, LAST_SESSION};
use serde_json::Value;

/// The rows a tab lists: each pair's files and the text's first line.
pub(crate) fn listed(
    store: &Store,
    rule: i64,
    tab: &str,
    hex: &HashMap<HashId, String>,
) -> Vec<(String, String)> {
    let pairs: Vec<(HashId, HashId)> = store
        .read(|c| {
            Ok(match tab {
                "pending" => auto::pending_pairs(c, rule, None)?
                    .into_iter()
                    .map(|(_, a, b)| (a, b))
                    .collect(),
                "actioned" => auto::actioned(c, rule, None)?
                    .into_iter()
                    .map(|(a, b, _, _)| (a, b))
                    .collect(),
                _ => auto::denied_pairs(c, rule, None)?
                    .into_iter()
                    .map(|d| d.kings)
                    .collect(),
            })
        })
        .unwrap();
    pairs
        .into_iter()
        .map(|(a, b)| (hex[&a].clone(), hex[&b].clone()))
        .collect()
}

fn recorded_pairs(state: &Value, tab: &str) -> Vec<(String, String)> {
    state[tab]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().to_owned(),
                r[1].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn texts(window: &AutoResolutionReviewWindow) -> Vec<String> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .map(|r| rows.row_data(r).unwrap().text.to_string())
        .collect()
}

/// The window opened on the rule named, by double-clicking it.
pub(crate) fn review(ui: &MainWindow, bound: &Bound, name: &str) -> AutoResolutionReviewWindow {
    let listed = ui.get_duplicates_rules();
    let row = (0..listed.row_count())
        .position(|r| listed.row_data(r).unwrap().cells.row_data(0).unwrap() == name)
        .unwrap();
    ui.invoke_duplicates_action("rule".into(), i32::try_from(row).unwrap(), false, false);
    ui.invoke_duplicates_action(
        "review rule".into(),
        i32::try_from(row).unwrap(),
        false,
        false,
    );
    let reviews = bound.auto_resolution_reviews.borrow();
    reviews.last().expect("it opens").1.clone_strong()
}

/// The steps' files in the trash.
fn trashed(store: &Store, hexes: &[String], ids: &HashMap<String, HashId>) -> Vec<String> {
    let trash = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
        .unwrap()
        .trash;
    let files: Vec<HashId> = hexes.iter().map(|h| ids[h]).collect();
    let current = store
        .read(|c| hydrus_store::media::current_domains(c, &files))
        .unwrap();
    let mut out: Vec<String> = hexes
        .iter()
        .filter(|h| current.get(&ids[*h]).is_some_and(|d| d.contains(&trash)))
        .cloned()
        .collect();
    out.sort();
    out
}

/// The reference's run's database, opened with a duplicates page, and each
/// file's hash in hex, and back.
pub(crate) struct Opened {
    pub(crate) _dir: tempfile::TempDir,
    pub(crate) store: Arc<Store>,
    pub(crate) ui: MainWindow,
    pub(crate) bound: Bound,
    pub(crate) hex: HashMap<HashId, String>,
    pub(crate) ids: HashMap<String, HashId>,
}

pub(crate) fn opened() -> Opened {
    let legacy = hydrus_testkit::legacy_fixture("auto_resolution");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hex: HashMap<HashId, String> = store
        .read(|c| {
            let ids: Vec<HashId> = c
                .prepare("SELECT hash_id FROM hashes")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            hydrus_store::master::hashes(c, &ids)
        })
        .unwrap()
        .into_iter()
        .map(|(id, h)| (id, h.to_hex()))
        .collect();
    let ids: HashMap<String, HashId> = hex.iter().map(|(id, h)| (h.clone(), *id)).collect();
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: FileSearchContext::default(),
                    search_2: FileSearchContext::default(),
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    Opened {
        _dir: dir,
        store,
        ui,
        bound,
        hex,
        ids,
    }
}

// leaf: audit-media-review-history
#[test]
fn pairs_are_approved_denied_and_undone_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let windows = headless::init();
    let Opened {
        _dir,
        store,
        ui,
        bound,
        hex,
        ids,
    } = opened();
    let rules = store.read(auto::rules).unwrap();
    let rule_id = |name: &str| rules.iter().find(|(_, r)| r.name == name).unwrap().0;
    let reviewed = recorded["reviewed"].as_str().unwrap();
    let undone = recorded["undone"].as_str().unwrap();
    let steps = recorded["steps"].as_array().unwrap();

    // a semi-automatic rule opens on its pending pairs
    let window = review(&ui, &bound, reviewed);
    assert_eq!(window.get_rule_name(), reviewed);
    assert_eq!(
        window.get_window_title(),
        "review duplicate auto-resolution actions"
    );
    assert_eq!(window.get_rule_name(), reviewed);
    assert_eq!(window.get_tab(), 0);
    assert_eq!(window.get_label(), "Found 2 pairs.");
    assert!(!window.get_can_act());
    let check = |step: &Value, rule: i64, window: &AutoResolutionReviewWindow| {
        let what = step["step"].as_str().unwrap();
        let state = &step["state"];
        for tab in ["pending", "actioned", "denied"] {
            let mut ours = listed(&store, rule, tab, &hex);
            let mut theirs = recorded_pairs(state, tab);
            if tab == "pending" {
                ours.sort();
                theirs.sort();
            }
            assert_eq!(ours, theirs, "{what}: {tab}");
        }
        let mut files: Vec<String> = ["pending", "actioned", "denied"]
            .iter()
            .flat_map(|t| recorded_pairs(state, t))
            .flat_map(|(a, b)| [a, b])
            .collect();
        files.sort();
        files.dedup();
        assert_eq!(
            trashed(&store, &files, &ids),
            strings(&step["trashed"]),
            "{what}"
        );
        let _ = window;
    };

    // approve the pair the reference approved (the newest action taken
    // after; the reference lists pending pairs in its table's order, ours
    // by their groups): one left, selected
    let approved = recorded_pairs(&steps[0]["state"], "actioned")[0].clone();
    let ours = listed(&store, rule_id(reviewed), "pending", &hex);
    let row = i32::try_from(ours.iter().position(|p| *p == approved).unwrap()).unwrap();
    window.invoke_row_clicked(row, false, false);
    assert!(window.get_can_act());
    window.invoke_approve();
    assert!(!window.get_asking());
    // (approving works off the UI thread, showing its progress)
    settle(&window);
    assert_eq!(window.get_label(), "1 pairs remaining.");
    assert_eq!(window.get_rows().row_count(), 1);
    assert!(window.get_rows().row_data(0).unwrap().selected);
    check(&steps[0], rule_id(reviewed), &window);
    // the action taken is listed, newest first, once its tab is shown
    window.invoke_tab_chosen(1);
    assert_eq!(window.get_label(), "Found 2 pairs.");
    assert!(texts(&window)[0].starts_with("set as duplicates--A better\n"));
    assert!(texts(&window)[0].ends_with("\nnow"), "{:?}", texts(&window));

    // deny the other: none left, so fetched again
    window.invoke_tab_chosen(0);
    window.invoke_row_clicked(0, false, false);
    window.invoke_deny();
    settle(&window);
    assert_eq!(window.get_label(), "Found 0 pairs.");
    check(&steps[1], rule_id(reviewed), &window);
    window.invoke_tab_chosen(2);
    assert_eq!(window.get_label(), "Found 2 pairs.");

    // undo the newest denial, asking first
    window.invoke_row_clicked(0, false, false);
    window.invoke_undo();
    assert_eq!(window.get_asking_message(), step_asked(&steps[2]));
    window.invoke_chosen(0);
    assert_eq!(window.get_label(), "Found 1 pairs.");
    check(&steps[2], rule_id(reviewed), &window);
    window.invoke_close_window();
    assert!(bound.auto_resolution_reviews.borrow().is_empty());

    // a fully automatic rule opens on its actions taken; undo the newest
    let window = review(&ui, &bound, undone);
    assert_eq!(window.get_rule_name(), undone);
    assert_eq!(window.get_tab(), 1);
    assert_eq!(window.get_label(), "Found 13 pairs.");
    window.invoke_tab_chosen(0);
    assert_eq!(
        window.get_label(),
        "This rule is fully automatic; it will not wait for human approval."
    );
    window.invoke_tab_chosen(1);
    window.invoke_row_clicked(0, false, false);
    window.invoke_undo();
    assert_eq!(window.get_asking_message(), step_asked(&steps[3]));
    // (the newest window)
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 760, 640);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("auto_resolution_review.png");
    headless::save_png(&shot, &pixels, 760, 640).unwrap();
    window.invoke_chosen(0);
    check(&steps[3], rule_id(undone), &window);
    // (the log keeps its entries)
    assert_eq!(window.get_rows().row_count(), 13);
    // editing the rules closes the reviews
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    assert!(bound.auto_resolution_reviews.borrow().is_empty());
}

fn step_asked(step: &Value) -> &str {
    step["asked"][0].as_str().unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_pending_pair_is_approved_in_the_duplicate_filter() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let windows = headless::init();
    let Opened {
        _dir,
        store,
        ui,
        bound,
        hex,
        ..
    } = opened();
    let rules = store.read(auto::rules).unwrap();
    let reviewed = recorded["reviewed"].as_str().unwrap();
    let rule = rules.iter().find(|(_, r)| r.name == reviewed).unwrap().0;
    let steps = recorded["steps"].as_array().unwrap();
    let window = review(&ui, &bound, reviewed);

    // the pair the reference approved, double-clicked: the filter opens on
    // the pending pairs from it, with approve and deny
    let approved = recorded_pairs(&steps[0]["state"], "actioned")[0].clone();
    let ours = listed(&store, rule, "pending", &hex);
    let row = ours.iter().position(|p| *p == approved).unwrap();
    window.invoke_row_activated(i32::try_from(row).unwrap());
    let filter = bound
        .auto_resolution_review_filter
        .borrow()
        .as_ref()
        .expect("the filter opened")
        .clone_strong();
    assert!(filter.get_reviewing());
    assert!(
        filter.get_index_text().starts_with("File One - 1/2"),
        "{}",
        filter.get_index_text()
    );
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 1000, 700);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("review_filter.png");
    headless::save_png(&shot, &pixels, 1000, 700).unwrap();
    // approved, whichever file is shown, then closed, committing
    filter.invoke_switch_media();
    filter.invoke_decide("approve".into());
    for _ in 0..5 {
        if bound.auto_resolution_review_filter.borrow().is_none() {
            break;
        }
        if filter.get_question().is_empty() {
            filter.invoke_close_requested();
        } else {
            filter.invoke_answer(0);
        }
    }
    assert!(bound.auto_resolution_review_filter.borrow().is_none());
    // the rule's action taken, as the reference's approval was, and the
    // review fetched again
    assert_eq!(
        listed(&store, rule, "pending", &hex),
        recorded_pairs(&steps[0]["state"], "pending")
    );
    assert_eq!(
        listed(&store, rule, "actioned", &hex),
        recorded_pairs(&steps[0]["state"], "actioned")
    );
    assert_eq!(window.get_label(), "Found 1 pairs.");

    // an actioned pair: the media viewer on its files still stored (the
    // worse one was deleted to the trash, which is still stored)
    window.invoke_tab_chosen(1);
    assert!(bound.viewer.borrow().is_none());
    window.invoke_row_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .expect("the viewer opened")
        .clone_strong();
    viewer.hide().unwrap();

    // and its files in a new page, from the right-click menu, the trashed
    // one too
    assert_eq!(window.get_menu_label(), "");
    window.invoke_row_clicked(0, false, false);
    assert_eq!(window.get_menu_label(), "show selected row in a new page");
    let pages_before = bound.pages.borrow().open_pages().len();
    window.invoke_show_in_page();
    assert_eq!(bound.pages.borrow().open_pages().len(), pages_before + 1);
    let mut shown: Vec<String> = bound
        .current
        .borrow()
        .borrow()
        .files()
        .iter()
        .map(|f| hex[f].clone())
        .collect();
    shown.sort();
    let mut pair = [approved.0, approved.1];
    pair.sort();
    assert_eq!(shown, pair);
}

/// Let an approval or denial at work finish and reach the window.
pub(crate) fn settle(window: &hydrus_gui::AutoResolutionReviewWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        slint::platform::update_timers_and_animations();
        if !window.get_working() && window.get_approve_text() == "approve" {
            // (one more pass, in case the work had not yet been noticed)
            std::thread::sleep(std::time::Duration::from_millis(150));
            slint::platform::update_timers_and_animations();
            if !window.get_working() {
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the decisions never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_failed_denial_keeps_pending_pairs_and_reports_the_error() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let _windows = headless::init();
    let opened = opened();
    let name = recorded["reviewed"].as_str().unwrap();
    let window = review(&opened.ui, &opened.bound, name);
    let before = window.get_rows().row_count();
    assert!(before > 0);
    opened.store.write(|ctx| {
        ctx.conn().execute_batch("CREATE TRIGGER reject_denial BEFORE UPDATE ON dup_auto_pairs BEGIN SELECT RAISE(ABORT, 'injected denial failure'); END;")?;
        Ok(())
    }).unwrap();
    window.invoke_row_clicked(0, false, false);
    window.invoke_deny();
    settle(&window);
    assert_eq!(window.get_rows().row_count(), before);
    assert!(window.get_label().contains("injected denial failure"));
    window.invoke_close_window();
}

/// Keep the store's writer busy until the returned function is called, so
/// that a worker's next write waits.
fn hold_writer(store: &Arc<Store>) -> impl FnOnce() {
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let store = Arc::clone(store);
    let holder = std::thread::spawn(move || {
        store
            .write(move |_| {
                held_tx.send(()).unwrap();
                let _ = go_rx.recv();
                Ok(())
            })
            .unwrap();
    });
    held_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the writer was taken");
    move || {
        go_tx.send(()).unwrap();
        holder.join().unwrap();
    }
}

fn spin_timers_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !ready() {
        assert!(std::time::Instant::now() < deadline, "{what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Every state a popup passed through, as the store held it.
fn popup_states(store: &Store) -> Vec<String> {
    store
        .read(|c| {
            Ok(c.prepare("SELECT what || ' ' || job FROM popup_log ORDER BY rowid")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap()
}

// leaf: audit-media-review-progress
#[test]
fn approving_and_denying_show_their_progress_on_the_button_and_in_a_popup_after_a_delay() {
    use hydrus_gui_model::auto_resolution_review::{POPUP_AFTER, set_popup_after};
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let _windows = headless::init();
    let o = opened();
    let name = recorded["reviewed"].as_str().unwrap();
    let window = review(&o.ui, &o.bound, name);
    assert_eq!(window.get_label(), "Found 2 pairs.");
    assert_eq!(POPUP_AFTER, std::time::Duration::from_secs(4));
    o.store
        .write(|ctx| {
            ctx.conn().execute_batch(
                "CREATE TABLE popup_log (what TEXT, job TEXT);
                 CREATE TRIGGER popup_added AFTER INSERT ON popups
                   BEGIN INSERT INTO popup_log VALUES ('added', NEW.job); END;
                 CREATE TRIGGER popup_changed AFTER UPDATE ON popups
                   BEGIN INSERT INTO popup_log VALUES ('changed', NEW.job); END;
                 CREATE TRIGGER popup_removed AFTER DELETE ON popups
                   BEGIN INSERT INTO popup_log VALUES ('removed', OLD.job); END;",
            )?;
            Ok(())
        })
        .unwrap();

    // approving the first pair: the button counts it while the work is held
    // up, and a quick job (four seconds are not up) makes no popup
    window.invoke_row_clicked(0, false, false);
    assert_eq!(window.get_approve_text(), "approve");
    let release = hold_writer(&o.store);
    window.invoke_approve();
    spin_timers_until("the button never showed the progress", || {
        window.get_approve_text() == "approving: 0/1"
    });
    assert!(window.get_working());
    assert_eq!(window.get_deny_text(), "deny");
    release();
    settle(&window);
    assert_eq!(window.get_label(), "Found 1 pairs.");
    assert!(popup_states(&o.store).is_empty());

    // denying the other, with the popup due at once: its title, its
    // progress, then it is finished and dismissed
    set_popup_after(Some(std::time::Duration::ZERO));
    window.invoke_row_clicked(0, false, false);
    let release = hold_writer(&o.store);
    window.invoke_deny();
    spin_timers_until("the button never showed the progress", || {
        window.get_deny_text() == "denying: 0/1"
    });
    assert!(window.get_working());
    assert_eq!(window.get_approve_text(), "approve");
    release();
    settle(&window);
    set_popup_after(None);
    assert_eq!(window.get_label(), "Found 0 pairs.");
    let states = popup_states(&o.store);
    assert_eq!(states.len(), 4, "{states:?}");
    assert!(states[0].starts_with("added ") && states[0].contains("denying auto-resolution decisions"));
    assert!(states[1].starts_with("changed ") && states[1].contains("denying: 0/1"));
    assert!(states[2].starts_with("changed ") && states[2].contains("\"dismissed\":true"));
    assert!(states[3].starts_with("removed "));
}
