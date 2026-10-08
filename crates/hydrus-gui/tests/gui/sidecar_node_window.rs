//! The sidecar source and destination window put through the reference's
//! recording (`oracle/fixtures/sidecar_editors.json`) as a user would: the
//! window's controls set, "change type" and "service" answered, and what it
//! shows read back at every step; then, from a second window replayed to each
//! step, what "apply" gives (as text and object) or why it won't.
//!
//! The one `nested` step (the JSON object names set whole on the model) is
//! not replayed here: the names are driven by their own test in `sidecars.rs`
//! against `sidecar_json_names.json`.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use hydrus_core::url::strings::StringProcessor;
use hydrus_gui::{SidecarNodeWindow, headless, sidecars_window};
use hydrus_gui_model::sidecar_editors::{
    self as editors, CANVAS_CHOICES, Context, Kind, SEPARATOR_CHOICES,
};
use hydrus_gui_model::sidecars::{exporter_text, importer_text};
use hydrus_legacy::objects::sidecars::{exporter, importer};
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::sidecar::{Importer, Source, TagDisplay};
use hydrus_store::Store;
use serde_json::{Value, json};
use slint::Model as _;

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

fn at(n: usize) -> i32 {
    i32::try_from(n).unwrap()
}

fn labels(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    model.iter().map(|s| s.to_string()).collect()
}

fn choice(options: &[String], value: i32) -> Value {
    json!({ "value": options[usize::try_from(value).unwrap()], "options": options })
}

/// What the window shows, as the recording has the model's boxes.
fn shown(window: &SidecarNodeWindow, destination: bool) -> Value {
    let separators = labels(&window.get_separators());
    let timestamp_kinds = labels(&window.get_timestamp_kinds());
    let detail = window.get_detail();
    let mut state = json!({
        "type": window.get_type_label().to_string(),
        "sidecar": if window.get_show_naming() {
            json!({
                "remove_ext": window.get_remove_ext(),
                "suffix": window.get_suffix().to_string(),
                "converter": window.get_converter_label().to_string(),
                "example": window.get_example().to_string(),
                "result": window.get_result().to_string(),
            })
        } else { Value::Null },
        "separator": if window.get_show_separator() {
            json!({
                "choice": choice(&separators, window.get_separator()),
                "custom": window.get_custom_separator().to_string(),
            })
        } else { Value::Null },
        "timestamp": if window.get_show_timestamp() {
            json!({
                "type": choice(&timestamp_kinds, window.get_timestamp_kind()),
                "file_service": if detail == 1 { choice(&labels(&window.get_file_services()), window.get_file_service()) } else { Value::Null },
                "deleted_service": if detail == 2 { choice(&labels(&window.get_deleted_services()), window.get_deleted_service()) } else { Value::Null },
                "canvas": if detail == 3 { choice(&labels(&window.get_canvases()), window.get_canvas()) } else { Value::Null },
                "domain": if detail == 4 { json!(window.get_domain().to_string()) } else { Value::Null },
            })
        } else { Value::Null },
    });
    if destination {
        state["tags"] = if window.get_show_tags() {
            json!({ "service": window.get_service_label().to_string() })
        } else {
            Value::Null
        };
        state["nested"] = if window.get_show_nested() {
            json!(
                window
                    .get_nested_rows()
                    .iter()
                    .map(|r| r.cells.row_data(0).unwrap().to_string())
                    .collect::<Vec<_>>()
            )
        } else {
            Value::Null
        };
        state["forced_name"] = if window.get_show_forced() {
            if window.get_forced_on() {
                json!(window.get_forced_name().to_string())
            } else {
                Value::Null
            }
        } else {
            Value::Null
        };
    } else {
        state["tags"] = if window.get_show_tags() {
            json!({
                "service": window.get_service_label().to_string(),
                "display": labels(&window.get_displays())[usize::try_from(window.get_display()).unwrap()],
            })
        } else {
            Value::Null
        };
        state["json_formula"] = json!(window.get_show_json());
        state["processing"] = json!(window.get_processing().to_string());
    }
    state
}

