//! Database > locations…, from the real menu (`MoveMediaFilesPanel`): the
//! list of media locations with weights and limits, the thumbnail location
//! override, "move files now" with its run-time question, and the
//! granularity window (this client, and an offline folder).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::database_menu_jobs::{from_menu_path, pump, shown};
use hydrus_gui::database_locations_window::{max_size_window, opened, runtime_question};
use hydrus_gui::{
    DatabaseLocationsWindow, MainWindow, Pages, SearchPage, bind, headless, message_window,
};
use hydrus_gui_model::database_locations as model;
use hydrus_store::Store;
use hydrus_store::storage_locations as locations;
use slint::{ComponentHandle as _, Model as _};

struct Rig {
    home: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    _bound: hydrus_gui::Bound,
    _windows: headless::Windows,
    /// What the folder picker answers next, and what it was asked.
    next: Rc<RefCell<Vec<PathBuf>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

fn rig() -> Rig {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    std::fs::create_dir_all(home.path().join("client_files")).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let next: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let (next, asked) = (next.clone(), asked.clone());
        hydrus_gui::set_picker(move |_, title| {
            asked.borrow_mut().push(title.to_owned());
            std::mem::take(&mut *next.borrow_mut())
        });
    }
    Rig {
        home,
        store,
        ui,
        _bound: bound,
        _windows: windows,
        next,
        asked,
    }
}

impl Rig {
    fn open(&self) -> DatabaseLocationsWindow {
        from_menu_path(&self.ui, &[], "locations\u{2026}");
        let window = opened().expect("the window opened");
        assert!(window.window().is_visible());
        window
    }
    fn review(&self) -> locations::Review {
        self.store.read(locations::review).unwrap()
    }
    fn weight_of(&self, path: &Path) -> Option<i64> {
        self.review()
            .locations
            .iter()
            .find(|l| l.path == path)
            .map(|l| l.weight)
    }
    /// Pick this folder for the next action.
    fn pick(&self, path: &Path) {
        *self.next.borrow_mut() = vec![path.to_path_buf()];
    }
    /// Answer the main window's question.
    fn answer(&self, yes: bool) -> String {
        let question = self.ui.get_question().to_string();
        assert!(!question.is_empty(), "a question is asked");
        self.ui.invoke_answer(yes);
        question
    }
}

