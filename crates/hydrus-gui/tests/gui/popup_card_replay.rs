//! The popup card's content and traceback controls, replayed state by state
//! against `oracle/fixtures/popup_content.json` (recorded by
//! `oracle/record_popup_content.py`: the reference's `PopupMessage` for the
//! same jobs): the main window's card for each job is compared with the
//! reference's labels, gauges and buttons, and the traceback button, its
//! text and the copied text are driven with real clicks.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Clip, MainWindow, Pages, bind, headless};
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

fn recorded() -> Value {
    hydrus_testkit::fixture_json("popup_content.json")
}

fn state<'a>(recorded: &'a Value, label: &str) -> &'a Value {
    recorded["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["label"] == label)
        .unwrap_or_else(|| panic!("{label} is recorded"))
}

/// The job the recorder made for a state (the same title, texts, gauges and
/// buttons).
fn job_for(label: &str, recorded: &Value) -> Job {
    let mut job = Job::new(false, false, 0.0);
    match label {
        "empty" => job = Job::new(true, true, 0.0),
        "title only" => job.status_title = Some("only a title".into()),
        "text 1 only" => job.status_text_1 = Some("only text".into()),
        "text 2 only" => job.status_text_2 = Some("only second text".into()),
        "gauge 1 only" => job.popup_gauge_1 = Some((1, 4)),
        "all" => {
            job = Job::new(true, true, 0.0);
            job.status_title = Some("a title".into());
            job.status_text_1 = Some("first".into());
            job.status_text_2 = Some("second".into());
            job.popup_gauge_1 = Some((1, 4));
            job.popup_gauge_2 = Some((0, 0));
        }
        "pulsing first, ranged second" => {
            job.popup_gauge_1 = Some((0, 0));
            job.popup_gauge_2 = Some((3, 8));
        }
        "long texts" => {
            job.status_text_1 = Some("x".repeat(1500));
            job.status_text_2 = Some("y".repeat(1500));
        }
        "traceback" => {
            job.status_title = Some("oh no".into());
            job.status_text_1 = Some("it broke".into());
            job.traceback = Some(recorded["traceback"].as_str().unwrap().to_owned());
        }
        "short traceback" => {
            job.status_title = Some("oh no".into());
            job.status_text_1 = Some("it broke".into());
            job.traceback = Some(recorded["short_traceback"].as_str().unwrap().to_owned());
        }
        other => panic!("no job for {other}"),
    }
    job
}