/// Do one recorded step to the window; what the reference said while doing it.
fn step(window: &SidecarNodeWindow, step: &[Value], context: &str) -> Vec<Value> {
    let arg = &step[1];
    let name = arg.as_str();
    let index_in = |options: Vec<String>| {
        at(options
            .iter()
            .position(|o| Some(o.as_str()) == name)
            .unwrap_or_else(|| panic!("{arg} in {options:?} ({context})")))
    };
    let mut said = Vec::new();
    match step[0].as_str().unwrap() {
        "suffix" => window.set_suffix(name.unwrap().into()),
        "remove_ext" => window.set_remove_ext(arg.as_bool().unwrap()),
        "example" => window.set_example(name.unwrap().into()),
        "separator" => {
            window.set_separator(index_in(
                SEPARATOR_CHOICES.iter().map(|s| (*s).to_owned()).collect(),
            ));
            if let Some(custom) = step[2].as_str() {
                window.set_custom_separator(custom.into());
            }
        }
        "timestamp_type" => {
            window.set_timestamp_kind(index_in(labels(&window.get_timestamp_kinds())));
        }
        "file_service" => window.set_file_service(index_in(labels(&window.get_file_services()))),
        "deleted_service" => {
            window.set_deleted_service(index_in(labels(&window.get_deleted_services())));
        }
        "canvas" => window.set_canvas(index_in(
            CANVAS_CHOICES.iter().map(|s| (*s).to_owned()).collect(),
        )),
        "domain" => window.set_domain(name.unwrap().into()),
        "display" => window.set_display(index_in(labels(&window.get_displays()))),
        "forced_name" => {
            window.set_forced_on(name.is_some());
            window.set_forced_name(name.unwrap_or_default().into());
        }
        "change_type" => {
            window.invoke_change_type();
            assert!(window.get_asking(), "{context}");
            let message = window.get_asking_message().to_string();
            let (question, described) = message.split_once("\n\n").unwrap();
            let choices: Vec<Value> = described
                .lines()
                .map(|l| {
                    let (label, description) = l.split_once(": ").unwrap();
                    json!([label, description])
                })
                .collect();
            said.push(json!({
                "buttons": window.get_asking_title().to_string(),
                "message": question,
                "choices": choices,
            }));
            let options = labels(&window.get_asking_choices());
            let wanted = kind_by_label(name.unwrap()).label();
            window.invoke_chosen(at(options.iter().position(|o| o == wanted).unwrap()));
            return said;
        }
        "service" => {
            window.invoke_choose_service();
            if window.get_asking() {
                let options = labels(&window.get_asking_choices());
                said.push(
                    json!({ "select": window.get_asking_title().to_string(), "choices": options }),
                );
                window.invoke_chosen(index_in(options));
            } else {
                // (only one to choose: taken)
                said.push(json!({ "auto": editors::SELECT_SERVICE, "choices": [window.get_service_label().to_string()] }));
            }
            return said;
        }
        other => panic!("{other} ({context})"),
    }
    window.invoke_changed();
    said
}

fn open(
    store: &Arc<Store>,
    slots: &sidecars_window::Slots,
    context: Context,
    node: &sidecars_window::Node,
) -> (
    SidecarNodeWindow,
    Rc<RefCell<Option<sidecars_window::Node>>>,
) {
    let done = Rc::new(RefCell::new(None));
    let window = sidecars_window::open_node(store, context, node, slots, {
        let done = done.clone();
        Rc::new(move |node| *done.borrow_mut() = Some(node))
    })
    .unwrap();
    (window, done)
}

