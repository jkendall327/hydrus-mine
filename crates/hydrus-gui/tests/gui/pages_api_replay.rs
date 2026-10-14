//! What the Client API says of the live pages while the window changes,
//! replayed against the reference's (`oracle/fixtures/pages_api.json`, from
//! `oracle/record_pages_api.py`): a search page "a", a notebook "nb" holding
//! "b" and "c", and a search page "d"; pages shown by clicking their tabs,
//! files selected and deselected with the window's select all / none, and
//! after each step `/manage_pages/get_pages` and every media page's
//! `get_page_info?simple=false`, asked of the real API router over the store
//! the window keeps in step, compared (page keys by page name).

use std::sync::Arc;

use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use tower::ServiceExt as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::common::widgets;

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn get(router: &axum::Router, uri: &str) -> Value {
    let request = axum::http::Request::get(uri)
        .header("Hydrus-Client-API-Access-Key", KEY)
        .body(axum::body::Body::empty())
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), 200, "{uri}");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    })
}

fn source(store: &Store, rows: &Value) -> Vec<Page> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let name = row["name"].as_str().unwrap();
            if let Some(children) = row.get("children") {
                return Page {
                    key: PageKey::random(),
                    name: name.into(),
                    content: PageContent::Pages(source(store, children)),
                };
            }
            let page = Page {
                key: PageKey::random(),
                name: name.into(),
                content: PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            };
            let key = page.key;
            let hashes: Vec<hydrus_core::Sha256> = row["hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().parse().unwrap())
                .collect();
            store
                .write(move |ctx| {
                    let ids = hydrus_store::master::hash_ids(ctx.conn(), &hashes)?;
                    let files: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
                    sessions::set_page_files(ctx.conn(), &key, &files)
                })
                .unwrap();
            page
        })
        .collect()
}

/// The page keys in a JSON value swapped for "key of <name>".
fn mask(value: &Value, keys: &[(String, String)]) -> Value {
    let mut text = value.to_string();
    for (key, name) in keys {
        text = text.replace(key, &format!("key of {name}"));
    }
    serde_json::from_str(&text).unwrap()
}

fn key_names(page: &Value, out: &mut Vec<(String, String)>) {
    out.push((
        page["page_key"].as_str().unwrap().to_owned(),
        page["name"].as_str().unwrap().to_owned(),
    ));
    for child in page["pages"].as_array().into_iter().flatten() {
        key_names(child, out);
    }
}

// leaf: audit-options-tabs-api
#[test]
fn the_api_sees_the_live_pages_their_files_and_selection_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("pages_api.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let session = Session {
        name: LAST_SESSION.into(),
        pages: source(&store, &recorded["layout"]),
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let router = hydrus_api::router(hydrus_api::AppState::new(store.clone()).unwrap());
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let win = ui.window();
    widgets::lay_out(win, 1100.0, 700.0);
    let observe = || -> Value {
        (bound.sync)();
        let pages = get(&router, "/manage_pages/get_pages");
        let mut keys = Vec::new();
        key_names(&pages["pages"], &mut keys);
        let mut infos = serde_json::Map::new();
        for (key, name) in &keys {
            if name == "top page notebook" {
                continue;
            }
            let info = get(
                &router,
                &format!("/manage_pages/get_page_info?page_key={key}&simple=false"),
            );
            infos.insert(name.clone(), info);
        }
        mask(&json!({ "pages": pages, "infos": infos }), &keys)
    };
    let clean = |api: &Value| -> Value {
        let mut api = api.clone();
        if let Some(pages) = api["pages"].as_object_mut() {
            pages.remove("version");
            pages.remove("hydrus_version");
        }
        if let Some(infos) = api["infos"].as_object_mut() {
            for info in infos.values_mut() {
                let info = info.as_object_mut().unwrap();
                info.remove("version");
                info.remove("hydrus_version");
                // (the reference lists the selected files in the order of a
                // Python set: as a set they are compared)
                if let Some(media) = info
                    .get_mut("page_info")
                    .and_then(|p| p.get_mut("media"))
                    .and_then(Value::as_object_mut)
                {
                    for field in ["hash_ids_selected", "hashes_selected"] {
                        let mut list = media[field].as_array().unwrap().clone();
                        list.sort_by_key(Value::to_string);
                        media.insert(field.into(), Value::Array(list));
                    }
                }
            }
        }
        api
    };
    for step in recorded["steps"].as_array().unwrap() {
        let action = step["do"].as_array().unwrap();
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        match action[0].as_str().unwrap() {
            "start" => {}
            "show" => {
                // the tab of that name, in the row it is in (a notebook's
                // inner row once the notebook is shown)
                let name = action[1].as_str().unwrap();
                let place = |ui: &MainWindow| {
                    let tabs = ui.get_tab_rows();
                    (0..tabs.row_count()).rev().find_map(|r| {
                        let at = tabs
                            .row_data(r)
                            .unwrap()
                            .full_names
                            .iter()
                            .position(|n| n == name)?;
                        Some((r, at))
                    })
                };
                let (r, at) = place(&ui).unwrap_or_else(|| {
                    ui.invoke_tab_chosen(0, 1);
                    headless::render(&windows.get(0).unwrap(), 1100, 700);
                    place(&ui).unwrap()
                });
                ui.invoke_tab_chosen(i32::try_from(r).unwrap(), i32::try_from(at).unwrap());
            }
            "select all" => ui.invoke_select_all(),
            "select none" => ui.invoke_select_none(),
            other => panic!("{other}"),
        }
        assert_eq!(clean(&observe()), clean(&step["api"]), "{}", step["do"]);
    }
}
