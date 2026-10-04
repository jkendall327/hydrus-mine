//! The sidecar editors, step by step as the reference's
//! (`oracle/record_sidecar_editors.py`, on the basic fixture): the source
//! and destination editors in both contexts (what they show, what "change
//! type" and "service" ask, and the value), new routers and what "ok" asks,
//! and the routers list's templates.

use hydrus_core::url::strings::StringProcessor;
use hydrus_gui_model::sidecar_editors::{
    self as editors, Context, ExporterEditor, ImporterEditor, Kind, NodeBoxes, Shown,
    TimestampDetail, change_type_choices,
};
use hydrus_gui_model::sidecars::{exporter_text, importer_text, router_text};
use hydrus_legacy::objects::sidecars::{exporter, importer, router};
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::sidecar::{Exporter, Importer, Router, Source, TagDisplay};
use hydrus_store::Store;
use serde_json::{Value, json};

fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn object(value: &Value) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

fn kind_by_label(label: &str) -> Kind {
    [
        Kind::MediaTags,
        Kind::MediaNotes,
        Kind::MediaUrls,
        Kind::MediaTimestamps,
        Kind::Txt,
        Kind::Json,
    ]
    .into_iter()
    .find(|k| k.label() == label)
    .unwrap_or_else(|| panic!("{label}"))
}

fn choice(options: &[String], value: usize) -> Value {
    json!({ "options": options, "value": options[value] })
}

/// What a source or destination shows, as the recording has it.
fn boxes_json(boxes: &NodeBoxes, shown: Shown) -> Value {
    let naming = &boxes.naming;
    let separators: Vec<String> = editors::SEPARATOR_CHOICES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let t = &boxes.timestamp;
    let names = |list: &[(hydrus_core::ServiceKey, String)]| {
        list.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>()
    };
    let canvases: Vec<String> = editors::CANVAS_CHOICES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    json!({
        "type": boxes.kind.label(),
        "sidecar": if shown.naming {
            json!({
                "remove_ext": naming.naming.remove_actual_filename_ext,
                "suffix": naming.naming.suffix,
                "converter": naming.converter_label(),
                "example": naming.example,
                "result": naming.result(),
            })
        } else {
            Value::Null
        },
        "separator": if shown.separator {
            json!({
                "choice": choice(&separators, boxes.separator.choice),
                "custom": boxes.separator.custom,
                "custom_enabled": boxes.separator.custom_enabled(),
            })
        } else {
            Value::Null
        },
        "timestamp": if shown.timestamp {
            let detail = t.detail();
            json!({
                "type": choice(&t.kind_labels(), t.kind),
                "file_service": if detail == TimestampDetail::FileService { choice(&names(&t.file_services), t.file_service) } else { Value::Null },
                "deleted_service": if detail == TimestampDetail::DeletedService { choice(&names(&t.deleted_services), t.deleted_service) } else { Value::Null },
                "canvas": if detail == TimestampDetail::Canvas { choice(&canvases, t.canvas) } else { Value::Null },
                "domain": if detail == TimestampDetail::Domain { json!(t.domain) } else { Value::Null },
            })
        } else {
            Value::Null
        },
    })
}

fn merge(mut a: Value, b: &Value) -> Value {
    for (k, v) in b.as_object().unwrap() {
        a[k] = v.clone();
    }
    a
}

/// Do a step to the shared boxes; `true` if it was one of theirs.
fn step_boxes(boxes: &mut NodeBoxes, step: &[Value]) -> bool {
    let arg = &step[1];
    match step[0].as_str().unwrap() {
        "suffix" => arg
            .as_str()
            .unwrap()
            .clone_into(&mut boxes.naming.naming.suffix),
        "remove_ext" => boxes.naming.naming.remove_actual_filename_ext = arg.as_bool().unwrap(),
        "example" => arg.as_str().unwrap().clone_into(&mut boxes.naming.example),
        "separator" => {
            boxes.separator.choice = editors::SEPARATOR_CHOICES
                .iter()
                .position(|c| c == arg)
                .unwrap();
            if let Some(custom) = step[2].as_str() {
                custom.clone_into(&mut boxes.separator.custom);
            }
        }
        "timestamp_type" => {
            boxes.timestamp.kind = boxes
                .timestamp
                .kind_labels()
                .iter()
                .position(|l| l == arg)
                .unwrap();
        }
        "file_service" => {
            boxes.timestamp.file_service = boxes
                .timestamp
                .file_services
                .iter()
                .position(|(_, n)| n == arg)
                .unwrap();
        }
        "deleted_service" => {
            boxes.timestamp.deleted_service = boxes
                .timestamp
                .deleted_services
                .iter()
                .position(|(_, n)| n == arg)
                .unwrap();
        }
        "canvas" => {
            boxes.timestamp.canvas = editors::CANVAS_CHOICES
                .iter()
                .position(|c| c == arg)
                .unwrap();
        }
        "domain" => arg
            .as_str()
            .unwrap()
            .clone_into(&mut boxes.timestamp.domain),
        _ => return false,
    }
    true
}

