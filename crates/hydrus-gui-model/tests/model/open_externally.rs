//! Actual Qt queue snapshots, stable-key washing and default dispatch boundaries.
use hydrus_core::{
    Mime,
    external_calls::{ActualCall, Callable, Manager, Pipeline, Process},
    open_externally::{CallRef, Routing},
};
use hydrus_gui_model::open_externally::{self as model, Queue};
use serde_json::{Value, json};
fn manager(fixture: &Value) -> Manager {
    Manager {
        calls: fixture["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut call = Callable::new(value["name"].as_str().unwrap());
                call.key =
                    [u8::from_str_radix(&value["key"].as_str().unwrap()[..2], 16).unwrap(); 32];
                call.pipeline = if value["pipeline"] == 1 {
                    Pipeline::File
                } else {
                    Pipeline::Url
                };
                // The real recorder creates four synthetic process calls and
                // only keys 05/06 as OS defaults. Pipeline alone does not imply OS.
                call.call = match call.key[0] {
                    5 => ActualCall::DefaultUrl,
                    6 => ActualCall::DefaultFile,
                    _ => ActualCall::Process(Process {
                        executable: "synthetic-program".into(),
                        arguments: vec![
                            if call.pipeline == Pipeline::File {
                                "%path%"
                            } else {
                                "%url%"
                            }
                            .into(),
                        ],
                        ..Process::default()
                    }),
                };
                call
            })
            .collect(),
    }
}
fn hex_key(key: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(64);
    for byte in key {
        write!(text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}
fn refs(queue: &Queue) -> Value {
    json!(
        queue
            .values()
            .iter()
            .map(|value| json!({"key":hex_key(&value.key),"name":value.name}))
            .collect::<Vec<_>>()
    )
}

#[test]
fn actual_qt_registered_choices_queue_order_and_protected_mime_rows() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let mut manager = manager(&fixture);
    manager.calls[0].name = "Zulu URL".into();
    let mut queue = Queue::new(&[(&manager.calls[4]).into()]);
    assert_eq!(refs(&queue), fixture["events"][0]["urls"]);
    let choices = model::choices(&manager, Pipeline::Url, &queue.values());
    assert_eq!(
        choices
            .iter()
            .map(|value| value.name.clone())
            .collect::<Vec<_>>(),
        vec!["Zulu URL", "alpha URL"]
    );
    // Cancel leaves an empty selection/unchanged queue; Add appends unselected.
    assert_eq!(refs(&queue), fixture["events"][1]["urls"]);
    queue.put(None, (&manager.calls[0]).into());
    assert_eq!(refs(&queue), fixture["events"][2]["urls"]);
    assert!(queue.selection.is_empty());
    queue.put(None, (&manager.calls[1]).into());
    assert_eq!(refs(&queue), fixture["events"][3]["urls"]);
    queue.click(0, false, false);
    queue.move_selected(true);
    assert_eq!(refs(&queue), fixture["events"][4]["urls"]);
    assert!(
        model::choices(&manager, Pipeline::Url, &queue.values()).is_empty(),
        "Edit excludes its own current identity too"
    );
    assert_eq!(model::exhausted(Pipeline::Url), fixture["notices"][0]);
    let selected = queue.selected();
    queue.delete(&selected);
    assert_eq!(refs(&queue), fixture["events"][6]["urls"]);
    queue.click(0, false, false);
    queue.put(queue.editing(), (&manager.calls[4]).into());
    assert_eq!(refs(&queue), fixture["events"][7]["urls"]);
    assert_eq!(
        model::choices(&manager, Pipeline::Url, &queue.values()).len(),
        1,
        "one choice still needs an owned chooser in GUI"
    );
    assert_eq!(
        model::mime_choices()
            .iter()
            .map(
                |mime| json!({"mime":mime.code(),"human":mime.human_name(),"label":mime.mimetype()})
            )
            .collect::<Vec<_>>(),
        fixture["mime_choices"].as_array().unwrap().clone()
    );
    let mut editor = model::Editor::new(Routing::default());
    editor.put(Mime::ImagePng, vec![(&manager.calls[2]).into()], true);
    editor.click(0, true, false);
    let selected = editor.selected();
    editor.delete(&selected);
    assert!(
        editor.routing.files.contains_key(&Mime::ImagePng),
        "GeneralFile anywhere in selection protects all rows"
    );
    editor.selection.select_only(Some(Mime::ImagePng));
    editor.delete(&editor.selected());
    assert!(!editor.routing.files.contains_key(&Mime::ImagePng));
}
#[test]
fn washing_renames_deduplicates_removes_missing_without_name_remap_and_persists() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let mut manager = manager(&fixture);
    let missing = CallRef {
        key: [9; 32],
        name: "missing".into(),
    };
    let mut routing = Routing {
        urls: vec![
            CallRef {
                key: [1; 32],
                name: "old label".into(),
            },
            (&manager.calls[0]).into(),
            missing.clone(),
            (&manager.calls[2]).into(),
        ],
        files: std::collections::BTreeMap::from([
            (Mime::GeneralFile, vec![]),
            (Mime::GeneralImage, vec![(&manager.calls[2]).into()]),
            (Mime::ImagePng, vec![]),
        ]),
    };
    routing.wash(&mut manager);
    assert_eq!(refs(&Queue::new(&routing.urls)), fixture["washed"]["urls"]);
    for (mime, values) in &routing.files {
        assert_eq!(
            refs(&Queue::new(values)),
            fixture["washed"]["rows"][mime.code().to_string()]
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    let saved = routing.clone();
    let calls = manager.clone();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &saved)?;
            hydrus_store::settings::set(ctx.conn(), &calls)
        })
        .unwrap();
    drop(store);
    let reopened = hydrus_store::Store::open(directory.path()).unwrap();
    assert_eq!(
        reopened
            .read(hydrus_store::settings::get::<Routing>)
            .unwrap(),
        routing
    );
    let mut absent = Routing {
        urls: vec![missing],
        ..routing.clone()
    };
    absent.wash(&mut manager);
    assert!(
        absent.urls.is_empty(),
        "a nonempty missing queue is washed empty without substituting another named call"
    );
    let mut default_manager = Manager::default();
    let first = default_manager.ensure_os(Pipeline::File);
    assert_eq!(default_manager.ensure_os(Pipeline::File), first);
    assert_eq!(default_manager.calls[0].pipeline, Pipeline::File);
}
#[test]
fn configured_empty_specific_and_missing_call_never_fall_through_custom_route() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let manager = manager(&fixture);
    let mut routing = Routing {
        urls: vec![(&manager.calls[0]).into()],
        files: std::collections::BTreeMap::from([
            (Mime::GeneralFile, vec![(&manager.calls[3]).into()]),
            (Mime::GeneralImage, vec![(&manager.calls[2]).into()]),
            (Mime::ImageJpeg, vec![]),
        ]),
    };
    let plan = model::file(
        &manager,
        &routing,
        Mime::ImageJpeg,
        "/synthetic/漢.jpg",
        "file:///synthetic/%E6%BC%A2.jpg",
        "1234",
        17,
    )
    .unwrap();
    assert_eq!(plan.call, ActualCall::DefaultFile);
    routing.files.remove(&Mime::ImageJpeg);
    assert_eq!(routing.file_calls(Mime::ImageJpeg)[0].key, [3; 32]);
    routing.files.remove(&Mime::GeneralImage);
    assert_eq!(routing.file_calls(Mime::ImageJpeg)[0].key, [4; 32]);
    let url = fixture["dispatch"][0]["inputs"]["2"].as_str().unwrap();
    let missing = CallRef {
        key: [9; 32],
        name: "missing".into(),
    };
    routing.urls = vec![missing, (&manager.calls[0]).into()];
    assert_eq!(
        format!(
            "Sorry, could not open that URL: {}",
            model::url(&manager, &routing, url).unwrap_err()
        ),
        fixture["notices"][3]
    );
    routing.urls = vec![(&manager.calls[2]).into()];
    assert!(
        model::url(&manager, &routing, url)
            .unwrap_err()
            .contains("was the wrong type (send single file)")
    );
}