fn cells(window: &DatabaseLocationsWindow) -> Vec<Vec<String>> {
    window
        .get_rows()
        .iter()
        .map(|row| row.cells.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn select(window: &DatabaseLocationsWindow, path: &Path) {
    let at = cells(window)
        .iter()
        .position(|row| row[0].ends_with(&path.display().to_string()))
        .expect("the location is listed");
    window.invoke_clicked(i32::try_from(at).unwrap(), false, false);
}

fn warning(text: &str) {
    let window = message_window().expect("a warning is shown");
    assert_eq!(window.get_message(), text);
    window.invoke_cancelled();
}

/// The paths a recorded step names, and what the recorder called them.
struct Names(Vec<(PathBuf, &'static str)>);

impl Names {
    fn norm(&self, text: &str) -> String {
        let mut pairs: Vec<_> = self.0.iter().collect();
        pairs.sort_by_key(|(path, _)| std::cmp::Reverse(path.display().to_string().len()));
        let mut text = text.to_owned();
        for (path, name) in pairs {
            text = text.replace(&path.display().to_string(), &format!("<{name}>"));
        }
        text
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.iter().find(|(_, n)| *n == name).unwrap().0.clone()
    }
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

/// Every step of the reference's panel (oracle/record_locations_panel.py) replayed through the
/// real window: the same selection, pick and answers, then the dialogs, list, buttons and saved
/// locations compared. Returns the differences found, step by step.
fn replay_locations_panel(
    rig: &Rig,
    names: &Names,
    window: &DatabaseLocationsWindow,
) -> Vec<String> {
    let recording = hydrus_testkit::fixture_json("locations_panel.json");
    let mut differences = Vec::new();
    for step in recording["steps"].as_array().unwrap() {
        let name = step["step"].as_str().unwrap();
        let inputs = &step["inputs"];
        let mut yesno = inputs["yesno"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| a.as_bool().unwrap());
        let mut yesyesno = inputs["yesyesno"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| a.as_str().map(str::to_owned));
        let mut edit = inputs["edit"].as_array().into_iter().flatten();
        *rig.next.borrow_mut() = inputs["pick"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(|n| names.path(n)))
            .collect();
        // the action: ["select", name] and/or the button's
        let mut words = step["action"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w.as_str().unwrap());
        let mut diff = |what: String| differences.push(format!("{name}: {what}"));
        while let Some(word) = words.next() {
            match word {
                "open" => {}
                "select" => select(window, &names.path(words.next().unwrap())),
                other => window.invoke_action(other.into()),
            }
        }
        // the dialogs, in order
        for dialog in step["dialogs"].as_array().unwrap() {
            let wanted = |i: usize| dialog[i].as_str().unwrap_or_default().to_owned();
            match dialog[0].as_str().unwrap() {
                "pick directory" => {
                    if rig.asked.borrow().last().map(String::as_str) != Some(wanted(1).as_str()) {
                        diff(format!("picker title {:?}", rig.asked.borrow().last()));
                    }
                }
                "warning" => match message_window().filter(|w| w.window().is_visible()) {
                    Some(w) => {
                        let text = names.norm(&w.get_message());
                        if text != wanted(1) {
                            diff(format!("warning {text:?}, not {:?}", wanted(1)));
                        }
                        w.invoke_cancelled();
                    }
                    None => diff(format!("no warning {:?}", wanted(1))),
                },
                "question" => {
                    let question = rig.ui.get_question().to_string();
                    if question.is_empty() {
                        diff(format!("no question {:?}", wanted(1)));
                    } else {
                        if names.norm(&question) != wanted(1) {
                            diff(format!("question {question:?}, not {:?}", wanted(1)));
                        }
                        rig.ui.invoke_answer(yesno.next().unwrap());
                    }
                }
                "choice" => match runtime_question() {
                    Some(q) if q.window().is_visible() => {
                        if q.get_message() != wanted(1).as_str() {
                            diff("run time question".into());
                        }
                        let choices: Vec<String> =
                            q.get_choices().iter().map(|c| c.to_string()).collect();
                        if choices != strings(&dialog[2]) || q.get_no_label() != wanted(3).as_str()
                        {
                            diff(format!("run time choices {choices:?}"));
                        }
                        match yesyesno.next().unwrap() {
                            None => q.invoke_cancelled(),
                            Some(label) => q.invoke_chosen(
                                i32::try_from(choices.iter().position(|c| *c == label).unwrap())
                                    .unwrap(),
                            ),
                        }
                    }
                    _ => diff("no run time question".into()),
                },
                "dialog" => {
                    let entry = &dialog[1];
                    let accepted = entry["accepted"].as_bool().unwrap();
                    if entry["title"] == "edit max size" {
                        let Some(d) = max_size_window().filter(|d| d.window().is_visible()) else {
                            diff("no max size dialog".into());
                            continue;
                        };
                        if d.get_window_title() != entry["title"].as_str().unwrap() {
                            diff(format!("title {:?}", d.get_window_title()));
                        }
                        if d.get_message() != entry["message"].as_str().unwrap() {
                            diff("max size message".into());
                        }
                        let initial = entry["initial"].as_i64();
                        let shown = (!d.get_no_limit()).then(|| {
                            i64::from(d.get_amount())
                                * 1024_i64.pow(u32::try_from(d.get_unit()).unwrap())
                        });
                        if initial != shown {
                            diff(format!("max size started at {shown:?}, not {initial:?}"));
                        }
                        edit.next();
                        if accepted {
                            match entry["entered"].as_i64() {
                                Some(bytes) => {
                                    let mut unit = 0;
                                    let mut amount = bytes;
                                    while unit < 4 && amount % 1024 == 0 && amount >= 1024 {
                                        amount /= 1024;
                                        unit += 1;
                                    }
                                    d.set_no_limit(false);
                                    d.set_amount(i32::try_from(amount).unwrap());
                                    d.set_unit(unit);
                                }
                                None => d.set_no_limit(true),
                            }
                            d.invoke_apply();
                        } else {
                            d.invoke_cancel();
                        }
                    } else {
                        let Some(d) =
                            hydrus_gui::database_locations_window::custom_runtime_window()
                                .filter(|d| d.window().is_visible())
                        else {
                            diff("no custom time dialog".into());
                            continue;
                        };
                        // (minutes, not hours and minutes)
                        let initial = entry["initial"].as_i64().unwrap() / 60;
                        if i64::from(d.get_value()) != initial {
                            diff(format!("custom time started at {} minutes", d.get_value()));
                        }
                        if accepted {
                            d.set_value(
                                i32::try_from(entry["entered"].as_i64().unwrap() / 60).unwrap(),
                            );
                            d.invoke_apply();
                        } else {
                            d.invoke_cancel();
                        }
                    }
                }
                "closed the panel" => {
                    if window.window().is_visible() {
                        diff("the window stayed open".into());
                    }
                }
                other => panic!("{other}"),
            }
        }
        if window.window().is_visible() {
            // the state
            let mut rows: Vec<Vec<String>> = cells(window)
                .into_iter()
                .map(|mut row| {
                    row[0] = names.norm(&row[0]);
                    if row[2] != "DOES NOT EXIST" && row[2] != "unknown" {
                        row[2] = "<free space>".into();
                    }
                    row
                })
                .collect();
            rows.sort();
            let mut wanted: Vec<Vec<String>> = step["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(strings)
                .collect();
            wanted.sort();
            if rows != wanted {
                diff(format!("rows {rows:?}, not {wanted:?}"));
            }
            let buttons: Vec<bool> = window.get_buttons().iter().collect();
            let wanted_buttons: Vec<bool> = step["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_bool().unwrap())
                .collect();
            if buttons != wanted_buttons {
                diff(format!("buttons {buttons:?}, not {wanted_buttons:?}"));
            }
            let thumbs = (
                names.norm(&window.get_thumbnail_location()),
                window.get_can_clear_thumbnails(),
            );
            if thumbs
                != (
                    step["thumbnails"][0].as_str().unwrap().to_owned(),
                    step["thumbnails"][1].as_bool().unwrap(),
                )
            {
                diff(format!("thumbnail location {thumbs:?}"));
            }
            if window.get_granularity() != step["granularity"].as_str().unwrap() {
                diff(format!("granularity {:?}", window.get_granularity()));
            }
            let rebalance = (
                window.get_rebalance_status().to_string(),
                window.get_can_rebalance(),
            );
            if rebalance
                != (
                    step["rebalance"][0].as_str().unwrap().to_owned(),
                    step["rebalance"][1].as_bool().unwrap(),
                )
            {
                diff(format!("rebalance {rebalance:?}"));
            }
        }
        let review = rig.review();
        let mut saved: Vec<(String, i64, Option<i64>)> = review
            .locations
            .iter()
            .filter(|l| l.weight > 0)
            .map(|l| {
                (
                    names.norm(&l.path.display().to_string()),
                    l.weight,
                    l.max_bytes,
                )
            })
            .collect();
        saved.sort();
        let mut wanted: Vec<(String, i64, Option<i64>)> = step["saved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l[0].as_str().unwrap().to_owned(),
                    l[1].as_i64().unwrap(),
                    l[2].as_i64(),
                )
            })
            .collect();
        wanted.sort();
        if saved != wanted {
            diff(format!("saved {saved:?}, not {wanted:?}"));
        }
        let thumbs = review
            .locations
            .iter()
            .find(|l| l.thumbnail_override)
            .map(|l| names.norm(&l.path.display().to_string()));
        if thumbs.as_deref() != step["saved_thumbnails"].as_str() {
            diff(format!("saved thumbnail location {thumbs:?}"));
        }
        // (the unused edit answers are the reference's own record of a dialog it did not open)
        let _ = edit;
    }
    differences
}

#[test]
fn the_locations_panel_replays_the_reference_s_steps() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    let scratch = tempfile::tempdir().unwrap();
    let mut pairs = vec![
        (media.clone(), "default"),
        (rig.home.path().to_path_buf(), "db"),
    ];
    for name in ["a", "b", "thumbs"] {
        let path = scratch.path().join(name);
        std::fs::create_dir(&path).unwrap();
        pairs.push((path, name));
    }
    pairs.push((scratch.path().join("missing"), "missing"));
    let names = Names(pairs);
    let window = rig.open();
    // What differs on purpose (DIFFERENCES.md, Database > locations): the reference warns that
    // the last location cannot be removed and then asks anyway (it does not return); and after
    // a removal its list keeps a row selected where the native list selects none.
    let unexplained: Vec<_> = replay_locations_panel(&rig, &names, &window)
        .into_iter()
        .filter(|d| {
            !(d.starts_with("remove the last location: no question")
                || (d.starts_with("remove the missing path, yes: buttons")
                    || d.starts_with("move files now, forget it: buttons")))
        })
        .collect();
    assert!(unexplained.is_empty(), "{}", unexplained.join("\n"));
}

// leaf: audit-media-database-locations-paths
#[test]
fn locations_lists_weights_and_adds_changes_and_removes_them_with_the_reference_s_guards() {
    let rig = rig();
    let window = rig.open();
    assert_eq!(window.get_warning(), model::WARNING);
    let media = rig.home.path().join("client_files");
    let rows = cells(&window);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], media.display().to_string());
    assert_eq!(rows[0][1], "yes", "beneath the db dir");
    assert_eq!(rows[0][4], "1");
    // nothing selected: nothing to change
    assert_eq!(window.get_buttons().iter().collect::<Vec<_>>(), [false; 4]);

    // adding a location gives it weight 1, and it is listed with its share
    let other = tempfile::tempdir().unwrap();
    rig.pick(other.path());
    window.invoke_action("add".into());
    assert_eq!(rig.asked.borrow().last().unwrap(), model::PICK_LOCATION);
    assert_eq!(rig.weight_of(other.path()), Some(1));
    let rows = cells(&window);
    assert_eq!(rows.len(), 2);
    let new = rows
        .iter()
        .find(|r| r[0] == other.path().display().to_string())
        .unwrap();
    assert_eq!((new[1].as_str(), new[4].as_str()), ("no", "1"));
    // ... once; entering it again is told so
    rig.pick(other.path());
    window.invoke_action("add".into());
    warning(model::ALREADY_ENTERED);
    // cancelling the picker adds nothing
    window.invoke_action("add".into());
    assert_eq!(rig.review().locations.len(), 2);

    // weights go up and down; down from 1 asks to remove
    select(&window, other.path());
    assert_eq!(
        window.get_buttons().iter().collect::<Vec<_>>(),
        [true, true, true, true]
    );
    window.invoke_action("increase".into());
    assert_eq!(rig.weight_of(other.path()), Some(2));
    window.invoke_action("decrease".into());
    assert_eq!(rig.weight_of(other.path()), Some(1));
    window.invoke_action("decrease".into());
    assert_eq!(
        rig.answer(false),
        model::remove_question(
            true,
            rig.review()
                .locations
                .iter()
                .any(|l| l.path == other.path() && l.files_share > 0.0)
        )
    );
    assert_eq!(rig.weight_of(other.path()), Some(1));
    window.invoke_action("remove".into());
    rig.answer(true);
    assert!(
        rig.weight_of(other.path()).is_none_or(|w| w == 0),
        "removed from the weighted locations"
    );

    // the last weighted location cannot be removed
    select(&window, &media);
    window.invoke_action("remove".into());
    warning(model::CANNOT_EMPTY_ALL);
    assert_eq!(rig.weight_of(&media), Some(1));
    // kept across a restart
    drop(window);
    let reopened = Store::open(rig.home.path()).unwrap();
    assert_eq!(
        reopened
            .read(locations::review)
            .unwrap()
            .locations
            .iter()
            .find(|l| l.path == media)
            .map(|l| l.weight),
        Some(1)
    );
}

// leaf: audit-media-database-locations-max-size
#[test]
fn a_location_s_maximum_size_is_edited_in_its_own_dialog() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    let other = tempfile::tempdir().unwrap();
    let path = other.path().to_path_buf();
    rig.store
        .write_and_refresh(move |ctx| locations::add_location(ctx.conn(), &path))
        .unwrap();
    let window = rig.open();

    // two unlimited locations: either can be limited
    select(&window, other.path());
    window.invoke_action("max".into());
    let dialog = max_size_window().expect("the dialog opened");
    assert!(dialog.window().is_visible());
    assert_eq!(dialog.get_window_title(), model::MAX_SIZE_TITLE);
    assert_eq!(dialog.get_message(), model::MAX_SIZE_MESSAGE);
    assert!(dialog.get_no_limit(), "unlimited to begin with");

    // cancelling changes nothing
    dialog.invoke_cancel();
    assert!(!dialog.window().is_visible());
    assert_eq!(
        rig.review()
            .locations
            .iter()
            .map(|l| l.max_bytes)
            .collect::<Vec<_>>(),
        [None, None]
    );

    // 5 GB (unit 3) is kept, shown in the list, and survives a restart
    window.invoke_action("max".into());
    let dialog = max_size_window().unwrap();
    dialog.set_no_limit(false);
    dialog.set_amount(5);
    dialog.set_unit(3);
    dialog.invoke_apply();
    assert!(!dialog.window().is_visible());
    let five = 5 * 1024_i64.pow(3);
    let limit = |store: &Store| {
        store
            .read(locations::review)
            .unwrap()
            .locations
            .into_iter()
            .find(|l| l.path == other.path())
            .unwrap()
            .max_bytes
    };
    assert_eq!(limit(&rig.store), Some(five));
    let row = cells(&window)
        .into_iter()
        .find(|r| r[0] == other.path().display().to_string())
        .unwrap();
    assert_eq!(
        row[5],
        hydrus_core::numbers::human_bytes(u64::try_from(five).unwrap())
    );
    assert_eq!(limit(&Store::open(rig.home.path()).unwrap()), Some(five));

    // activating the row opens it with the saved value, and "no limit" clears it
    window.invoke_activated(
        i32::try_from(
            cells(&window)
                .iter()
                .position(|r| r[0] == other.path().display().to_string())
                .unwrap(),
        )
        .unwrap(),
    );
    let dialog = max_size_window().unwrap();
    assert!(!dialog.get_no_limit());
    assert_eq!((dialog.get_amount(), dialog.get_unit()), (5, 3));
    dialog.set_no_limit(true);
    dialog.invoke_apply();
    assert_eq!(limit(&rig.store), None);

    // with one limited, the other must stay unlimited: no dialog to set it
    select(&window, &media);
    window.invoke_action("max".into());
    let dialog = max_size_window().unwrap();
    dialog.set_no_limit(false);
    dialog.set_amount(5);
    dialog.set_unit(3);
    dialog.invoke_apply();
    select(&window, other.path());
    assert!(!window.get_buttons().row_data(2).unwrap());
    select(&window, &media);
    assert!(window.get_buttons().row_data(2).unwrap());
}

// leaf: audit-media-database-locations-thumbnails
#[test]
fn the_thumbnail_location_override_is_set_and_cleared() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    let window = rig.open();
    assert_eq!(window.get_thumbnail_location(), "none set");
    assert!(!window.get_can_clear_thumbnails());

    // a regular file location cannot be the thumbnail location
    rig.pick(&media);
    window.invoke_action("set thumbnails".into());
    assert_eq!(rig.asked.borrow().last().unwrap(), model::PICK_THUMBNAILS);
    warning(model::IS_FILE_LOCATION);
    assert_eq!(window.get_thumbnail_location(), "none set");

    // another folder can
    let thumbs = tempfile::tempdir().unwrap();
    rig.pick(thumbs.path());
    window.invoke_action("set thumbnails".into());
    assert_eq!(
        window.get_thumbnail_location(),
        thumbs.path().display().to_string()
    );
    assert!(window.get_can_clear_thumbnails());
    let overridden = |store: &Store| {
        store
            .read(locations::review)
            .unwrap()
            .locations
            .into_iter()
            .filter(|l| l.thumbnail_override)
            .map(|l| l.path)
            .collect::<Vec<_>>()
    };
    assert_eq!(overridden(&rig.store), [thumbs.path().to_path_buf()]);
    assert_eq!(
        overridden(&Store::open(rig.home.path()).unwrap()),
        [thumbs.path().to_path_buf()]
    );
    // ... and that folder cannot then be added as a file location
    rig.pick(thumbs.path());
    window.invoke_action("add".into());
    warning(model::IS_THUMBNAIL_LOCATION);

    // clearing asks first
    window.invoke_action("clear thumbnails".into());
    assert_eq!(rig.answer(false), model::CLEAR_THUMBNAILS);
    assert_eq!(overridden(&rig.store).len(), 1);
    window.invoke_action("clear thumbnails".into());
    rig.answer(true);
    assert!(overridden(&rig.store).is_empty());
    assert_eq!(window.get_thumbnail_location(), "none set");
    assert!(!window.get_can_clear_thumbnails());
}