fn change_type_said(allowed: &[Kind], current: Kind, destination: bool) -> Value {
    let choices: Vec<Value> = change_type_choices(allowed, current, destination)
        .unwrap()
        .iter()
        .map(|(label, description, _)| json!([label, description]))
        .collect();
    json!({
        "buttons": editors::TYPE_TITLE,
        "message": if destination { editors::DESTINATION_TYPE_QUESTION } else { editors::SOURCE_TYPE_QUESTION },
        "choices": choices,
    })
}

fn service_said(choices: &[(String, hydrus_core::ServiceKey)]) -> Value {
    let names: Vec<&String> = choices.iter().map(|(n, _)| n).collect();
    if names.len() == 1 {
        json!({ "auto": editors::SELECT_SERVICE, "choices": names })
    } else {
        json!({ "select": editors::SELECT_SERVICE, "choices": names })
    }
}

#[test]
fn the_sidecar_editors_show_and_edit_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("sidecar_editors.json");
    let (_dir, store) = store();
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let namer = |key: &str| recorded["names"][key].as_str().map(str::to_owned);
    let service_name = |key: &str| namer(key).unwrap_or_else(|| "unknown service".to_owned());
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    for case in recorded["cases"].as_array().unwrap() {
        let context = if case["context"] == "import" {
            Context::Import
        } else {
            Context::Export
        };
        let start = case["start"].as_str().unwrap();
        let editor_kind = case["editor"].as_str().unwrap();
        let states = case["states"].as_array().unwrap();
        let at0 = format!("{editor_kind} {} {start}", case["context"]);
        match editor_kind {
            "importer" => {
                let starting: Importer = match start {
                    "txt" => editors::new_importer(Kind::Txt, StringProcessor::default()),
                    "json" => editors::new_importer(Kind::Json, StringProcessor::default()),
                    _ => Importer {
                        source: Source::MediaTags {
                            service_key: my_tags.clone(),
                            display: TagDisplay::DisplayActual,
                        },
                        processor: StringProcessor::default(),
                    },
                };
                let mut e = ImporterEditor::new(&starting, services);
                for state in states {
                    let at = format!("{at0}: {}", state["step"]);
                    if let Some(step) = state["step"].as_array() {
                        let mut said: Vec<Value> = Vec::new();
                        if !step_boxes(&mut e.boxes, step) {
                            match step[0].as_str().unwrap() {
                                "change_type" => {
                                    said.push(change_type_said(
                                        context.importers(),
                                        e.boxes.kind,
                                        false,
                                    ));
                                    e.change_type(
                                        kind_by_label(step[1].as_str().unwrap()),
                                        services,
                                    );
                                }
                                "display" => {
                                    e.display = if step[1] == "stored tags" {
                                        TagDisplay::Storage
                                    } else {
                                        TagDisplay::DisplayActual
                                    };
                                }
                                "service" => {
                                    let choices = e.service_choices(services);
                                    said.push(service_said(&choices));
                                    let (_, key) =
                                        choices.iter().find(|(n, _)| n == &step[1]).unwrap();
                                    e.boxes.service_key = hex::encode(key.as_bytes());
                                }
                                other => panic!("{other}"),
                            }
                        }
                        assert_eq!(Value::Array(said), state["said"], "{at}");
                    }
                    let theirs = &state["state"];
                    let shown = e.shown();
                    let ours = merge(
                        boxes_json(&e.boxes, shown),
                        &json!({
                            "tags": if shown.tags {
                                json!({
                                    "service": service_name(&e.boxes.service_key),
                                    "display": editors::TAG_DISPLAY_CHOICES[usize::from(e.display == TagDisplay::DisplayActual)],
                                })
                            } else {
                                Value::Null
                            },
                            "json_formula": shown.json_formula,
                            "processing": e.processor.button_label(),
                        }),
                    );
                    for key in ours.as_object().unwrap().keys() {
                        assert_eq!(ours[key], theirs[key], "{at}: {key}");
                    }
                    match e.value() {
                        Ok(value) => {
                            assert_eq!(
                                importer_text(&value, &namer),
                                theirs["value"]["text"],
                                "{at}"
                            );
                            assert_eq!(
                                value,
                                importer(&object(&theirs["value"]["object"])).unwrap(),
                                "{at}"
                            );
                        }
                        Err(error) => assert_eq!(error, theirs["value"]["error"], "{at}"),
                    }
                }
            }
            "exporter" => {
                let starting: Exporter = match start {
                    "txt" => editors::new_exporter(Kind::Txt),
                    _ => editors::new_exporter(Kind::MediaTags),
                };
                let mut e = ExporterEditor::new(&starting, services);
                for state in states {
                    let at = format!("{at0}: {}", state["step"]);
                    if let Some(step) = state["step"].as_array() {
                        let mut said: Vec<Value> = Vec::new();
                        if !step_boxes(&mut e.boxes, step) {
                            match step[0].as_str().unwrap() {
                                "change_type" => {
                                    said.push(change_type_said(
                                        context.exporters(),
                                        e.boxes.kind,
                                        true,
                                    ));
                                    e.change_type(
                                        kind_by_label(step[1].as_str().unwrap()),
                                        services,
                                    );
                                }
                                "service" => {
                                    let choices = e.service_choices(services);
                                    said.push(service_said(&choices));
                                    let (_, key) =
                                        choices.iter().find(|(n, _)| n == &step[1]).unwrap();
                                    e.boxes.service_key = hex::encode(key.as_bytes());
                                }
                                "forced_name" => {
                                    e.forced_name = step[1].as_str().map(str::to_owned);
                                }
                                "nested" => {
                                    e.nested = serde_json::from_value(step[1].clone()).unwrap();
                                }
                                other => panic!("{other}"),
                            }
                        }
                        assert_eq!(Value::Array(said), state["said"], "{at}");
                    }
                    check_exporter(&e, &state["state"], &namer, &at);
                }
            }
            "router" => {
                let starting: Router = match start {
                    "new" => editors::new_router(context),
                    "txt notes" => Router {
                        importers: vec![editors::new_importer(
                            Kind::Txt,
                            StringProcessor::default(),
                        )],
                        processor: editors::default_router_processor(),
                        exporter: editors::new_exporter(Kind::MediaNotes),
                    },
                    _ => Router {
                        importers: vec![editors::new_importer(
                            Kind::MediaNotes,
                            StringProcessor::default(),
                        )],
                        processor: editors::default_router_processor(),
                        exporter: editors::new_exporter(Kind::Txt),
                    },
                };
                for state in states {
                    let at = format!("{at0}: {}", state["step"]);
                    if state["step"].is_array() {
                        let mut said: Vec<Value> = editors::ok_questions(&starting)
                            .into_iter()
                            .map(|q| json!({ "asked": q }))
                            .take(1)
                            .collect();
                        // (each question answered no: it stops at the first)
                        said.push(json!({ "ok": said.is_empty() }));
                        assert_eq!(Value::Array(said), state["said"], "{at}");
                    }
                    let theirs = &state["state"];
                    let sources: Vec<String> = starting
                        .importers
                        .iter()
                        .map(|i| importer_text(i, &namer))
                        .collect();
                    assert_eq!(json!(sources), theirs["sources"], "{at}");
                    assert_eq!(
                        starting.processor.button_label(),
                        theirs["processing"],
                        "{at}"
                    );
                    let e = ExporterEditor::new(&starting.exporter, services);
                    check_exporter(&e, &theirs["destination"], &namer, &at);
                }
            }
            _ => {
                let theirs = &states[0]["state"];
                assert_eq!(theirs["labels"], json!([]), "{at0}");
                let ours = editors::templates(context, services);
                let their_templates = theirs["templates"].as_array().unwrap();
                assert_eq!(ours.len(), their_templates.len(), "{at0}");
                for ((label, description, routers), t) in ours.iter().zip(their_templates) {
                    assert_eq!(*label, t["label"], "{at0}");
                    assert_eq!(*description, t["description"], "{at0}");
                    let their_routers = t["routers"].as_array().unwrap();
                    assert_eq!(routers.len(), their_routers.len(), "{at0}");
                    for (r, theirs) in routers.iter().zip(their_routers) {
                        assert_eq!(router_text(r, true, &namer), theirs["text"], "{at0}");
                        assert_eq!(*r, router(&object(&theirs["object"])).unwrap(), "{at0}");
                    }
                }
            }
        }
    }
}

