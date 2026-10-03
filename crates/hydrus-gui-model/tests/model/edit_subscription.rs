//! The edit subscription dialog against the reference's, recorded by
//! `oracle/record_edit_subscription.py`: each recorded subscription's
//! fields as the dialog opens, then a run of the queries list's buttons
//! on "artist one" (pasting queries, check now, pause/play, delete) with
//! what each asked, how it was answered, and the queries after it, and
//! last what "apply" gives back after the fields are changed.

use serde_json::Value as Json;

use hydrus_gui_model::edit_subscription::{EMPTY_CLIPBOARD, EditSubscription, Paste};
use hydrus_gui_model::subscriptions_dialog::DELETE_QUESTION;

use crate::subscriptions_list::dialog_subscription;

/// The subscriptions the recorder opened, from the list's recording.
fn opened(now: i64) -> Vec<EditSubscription> {
    let list = hydrus_testkit::fixture_json("subscriptions_list.json");
    assert_eq!(list["now"].as_i64().unwrap(), now);
    list["subscriptions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let (_, name, settings, queries) = dialog_subscription(s, now);
            EditSubscription::new(name, settings, queries)
        })
        .collect()
}

fn queries(dialog: &EditSubscription, now: i64) -> Json {
    Json::Array(
        dialog
            .order(now)
            .into_iter()
            .map(|k| {
                let s = &dialog.get(k).unwrap().query.state;
                serde_json::json!({
                    "text": s.query_text,
                    "paused": s.paused,
                    "check_now": s.check_now,
                    "dead": s.dead,
                })
            })
            .collect(),
    )
}

#[test]
fn the_dialog_opens_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("edit_subscription.json");
    let now = recorded["now"].as_i64().unwrap();
    let dialogs = opened(now);
    assert_eq!(dialogs.len(), recorded["opened"].as_array().unwrap().len());
    for (dialog, want) in dialogs.iter().zip(recorded["opened"].as_array().unwrap()) {
        let s = &dialog.settings;
        // (the fixture has no downloaders, so none is found)
        let ours = serde_json::json!({
            "name": dialog.name,
            "delay": dialog.delay_text(now),
            "downloader": dialog.downloader_label(false),
            "initial_file_limit": s.initial_file_limit,
            "periodic_file_limit": s.periodic_file_limit,
            "random_sample": s.this_is_a_random_sample,
            "paused": s.paused,
            "show_a_popup_while_working": s.show_a_popup_while_working,
            "publish_files_to_popup_button": s.publish_files_to_popup_button,
            "publish_files_to_page": s.publish_files_to_page,
            "publish_label_override": s.publish_label_override,
            "merge_query_publish_events": s.merge_query_publish_events,
        });
        assert_eq!(&ours, want, "{}", dialog.name);
    }
}

#[test]
fn the_query_buttons_act_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("edit_subscription.json");
    let now = recorded["now"].as_i64().unwrap();
    let mut dialog = opened(now)
        .into_iter()
        .find(|d| d.name == "artist one")
        .unwrap();
    for (i, step) in recorded["steps"].as_array().unwrap().iter().enumerate() {
        let at = format!("step {i} ({})", step["do"]);
        let rows: Vec<&str> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        dialog.select_texts(&rows);
        let asked = step["asked"].as_array().unwrap();
        match step["do"].as_str().unwrap() {
            "paste queries" => {
                let clipboard = step["clipboard"].as_str().unwrap();
                match dialog.paste(clipboard) {
                    Paste::Empty => {
                        assert_eq!(asked[0]["kind"], "warning", "{at}");
                        assert_eq!(asked[0]["message"], EMPTY_CLIPBOARD, "{at}");
                    }
                    Paste::Nothing(message) => {
                        assert_eq!(asked[0]["kind"], "information", "{at}");
                        assert_eq!(asked[0]["message"], message.as_str(), "{at}");
                    }
                    Paste::Ask {
                        message,
                        yeses,
                        no,
                        plan,
                    } => {
                        let q = &asked[0];
                        assert_eq!(q["message"], message.as_str(), "{at}");
                        assert_eq!(q["no"], no.as_str(), "{at}");
                        let revive = if yeses.len() == 1 {
                            assert_eq!(q["kind"], "yes/no", "{at}");
                            assert_eq!(q["yes"], yeses[0].as_str(), "{at}");
                            q["answer"].as_bool().unwrap().then_some(true)
                        } else {
                            assert_eq!(q["kind"], "yes/yes/no", "{at}");
                            assert_eq!(q["yeses"], serde_json::json!(yeses), "{at}");
                            q["answer"].as_u64().map(|a| a == 0)
                        };
                        if let Some(revive) = revive {
                            dialog.apply_paste(&plan, revive);
                        }
                    }
                }
            }
            "check now" => {
                let mut check =
                    hydrus_gui_model::edit_subscription::CheckQueriesNow::new(&dialog, now);
                let mut n = 0;
                let mut cancelled = false;
                while let Some(choice) = check.next(&dialog) {
                    let q = &asked[n];
                    assert_eq!(q["title"], choice.title.as_str(), "{at}");
                    assert_eq!(q["message"], choice.message.as_str(), "{at}");
                    assert_eq!(q["choices"], serde_json::json!(choice.choices), "{at}");
                    n += 1;
                    let Some(a) = q["answer"].as_u64() else {
                        cancelled = true;
                        break;
                    };
                    check.answer(&dialog, a as usize);
                }
                assert_eq!(n, asked.len(), "{at}");
                if !cancelled {
                    check.apply(&mut dialog);
                }
            }
            "pause/play" => dialog.pause_play(now),
            "delete" => {
                assert_eq!(asked[0]["message"], DELETE_QUESTION, "{at}");
                if asked[0]["answer"].as_bool().unwrap() {
                    dialog.delete_selected(now);
                }
            }
            other => panic!("{other}"),
        }
        assert_eq!(queries(&dialog, now), step["queries"], "{at}");
        assert_eq!(dialog.delay_text(now), step["delay"], "{at}");
    }

    // the fields changed, then "apply"
    let edits = &recorded["edits"];
    dialog.name = edits["name"].as_str().unwrap().into();
    let s = &mut dialog.settings;
    s.paused = edits["paused"].as_bool().unwrap();
    s.initial_file_limit = edits["initial_file_limit"].as_u64();
    s.periodic_file_limit = edits["periodic_file_limit"].as_u64();
    s.this_is_a_random_sample = edits["random_sample"].as_bool().unwrap();
    s.show_a_popup_while_working = edits["show_a_popup_while_working"].as_bool().unwrap();
    s.publish_files_to_popup_button = edits["publish_files_to_popup_button"].as_bool().unwrap();
    s.publish_files_to_page = edits["publish_files_to_page"].as_bool().unwrap();
    s.publish_label_override = edits["publish_label_override"].as_str().map(Into::into);
    s.merge_query_publish_events = edits["merge_query_publish_events"].as_bool().unwrap();
    let value = &recorded["value"];
    assert_eq!(dialog.name, value["name"]);
    assert_eq!(dialog.settings.no_work_until, value["no_work_until"]);
    assert_eq!(queries(&dialog, now), value["queries"]);
}