/// With a thumbnail location set in the window, "move files now" moves the thumbnail folders
/// there (and only them), and clearing it moves them back.
// leaf: audit-media-database-locations-thumbnails
#[test]
fn moving_files_now_puts_the_thumbnail_folders_at_the_thumbnail_location() {
    let rig = rig();
    let media = rig.home.path().join("client_files");
    // a thumbnail and a file, in the layout their folders have to begin with
    let thumb = hydrus_store::storage::prefix_dir(&media, "t3a").join("3ab4567.thumbnail");
    let file = hydrus_store::storage::prefix_dir(&media, "f3a").join("3ab4567.png");
    for path in [&thumb, &file] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    let window = rig.open();
    let thumbs = tempfile::tempdir().unwrap();
    rig.pick(thumbs.path());
    window.invoke_action("set thumbnails".into());
    assert_eq!(
        window.get_thumbnail_location(),
        thumbs.path().display().to_string()
    );
    let finish = |window: &DatabaseLocationsWindow| {
        window.invoke_action("rebalance".into());
        runtime_question().unwrap().invoke_chosen(4);
        let deadline = Instant::now() + Duration::from_secs(60);
        while rig.store.read(locations::next_move).unwrap().is_some() {
            assert!(
                Instant::now() < deadline,
                "the folders never finished moving"
            );
            pump(Duration::from_millis(50));
        }
    };
    finish(&window);
    let at = |base: &Path| hydrus_store::storage::prefix_dir(base, "t3a").join("3ab4567.thumbnail");
    assert!(
        at(thumbs.path()).is_file(),
        "the thumbnail folder moved to the override"
    );
    assert!(!thumb.exists());
    assert!(file.is_file(), "the file folders stayed");
    let review = rig.review();
    let there = review
        .locations
        .iter()
        .find(|l| l.path == thumbs.path())
        .unwrap();
    assert!((there.thumbnails_share - 1.0).abs() < f64::EPSILON);
    assert!(there.files_share.abs() < f64::EPSILON);
    // clearing it sends them back
    let window = rig.open();
    window.invoke_action("clear thumbnails".into());
    rig.answer(true);
    finish(&window);
    assert!(thumb.is_file());
    assert!(!at(thumbs.path()).exists());
}