fn check_exporter(
    e: &ExporterEditor,
    theirs: &Value,
    namer: &dyn Fn(&str) -> Option<String>,
    at: &str,
) {
    let shown = e.shown();
    let ours = merge(
        boxes_json(&e.boxes, shown),
        &json!({
            "tags": if shown.tags {
                json!({ "service": namer(&e.boxes.service_key).unwrap_or_else(|| "unknown service".into()) })
            } else {
                Value::Null
            },
            "forced_name": if shown.forced_name { json!(e.forced_name) } else { Value::Null },
            "nested": if shown.nested { json!(e.nested) } else { Value::Null },
        }),
    );
    for key in ours.as_object().unwrap().keys() {
        assert_eq!(ours[key], theirs[key], "{at}: {key}");
    }
    match e.value() {
        Ok(value) => {
            assert_eq!(
                exporter_text(&value, namer),
                theirs["value"]["text"],
                "{at}"
            );
            assert_eq!(
                value,
                exporter(&object(&theirs["value"]["object"])).unwrap(),
                "{at}"
            );
        }
        Err(error) => assert_eq!(error, theirs["value"]["error"], "{at}"),
    }
}

#[test]
fn recorded_router_tables_and_child_strings_use_real_sidecars_and_read_only_media() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let files = tempfile::tempdir().unwrap();
    for case in hydrus_testkit::fixture_json("sidecar_testing.json")
        .as_array()
        .unwrap()
    {
        for (name, text) in case["documents"].as_object().unwrap() {
            std::fs::write(files.path().join(name), text.as_str().unwrap()).unwrap();
        }
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let value = router(&object).unwrap();
        let objects = if let Some(media) = case["media"].as_array() {
            media
                .iter()
                .map(|item| {
                    let hash = item["hash"]
                        .as_str()
                        .unwrap()
                        .parse::<hydrus_core::Sha256>()
                        .unwrap();
                    let id = store
                        .write(move |ctx| hydrus_store::master::intern_hash(ctx.conn(), &hash))
                        .unwrap();
                    let urls = item["urls"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|url| url.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>();
                    let notes = item["notes"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(name, text)| (name.clone(), text.as_str().unwrap().to_owned()))
                        .collect::<Vec<_>>();
                    store
                        .write_content(move |writer| {
                            writer.add_urls(&[id], &urls)?;
                            for (name, text) in notes {
                                writer.set_note(id, &name, &text)?;
                            }
                            Ok(())
                        })
                        .unwrap();
                    editors::TestObject::Media(id)
                })
                .collect::<Vec<_>>()
        } else if case["case"] == "no_examples" {
            Vec::new()
        } else {
            ["one.png", "two.png", "missing.png"]
                .into_iter()
                .map(|name| {
                    editors::TestObject::File(
                        files.path().join(name).to_string_lossy().into_owned(),
                    )
                })
                .collect()
        };
        let before = store.snapshot().revision;
        for table in case["tables"].as_array().unwrap() {
            let source = usize::try_from(table["source"].as_u64().unwrap()).unwrap();
            let mut rows = editors::router_test_rows(&store, &value, source, &objects);
            for row in &mut rows {
                row[0] = row[0]
                    .replace(files.path().to_str().unwrap(), "<examples>")
                    .replace('\\', "/");
            }
            assert_eq!(
                serde_json::to_value(rows).unwrap(),
                table["rows"],
                "{}",
                case["case"]
            );
        }
        assert_eq!(
            serde_json::to_value(editors::router_test_strings(&store, &value, &objects)).unwrap(),
            case["processor_texts"]
        );
        assert_eq!(
            store.snapshot().revision,
            before,
            "testing must not write media or export sidecars"
        );
        assert!(case["processor_context"].as_object().unwrap().is_empty());
    }
}
