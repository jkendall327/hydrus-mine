//! `/manage_pages/*` beyond what the recorded scenario shows (`pages`,
//! replayed in `conformance.rs`, with one page and no client open): pages
//! in notebooks and which are selected, what is asked of pages while the
//! client is open, the client's media viewers, and selections.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::{Value as Json, json};
use tower::ServiceExt as _;

use hydrus_core::HashId;
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_core::search::context::FileSearchContext;
use hydrus_store::sessions::{self, LAST_SESSION, MediaViewer, PageCommand};

mod common;

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

async fn call(router: &axum::Router, request: Request<Body>) -> (u16, Json) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
}

async fn get(router: &axum::Router, path: &str) -> (u16, Json) {
    let request = Request::get(path)
        .header("Hydrus-Client-API-Access-Key", KEY)
        .body(Body::empty())
        .unwrap();
    call(router, request).await
}

async fn post(router: &axum::Router, path: &str, body: &Json) -> (u16, Json) {
    let request = Request::post(path)
        .header("Hydrus-Client-API-Access-Key", KEY)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    call(router, request).await
}

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

/// The fixture with a session of `a`, a notebook `nb` of `b` and `c`, and
/// `d`.
struct Pages {
    fixture: common::Fixture,
    router: axum::Router,
    a: PageKey,
    nb: PageKey,
    b: PageKey,
    c: PageKey,
    d: PageKey,
}

fn pages() -> Pages {
    let fixture = common::imported_store("basic");
    let (a, b, c, d) = (search("a"), search("b"), search("c"), search("d"));
    let nb = Page {
        key: PageKey::random(),
        name: "nb".into(),
        content: PageContent::Pages(vec![b.clone(), c.clone()]),
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![a.clone(), nb.clone(), d.clone()],
    };
    fixture
        .state
        .store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let router = hydrus_api::router(fixture.state.clone());
    Pages {
        fixture,
        router,
        a: a.key,
        nb: nb.key,
        b: b.key,
        c: c.key,
        d: d.key,
    }
}

