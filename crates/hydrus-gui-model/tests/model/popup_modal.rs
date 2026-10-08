//! The modal popup's lifecycle against the reference's (`FrameGUI.AddModalMessage`
//! and `PopupMessageDialogPanel`, `oracle/fixtures/popup_modal.json` from
//! `oracle/record_popup_modal.py`): when a dialog is thrown up, what it is
//! titled and whether it has a close button, what closing it does, and when
//! it closes itself.

use hydrus_gui_model::popup_modal::{
    CANCEL_QUESTION, CANNOT_CANCEL, Conditions, Effect, JobFacts, Modals,
};
use serde_json::{Value, json};

const ACTIVE: Conditions = Conditions {
    minimised_or_hidden: false,
    dialog_open: false,
    active: true,
};

fn job(title: Option<&str>, cancellable: bool, done: bool, dismissed: bool) -> JobFacts {
    JobFacts {
        key: [1; 32],
        title: title.map(str::to_owned),
        done,
        dismissed,
        cancellable,
    }
}

/// What the effects did, in the recording's form.
#[derive(Default)]
struct Outcome {
    dialogs: Vec<Value>,
    pubs: Vec<&'static str>,
    events: Vec<Value>,
    cancelled: bool,
    closed: bool,
}

impl Outcome {
    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::PauseAllMedia => self.pubs.push("pause_all_media"),
                Effect::Show {
                    title,
                    hide_close_button,
                    ..
                } => self
                    .dialogs
                    .push(json!({"title": title, "hide_buttons": hide_close_button})),
                Effect::Release(_) => self.pubs.push("message"),
                Effect::Cancel(_) => self.cancelled = true,
                Effect::Ask => self
                    .events
                    .push(json!({"event": "asked", "text": CANCEL_QUESTION})),
                Effect::Warn => self
                    .events
                    .push(json!({"event": "warned", "text": CANNOT_CANCEL})),
                Effect::Close => self.closed = true,
            }
        }
    }

    /// The recording's `close_attempt`: allowed if the dialog closed.
    fn close_attempt(&mut self, was_closed: bool) {
        self.events
            .push(json!({"event": "close_attempt", "allowed": was_closed}));
    }
}

fn recorded_dialogs(case: &Value) -> Vec<Value> {
    case["dialogs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| json!({"title": d["title"], "hide_buttons": d["hide_buttons"]}))
        .collect()
}

fn case<'a>(cases: &'a [Value], label: &str) -> &'a Value {
    cases
        .iter()
        .find(|c| c["label"] == label)
        .unwrap_or_else(|| panic!("no recorded case {label:?}"))
}

fn check_add(cases: &[Value], label: &str, around: Conditions, facts: &JobFacts) {
    let recorded = case(cases, label);
    let mut modals = Modals::default();
    let mut outcome = Outcome::default();
    outcome.apply(modals.add(facts, &around));
    assert_eq!(
        outcome.dialogs,
        recorded_dialogs(recorded),
        "{label}: dialogs"
    );
    let pubs: Vec<&str> = recorded["pubs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(outcome.pubs, pubs, "{label}: pubs");
    assert_eq!(
        modals.pending() as u64,
        recorded["pending"].as_u64().unwrap(),
        "{label}: pending"
    );
}

// leaf: audit-options-popups-modal
#[test]
fn a_modal_job_is_thrown_up_or_waits_or_goes_to_the_popups_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("popup_modal.json");
    assert_eq!(recorded["cancel_question"], CANCEL_QUESTION);
    let cases = recorded["cases"].as_array().unwrap();
    let running = job(Some("a long job"), true, false, false);
    let around = |f: fn(&mut Conditions)| {
        let mut c = ACTIVE;
        f(&mut c);
        c
    };
    check_add(
        cases,
        "active, cancellable job with a title",
        ACTIVE,
        &running,
    );
    check_add(
        cases,
        "active, job with no title",
        ACTIVE,
        &job(None, true, false, false),
    );
    check_add(
        cases,
        "active, pausable job that cannot be cancelled",
        ACTIVE,
        &job(Some("a long job"), false, false, false),
    );
    check_add(
        cases,
        "active, job that is neither (done from the start)",
        ACTIVE,
        &job(Some("a long job"), false, true, false),
    );
    check_add(
        cases,
        "job already done goes to the popups",
        ACTIVE,
        &job(Some("a long job"), false, true, false),
    );
    check_add(
        cases,
        "dismissed job is dropped",
        ACTIVE,
        &job(Some("a long job"), false, true, true),
    );
    check_add(
        cases,
        "main window minimised: pending",
        around(|c| c.minimised_or_hidden = true),
        &running,
    );
    check_add(
        cases,
        "hidden to the tray: pending",
        around(|c| c.minimised_or_hidden = true),
        &running,
    );
    check_add(
        cases,
        "another dialog open: pending",
        around(|c| c.dialog_open = true),
        &running,
    );
    check_add(
        cases,
        "main window not active: pending",
        around(|c| c.active = false),
        &running,
    );

    // waiting jobs are retried one per page update, once things are quiet
    let retried = case(cases, "pending jobs are retried one per page update");
    let mut modals = Modals::default();
    let waiting = Conditions {
        minimised_or_hidden: true,
        ..ACTIVE
    };
    let mut second = running.clone();
    second.key = [2; 32];
    modals.add(&running, &waiting);
    modals.add(&second, &waiting);
    assert_eq!(modals.pending() as u64, retried["pending_at_start"]);
    assert_eq!(modals.page_update(), Some([1; 32]));
    assert_eq!(modals.pending() as u64, retried["pending_after_one"]);
    assert_eq!(modals.page_update(), Some([2; 32]));
    assert_eq!(modals.pending() as u64, retried["pending_after_two"]);
    assert_eq!(modals.page_update(), None);

    // a second job waits while the first one's dialog is up, and comes
    // when it has closed
    let mut modals = Modals::default();
    assert_eq!(modals.add(&running, &ACTIVE).len(), 2);
    assert!(modals.add(&second, &ACTIVE).is_empty());
    assert_eq!(modals.pending(), 1);
    modals.close_requested(&job(Some("a long job"), false, true, false));
    let key = modals.page_update().unwrap();
    assert_eq!(key, [2; 32]);
    assert_eq!(modals.add(&second, &ACTIVE).len(), 2);
}

