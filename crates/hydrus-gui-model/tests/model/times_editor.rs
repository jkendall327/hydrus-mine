//! The "manage times" dialog replayed against the reference's
//! (`oracle/fixtures/manage_times.json`, from
//! `oracle/record_manage_times.py`, in UTC with the time held still): the
//! grid's times, the web domain and file service lists, the warning,
//! what is asked, copied and pasted, and what apply writes, step by step.

use hydrus_core::ServiceId;
use hydrus_gui_model::times_editor::{
    DELETE_QUESTION, DOMAIN_EXISTS, DOMAIN_PROMPT, FileTimes, Main, Pasted, SOME_FILES_NO,
    SOME_FILES_QUESTION, SOME_FILES_YES, ServiceName, TimeRange, TimesEditor,
};
use serde_json::{Value, json};

fn main_of(name: &str) -> Main {
    match name {
        "modified" => Main::Modified,
        "archived" => Main::Archived,
        "viewed" => Main::Viewed,
        _ => Main::Preview,
    }
}

fn state(editor: &TimesEditor, now: i64, tz: &jiff::tz::TimeZone) -> Value {
    let rows: Vec<Value> = editor
        .main
        .iter()
        .filter(|t| t.shown)
        .map(|t| {
            json!([
                t.which.label(),
                editor.main_text(t.which, now, tz),
                t.enabled
            ])
        })
        .collect();
    let domains: Vec<Value> = editor
        .domain_rows()
        .into_iter()
        .map(|(d, r)| json!([d, r.value.text(now, tz)]))
        .collect();
    let services: Vec<Value> = editor
        .file_service_rows()
        .into_iter()
        .map(|(s, k, r)| json!(editor.file_service_text(s, k, r, now, tz)))
        .collect();
    json!({
        "rows": rows,
        "warning": editor.warning(),
        "domains": domains,
        "file_services": services,
        "copy_shown": editor.can_copy(),
    })
}

fn asked(question: &str, yes: &str, no: &str) -> Value {
    json!({ "asked": question, "yes": yes, "no": no })
}

