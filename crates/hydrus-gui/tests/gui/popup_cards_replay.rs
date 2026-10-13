//! The popup cards put through the reference's
//! (`oracle/fixtures/popup_cards.json`, from `oracle/record_popup_cards.py`):
//! the same jobs added to a real store and shown by the real main window,
//! the buttons clicked as the user would (pause, cancel, "show traceback",
//! "copy traceback information", "yes" and "no", the summary bar's arrow),
//! and after each step the cards' title, texts, gauges and buttons, the
//! jobs' state, the clipboard and the summary compared.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_download::popups::Working;
use hydrus_gui::{Clip, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::popups::{self, Job};

use crate::common::widgets;

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

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn add(store: &Store, job: &Job) {
    let job = job.clone();
    store
        .write(move |ctx| popups::add(ctx.conn(), &job, now()))
        .unwrap();
}

fn stored(store: &Store, key: [u8; 32]) -> Option<Job> {
    store
        .read(|conn| popups::all(conn, now()))
        .unwrap()
        .into_iter()
        .find(|j| j.key == key)
}

fn window(store: &Arc<Store>) -> (MainWindow, hydrus_gui::Bound) {
    // (the headless windows stay for the test thread's life)
    std::mem::forget(headless::init());
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    widgets::lay_out(ui.window(), 1200.0, 800.0);
    (ui, bound)
}

/// What a card shows, in the shape `card` makes of the recorded one: each
/// text as the user reads it (nothing where it is hidden), each gauge as how
/// full it is or "pulsing" (nothing where hidden), and its buttons.
fn native_card(ui: &MainWindow, i: usize) -> Value {
    let row = ui.get_popups().row_data(i).unwrap();
    let gauge = |has: bool, going: bool, fraction: f32| match (has, going) {
        (false, _) => Value::Null,
        (true, true) => json!("pulsing"),
        (true, false) => json!(format!("{fraction:.3}")),
    };
    json!({
        "title": row.title.as_str(),
        "text_1": row.text_1.as_str(),
        "gauge_1": gauge(row.has_gauge_1, row.gauge_1_going, row.gauge_1),
        "text_2": row.text_2.as_str(),
        "gauge_2": gauge(row.has_gauge_2, row.gauge_2_going, row.gauge_2),
        "question": if row.has_question { row.question.as_str() } else { "" },
        "pause": row.pausable,
        "cancel": row.cancellable,
        "traceback": row.traceback != "",
    })
}

fn card(recorded: &Value) -> Value {
    let text = |t: &Value| {
        if t["shown"] == true {
            t["text"].clone()
        } else {
            json!("")
        }
    };
    let gauge = |g: &Value| {
        if g["shown"] != true {
            Value::Null
        } else if g["pulsing"] == true {
            json!("pulsing")
        } else {
            let (value, range) = (g["value"].as_f64().unwrap(), g["range"].as_f64().unwrap());
            json!(format!("{:.3}", (value / range) as f32))
        }
    };
    json!({
        "title": text(&recorded["title"]),
        "text_1": text(&recorded["text_1"]),
        "gauge_1": gauge(&recorded["gauge_1"]),
        "text_2": text(&recorded["text_2"]),
        "gauge_2": gauge(&recorded["gauge_2"]),
        "question": text(&recorded["question"]),
        "pause": recorded["pause_shown"],
        "cancel": recorded["cancel_shown"],
        "traceback": recorded["show_tb"]["shown"],
    })
}

fn cards(ui: &MainWindow) -> Vec<Value> {
    (0..ui.get_popups().row_count())
        .map(|i| native_card(ui, i))
        .collect()
}

// leaf: audit-options-popups-content
#[test]
fn cards_show_the_title_texts_and_gauges_and_pausing_hides_them_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("popup_cards.json");
    let steps = recorded["content"].as_array().unwrap();
    let (_dirs, store) = store();
    // the recorded jobs, in order: every field (pausable), a text, nothing
    // at all, and a second text past the cutoff
    let mut full = Job::new(true, false, 0.0);
    full.status_title = Some("a title".into());
    full.status_text_1 = Some("first".into());
    full.status_text_2 = Some("second".into());
    full.popup_gauge_1 = Some((1, 4));
    full.popup_gauge_2 = Some((0, 0));
    let plain = Job::text("plain", 1.0);
    let bare = Job::new(false, false, 2.0);
    let mut long = Job::text("long", 3.0);
    long.status_text_2 = Some("y".repeat(1500));
    for job in [&full, &plain, &bare, &long] {
        add(&store, job);
    }
    let (ui, _bound) = window(&store);
    let expect = |step: &Value| -> Vec<Value> {
        step["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(card)
            .collect()
    };
    assert_eq!(cards(&ui), expect(&steps[0]), "as shown");
    // the pause button clicked (it is the only one), and clicked again
    for step in &steps[1..] {
        widgets::click(
            ui.window(),
            if step["do"] == "pause" { "pause" } else { "play" },
        );
        assert_eq!(cards(&ui), expect(step), "{}", step["do"]);
        assert_eq!(
            stored(&store, full.key).unwrap().paused,
            step["do"] == "pause"
        );
    }
}

// leaf: audit-options-popups-cancel
#[test]
fn the_stop_button_cancels_a_cancellable_job_and_nothing_else_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("popup_cards.json");
    let steps = recorded["cancel"].as_array().unwrap();
    let (_dirs, store) = store();
    let stoppable = Job::new(true, true, 0.0);
    let mut stoppable = stoppable;
    stoppable.status_text_1 = Some("stoppable".into());
    let mut fixed = Job::text("cannot stop", 1.0);
    fixed.cancellable = false;
    add(&store, &stoppable);
    add(&store, &fixed);
    let (ui, _bound) = window(&store);
    let shown = &steps[0]["cards"];
    assert_eq!(
        cards(&ui),
        shown.as_array().unwrap().iter().map(card).collect::<Vec<_>>()
    );
    let job_of = |c: &Value| {
        let job = &c["job"];
        json!([
            job["done"], job["cancelled"], job["cancellable"], job["pausable"],
            job["paused"]
        ])
    };
    let native_job = |key| {
        let job = stored(&store, key).unwrap();
        json!([job.done, job.cancelled, job.cancellable, job.pausable, job.paused])
    };
    assert_eq!(native_job(stoppable.key), job_of(&shown[0]));
    assert_eq!(native_job(fixed.key), job_of(&shown[1]));
    widgets::click(ui.window(), "cancel");
    let after = &steps[1]["cards"];
    assert_eq!(native_job(stoppable.key), job_of(&after[0]), "cancelled");
    assert_eq!(native_job(fixed.key), job_of(&after[1]), "untouched");
    // the reference keeps the cancelled card, without its buttons, until
    // dismissed; every card the native window still shows matches
    let native = cards(&ui);
    let expected: Vec<Value> = after.as_array().unwrap().iter().map(card).collect();
    for card in &native {
        assert!(expected.contains(card), "{card}");
    }
}

