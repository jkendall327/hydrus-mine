//! "manage times" and its date-time editor, opened from the thumbnails'
//! "manage > times" on files given the recorded times and driven as a user
//! would (rows clicked, domains and file services selected and edited, the
//! copy menu's entries, paste, the questions answered, the editor's fields
//! typed and its buttons pressed), replayed step by step against the
//! reference's recordings (`oracle/fixtures/manage_times.json` from
//! `oracle/record_manage_times.py`, `oracle/fixtures/datetime_editor.json`
//! from `oracle/record_datetime_editor.py`), in UTC with the time held
//! still: what the windows show, ask and say after each step, what is
//! copied, and what "apply" writes to the store.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle, Model as _};

use hydrus_core::HashId;
use hydrus_core::content::CanvasType;
use hydrus_gui::{
    Bound, Clip, DateTimeEditorWindow, MainWindow, ManageTimesWindow, Pages, SearchPage, bind,
    headless,
};
use hydrus_store::Store;
use hydrus_store::content::FileTime;
use hydrus_store::import::import_legacy;
use hydrus_store::media::MediaResult;

use crate::common::widgets;

const UTC: jiff::tz::TimeZone = jiff::tz::TimeZone::UTC;

/// The main window on a page of files given the recorded times, with what
/// is copied caught and what is pasted given.
struct Replay {
    _legacy: tempfile::TempDir,
    _native: tempfile::TempDir,
    store: Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    files: Vec<HashId>,
    copied: Rc<RefCell<Vec<String>>>,
    clipboard: Rc<RefCell<String>>,
}

fn service_id(store: &Store, name: &str) -> hydrus_core::ServiceId {
    store
        .snapshot()
        .services
        .all()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no service {name}"))
        .id
}

