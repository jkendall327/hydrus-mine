//! Which registered call an "open externally" runs, and with what, replaying
//! the reference's recorded launches (`dispatch` and `specific_empty` in
//! `oracle/fixtures/open_externally.json`, from
//! `oracle/record_open_externally.py`): the first call of the URL queue and of
//! the most specific file-type row, an explicitly empty row meaning the OS
//! default, and an empty URL queue meaning the OS default.
#![cfg(unix)]
use super::open_externally::{await_output, capture_process};
use hydrus_core::Mime;
use hydrus_core::external_calls::{ActualCall, Callable, Manager, Parameter, Pipeline};
use hydrus_core::open_externally::{CallRef, Routing};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::settings;
use serde_json::Value;
use slint::ComponentHandle as _;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn key(value: &Value) -> [u8; 32] {
    [u8::from_str_radix(&value["key"].as_str().unwrap()[..2], 16).unwrap(); 32]
}

fn refs(manager: &Manager, recorded: &Value) -> Vec<CallRef> {
    recorded
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let call = manager.calls.iter().find(|c| c.key == key(value)).unwrap();
            assert_eq!(call.name, value["name"].as_str().unwrap());
            call.into()
        })
        .collect()
}

/// The path a recorded input names, from the file store's own folders on.
fn in_store(path: &str) -> &str {
    &path[path.find("client_files/").unwrap() + "client_files".len()..]
}

// leaf: audit-options-nested-open-file-call-list-order
#[test]
fn opening_runs_the_first_call_the_reference_ran_with_its_inputs() {
    let fixture = hydrus_testkit::fixture_json("open_externally.json");
    let dispatch = fixture["dispatch"].as_array().unwrap();
    let _windows = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let output = tempfile::tempdir().unwrap();
    let captured = output.path().join("owned routing capture.txt");
    // the recorder's calls; its two URL and two file calls write what they get
    let manager = Manager {
        calls: fixture["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut call = Callable::new(value["name"].as_str().unwrap());
                call.key = key(value);
                call.pipeline = if value["pipeline"] == 1 {
                    Pipeline::File
                } else {
                    Pipeline::Url
                };
                call.call = match (call.pipeline, call.name.starts_with("Default OS")) {
                    (Pipeline::File, true) => ActualCall::DefaultFile,
                    (Pipeline::Url, true) => ActualCall::DefaultUrl,
                    (Pipeline::File, false) => ActualCall::Process(capture_process(
                        &captured,
                        &[Parameter::Path, Parameter::Uri, Parameter::Hash, Parameter::FileId],
                        &format!("{}|%path%|%path_uri%|%hash%|%file_id%", call.name),
                    )),
                    (Pipeline::Url, false) => ActualCall::Process(capture_process(
                        &captured,
                        &[Parameter::Url],
                        &format!("{}|%url%", call.name),
                    )),
                };
                call
            })
            .collect(),
    };
    let mime = |code: &str| Mime::from_code(code.parse().unwrap()).unwrap();
    // the routes as the reference's Options left them (`washed`)
    let washed = Routing {
        urls: refs(&manager, &fixture["washed"]["urls"]),
        files: fixture["washed"]["rows"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(code, calls)| (mime(code), refs(&manager, calls)))
            .collect(),
    };
    let (calls, routes) = (manager.clone(), washed.clone());
    store
        .write(move |ctx| {
            settings::set(ctx.conn(), &calls)?;
            settings::set(ctx.conn(), &routes)
        })
        .unwrap();
    let launched = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
    });
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let ids = page.borrow().results().to_vec();
    let media = store
        .read(|conn| hydrus_store::media::load_basic(conn, &ids))
        .unwrap();
    let recorded_hash = dispatch[1]["inputs"]["5"].as_str().unwrap();
    let jpeg = media
        .iter()
        .find(|m| m.hash.to_hex() == recorded_hash)
        .unwrap();
    assert_eq!(jpeg.info.as_ref().unwrap().mime, Mime::ImageJpeg);
    page.borrow_mut()
        .hit(Some(ids.iter().position(|id| *id == jpeg.hash_id).unwrap()), false, false);
    let path = hydrus_gui::thumbnail_menu::paths(&store, &[jpeg.hash_id])
        .pop()
        .unwrap();

    // 1: a URL goes to the first URL call, with the URL
    let url = dispatch[0]["inputs"]["2"].as_str().unwrap();
    assert!(bound.external_launches.url(&store, url));
    await_output(&captured, &format!("{}|{url}", dispatch[0]["name"].as_str().unwrap()));
    std::fs::remove_file(&captured).unwrap();

    // 2: a JPEG goes to its image row's first call, with the file's path,
    // URI, hash and id
    ui.invoke_open_externally();
    let inputs = &dispatch[1]["inputs"];
    let recorded_path = inputs["0"].as_str().unwrap();
    assert!(path.ends_with(in_store(recorded_path)), "{path} / {recorded_path}");
    assert_eq!(
        inputs["1"].as_str().unwrap(),
        format!("file://{recorded_path}")
    );
    assert_eq!(i64::from(jpeg.hash_id.0), inputs["6"].as_i64().unwrap());
    await_output(
        &captured,
        &format!(
            "{}|{path}|file://{path}|{recorded_hash}|{}",
            dispatch[1]["name"].as_str().unwrap(),
            jpeg.hash_id.0
        ),
    );
    std::fs::remove_file(&captured).unwrap();
    assert!(launched.borrow().is_empty());

    // 3: an explicitly empty JPEG row is the OS default, over image's call
    let calls_by_name = |name: &str| {
        let call = manager.calls.iter().find(|c| c.name == name).unwrap();
        CallRef::from(call)
    };
    let specific = Routing {
        urls: washed.urls.clone(),
        files: BTreeMap::from([
            (Mime::GeneralFile, vec![calls_by_name("File two")]),
            (Mime::GeneralImage, vec![calls_by_name("File one 日本")]),
            (Mime::ImageJpeg, Vec::new()),
        ]),
    };
    let mut washed_specific = specific.clone();
    washed_specific.wash(&mut manager.clone());
    assert_eq!(
        washed_specific.file_calls(Mime::ImageJpeg),
        refs(&manager, &fixture["specific_empty"]).as_slice()
    );
    store
        .write(move |ctx| settings::set(ctx.conn(), &specific))
        .unwrap();
    ui.invoke_open_externally();
    assert_eq!(dispatch[2]["name"], "Default OS File Launch");
    assert_eq!(launched.borrow().as_slice(), std::slice::from_ref(&path));
    assert!(!captured.exists());

    // 4: a missing URL call is reported, not run (the reference's notice)
    let missing = Routing {
        urls: vec![CallRef {
            key: [9; 32],
            name: "missing".into(),
        }],
        files: BTreeMap::from([(Mime::GeneralFile, Vec::new())]),
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &missing))
        .unwrap();
    assert!(!bound.external_launches.url(&store, url));
    let notice = bound.external_launches.notice().unwrap();
    assert_eq!(notice.get_message().as_str(), fixture["notices"][3]);
    notice.invoke_cancelled();

    // 5: an empty URL queue is the OS default
    let empty = Routing::default();
    store
        .write(move |ctx| settings::set(ctx.conn(), &empty))
        .unwrap();
    assert!(bound.external_launches.url(&store, url));
    assert_eq!(dispatch[3]["name"], "Default OS URL Launch");
    assert_eq!(launched.borrow().last().unwrap(), url);
    assert_eq!(dispatch.len(), 4, "the missing call ran nothing");
    hydrus_gui::set_launcher(|_| {});
}