/// Each page's name and whether it is selected, from `get_pages`.
fn selected(body: &Json) -> Vec<(String, bool)> {
    fn walk(page: &Json, out: &mut Vec<(String, bool)>) {
        out.push((
            page["name"].as_str().unwrap().to_owned(),
            page["selected"].as_bool().unwrap(),
        ));
        for child in page["pages"].as_array().into_iter().flatten() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(&body["pages"], &mut out);
    out
}

fn names(list: &[(&str, bool)]) -> Vec<(String, bool)> {
    list.iter().map(|(n, s)| ((*n).to_owned(), *s)).collect()
}

#[tokio::test]
async fn the_selected_pages_lead_to_the_page_shown() {
    let p = pages();
    let (status, body) = get(&p.router, "/manage_pages/get_pages").await;
    assert_eq!(status, 200);
    // (each notebook's first, with none shown yet)
    assert_eq!(
        selected(&body),
        names(&[
            ("top page notebook", true),
            ("a", true),
            ("nb", false),
            ("b", false),
            ("c", false),
            ("d", false)
        ])
    );
    let nb = &body["pages"]["pages"][1];
    assert_eq!(nb["page_type"], 10);
    assert_eq!(nb["is_media_page"], false);
    assert_eq!(nb["pages"][1]["page_key"], p.c.to_hex());
    assert!(body["pages"]["pages"][0].get("pages").is_none());

    let (status, _) = post(
        &p.router,
        "/manage_pages/focus_page",
        &json!({ "page_key": p.c.to_hex() }),
    )
    .await;
    assert_eq!(status, 200);
    let (_, body) = get(&p.router, "/manage_pages/get_pages").await;
    assert_eq!(
        selected(&body),
        names(&[
            ("top page notebook", true),
            ("a", false),
            ("nb", true),
            ("b", false),
            ("c", true),
            ("d", false)
        ])
    );
    // a notebook focused shows its first page
    post(
        &p.router,
        "/manage_pages/focus_page",
        &json!({ "page_key": p.nb.to_hex() }),
    )
    .await;
    let (_, body) = get(&p.router, "/manage_pages/get_pages").await;
    assert_eq!(
        selected(&body),
        names(&[
            ("top page notebook", true),
            ("a", false),
            ("nb", true),
            ("b", true),
            ("c", false),
            ("d", false)
        ])
    );
    // and a notebook's info is a notebook's
    let (status, body) = get(
        &p.router,
        &format!("/manage_pages/get_page_info?page_key={}", p.nb.to_hex()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body["page_info"],
        json!({
            "name": "nb",
            "page_key": p.nb.to_hex(),
            "page_state": 0,
            "page_type": 10,
            "is_media_page": false,
        })
    );
    let _ = (p.a, p.d);
}

#[tokio::test]
async fn what_is_asked_waits_for_an_open_client() {
    let p = pages();
    let store = p.fixture.state.store.clone();
    let open = hydrus_store::store::lock_gui(store.dir()).unwrap().unwrap();
    let (status, _) = post(
        &p.router,
        "/manage_pages/add_files",
        &json!({ "page_key": p.b.to_hex(), "file_ids": [2, 1] }),
    )
    .await;
    assert_eq!(status, 200);
    for (path, key) in [("focus_page", p.c), ("refresh_page", p.nb)] {
        let (status, _) = post(
            &p.router,
            &format!("/manage_pages/{path}"),
            &json!({ "page_key": key.to_hex() }),
        )
        .await;
        assert_eq!(status, 200);
    }
    // (the session as it was: the client does these to its pages)
    let b = p.b;
    let (files, shown, asked) = store
        .write(move |ctx| {
            Ok((
                sessions::page_files(ctx.conn(), &b)?,
                sessions::shown(ctx.conn(), LAST_SESSION)?,
                sessions::take_commands(ctx.conn())?,
            ))
        })
        .unwrap();
    assert!(files.is_empty());
    assert_eq!(shown, None);
    assert_eq!(
        asked,
        [
            (p.b, PageCommand::AddFiles(vec![HashId(2), HashId(1)])),
            (p.c, PageCommand::Focus),
            (p.nb, PageCommand::Refresh),
        ]
    );
    // with the client closed, they are done to the session
    drop(open);
    post(
        &p.router,
        "/manage_pages/add_files",
        &json!({ "page_key": p.b.to_hex(), "file_ids": [2, 1] }),
    )
    .await;
    let files = store.read(|conn| sessions::page_files(conn, &p.b)).unwrap();
    assert_eq!(files, [HashId(2), HashId(1)]);
}

#[tokio::test]
async fn media_viewers_are_the_open_clients() {
    let p = pages();
    let store = p.fixture.state.store.clone();
    store
        .write(|ctx| {
            sessions::set_media_viewers(
                ctx.conn(),
                &[
                    MediaViewer {
                        canvas_key: [7; 32],
                        canvas_type: 0,
                        file: Some(HashId(1)),
                    },
                    MediaViewer {
                        canvas_key: [8; 32],
                        canvas_type: 0,
                        file: None,
                    },
                ],
            )
        })
        .unwrap();
    // (what a closed client left behind isn't open)
    let (status, body) = get(&p.router, "/manage_pages/get_media_viewers").await;
    assert_eq!(status, 200);
    assert_eq!(body["media_viewers"], json!([]));

    let _open = hydrus_store::store::lock_gui(store.dir()).unwrap().unwrap();
    let (_, body) = get(&p.router, "/manage_pages/get_media_viewers").await;
    let viewers = body["media_viewers"].as_array().unwrap();
    assert_eq!(viewers.len(), 2);
    assert_eq!(viewers[0]["canvas_type"], 0);
    assert_eq!(viewers[0]["canvas_key"], "07".repeat(32));
    assert_eq!(viewers[0]["current_media"]["file_id"], 1);
    // (the file as file_metadata describes it, with notes and milliseconds)
    let (_, metadata) = get(
        &p.router,
        "/get_files/file_metadata?file_id=1&include_notes=true&include_milliseconds=true&detailed_url_information=true",
    )
    .await;
    assert_eq!(viewers[0]["current_media"], metadata["metadata"][0]);
    assert_eq!(viewers[1]["current_media"], Json::Null);
}

#[tokio::test]
async fn a_pages_selection_is_reported() {
    let p = pages();
    let store = p.fixture.state.store.clone();
    let hashes = store
        .read(|conn| hydrus_store::master::hashes(conn, &[HashId(1), HashId(2), HashId(3)]))
        .unwrap();
    let a = p.a;
    store
        .write(move |ctx| {
            sessions::set_page_files(ctx.conn(), &a, &[HashId(3), HashId(1), HashId(2)])?;
            sessions::set_page_selected(ctx.conn(), &a, &[HashId(1), HashId(2)])
        })
        .unwrap();
    let (status, body) = get(
        &p.router,
        &format!(
            "/manage_pages/get_page_info?page_key={}&simple=false",
            p.a.to_hex()
        ),
    )
    .await;
    assert_eq!(status, 200);
    let hex = |id: u32| hashes[&HashId(id)].to_hex();
    assert_eq!(
        body["page_info"]["media"],
        json!({
            "num_files": 3,
            "hash_ids": [3, 1, 2],
            "num_files_selected": 2,
            "hash_ids_selected": [1, 2],
            "hashes": [hex(3), hex(1), hex(2)],
            "hashes_selected": [hex(1), hex(2)],
        })
    );
}

#[tokio::test]
async fn a_store_without_pages_has_a_top_notebook_that_keeps_its_key() {
    let p = pages();
    p.fixture
        .state
        .store
        .write(|ctx| sessions::delete(ctx.conn(), LAST_SESSION))
        .unwrap();
    let (status, first) = get(&p.router, "/manage_pages/get_pages").await;
    assert_eq!(status, 200);
    assert_eq!(first["pages"]["pages"], json!([]));
    let (_, again) = get(&p.router, "/manage_pages/get_pages").await;
    assert_eq!(first["pages"]["page_key"], again["pages"]["page_key"]);
}
