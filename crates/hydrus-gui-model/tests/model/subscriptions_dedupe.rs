//! The manage subscriptions dialog's "deduplicate" against the
//! reference's, recorded by `oracle/record_subscriptions_dedupe.py`: each
//! run's questions in turn, answered as the recording answered them, and
//! every subscription's queries (text and file count) after it.

use serde_json::Value as Json;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui_model::subscriptions_dedupe::{Answer, Dedupe, Question, can_dedupe};
use hydrus_gui_model::subscriptions_dialog::{DialogQuery, Subscriptions};
use hydrus_store::queues::StatusCounts;

use crate::subscriptions_list::{checker, status};

fn dialog(recorded: &Json) -> Subscriptions {
    Subscriptions::new(
        recorded["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                let settings = SubscriptionSettings {
                    gug_name: case["gug"].as_str().unwrap().into(),
                    checker: checker(&case["checker"]),
                    ..SubscriptionSettings::default()
                };
                let queries = case["queries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        let mut files = StatusCounts::new();
                        for seed in q["seeds"].as_array().unwrap() {
                            *files.entry(status(seed[0].as_i64().unwrap())).or_default() += 1;
                        }
                        DialogQuery {
                            files,
                            // (a queue each, to see them deleted)
                            queue: Some(i64::try_from(i).unwrap()),
                            ..DialogQuery::new(QueryState::new(q["text"].as_str().unwrap()))
                        }
                    })
                    .collect();
                (
                    None,
                    case["name"].as_str().unwrap().into(),
                    settings,
                    queries,
                )
            })
            .collect(),
    )
}

/// Every subscription by name: its name, downloader and queries.
/// Each subscription: its name, downloader, and queries' texts and file
/// counts.
type State = Vec<(String, String, Vec<(String, usize)>)>;

fn state(dialog: &Subscriptions) -> State {
    let mut subs: Vec<_> = dialog
        .subscriptions
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                s.settings.gug_name.clone(),
                s.queries
                    .iter()
                    .map(|q| (q.state.query_text.clone(), q.files.values().sum()))
                    .collect(),
            )
        })
        .collect();
    subs.sort();
    subs
}

fn recorded_state(step: &Json) -> State {
    step["subscriptions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["gug"].as_str().unwrap().to_owned(),
                s["queries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|q| {
                        (
                            q[0].as_str().unwrap().to_owned(),
                            usize::try_from(q[1].as_u64().unwrap()).unwrap(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Our question as the recording writes it, and the answer the recorded
/// one was given, as ours.
fn check(ours: &Question, theirs: &Json, at: &str) -> Answer {
    let answer = &theirs["answer"];
    let pick = |labels: &[String]| match answer.as_str() {
        Some(start) => Answer::Index(labels.iter().position(|l| l.starts_with(start)).unwrap()),
        None => Answer::Cancel,
    };
    match (ours, theirs["kind"].as_str().unwrap()) {
        (Question::Choice(choice), "choice") => {
            assert_eq!(choice.title, theirs["title"].as_str().unwrap(), "{at}");
            assert_eq!(choice.message, theirs["message"].as_str().unwrap(), "{at}");
            let labels: Vec<String> = serde_json::from_value(theirs["choices"].clone()).unwrap();
            assert_eq!(choice.choices, labels, "{at}");
            pick(&labels)
        }
        (Question::YesNo(message), "yes/no") => {
            assert_eq!(message, theirs["message"].as_str().unwrap(), "{at}");
            // (anything but no, as the reference takes it)
            Answer::Yes(!(answer.is_null() || *answer == Json::Bool(false)))
        }
        (Question::YesYesNo { message, yeses, no }, "yes/yes/no") => {
            assert_eq!(message, theirs["message"].as_str().unwrap(), "{at}");
            let labels: Vec<String> = serde_json::from_value(theirs["yeses"].clone()).unwrap();
            assert_eq!(*yeses, labels, "{at}");
            assert_eq!(no, theirs["no"].as_str().unwrap(), "{at}");
            pick(&labels)
        }
        (Question::Multiple { title, choices }, "multiple") => {
            assert_eq!(title, theirs["title"].as_str().unwrap(), "{at}");
            let labels: Vec<String> = serde_json::from_value(theirs["choices"].clone()).unwrap();
            assert_eq!(*choices, labels, "{at}");
            // (all ticked to start)
            assert_eq!(theirs["checked"], theirs["choices"], "{at}");
            match answer.as_array() {
                Some(texts) => {
                    Answer::Texts(serde_json::from_value(Json::Array(texts.clone())).unwrap())
                }
                None => Answer::Cancel,
            }
        }
        (Question::Warning(message), "warning") => {
            assert_eq!(message, theirs["message"].as_str().unwrap(), "{at}");
            Answer::Cancel
        }
        (ours, kind) => panic!("{at}: we asked {ours:?}, the reference a {kind}"),
    }
}

// leaf: subscriptions-dedupe
#[test]
fn deduplicate_asks_and_dedupes_as_the_reference() {
    let recorded = hydrus_testkit::fixture_json("subscriptions_dedupe.json");
    let now = recorded["now"].as_i64().unwrap();
    let mut dialog = dialog(&recorded);
    assert_eq!(state(&dialog), recorded_state(&recorded["start"]));
    assert_eq!(
        can_dedupe(&dialog, now),
        recorded["start"]["can_dedupe"].as_bool().unwrap()
    );
    for (n, step) in recorded["actions"].as_array().unwrap().iter().enumerate() {
        let asked = step["asked"].as_array().unwrap();
        let (mut dedupe, first) = Dedupe::start(&dialog, now);
        let mut question = Some(first);
        let mut i = 0;
        while let Some(q) = question {
            let at = format!("action {n}, question {i}");
            let theirs = asked
                .get(i)
                .unwrap_or_else(|| panic!("{at}: we asked {q:?} more"));
            let answer = check(&q, theirs, &at);
            question = dedupe.answer(&mut dialog, &answer);
            i += 1;
        }
        assert_eq!(i, asked.len(), "action {n}: the reference asked more");
        assert_eq!(state(&dialog), recorded_state(step), "action {n}");
        assert_eq!(
            can_dedupe(&dialog, now),
            step["can_dedupe"].as_bool().unwrap(),
            "action {n}"
        );
    }
    // the queries gone are deleted with the dialog's changes
    let deleted: usize = dialog
        .subscriptions
        .iter()
        .map(|s| s.deleted_queries.len())
        .sum();
    assert_eq!(deleted, 15 - 7);
}