// leaf: audit-media-database-locations-rebalance
#[test]
fn move_files_now_asks_for_a_run_time_and_moves_folders_with_a_popup() {
    let rig = rig();
    let window = rig.open();
    assert_eq!(window.get_rebalance_status(), model::rebalance_label(false));
    assert!(!window.get_can_rebalance());

    let other = tempfile::tempdir().unwrap();
    rig.pick(other.path());
    window.invoke_action("add".into());
    assert_eq!(window.get_rebalance_status(), model::rebalance_label(true));
    assert!(window.get_can_rebalance());

    // a location that is not there stops it
    let gone = tempfile::tempdir().unwrap();
    let gone_path = gone.path().to_path_buf();
    rig.pick(&gone_path);
    window.invoke_action("add".into());
    drop(gone);
    window.invoke_action("rebalance".into());
    warning(&model::missing_path_warning(&model::location_repr(
        &gone_path.display().to_string(),
        1,
        None,
    )));
    std::fs::create_dir_all(&gone_path).unwrap();

    // the question, in the reference's words; "forget it" moves nothing
    window.invoke_action("rebalance".into());
    let question = runtime_question().expect("asked for a run time");
    assert_eq!(question.get_message(), model::RUNTIME_QUESTION);
    assert_eq!(question.get_no_label(), "forget it");
    let choices: Vec<String> = question
        .get_choices()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(
        choices,
        model::RUNTIMES
            .iter()
            .map(|(l, _)| (*l).to_owned())
            .collect::<Vec<_>>()
    );
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(shown(&rig.store).is_empty());
    assert!(rig.store.read(locations::next_move).unwrap().is_some());

    // "run indefinitely" runs to the end, with the popup
    window.invoke_action("rebalance".into());
    runtime_question().unwrap().invoke_chosen(4);
    assert!(
        !window.window().is_visible(),
        "the reference closes the dialog once the job is started"
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while rig.store.read(locations::next_move).unwrap().is_some() {
        assert!(Instant::now() < deadline, "the files never finished moving");
        pump(Duration::from_millis(50));
    }
    // (the finished popup dismisses itself, so there is none left to read)
    assert!(shown(&rig.store).is_empty());
    // every location now holds folders
    let moved = rig.review();
    assert!(
        moved.locations.iter().all(|l| l.files_share > 0.0),
        "{moved:?}"
    );
}

fn granularity_window(
    rig: &Rig,
    window: &DatabaseLocationsWindow,
) -> hydrus_gui::GranularityWindow {
    let _ = rig;
    window.invoke_action("granularity".into());
    let granularity = hydrus_gui::granularity_window::opened().expect("the window opened");
    assert!(granularity.window().is_visible());
    granularity
}

// leaf: audit-media-database-locations-granularity-client
#[test]
fn this_client_s_granularity_is_changed_after_the_last_check_and_reported() {
    use hydrus_gui_model::database_granularity as granularity;
    let rig = rig();
    let media = rig.home.path().join("client_files");
    // a file in the granularity-2 layout
    let file = "3ab4567.png";
    let from = hydrus_store::storage::prefix_dir(&media, "f3a");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join(file), b"x").unwrap();
    let window = rig.open();
    assert_eq!(window.get_granularity(), model::granularity_label(2));
    let g = granularity_window(&rig, &window);
    assert_eq!(g.get_warning(), granularity::WARNING);
    assert_eq!(g.get_intro(), granularity::INTRO);
    assert_eq!(g.get_offline_label(), granularity::OFFLINE);
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));

    // the last check, in the reference's words; "no" changes nothing
    let (title, message, yes, no) = granularity::client_question(2);
    g.invoke_action("2to3".into());
    let question = hydrus_gui::granularity_window::question().expect("the last check");
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    assert_eq!(question.get_no_label(), no);
    assert_eq!(
        question
            .get_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        [yes]
    );
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(from.join(file).is_file());
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));

    // "yes" moves the files, says how it went, and the windows follow
    g.invoke_action("2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while message_window().is_none_or(|m| !m.window().is_visible()) {
        assert!(Instant::now() < deadline, "the migration never reported");
        pump(Duration::from_millis(50));
    }
    let report = message_window().unwrap();
    assert!(
        report.get_message().starts_with("1 files moved in "),
        "{}",
        report.get_message()
    );
    assert!(
        report
            .get_message()
            .contains("You now have finer, lower-latency file storage.")
    );
    report.invoke_cancelled();
    assert!(
        hydrus_store::storage::prefix_dir(&media, "f3a")
            .join("b")
            .join(file)
            .is_file()
    );
    assert!(!from.join(file).exists());
    assert_eq!(g.get_client_label(), granularity::granularity_label(3));
    assert_eq!(window.get_granularity(), model::granularity_label(3));
    let reopened = Store::open(rig.home.path()).unwrap();
    assert_eq!(
        reopened
            .read(hydrus_store::storage::FileStorage::load)
            .unwrap()
            .granularity(),
        3
    );

    // and back again
    let (_, message, _, _) = granularity::client_question(3);
    g.invoke_action("3to2".into());
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_message(), message);
    question.invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while g.get_client_label() != granularity::granularity_label(2) {
        assert!(Instant::now() < deadline, "the migration never finished");
        pump(Duration::from_millis(50));
    }
    assert!(from.join(file).is_file());
}