/// Give a file the times of a recorded file (`files[i]` of a case): its
/// modified, archived and viewed times, inbox, web domain times and file
/// service times, and no others.
fn seed(store: &Store, file: HashId, spec: &Value) {
    let ms = |key: &str| spec[key].as_i64();
    let (modified, archived, viewed, preview) =
        (ms("modified"), ms("archived"), ms("viewed"), ms("preview"));
    let inbox = spec["inbox"].as_bool().unwrap_or(false);
    let services: Vec<(hydrus_core::ServiceId, i64, i64)> = spec["services"]
        .as_array()
        .map(|s| {
            s.iter()
                .map(|x| {
                    (
                        service_id(store, x[0].as_str().unwrap()),
                        x[1].as_i64().unwrap(),
                        x[2].as_i64().unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    store
        .write(move |tx| {
            let conn = tx.conn();
            let id = file;
            conn.execute(
                "UPDATE files SET file_modified_ms = ?1 WHERE hash_id = ?2",
                rusqlite::params![modified, id],
            )?;
            for table in [
                "file_inbox",
                "file_archived",
                "file_viewing_stats",
                "file_domain_modified",
                "file_domain_current",
                "file_domain_deleted",
            ] {
                conn.execute(&format!("DELETE FROM {table} WHERE hash_id = ?1"), [id])?;
            }
            if inbox {
                conn.execute("INSERT INTO file_inbox (hash_id) VALUES (?1)", [id])?;
            }
            if let Some(ms) = archived {
                conn.execute(
                    "INSERT INTO file_archived (hash_id, archived_ms) VALUES (?1, ?2)",
                    rusqlite::params![id, ms],
                )?;
            }
            for (canvas, ms) in [
                (CanvasType::MediaViewer, viewed),
                (CanvasType::Preview, preview),
            ] {
                if let Some(ms) = ms {
                    conn.execute(
                        "INSERT INTO file_viewing_stats (hash_id, canvas_type, views, viewtime_ms, last_viewed_ms) VALUES (?1, ?2, 1, 0, ?3)",
                        rusqlite::params![id, canvas.code(), ms],
                    )?;
                }
            }
            for &(service, kind, ms) in &services {
                let service = i64::from(service.get());
                if kind == 3 {
                    conn.execute(
                        "INSERT INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?1, ?2, ?3)",
                        rusqlite::params![service, id, ms],
                    )?;
                } else {
                    conn.execute(
                        "INSERT OR IGNORE INTO file_domain_deleted (service_id, hash_id) VALUES (?1, ?2)",
                        rusqlite::params![service, id],
                    )?;
                    let column = if kind == 4 {
                        "deleted_ms"
                    } else {
                        "original_added_ms"
                    };
                    conn.execute(
                        &format!("UPDATE file_domain_deleted SET {column} = ?1 WHERE service_id = ?2 AND hash_id = ?3"),
                        rusqlite::params![ms, service, id],
                    )?;
                }
            }
            Ok(())
        })
        .unwrap();
    let domains: Vec<(String, i64)> = spec["domains"]
        .as_object()
        .map(|d| {
            d.iter()
                .map(|(k, v)| (k.clone(), v.as_i64().unwrap()))
                .collect()
        })
        .unwrap_or_default();
    store
        .write_content(move |w| {
            for (domain, ms) in &domains {
                w.set_file_time(&[file], &FileTime::DomainModified(domain.clone()), *ms)?;
            }
            Ok(())
        })
        .unwrap();
}

fn load(store: &Store, file: HashId) -> MediaResult {
    let services = store.snapshot().services.clone();
    store
        .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
        .unwrap()
        .results
        .remove(0)
}

impl Replay {
    /// The basic fixture's first files given `specs`' times, on a page of
    /// their own, held at `now` (seconds) in UTC (on a thread whose
    /// headless platform is set).
    fn new(specs: &[Value], now: i64) -> Self {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let native = tempfile::tempdir().unwrap();
        import_legacy(
            legacy.path(),
            &native.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store: Arc<Store> = Store::open(native.path()).unwrap();
        let count = specs.len();
        let files: Vec<HashId> = store
            .read(move |c| {
                let mut stmt = c.prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT ?1")?;
                let rows = stmt
                    .query_map([i64::try_from(count).unwrap()], |r| r.get::<_, HashId>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(files.len(), count, "the fixture has too few files");
        for (&file, spec) in files.iter().zip(specs) {
            seed(&store, file, spec);
        }
        hydrus_gui::manage_times_window::hold_time(now * 1000, UTC);
        let copied: Rc<RefCell<Vec<String>>> = Rc::default();
        hydrus_gui::set_clipper({
            let copied = copied.clone();
            move |clip| {
                if let Clip::Text(text) = clip {
                    copied.borrow_mut().push(text.clone());
                }
            }
        });
        let clipboard: Rc<RefCell<String>> = Rc::default();
        hydrus_gui::set_paster({
            let clipboard = clipboard.clone();
            move || clipboard.borrow().clone()
        });
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let mut page = SearchPage::new(store.clone());
        page.add_files(&files);
        let bound = bind(&ui, Pages::single(page));
        Replay {
            _legacy: legacy,
            _native: native,
            store,
            ui,
            bound,
            files,
            copied,
            clipboard,
        }
    }

    /// Every file selected (a click on the first, a shift-click on the
    /// last), then "manage > times".
    fn open(&self) -> ManageTimesWindow {
        let last = i32::try_from(self.files.len()).unwrap() - 1;
        self.ui.invoke_thumbnail_clicked(0, false, false);
        if last > 0 {
            self.ui.invoke_thumbnail_clicked(last, false, true);
        }
        let selected = self.bound.current.borrow().borrow().selected_files();
        assert_eq!(selected, self.files);
        self.ui.invoke_thumbnail_menu_requested(last);
        let manage = self.ui.get_thumbnail_menu().manage;
        let id = (0..manage.row_count())
            .map(|i| manage.row_data(i).unwrap())
            .find(|r| r.label == "times")
            .expect("manage > times")
            .id;
        self.ui.invoke_menu_chosen(id);
        self.bound
            .manage_times
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
            .expect("the dialog opens")
    }

    fn editor(&self) -> Option<DateTimeEditorWindow> {
        self.bound
            .datetime_editor
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }
}

fn cells(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<Vec<String>> {
    (0..rows.row_count())
        .map(|i| {
            let cells = rows.row_data(i).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

/// What the dialog shows, as the recording has it.
fn dialog_state(dialog: &ManageTimesWindow) -> Value {
    let times = dialog.get_times();
    let rows: Vec<Value> = (0..times.row_count())
        .map(|i| times.row_data(i).unwrap())
        .map(|r| json!([r.label.as_str(), r.text.as_str(), r.enabled]))
        .collect();
    let warning = dialog.get_warning();
    json!({
        "rows": rows,
        "warning": if warning.is_empty() { Value::Null } else { json!(warning.as_str()) },
        "domains": cells(&dialog.get_domains()),
        "file_services": cells(&dialog.get_file_services()),
        "copy_shown": dialog.get_can_copy(),
    })
}

/// What the date-time editor shows, as the recording has it (but for
/// whether its value is a change, which it does not show: the editor's
/// model replays that in hydrus-gui-model).
fn editor_state(editor: &DateTimeEditorWindow) -> Value {
    let label = editor.get_label();
    let step: i64 = editor.get_step().parse().unwrap();
    json!({
        "label": if label.is_empty() { Value::Null } else { json!(label.as_str()) },
        "step_shown": editor.get_step_shown(),
        "date": editor.get_date().as_str(),
        "time": editor.get_time().as_str(),
        "step": step as f64 / 1000.0,
        "value": editor.get_value().as_str(),
    })
}

/// Type a time (milliseconds, UTC) into the editor, and a step if given.
fn type_time(editor: &DateTimeEditorWindow, ms: i64, step: i64) {
    let zoned = jiff::Timestamp::from_millisecond(ms).unwrap().to_zoned(UTC);
    editor.set_date(zoned.strftime("%Y-%m-%d").to_string().into());
    editor.set_time(zoned.strftime("%H:%M:%S%.3f").to_string().into());
    editor.invoke_edited();
    if step != editor.get_step().parse::<i64>().unwrap() {
        assert!(editor.get_step_shown(), "a step typed where none shows");
        editor.set_step(step.to_string().into());
        editor.invoke_edited();
    }
}

/// Python's JSON errors, compared up to the error itself.
fn cut(message: &str) -> &str {
    message
        .find("general error was:")
        .map_or(message, |i| &message[..i])
}

/// Answer what the dialog asks, in turn, as the step does, saying what was
/// asked and told.
fn answer(dialog: &ManageTimesWindow, said: &mut Vec<Value>, yes: impl Fn() -> bool) {
    while dialog.get_asking() {
        let message = dialog.get_asking_message().to_string();
        let choices: Vec<String> = {
            let c = dialog.get_asking_choices();
            (0..c.row_count())
                .map(|i| c.row_data(i).unwrap().to_string())
                .collect()
        };
        if choices.len() == 2 {
            said.push(json!({ "asked": message, "yes": choices[0], "no": choices[1] }));
            dialog.invoke_chosen(i32::from(!yes()));
        } else {
            let title = dialog.get_asking_title().to_string();
            if title == "Warning" {
                said.push(json!({ "warning": message }));
            } else {
                said.push(json!({ "critical": title, "message": message }));
            }
            dialog.invoke_chosen(0);
        }
    }
}

/// Select these rows of a list (a click, then ctrl-clicks).
fn select(rows: &[usize], click: impl Fn(i32, bool, bool)) {
    for (n, &row) in rows.iter().enumerate() {
        click(i32::try_from(row).unwrap(), n > 0, false);
    }
}

/// The time a recorded update says a file has, read from the store.
fn stored(store: &Store, file: HashId, kind: i64, location: &Value, names: &Value) -> Option<i64> {
    let result = load(store, file);
    let service = |key: &Value| {
        let name = names
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, k)| *k == key)
            .unwrap()
            .0;
        service_id(store, name)
    };
    match kind {
        0 => result
            .domain_modified
            .iter()
            .find(|(d, _)| d == location.as_str().unwrap())
            .map(|(_, t)| t.0),
        1 => result.info.and_then(|i| i.file_modified).map(|t| t.0),
        3 => result
            .current
            .iter()
            .find(|c| c.service == service(location))
            .and_then(|c| c.added)
            .map(|t| t.0),
        4 => result
            .deleted
            .iter()
            .find(|d| d.service == service(location))
            .and_then(|d| d.deleted)
            .map(|t| t.0),
        5 => result.archived.map(|t| t.0),
        6 => result
            .viewing
            .iter()
            .find(|v| i64::from(v.canvas.code()) == location.as_i64().unwrap())
            .and_then(|v| v.last_viewed)
            .map(|t| t.0),
        7 => result
            .deleted
            .iter()
            .find(|d| d.service == service(location))
            .and_then(|d| d.originally_added)
            .map(|t| t.0),
        other => panic!("{other}"),
    }
}

fn wait_for_file_work(bound: &Bound) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while bound.metadata_jobs.running() != 0 {
        assert!(
            std::time::Instant::now() < until,
            "metadata worker did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        slint::platform::update_timers_and_animations();
    }
}

// leaf: audit-media-times-domain, audit-media-times-services, audit-media-times-clipboard
#[test]
fn manage_times_replays_the_references_dialog_through_the_window() {
    let recorded = hydrus_testkit::fixture_json("manage_times.json");
    let now = recorded["now"].as_i64().unwrap();
    let names = &recorded["services"];
    let _windows = headless::init();
    // ("many files" asks before applying to over 100 files: the basic
    // fixture has fewer, and the model replays it)
    for case in recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] != "many files")
    {
        let name = case["name"].as_str().unwrap();
        let specs = case["files"].as_array().unwrap();
        let replay = Replay::new(specs, now);
        let hashes: Vec<&str> = specs.iter().map(|f| f["hash"].as_str().unwrap()).collect();
        let dialog = replay.open();
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(dialog_state(&dialog), steps[0]["state"], "{name} start");
        let mut closed = false;
        let mut last_notice = String::new();
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("{name} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let mut said: Vec<Value> = Vec::new();
            let mut unclickable = false;
            replay.copied.borrow_mut().clear();
            let domain_rows = |dialog: &ManageTimesWindow, chosen: &Value| -> Vec<usize> {
                let rows = cells(&dialog.get_domains());
                let chosen: Vec<&str> = chosen
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| d.as_str().unwrap())
                    .collect();
                (0..rows.len())
                    .filter(|&i| chosen.contains(&rows[i][0].as_str()))
                    .collect()
            };
            // the date-time editor, opened by the step: what it starts
            // with said, the recorded time typed and applied
            let edit = |said: &mut Vec<Value>, ms: &Value, step: i64, say: bool| {
                let editor = replay.editor().expect("the date-time editor opens");
                if say {
                    // what it was given: its label's "Originally, ...", or
                    // with none (a time the files share) its value
                    let label = editor.get_label().to_string();
                    let given = match label.strip_prefix("Originally, ") {
                        Some(given) => given.to_owned(),
                        None => editor.get_value().to_string(),
                    };
                    said.push(json!({ "editing": given }));
                }
                type_time(&editor, ms.as_i64().unwrap(), step);
                editor.invoke_apply();
                assert!(replay.editor().is_none(), "{context}");
            };
            match action[0].as_str().unwrap() {
                "edit_main" => {
                    let label = match action[1].as_str().unwrap() {
                        "modified" => "file modified time: ",
                        "archived" => "archived time: ",
                        "viewed" => "last viewed in media viewer: ",
                        _ => "last viewed in preview viewer: ",
                    };
                    let times = dialog.get_times();
                    let row = (0..times.row_count())
                        .find(|&i| times.row_data(i).unwrap().label == label)
                        .unwrap();
                    // (the row's time is its button)
                    let text = times.row_data(row).unwrap().text.to_string();
                    let win = dialog.window();
                    widgets::lay_out(win, 900.0, 700.0);
                    widgets::click(win, &text);
                    if times.row_data(row).unwrap().enabled {
                        edit(&mut said, &action[2], action[3].as_i64().unwrap(), false);
                    } else {
                        // (a time no file has: its button is disabled; the
                        // recorder set it in the panel behind the row, which
                        // writes nothing, no file having it)
                        assert!(replay.editor().is_none(), "{context}");
                        unclickable = true;
                    }
                }
                "add_domain" => {
                    dialog.invoke_add_domain();
                    assert!(dialog.get_asking_wants_text(), "{context}");
                    said.push(json!({ "entered": dialog.get_asking_message().as_str() }));
                    dialog.set_asking_text(action[1].as_str().unwrap().into());
                    dialog.invoke_chosen(0);
                    if replay.editor().is_some() {
                        edit(&mut said, &action[2], 0, true);
                    }
                    answer(&dialog, &mut said, || true);
                }
                "edit_domains" => {
                    let rows = domain_rows(&dialog, &action[1]);
                    select(&rows, |r, c, s| dialog.invoke_domain_clicked(r, c, s));
                    dialog.invoke_edit_domains();
                    edit(&mut said, &action[2], action[3].as_i64().unwrap(), true);
                    let all = action[4].as_bool().unwrap_or(false);
                    answer(&dialog, &mut said, || all);
                }
                "delete_domains" => {
                    let rows = domain_rows(&dialog, &action[1]);
                    select(&rows, |r, c, s| dialog.invoke_domain_clicked(r, c, s));
                    dialog.invoke_delete_domains();
                    let yes = action[2].as_bool().unwrap();
                    answer(&dialog, &mut said, || yes);
                }
                "edit_services" => {
                    let rows = cells(&dialog.get_file_services());
                    let chosen: Vec<usize> = (0..rows.len())
                        .filter(|&i| {
                            action[1].as_array().unwrap().iter().any(|c| {
                                let kind = match c[1].as_i64().unwrap() {
                                    3 => "imported time",
                                    4 => "deleted time",
                                    _ => "previous imported time (for undelete)",
                                };
                                rows[i][0] == c[0].as_str().unwrap() && rows[i][1] == kind
                            })
                        })
                        .collect();
                    select(&chosen, |r, c, s| dialog.invoke_service_clicked(r, c, s));
                    dialog.invoke_edit_services();
                    edit(&mut said, &action[2], action[3].as_i64().unwrap(), true);
                }
                "copy" => {
                    // the copy menu's entries, as the reference's
                    let entry = match action[1]
                        .as_array()
                        .map(|k| k.iter().map(|x| x.as_i64().unwrap()).collect::<Vec<_>>())
                    {
                        None => 0,
                        Some(k) => match k.as_slice() {
                            [1] => 1,
                            [5] => 2,
                            [6] => 3,
                            [0] => 4,
                            _ => 5,
                        },
                    };
                    if dialog.get_can_copy() {
                        dialog.invoke_copy(entry);
                    }
                }
                "paste" => {
                    let text = match action[1].as_str().unwrap() {
                        "COPIED" => replay.clipboard.borrow().clone(),
                        other => other.to_owned(),
                    };
                    *replay.clipboard.borrow_mut() = text;
                    dialog.invoke_paste();
                    answer(&dialog, &mut said, || true);
                }
                "ok" => {
                    let yes = action[1].as_bool().unwrap();
                    dialog.invoke_apply();
                    answer(&dialog, &mut said, || yes);
                    closed = replay.bound.manage_times.borrow().is_none();
                    assert_eq!(closed, yes, "{context}");
                    wait_for_file_work(&replay.bound);
                    for update in step["written"]["updates"].as_array().unwrap() {
                        let tuple = &update[2][2];
                        let (kind, location) = (tuple[0].as_i64().unwrap(), &tuple[1]);
                        for hash in update[1].as_array().unwrap() {
                            let i = hashes.iter().position(|h| hash == h).unwrap();
                            assert_eq!(
                                stored(&replay.store, replay.files[i], kind, location, names),
                                tuple[2].as_i64(),
                                "{context}: {update}"
                            );
                        }
                    }
                }
                other => panic!("{other}"),
            }
            // (the dialog shows its last notice until the next: a new one
            // is a changed one; the recording has no two alike in a row)
            let notice = dialog.get_notice().to_string();
            if notice != last_notice {
                said.push(json!({ "notice": notice }));
                last_notice = notice;
            }
            let theirs: Vec<Value> = step["said"].as_array().unwrap().clone();
            assert_eq!(said.len(), theirs.len(), "{context}: {said:?}");
            for (ours, theirs) in said.iter().zip(&theirs) {
                match (ours["message"].as_str(), theirs["message"].as_str()) {
                    (Some(o), Some(t)) => {
                        assert_eq!(cut(o), cut(t), "{context}");
                        assert_eq!(ours["critical"], theirs["critical"], "{context}");
                    }
                    _ => assert_eq!(ours, theirs, "{context}"),
                }
            }
            let copied = replay.copied.borrow().clone();
            assert_eq!(json!(copied), step["copied"], "{context}");
            if let Some(last) = copied.last() {
                replay.clipboard.borrow_mut().clone_from(last);
            }
            if !closed && !unclickable {
                assert_eq!(dialog_state(&dialog), step["state"], "{context}");
            }
        }
        assert!(closed, "{name}: applied");
    }
}

// leaf: audit-media-datetime-fields, audit-media-datetime-clipboard
#[test]
fn the_date_time_editor_replays_the_references_through_the_window() {
    let recorded = hydrus_testkit::fixture_json("datetime_editor.json");
    let now = recorded["now"].as_i64().unwrap();
    let _windows = headless::init();
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        // the files' modified times, edited from the dialog's first row
        let specs: Vec<Value> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ms| json!({ "modified": ms }))
            .collect();
        let replay = Replay::new(&specs, now);
        let dialog = replay.open();
        dialog.invoke_time_clicked(0);
        let mut editor = replay.editor().expect("the date-time editor opens");
        let step = case["step"].as_i64().unwrap();
        if step != 0 {
            // a step set and applied, then the time edited again: the
            // editor starts from the stepped time
            editor.set_step(step.to_string().into());
            editor.invoke_edited();
            editor.invoke_apply();
            dialog.invoke_time_clicked(0);
            editor = replay.editor().expect("the date-time editor opens again");
        }
        let steps = case["steps"].as_array().unwrap();
        let theirs = |state: &Value| {
            let mut state = state.clone();
            state.as_object_mut().unwrap().remove("changed");
            state
        };
        assert_eq!(
            editor_state(&editor),
            theirs(&steps[0]["state"]),
            "{name} start"
        );
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("{name} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            replay.copied.borrow_mut().clear();
            editor.set_notice("".into());
            match action[0].as_str().unwrap() {
                "date" => {
                    editor.set_date(action[1].as_str().unwrap().into());
                    editor.invoke_edited();
                }
                "time" => {
                    editor.set_time(action[1].as_str().unwrap().into());
                    editor.invoke_edited();
                }
                "step" => {
                    let ms = (action[1].as_f64().unwrap() * 1000.0).round() as i64;
                    assert!(editor.get_step_shown(), "{context}");
                    editor.set_step(ms.to_string().into());
                    editor.invoke_edited();
                }
                "now" => editor.invoke_now(),
                "copy" => editor.invoke_copy(),
                "paste" => {
                    *replay.clipboard.borrow_mut() = action[1].as_str().unwrap().to_owned();
                    editor.invoke_paste();
                }
                other => panic!("{other}"),
            }
            let mut said = Vec::new();
            while editor.get_asking() {
                let title = editor.get_asking_title().to_string();
                let message = editor.get_asking_message().to_string();
                said.push(if title == "Warning" {
                    json!({ "warning": message })
                } else {
                    json!({ "critical": title, "message": message })
                });
                editor.invoke_dismissed();
            }
            let notice = editor.get_notice();
            if !notice.is_empty() {
                said.push(json!({ "notice": notice.as_str() }));
            }
            assert_eq!(json!(said), step["said"], "{context}");
            assert_eq!(json!(*replay.copied.borrow()), step["copied"], "{context}");
            assert_eq!(editor_state(&editor), theirs(&step["state"]), "{context}");
        }
        editor.invoke_cancel();
        dialog.invoke_cancel();
    }
}
