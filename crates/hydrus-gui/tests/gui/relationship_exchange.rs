//! Siblings/parents clipboard and .txt pair exchange, replayed from
//! `oracle/record_relationship_exchange.py` through the real window.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::options_gui_support::Client;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn open(client: &Client, kind: &str) -> hydrus_gui::TagRelationshipsWindow {
    let ui = &client.ui;
    let top = (0..ui.get_menu_titles().row_count())
        .find(|&i| ui.get_menu_titles().row_data(i).unwrap().label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let at = (0..pane.lines.row_count())
        .find(|&i| pane.lines.row_data(i).unwrap().label.starts_with(kind))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    client
        .bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn rows(w: &hydrus_gui::TagRelationshipsWindow) -> Vec<Vec<String>> {
    w.get_rows()
        .iter()
        .map(|r| r.cells.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn pair_of(row: &[String]) -> (String, String) {
    (row[1].clone(), row[2].clone())
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

// Not tagged: the self-pair and two-ideals steps differ from the reference
// (DIFFERENCES.md) and are filtered out below, so this proves only part of
// the exchange leaves.
#[test]
fn pair_import_and_export_replay_the_reference_on_local_and_repository_services() {
    let recording = hydrus_testkit::fixture_json("relationship_exchange.json");
    let client = Client::basic();
    let dir = tempfile::tempdir().unwrap();
    client
        .store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![63; 16]),
                "exchange repository",
                &hydrus_store::services::ServiceKind::TagRepository(
                    hydrus_store::services::RepositoryConfig::default(),
                ),
            )?;
            Ok(())
        })
        .unwrap();
    let copied = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    for run in recording.as_array().unwrap() {
        let name = run["kind"].as_str().unwrap();
        let local = run["local"].as_bool().unwrap();
        let kind = if name == "siblings" {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        // The recording's local service is the client's first local tag service.
        let service_name = if local {
            run["service"].as_str().unwrap()
        } else {
            "exchange repository"
        };
        let service = client
            .store
            .snapshot()
            .services
            .by_name(service_name)
            .unwrap()
            .id;
        for pair in run["initial"].as_array().unwrap() {
            tag_relations::apply(
                &client.store,
                kind,
                vec![RelationUpdate {
                    service,
                    left: hydrus_core::Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: hydrus_core::Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: RelationAction::Add,
                }],
            )
            .unwrap();
        }
        // the recent reasons start empty, as in the recording
        client
            .store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::reference_options::RecentPetitionReasons::default(),
                )
            })
            .unwrap();
        let w = open(&client, name);
        let at = w
            .get_service_names()
            .iter()
            .position(|n| n == service_name)
            .unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        w.set_show_all(true);
        w.invoke_filters_changed();
        // Documented difference: hydrus-rs rejects self-pairs, and a sibling
        // batch giving one tag two ideals, where the reference stores them.
        let mut rejected: BTreeSet<(String, String)> = BTreeSet::new();
        for step in run["steps"].as_array().unwrap() {
            let text = step["text"].as_str().unwrap();
            let asked = step["asked"].as_array().unwrap();
            let file = step["how"] == "txt";
            if file {
                let path = dir.path().join(format!("{name}.txt"));
                std::fs::write(&path, text).unwrap();
                let title = asked[0]["message"].as_str().unwrap().to_owned();
                hydrus_gui::set_picker(move |_, t| {
                    assert_eq!(t, title);
                    vec![path.clone()]
                });
            } else {
                let text = text.to_owned();
                hydrus_gui::set_paster(move || text.clone());
            }
            w.invoke_import_pairs(file);
            let divergent = text == "self\nself" || (name == "siblings" && text == "p\nq\np\nr");
            if divergent {
                let lines: Vec<&str> = text.lines().collect();
                for chunk in lines.chunks(2) {
                    rejected.insert((chunk[0].to_owned(), chunk[1].to_owned()));
                }
                // the reason the reference asked is still asked; then the rejection
                let mut asked_rejection = false;
                while !w.get_question().is_empty() {
                    if w.get_ask_reason() {
                        w.set_reason("oracle reason".into());
                    } else {
                        let q = w.get_question().to_string();
                        assert!(
                            q.starts_with("Cannot add self-referencing")
                                || q.starts_with("The relationships include a cycle"),
                            "{q}"
                        );
                        asked_rejection = true;
                    }
                    w.invoke_answered(true);
                }
                assert!(asked_rejection, "{text}");
            } else {
                for question in asked {
                    match question["kind"].as_str().unwrap() {
                        "reason" => {
                            assert!(w.get_ask_reason(), "{text}");
                            assert_eq!(w.get_question(), question["message"].as_str().unwrap());
                            let offered: Vec<String> = w
                                .get_reason_suggestions()
                                .iter()
                                .map(|s| s.to_string())
                                .collect();
                            assert_eq!(offered, strings(&question["suggestions"]), "{text}");
                            w.set_reason(question["answer"].as_str().unwrap().into());
                            w.invoke_answered(true);
                        }
                        "yes_no" => {
                            assert!(!w.get_ask_reason(), "{text}");
                            assert_eq!(w.get_question(), question["message"].as_str().unwrap());
                            assert_eq!(w.get_yes_label(), question["yes"].as_str().unwrap());
                            assert_eq!(w.get_no_label(), question["no"].as_str().unwrap());
                            w.invoke_answered(question["answer"].as_bool().unwrap());
                        }
                        // shown once the import is entered
                        "information" | "file_dialog" => {}
                        other => panic!("unexpected {other}"),
                    }
                }
                assert_eq!(w.get_question(), "", "{text}");
            }
            let information: Vec<&str> = asked
                .iter()
                .filter(|q| q["kind"] == "information")
                .map(|q| q["message"].as_str().unwrap())
                .collect();
            assert_eq!(
                w.get_error(),
                information.first().copied().unwrap_or(""),
                "{text}"
            );
            let expected: BTreeSet<Vec<String>> = step["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(strings)
                .filter(|r| !rejected.contains(&pair_of(r)))
                .collect();
            let shown: BTreeSet<Vec<String>> = rows(&w).into_iter().collect();
            assert_eq!(shown, expected, "{name} local={local} after {text:?}");
        }
        // the list's default order is the reference's
        let order: Vec<(String, String)> = run["display_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_str().unwrap().into(), p[1].as_str().unwrap().into()))
            .filter(|p| !rejected.contains(p))
            .collect();
        let shown: Vec<(String, String)> = rows(&w).iter().map(|r| pair_of(r)).collect();
        assert_eq!(shown, order);
        for export in run["exports"].as_array().unwrap() {
            let selected: Vec<(String, String)> = export["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (p[0].as_str().unwrap().into(), p[1].as_str().unwrap().into()))
                .collect();
            let text = export["text"].as_str().unwrap();
            if selected.is_empty() {
                // the reference's export menu is disabled without a selection
                assert!(!w.get_has_selection());
                assert_eq!(text, "");
                continue;
            }
            for (i, pair) in selected.iter().enumerate() {
                let at = rows(&w).iter().position(|r| pair_of(r) == *pair).unwrap();
                w.invoke_row_clicked(i32::try_from(at).unwrap(), i > 0, false);
            }
            copied.borrow_mut().clear();
            w.invoke_export_pairs(false);
            assert_eq!(*copied.borrow(), [text]);
            let path = dir.path().join(format!("{name}-export.txt"));
            hydrus_gui::set_picker({
                let path = path.clone();
                move |_, title| {
                    assert_eq!(title, "Set the export path.");
                    vec![path.clone()]
                }
            });
            w.invoke_export_pairs(true);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
        // What the import staged is what Apply commits.
        let last = run["steps"].as_array().unwrap().last().unwrap()["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(strings)
            .filter(|r| !rejected.contains(&pair_of(r)))
            .collect::<Vec<_>>();
        w.invoke_apply();
        assert!(client.bound.tag_relationships.borrow().is_none());
        let w = open(&client, name);
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        w.set_show_all(true);
        w.invoke_filters_changed();
        let committed: BTreeSet<(String, String, String)> = rows(&w)
            .into_iter()
            .map(|r| (r[0].clone(), r[1].clone(), r[2].clone()))
            .collect();
        let expected: BTreeSet<(String, String, String)> = last
            .into_iter()
            .filter(|r| !(local && r[0] == "(-) "))
            .map(|r| {
                // a local service commits at once; a repository keeps its pends
                let status = if local { String::new() } else { r[0].clone() };
                (status, r[1].clone(), r[2].clone())
            })
            .collect();
        assert_eq!(committed, expected, "{name} local={local} after apply");
        w.invoke_cancel();
    }
}