// leaf: audit-media-database-locations-granularity-offline
#[test]
fn an_offline_folder_is_regranularised_after_its_ready_check_and_scan() {
    use hydrus_gui_model::database_granularity as granularity;
    let rig = rig();
    let window = rig.open();
    let g = granularity_window(&rig, &window);

    // a backup folder in the granularity-2 layout
    let backup = tempfile::tempdir().unwrap();
    for prefix in hydrus_store::granularity::prefixes('f', 2) {
        std::fs::create_dir_all(hydrus_store::storage::prefix_dir(backup.path(), &prefix)).unwrap();
    }
    let file = "3ab4567.png";
    let old = hydrus_store::storage::prefix_dir(backup.path(), "f3a");
    std::fs::write(old.join(file), b"x").unwrap();

    // the ready check, then the folder picker; declining either does nothing
    let (title, message, yes, no) = granularity::folder_ready();
    g.invoke_action("folder 2to3".into());
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    assert_eq!(question.get_no_label(), no);
    assert_eq!(
        question
            .get_choices()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        [yes]
    );
    question.invoke_cancelled();
    assert!(rig.asked.borrow().is_empty());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    assert_eq!(rig.asked.borrow().as_slice(), [granularity::PICK_FOLDER]);

    // the scan of the chosen folder is reported before anything moves
    rig.pick(backup.path());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let (title, message) =
        granularity::folder_check(hydrus_store::granularity::estimate(backup.path()), 2, 3);
    assert_eq!(title, "Looking good.");
    let question = hydrus_gui::granularity_window::question().unwrap();
    assert_eq!(question.get_window_title(), title);
    assert_eq!(question.get_message(), message);
    question.invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(old.join(file).is_file());

    // going ahead moves its files and says how it went; the client is untouched
    rig.pick(backup.path());
    g.invoke_action("folder 2to3".into());
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    hydrus_gui::granularity_window::question()
        .unwrap()
        .invoke_chosen(0);
    let deadline = Instant::now() + Duration::from_secs(60);
    while message_window().is_none_or(|m| !m.window().is_visible()) {
        assert!(Instant::now() < deadline, "the migration never reported");
        pump(Duration::from_millis(50));
    }
    let report = message_window().unwrap().get_message().to_string();
    assert!(report.starts_with("1 files moved in "), "{report}");
    assert!(report.contains("Your folder has been granularised to level 3."));
    message_window().unwrap().invoke_cancelled();
    assert!(old.join("b").join(file).is_file());
    assert_eq!(hydrus_store::granularity::estimate(backup.path()), Some(3));
    assert_eq!(g.get_client_label(), granularity::granularity_label(2));
}