#[test]
fn the_manage_times_dialog_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("manage_times.json");
    let now = recorded["now"].as_i64().unwrap();
    let tz = jiff::tz::TimeZone::UTC;
    let services: Vec<ServiceName> = recorded["services"]
        .as_object()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, (name, key))| ServiceName {
            id: ServiceId(u32::try_from(i).unwrap() + 1),
            name: name.clone(),
            key: key.as_str().unwrap().to_owned(),
        })
        .collect();
    let id = |name: &str| services.iter().find(|s| s.name == name).unwrap().id;
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let specs = case["files"].as_array().unwrap();
        let hashes: Vec<&str> = specs.iter().map(|f| f["hash"].as_str().unwrap()).collect();
        let files: Vec<FileTimes> = specs
            .iter()
            .map(|f| FileTimes {
                inbox: f["inbox"].as_bool().unwrap_or(false),
                modified: f["modified"].as_i64(),
                archived: f["archived"].as_i64(),
                viewed: f["viewed"].as_i64(),
                preview: f["preview"].as_i64(),
                domains: f["domains"]
                    .as_object()
                    .map(|d| {
                        d.iter()
                            .map(|(k, v)| (k.clone(), v.as_i64().unwrap()))
                            .collect()
                    })
                    .unwrap_or_default(),
                services: f["services"]
                    .as_array()
                    .map(|s| {
                        s.iter()
                            .map(|x| {
                                (
                                    id(x[0].as_str().unwrap()),
                                    x[1].as_i64().unwrap(),
                                    x[2].as_i64().unwrap(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        let mut editor = TimesEditor::new(files, services.clone(), now * 1000);
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(state(&editor, now, &tz), steps[0]["state"], "{name} start");
        let mut last_copied: Option<String> = None;
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("{name} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let mut said: Vec<Value> = Vec::new();
            let mut copied: Vec<Value> = Vec::new();
            let mut written = None;
            let edited = |value: TimeRange, ms: &Value, step: &Value| {
                let mut value = value.with(ms.as_i64(), false);
                value.step_ms = step.as_i64().unwrap_or(0);
                value
            };
            match action[0].as_str().unwrap() {
                "edit_main" => {
                    let which = main_of(action[1].as_str().unwrap());
                    let value = edited(editor.main_time(which).value, &action[2], &action[3]);
                    editor.set_main(which, value);
                }
                "add_domain" => {
                    said.push(json!({ "entered": DOMAIN_PROMPT }));
                    let domain = action[1].as_str().unwrap();
                    if editor.has_domain(domain) {
                        said.push(json!({ "warning": DOMAIN_EXISTS }));
                    } else {
                        let start = editor.new_domain_value();
                        said.push(json!({ "editing": start.text(now, &tz) }));
                        editor.add_domain(domain, edited(start, &action[2], &json!(0)));
                    }
                }
                "edit_domains" => {
                    // (the first selected in the list's order)
                    let chosen: Vec<String> = serde_json::from_value(action[1].clone()).unwrap();
                    let first = editor
                        .domain_rows()
                        .into_iter()
                        .map(|(d, _)| d.to_owned())
                        .find(|d| chosen.contains(d))
                        .unwrap();
                    let start = editor.domain_value(&first).unwrap();
                    said.push(json!({ "editing": start.text(now, &tz) }));
                    let value = edited(start, &action[2], &action[3]);
                    if value != start {
                        let mut all = false;
                        if editor.domain_edit_asks(&first) {
                            said.push(asked(SOME_FILES_QUESTION, SOME_FILES_YES, SOME_FILES_NO));
                            all = action[4].as_bool().unwrap();
                        }
                        editor.edit_domains(&chosen, value, all);
                    }
                }
                "delete_domains" => {
                    said.push(asked(DELETE_QUESTION, "yes", "no"));
                    if action[2].as_bool().unwrap() {
                        let chosen: Vec<String> =
                            serde_json::from_value(action[1].clone()).unwrap();
                        editor.delete_domains(&chosen);
                    }
                }
                "edit_services" => {
                    let chosen: Vec<(ServiceId, i64)> = action[1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| (id(r[0].as_str().unwrap()), r[1].as_i64().unwrap()))
                        .collect();
                    let (service, kind) = editor
                        .file_service_rows()
                        .into_iter()
                        .map(|(s, k, _)| (s, k))
                        .find(|r| chosen.contains(r))
                        .unwrap();
                    let start = editor.file_service_value(service, kind).unwrap();
                    said.push(json!({ "editing": start.text(now, &tz) }));
                    let value = edited(start, &action[2], &action[3]);
                    if value != start {
                        editor.edit_file_services(&chosen, value);
                    }
                }
                "copy" => {
                    let kinds: Option<Vec<i64>> =
                        serde_json::from_value(action[1].clone()).unwrap();
                    if let Some((text, notice)) = editor.copy(kinds.as_deref()) {
                        said.push(json!({ "notice": notice }));
                        copied.push(json!(text));
                        last_copied = Some(text);
                    }
                }
                "paste" => {
                    let text = match action[1].as_str().unwrap() {
                        "COPIED" => last_copied.clone().unwrap(),
                        other => other.to_owned(),
                    };
                    let mut answers = Vec::new();
                    loop {
                        match editor.paste(&text, &answers) {
                            Pasted::Ask(q) => {
                                said.push(asked(q, SOME_FILES_YES, SOME_FILES_NO));
                                answers.push(true);
                            }
                            Pasted::Done(notice) => {
                                said.push(json!({ "notice": notice }));
                                break;
                            }
                            Pasted::Error(message) => {
                                said.push(
                                    json!({ "critical": "Clipboard Error!", "message": message }),
                                );
                                break;
                            }
                        }
                    }
                }
                "ok" => {
                    let mut ok = true;
                    if let Some(q) = editor.ok_question() {
                        said.push(asked(q, "yes", "no"));
                        ok = action[1].as_bool().unwrap();
                    }
                    let updates: Vec<Value> = editor
                        .updates()
                        .iter()
                        .map(|u| {
                            let files: Vec<&str> = u.files.iter().map(|&i| hashes[i]).collect();
                            let tuple: Value =
                                serde_json::from_str(&editor.serialised(&u.time)).unwrap();
                            json!([
                                if u.time.ms.is_some() { "set" } else { "delete" },
                                files,
                                tuple
                            ])
                        })
                        .collect();
                    let modified = editor.file_modified_update().map(|(files, ms, step)| {
                        let files: Vec<&str> = files.iter().map(|&i| hashes[i]).collect();
                        json!([files, ms, step])
                    });
                    written =
                        Some(json!({ "ok": ok, "updates": updates, "file_modified": modified }));
                }
                other => panic!("{other}"),
            }
            // (the reference's JSON errors are Python's: compared up to
            // the error itself)
            let theirs: Vec<Value> = step["said"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&said)
                .map(
                    |(t, o)| match (t["message"].as_str(), o["message"].as_str()) {
                        (Some(tm), Some(om)) => {
                            let cut =
                                |m: &str| m[..m.find("general error was:").unwrap()].to_owned();
                            assert_eq!(cut(tm), cut(om), "{context}");
                            o.clone()
                        }
                        _ => t.clone(),
                    },
                )
                .collect();
            assert_eq!(json!(said), json!(theirs), "{context}");
            assert_eq!(json!(copied), step["copied"], "{context}");
            if let Some(written) = written {
                assert_eq!(written, step["written"], "{context}");
            }
            assert_eq!(state(&editor, now, &tz), step["state"], "{context}");
        }
    }
}