// leaf: audit-options-popups-summary
#[test]
fn the_summary_counts_every_message_and_its_arrow_hides_and_shows_the_ten_oldest() {
    let recorded = hydrus_testkit::fixture_json("popup_cards.json");
    let steps = recorded["summary"].as_array().unwrap();
    let (_dirs, store) = store();
    for i in 0..12 {
        add(&store, &Job::text(format!("m{i}"), f64::from(i)));
    }
    let (ui, _bound) = window(&store);
    let win = ui.window();
    let state = || {
        let texts: Vec<String> = (0..12)
            .map(|i| format!("m{i}"))
            .filter(|t| widgets::shows(win, t))
            .collect();
        json!({ "summary": ui.get_popup_summary().as_str(), "texts": texts })
    };
    let expect = |step: &Value| {
        let state = &step["state"];
        json!({
            "summary": state["summary"],
            "texts": if state["panel_shown"] == true { state["texts"].clone() } else { json!([]) },
        })
    };
    assert_eq!(state(), expect(&steps[0]), "as shown");
    for step in &steps[1..] {
        widgets::click(win, if step["do"] == "collapse" { "▾" } else { "▴" });
        assert_eq!(state(), expect(step), "{}", step["do"]);
        // nothing was cancelled or deleted by hiding the cards
        assert_eq!(store.read(|c| popups::all(c, now())).unwrap().len(), 12);
    }
}