fn window(store: &Arc<Store>, job: &Job) -> (MainWindow, hydrus_gui::Bound) {
    store
        .write({
            let job = job.clone();
            move |ctx| popups::add(ctx.conn(), &job, now())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    widgets::lay_out(ui.window(), 1100.0, 760.0);
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (ui, bound)
}

/// What the card shows for each of the reference card's pieces, as the
/// recording states it.
fn assert_card_is(ui: &MainWindow, want: &Value, when: &str) {
    let row = ui.get_popups().row_data(0).unwrap();
    let shown = |hidden: &str, text: &str| -> String {
        if want[hidden].as_bool().unwrap() {
            String::new()
        } else {
            want[text].as_str().unwrap().to_owned()
        }
    };
    assert_eq!(
        row.title.as_str(),
        shown("title_hidden", "title"),
        "{when}: title"
    );
    assert_eq!(
        row.text_1.as_str(),
        shown("text_1_hidden", "text_1"),
        "{when}: text 1"
    );
    assert_eq!(
        row.text_2.as_str(),
        shown("text_2_hidden", "text_2"),
        "{when}: text 2"
    );
    for (n, has, progress, going) in [
        (1, row.has_gauge_1, row.gauge_1, row.gauge_1_going),
        (2, row.has_gauge_2, row.gauge_2, row.gauge_2_going),
    ] {
        let gauge = &want[format!("gauge_{n}")];
        assert_eq!(has, !gauge.is_null(), "{when}: gauge {n} shown");
        if !gauge.is_null() {
            let pulsing = gauge["pulsing"].as_bool().unwrap();
            assert_eq!(going, pulsing, "{when}: gauge {n} pulsing");
            if !pulsing {
                let expected = gauge["value"].as_f64().unwrap() / gauge["range"].as_f64().unwrap();
                assert!(
                    (f64::from(progress) - expected).abs() < 1e-6,
                    "{when}: gauge {n}"
                );
            }
        }
    }
    assert_eq!(
        row.pausable,
        !want["pause_hidden"].as_bool().unwrap(),
        "{when}: pause button"
    );
    assert_eq!(
        row.cancellable,
        !want["cancel_hidden"].as_bool().unwrap(),
        "{when}: cancel button"
    );
    assert_eq!(
        !row.traceback.is_empty(),
        !want["show_tb_hidden"].as_bool().unwrap(),
        "{when}: traceback button"
    );
    if !row.traceback.is_empty() {
        assert_eq!(
            row.traceback.as_str(),
            want["tb_text"].as_str().unwrap(),
            "{when}: tb text"
        );
    }
}

// leaf: audit-options-popups-content
#[test]
fn a_card_shows_what_the_references_shows_for_the_same_job() {
    let _windows = headless::init();
    let recorded = recorded();
    for label in [
        "empty",
        "title only",
        "text 1 only",
        "text 2 only",
        "gauge 1 only",
        "all",
        "pulsing first, ranged second",
        "long texts",
    ] {
        let (_dirs, store) = store();
        let job = job_for(label, &recorded);
        let (ui, _bound) = window(&store, &job);
        assert_eq!(ui.get_popups().row_count(), 1, "{label}");
        assert_card_is(&ui, state(&recorded, label), label);
        if label == "all" {
            // paused and played again, the pause button's click
            ui.invoke_popup_pause_play(0);
            assert_card_is(&ui, state(&recorded, "paused"), "paused");
            ui.invoke_popup_pause_play(0);
            assert_card_is(&ui, state(&recorded, "played"), "played");
        }
    }
}

// leaf: audit-options-popups-traceback
#[test]
fn the_traceback_button_shows_hides_and_copies_as_the_references_does() {
    let _windows = headless::init();
    let recorded = recorded();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    for (label, shown, copy) in [
        ("traceback", "traceback shown", "traceback copied"),
        (
            "short traceback",
            "traceback shown",
            "short traceback copied",
        ),
    ] {
        let (_dirs, store) = store();
        let job = job_for(label, &recorded);
        let (ui, _bound) = window(&store, &job);
        let window = ui.window();
        let at_start = state(&recorded, label);
        assert_card_is(&ui, at_start, label);
        // the button says "show traceback" and the text is not shown yet
        let button = at_start["show_tb_label"].as_str().unwrap();
        let body = at_start["tb_text"].as_str().unwrap();
        assert!(widgets::shows(window, button), "{label}: the button");
        assert!(!widgets::shows(window, body), "{label}: the text is hidden");
        // clicked: the button says "hide traceback" and the text is shown
        widgets::click(window, button);
        let after = state(&recorded, shown);
        assert_eq!(after["tb_text_hidden"], false);
        assert!(
            widgets::shows(window, after["show_tb_label"].as_str().unwrap()),
            "{label}: hide button"
        );
        assert!(!widgets::shows(window, button), "{label}: show label gone");
        assert!(widgets::shows(window, body), "{label}: the text is shown");
        // clicked again: hidden again
        widgets::click(window, after["show_tb_label"].as_str().unwrap());
        assert!(widgets::shows(window, button));
        assert!(!widgets::shows(window, body));
        // copy: the reference's text is a header line then the job's
        // title, texts and traceback; the header is hydrus-rs's own
        copied.borrow_mut().clear();
        let button = state(&recorded, copy)["copy_tb_label"].as_str().unwrap();
        widgets::click(window, button);
        let want = state(&recorded, copy)["published"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()[1]
            .as_str()
            .unwrap()
            .to_owned();
        let copied = copied.borrow();
        assert_eq!(copied.len(), 1, "{label}");
        let (info, trace) = copied[0].split_once('\n').unwrap();
        let (their_info, their_trace) = want.split_once('\n').unwrap();
        // (the reference's is "v<version>, <platform>, source|frozen"; ours
        // names hydrus-rs and its version, as DIFFERENCES.md says)
        assert!(info.contains(env!("CARGO_PKG_VERSION")), "{info}");
        assert!(their_info.starts_with('v') && their_info.ends_with("source"));
        assert_eq!(trace, their_trace, "{label}: the copied job");
    }
}
