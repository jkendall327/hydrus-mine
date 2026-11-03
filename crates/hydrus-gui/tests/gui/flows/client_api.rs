//! Flow (`oracle/record_flow_client_api.py`): the Client API sends a post's
//! URL to a page named "api downloads" with a tag for "my tags"; the page
//! appears in the client and the daemon downloads the file; the API then
//! searches for the tag, reads the file's metadata, adds another tag and a
//! note. Each call's answer, the pages the client shows, the page's file log
//! and the file must be what the reference gave.

use serde_json::{Value as Json, json};
use slint::ComponentHandle as _;

use hydrus_core::pages::{Page, PageContent};
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_store::queues;

use super::{Daemon, Known, Normaliser, Site, State, compare, hashes_in_log, install};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

const POST: u32 = 5;
const PAGE: &str = "api downloads";
/// "my tags"'s key, as hex.
const MY_TAGS: &str = "6c6f63616c2074616773";

/// `text` for a URL's query.
fn encoded(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The media pages, in order (as `GetMediaPages` lists them).
fn media_pages(pages: &[Page], out: &mut Vec<Page>) {
    for page in pages {
        match &page.content {
            PageContent::Pages(children) => media_pages(children, out),
            _ => out.push(page.clone()),
        }
    }
}

/// `/manage_pages/get_pages`'s tree without its keys (as the recorder
/// keeps it).
fn pages_of(pages: &Json) -> Json {
    let mut out = json!({
        "name": pages["name"],
        "page_type": pages["page_type"],
        "selected": pages.get("selected").cloned().unwrap_or(Json::Null),
    });
    if let Some(children) = pages.get("pages") {
        out["pages"] = Json::Array(children.as_array().unwrap().iter().map(pages_of).collect());
    }
    out
}

/// Of `/get_files/file_metadata`'s answer, what this flow changes.
fn metadata_of(answer: &Json, n: &Normaliser) -> Json {
    Json::Array(
        answer["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                let mut mine: Vec<String> = m["tags"][MY_TAGS]["storage_tags"]["0"]
                    .as_array()
                    .map(|t| t.iter().map(|t| t.as_str().unwrap().to_owned()).collect())
                    .unwrap_or_default();
                mine.sort();
                let mut urls: Vec<String> = m["known_urls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|u| n.text(u.as_str().unwrap()))
                    .collect();
                urls.sort();
                json!({
                    "hash": m["hash"],
                    "is_inbox": m["is_inbox"],
                    "known_urls": urls,
                    "my tags": mine,
                    "notes": m.get("notes").cloned().unwrap_or(Json::Null),
                })
            })
            .collect(),
    )
}

/// The "api downloads" page's queue, once the client shows it.
fn api_queue(bound: &Bound) -> Option<i64> {
    (bound.sync)();
    let session = bound.pages.borrow().session().clone();
    let mut pages = Vec::new();
    media_pages(&session.pages, &mut pages);
    pages
        .iter()
        .find(|p| p.name == PAGE)
        .and_then(|p| match &p.content {
            PageContent::Downloader { queues, .. } => queues.first().copied(),
            _ => None,
        })
}

#[test]
fn the_client_api_drives_a_download_then_tags_it_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_client_api.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_client_api.json", &recording);
    let n = Normaliser::new(&site);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: Some(&site),
        n: &n,
    };
    let ok = |(status, answer): (u16, Json)| -> Json {
        assert_eq!(status, 200, "{answer}");
        answer
    };
    let step = |name: &str, calls: Vec<Json>| -> Json {
        let files = api_queue(&bound).map_or(json!([]), |q| state.file_log(q));
        let session = bound.pages.borrow().session().clone();
        let mut pages = Vec::new();
        media_pages(&session.pages, &mut pages);
        json!({
            "step": name,
            "calls": calls,
            "gui pages": pages.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "page": files,
            "files": state.files(hashes_in_log(&files)),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. add url: to the named page, with a tag
    let mut calls = Vec::new();
    let answer = ok(daemon.api(
        "POST",
        "/add_urls/add_url",
        Some(&json!({
            "url": format!("{}/post/{POST}", site.base),
            "destination_page_name": PAGE,
            "service_keys_to_additional_tags": {MY_TAGS: ["from api"]},
        })),
    ));
    let mut kept = serde_json::Map::new();
    for (k, v) in answer.as_object().unwrap() {
        if k != "version" && k != "hydrus_version" {
            kept.insert(
                k.clone(),
                v.as_str().map_or_else(|| v.clone(), |s| json!(n.text(s))),
            );
        }
    }
    calls.push(json!({"call": "add_url", "answer": kept}));
    let page_done = || {
        let Some(queue) = api_queue(&bound) else {
            return false;
        };
        let files = store.read(|c| queues::file_seeds(c, queue)).unwrap();
        !files.is_empty()
            && files
                .iter()
                .all(|f| f.status != queues::SeedStatus::Unknown)
    };
    site.wait_for_a_request(20, "the api's url");
    super::wait_until_idle(&daemon, page_done);
    let pages = ok(daemon.api("GET", "/manage_pages/get_pages", None));
    calls.push(json!({"call": "get_pages", "answer": pages_of(&pages["pages"])}));
    steps.push(step("add url", calls));

    // 2. search for the tag, and the file's metadata
    let mut calls = Vec::new();
    let found = ok(daemon.api(
        "GET",
        &format!(
            "/get_files/search_files?tags={}&return_hashes=true&return_file_ids=false",
            encoded(r#"["from api"]"#)
        ),
        None,
    ));
    let hashes = found["hashes"].clone();
    calls.push(json!({"call": "search_files", "answer": {"hashes": hashes}}));
    let metadata = || {
        metadata_of(
            &ok(daemon.api(
                "GET",
                &format!(
                    "/get_files/file_metadata?hashes={}&include_notes=true",
                    encoded(&hashes.to_string())
                ),
                None,
            )),
            &n,
        )
    };
    calls.push(json!({"call": "file_metadata", "answer": metadata()}));
    steps.push(step("search", calls));

    // 3. another tag and a note
    let mut calls = Vec::new();
    let answer = ok(daemon.api(
        "POST",
        "/add_tags/add_tags",
        Some(&json!({"hashes": hashes, "service_keys_to_tags": {MY_TAGS: ["api tag"]}})),
    ));
    calls.push(json!({"call": "add_tags", "answer": answer}));
    let mut answer = ok(daemon.api(
        "POST",
        "/add_notes/set_notes",
        Some(&json!({"hash": hashes[0], "notes": {"comment": "from the api"}})),
    ));
    for k in ["version", "hydrus_version"] {
        answer.as_object_mut().unwrap().remove(k);
    }
    calls.push(json!({"call": "set_notes", "answer": answer}));
    super::wait_until_idle(&daemon, || true);
    calls.push(json!({"call": "file_metadata", "answer": metadata()}));
    steps.push(step("tag and note", calls));

    compare(&steps, &recording, KNOWN);
}