// leaf: audit-shared-sidecar-source-types
// leaf: audit-shared-sidecar-destination-types
// leaf: audit-shared-sidecar-destination-note
// leaf: audit-shared-sidecar-destination-timestamp
// leaf: audit-shared-sidecar-source-timestamp
// leaf: sidecar-details
// leaf: sidecar-txt-separator
// leaf: audit-shared-sidecar-source-tags
// leaf: audit-shared-sidecar-destination-tags
#[test]
fn the_sidecar_node_window_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("sidecar_editors.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let namer = |key: &str| recorded["names"][key].as_str().map(str::to_owned);
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let mut checked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let kind = case["editor"].as_str().unwrap();
        if kind != "importer" && kind != "exporter" {
            continue;
        }
        let context = if case["context"] == "import" {
            Context::Import
        } else {
            Context::Export
        };
        let start = case["start"].as_str().unwrap();
        let destination = kind == "exporter";
        let node = if destination {
            sidecars_window::Node::Destination(match start {
                "txt" => editors::new_exporter(Kind::Txt),
                _ => editors::new_exporter(Kind::MediaTags),
            })
        } else {
            sidecars_window::Node::Source(match start {
                "txt" => editors::new_importer(Kind::Txt, StringProcessor::default()),
                "json" => editors::new_importer(Kind::Json, StringProcessor::default()),
                _ => Importer {
                    source: Source::MediaTags {
                        service_key: my_tags.clone(),
                        display: TagDisplay::DisplayActual,
                    },
                    processor: StringProcessor::default(),
                },
            })
        };
        let states = case["states"].as_array().unwrap();
        // (the steps from a `nested` one on are not replayed here)
        let usable = states
            .iter()
            .position(|s| s["step"][0] == "nested")
            .unwrap_or(states.len());
        for upto in 0..usable {
            let at_state = format!(
                "{kind} {} {start}: {}",
                case["context"], states[upto]["step"]
            );
            // one window, every step to here, what it shows
            let slots = sidecars_window::Slots::default();
            let (window, _) = open(&store, &slots, context, &node);
            for state in &states[..=upto] {
                if let Some(s) = state["step"].as_array() {
                    let said = step(&window, s, &at_state);
                    assert_eq!(Value::Array(said), state["said"], "{at_state}");
                }
            }
            let theirs = &states[upto]["state"];
            let ours = shown(&window, destination);
            // (the same boxes are shown as the reference's, bar its final value)
            let mut theirs_keys: Vec<&String> = theirs.as_object().unwrap().keys().collect();
            theirs_keys.retain(|k| *k != "value");
            theirs_keys.sort();
            let mut ours_keys: Vec<&String> = ours.as_object().unwrap().keys().collect();
            ours_keys.sort();
            assert_eq!(ours_keys, theirs_keys, "{at_state}");
            for key in ours.as_object().unwrap().keys() {
                let mut ours_key = ours[key].clone();
                let mut theirs_key = theirs[key].clone();
                if key == "separator" && !ours_key.is_null() {
                    // (whether the custom box is enabled is the window's own layout)
                    theirs_key.as_object_mut().unwrap().remove("custom_enabled");
                    ours_key.as_object_mut().unwrap().remove("custom_enabled");
                }
                assert_eq!(ours_key, theirs_key, "{at_state}: {key}");
            }
            window.invoke_cancel();

            // a second, "apply"ed: what it gives, or why not
            let slots = sidecars_window::Slots::default();
            let (window, done) = open(&store, &slots, context, &node);
            for state in &states[..=upto] {
                if let Some(s) = state["step"].as_array() {
                    step(&window, s, &at_state);
                }
            }
            window.invoke_apply();
            let value = &theirs["value"];
            match done.borrow().as_ref() {
                Some(sidecars_window::Node::Source(got)) => {
                    assert_eq!(
                        json!(importer_text(got, &namer)),
                        value["text"],
                        "{at_state}"
                    );
                    assert_eq!(
                        *got,
                        importer(&object(&value["object"])).unwrap(),
                        "{at_state}"
                    );
                }
                Some(sidecars_window::Node::Destination(got)) => {
                    assert_eq!(
                        json!(exporter_text(got, &namer)),
                        value["text"],
                        "{at_state}"
                    );
                    assert_eq!(
                        *got,
                        exporter(&object(&value["object"])).unwrap(),
                        "{at_state}"
                    );
                }
                None => {
                    assert!(window.get_asking(), "{at_state}");
                    assert_eq!(
                        json!(window.get_asking_message().to_string()),
                        value["error"],
                        "{at_state}"
                    );
                }
            }
            if done.borrow().is_none() {
                window.invoke_cancel();
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 35, "every recorded source and destination state");
}

// leaf: audit-shared-sidecar-destination-tags
#[test]
fn a_destination_whose_tag_service_is_gone_warns_as_the_reference_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    // hydrus/client/gui/metadata/ClientGUIMetadataMigrationExporters.py: the warning
    // shown when the exporter's tag service is not one the client has
    let warning = "Hey, the tag service for your exporter does not seem to exist! Maybe it was deleted. Please select a new one that does.";
    let gone = hydrus_parse::sidecar::Exporter::MediaTags {
        service_key: hex::encode([0x5e; 32]),
    };
    let slots = sidecars_window::Slots::default();
    let (window, _) = open(
        &store,
        &slots,
        Context::Export,
        &sidecars_window::Node::Destination(gone),
    );
    assert!(window.get_asking());
    assert_eq!(window.get_asking_message(), warning);
    window.invoke_cancel();
    // a service the client has is not warned about
    let (window, _) = open(
        &store,
        &slots,
        Context::Export,
        &sidecars_window::Node::Destination(editors::new_exporter(Kind::MediaTags)),
    );
    assert!(!window.get_asking());
    window.invoke_cancel();
}
