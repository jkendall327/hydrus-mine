//! The date-time editor replayed against the reference's
//! (`oracle/fixtures/datetime_editor.json`, from
//! `oracle/record_datetime_editor.py`, in UTC with the time held still):
//! its label, step, date and time, the value it gives and whether that is
//! a change, and what copy and paste say, step by step.

use hydrus_gui_model::datetime_editor::{DateTimeEditor, Said};
use hydrus_gui_model::times_editor::TimeRange;
use serde_json::{Value, json};

fn state(editor: &DateTimeEditor, now: i64) -> Value {
    json!({
        "label": editor.label(now),
        "step_shown": editor.step_shown(),
        "date": editor.date.strftime("%Y-%m-%d").to_string(),
        "time": editor.time.strftime("%H:%M:%S%.3f").to_string(),
        "step": editor.step_ms as f64 / 1000.0,
        "value": editor.value().text(now, &jiff::tz::TimeZone::UTC),
        "changed": editor.has_changes(),
    })
}

fn said(said: &[Said]) -> Vec<Value> {
    said.iter()
        .map(|s| match s {
            Said::Notice(n) => json!({ "notice": n }),
            Said::Warning(w) => json!({ "warning": w }),
            Said::Critical(t, m) => json!({ "critical": t, "message": m }),
        })
        .collect()
}

// leaf: audit-media-datetime-fields, audit-media-datetime-clipboard
#[test]
fn the_date_time_editor_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("datetime_editor.json");
    let now = recorded["now"].as_i64().unwrap();
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut value = TimeRange::default();
        for ms in case["files"].as_array().unwrap() {
            value.add(ms.as_i64(), 1);
        }
        value.step_ms = case["step"].as_i64().unwrap();
        let mut editor = DateTimeEditor::new(value, now * 1000, jiff::tz::TimeZone::UTC);
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(state(&editor, now), steps[0]["state"], "{name} start");
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("{name} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let mut ours = Vec::new();
            let mut copied = Vec::new();
            match action[0].as_str().unwrap() {
                "date" => {
                    editor.date =
                        jiff::civil::Date::strptime("%Y-%m-%d", action[1].as_str().unwrap())
                            .unwrap();
                }
                "time" => {
                    editor.time =
                        jiff::civil::Time::strptime("%H:%M:%S%.f", action[1].as_str().unwrap())
                            .unwrap();
                }
                "step" => {
                    editor.step_ms = (action[1].as_f64().unwrap() * 1000.0).round() as i64;
                }
                "now" => editor.now(),
                "copy" => {
                    let (text, notice) = editor.copy();
                    copied.push(json!(text));
                    ours.push(notice);
                }
                "paste" => ours = editor.paste(action[1].as_str().unwrap()),
                other => panic!("{other}"),
            }
            assert_eq!(json!(said(&ours)), step["said"], "{context}");
            assert_eq!(json!(copied), step["copied"], "{context}");
            assert_eq!(state(&editor, now), step["state"], "{context}");
        }
    }
}