// leaf: audit-options-popups-modal
#[test]
fn closing_a_modal_dialog_asks_warns_or_releases_and_the_dialog_closes_itself_as_the_reference_does()
 {
    let recorded = hydrus_testkit::fixture_json("popup_modal.json");
    let cases = recorded["cases"].as_array().unwrap();
    let running = job(Some("a long job"), true, false, false);
    let events = |label: &str| case(cases, label)["events"].as_array().unwrap().clone();
    let pubs = |label: &str| -> Vec<String> {
        case(cases, label)["pubs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .collect()
    };
    let job_state = |label: &str| case(cases, label)["job"].clone();

    // close a running cancellable job: asked, and yes cancels and releases it
    let label = "close a running cancellable job, answer yes";
    let mut modals = Modals::default();
    let mut out = Outcome::default();
    out.apply(modals.add(&running, &ACTIVE));
    out.events.clear();
    out.apply(modals.close_requested(&running));
    out.apply(modals.answered(true));
    out.close_attempt(out.closed);
    assert_eq!(out.events, events(label));
    assert_eq!(out.pubs, pubs(label));
    assert_eq!(out.cancelled, job_state(label)["cancelled"]);
    assert_eq!(modals.open_key(), None);

    // ... no leaves it running, the dialog up, and nothing released
    let label = "close a running cancellable job, answer no";
    let mut modals = Modals::default();
    let mut out = Outcome::default();
    out.apply(modals.add(&running, &ACTIVE));
    out.apply(modals.close_requested(&running));
    out.apply(modals.answered(false));
    out.close_attempt(out.closed);
    assert_eq!(out.events, events(label));
    assert_eq!(out.pubs, pubs(label));
    assert!(!out.cancelled);
    assert_eq!(modals.open_key(), Some([1; 32]));

    // a job that cannot be cancelled: warned, and the dialog stays
    let label = "close a running job that cannot be cancelled";
    let pausable = job(Some("a long job"), false, false, false);
    let mut modals = Modals::default();
    let mut out = Outcome::default();
    out.apply(modals.add(&pausable, &ACTIVE));
    out.apply(modals.close_requested(&pausable));
    out.close_attempt(out.closed);
    assert_eq!(out.events, events(label));
    assert_eq!(out.pubs, pubs(label));
    assert_eq!(modals.open_key(), Some([1; 32]));

    // the job finishes: the dialog closes itself and releases the job
    let label = "the dialog closes itself when the job finishes";
    let mut modals = Modals::default();
    let mut out = Outcome::default();
    out.apply(modals.add(&running, &ACTIVE));
    assert!(modals.tick(&running).is_empty(), "not while it runs");
    let done = job(Some("a long job"), false, true, false);
    let effects = modals.tick(&done);
    let recorded_events = events(label);
    assert_eq!(
        recorded_events[1],
        json!({"event": "job_finished", "ok_signal_emitted": !effects.is_empty()})
    );
    out.apply(effects);
    assert_eq!(out.pubs, pubs(label));
    assert!(out.closed);
    assert_eq!(modals.open_key(), None);

    // a question open on the job keeps the dialog from closing itself
    let label = "no ok signal while a question is open";
    let mut modals = Modals::default();
    modals.add(&running, &ACTIVE);
    modals.close_requested(&running);
    let effects = modals.tick(&done);
    assert_eq!(
        events(label)[0],
        json!({"event": "finished_while_asking", "ok_signal_emitted": !effects.is_empty()})
    );
    assert_eq!(modals.open_key(), Some([1; 32]));
}