// leaf: audit-options-popups-traceback
#[test]
fn a_card_shows_hides_and_copies_its_traceback_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("popup_cards.json");
    let steps = recorded["traceback"].as_array().unwrap();
    let (_dirs, store) = store();
    let mut broken = Job::text("it broke", 0.0);
    broken.status_title = Some("oh no".into());
    broken.traceback = Some("trace line 1\ntrace line 2\n".to_owned() + &"z".repeat(2000));
    add(&store, &broken);
    let (ui, _bound) = window(&store);
    let win = ui.window();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    // what the user reads: the buttons, and the traceback text if shown
    let tb_text = steps[0]["card"]["tb_text"]["text"].as_str().unwrap();
    let state = || {
        let hide = widgets::shows(win, "hide traceback");
        json!({
            "title": ui.get_popups().row_data(0).unwrap().title.as_str(),
            "text_1": ui.get_popups().row_data(0).unwrap().text_1.as_str(),
            "show_button": if hide { "hide traceback" } else if widgets::shows(win, "show traceback") { "show traceback" } else { "" },
            "tb_shown": widgets::shows(win, tb_text),
            "copy_button": widgets::shows(win, "copy traceback information"),
        })
    };
    let expect = |step: &Value| {
        let card = &step["card"];
        json!({
            "title": card["title"]["text"],
            "text_1": card["text_1"]["text"],
            "show_button": card["show_tb"]["text"],
            "tb_shown": card["tb_text"]["shown"],
            "copy_button": card["copy_tb_shown"],
        })
    };
    assert_eq!(state(), expect(&steps[0]), "as shown");
    for step in &steps[1..] {
        match step["do"].as_str().unwrap() {
            "show traceback" => widgets::click(win, "show traceback"),
            "hide traceback" => widgets::click(win, "hide traceback"),
            _ => widgets::click(win, "copy traceback information"),
        }
        assert_eq!(state(), expect(step), "{}", step["do"]);
    }
    // the copy: a line saying what is running, then the job's complete text
    let copied = copied.borrow();
    assert_eq!(copied.len(), 1);
    let reference = steps[3]["clipboard"].as_str().unwrap();
    let (reference_info, reference_trace) = reference.split_once('\n').unwrap();
    let (info, trace) = copied[0].split_once('\n').unwrap();
    assert_eq!(trace, reference_trace, "the complete text, not the cut one");
    assert_eq!(trace, steps[3]["job_string"].as_str().unwrap());
    // ("v<version>, <platform>, <source|frozen>" there; hydrus-rs names itself)
    assert!(reference_info.starts_with('v'));
    assert!(
        info.starts_with("hydrus-rs ") && info.ends_with(std::env::consts::OS),
        "{info}"
    );
}

// leaf: audit-options-popups-yes-no
#[test]
fn yes_and_no_buttons_answer_finish_and_dismiss_the_question_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("popup_cards.json");
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    widgets::lay_out(ui.window(), 1200.0, 800.0);
    for step in recorded["question"].as_array().unwrap() {
        let answer = step["answer"].as_bool().unwrap();
        let producer = Working::new(&store, "asking", true);
        producer.set_question(Some(step["before"]["question"]["text"].as_str().unwrap().into()));
        producer.show();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        headless::render(&windows.get(0).unwrap(), 1200, 800);
        let before = &step["before"];
        let row = ui.get_popups().row_data(0).unwrap();
        assert_eq!(row.question.as_str(), before["question"]["text"]);
        assert_eq!(widgets::shows(ui.window(), "yes"), before["yes_shown"]);
        assert_eq!(widgets::shows(ui.window(), "no"), before["no_shown"]);
        assert_eq!(producer.question_answer(), None);
        widgets::click(ui.window(), if answer { "yes" } else { "no" });
        // answered: finished and dismissed (the reference's card waits for
        // its next update to go; this window drops it at once), its buttons
        // gone, and the producer told
        let after = &step["after"]["job"];
        assert_eq!(after["dismissed"], true);
        assert_eq!(after["answer"], answer);
        assert_eq!(producer.question_answer(), Some(answer));
        assert_eq!(ui.get_popups().row_count(), 0);
        assert!(!widgets::shows(ui.window(), "yes"));
        assert!(!widgets::shows(ui.window(), "no"));
        assert!(store.read(|conn| popups::all(conn, 0)).unwrap().is_empty());
        drop(bound);
    }
}
