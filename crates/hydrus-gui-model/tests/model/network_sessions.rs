//! Replay actual Qt fields and exercise atomic, incremental network drafts.
use hydrus_gui_model::network_sessions::{self as model, CookieDraft, HeaderDraft, HeaderRow};
use hydrus_store::{
    Store,
    network::{self, Approval, Cookie, CustomHeader, NetworkContext},
};
fn cookie(name: &str) -> Cookie {
    Cookie {
        name: name.into(),
        value: Some("abc".into()),
        domain: ".example.com".into(),
        path: "/".into(),
        expires: None,
        secure: true,
        rest: vec![("HttpOnly".into(), None)],
    }
}
fn row(name: &str, value: &str) -> HeaderRow {
    HeaderRow {
        context: NetworkContext::domain("example.com"),
        header: CustomHeader {
            name: name.into(),
            value: value.into(),
            approval: Approval::Approved,
            reason: "login reason".into(),
        },
    }
}
#[test]
fn reference_rows_validation_and_confirmation() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/network_sessions.json"
    ))
    .unwrap();
    assert_eq!(model::CLEAR_QUESTION, f["questions"][1]);
    assert_eq!(model::DELETE_QUESTION, f["questions"][0]);
    let c = cookie("session");
    // The fixture also contains a permanent token cookie.
    for r in f["cookie_rows"].as_array().unwrap() {
        let mut c = c.clone();
        c.name = r[0].as_str().unwrap().into();
        c.value = Some(r[1].as_str().unwrap().into());
        c.path = r[3].as_str().unwrap().into();
        if c.name == "token" {
            c.expires = Some(4_102_444_800);
        }
        assert_eq!(
            serde_json::json!(&model::cookie_cells(&c, 1_700_000_000)[..5]),
            *r
        );
    }
    let mut token = cookie("token");
    token.value = Some("123".into());
    token.path = "/private".into();
    token.expires = Some(4_102_444_800);
    assert_eq!(
        serde_json::json!(model::session_cells(
            &NetworkContext::domain("example.com"),
            &[c.clone(), token],
            1_700_000_000
        )),
        f["session_row"]
    );
    assert_eq!(
        serde_json::json!(model::header_cells(&row("Authorization", "token"))),
        f["header_row"]
    );
    for case in f["cookie_cases"].as_array().unwrap() {
        let input = case["input"].as_array().unwrap();
        let mut c = cookie(input[0].as_str().unwrap());
        c.value = Some(input[1].as_str().unwrap().into());
        c.domain = input[2].as_str().unwrap().into();
        c.path = input[3].as_str().unwrap().into();
        let result = model::validate_cookie(c);
        if case["error"].is_null() {
            assert_eq!(result.unwrap().name, case["value"][0]);
        } else {
            assert_eq!(result.unwrap_err(), case["error"]);
        }
    }
    for case in f["header_cases"].as_array().unwrap() {
        let input = &case["input"];
        let r = row(input[0].as_str().unwrap(), input[1].as_str().unwrap());
        let result = model::validate_header(&r.context, r.header);
        if case["error"].is_null() {
            assert_eq!(result.unwrap().name, case["value"][1]);
            assert_eq!(
                model::approval_text(Approval::from_code(input[2].as_i64().unwrap()).unwrap()),
                case["value"][3]
            );
        } else {
            assert_eq!(result.unwrap_err(), case["error"]);
        }
    }
    assert!(
        model::validate_cookie(Cookie {
            path: "relative".into(),
            ..cookie("a")
        })
        .is_err()
    );
    assert!(
        model::validate_header(
            &NetworkContext::global(),
            CustomHeader {
                name: "Bad Key".into(),
                ..row("X", "y").header
            }
        )
        .is_err()
    );
}
#[test]
fn detached_cookie_drafts_preserve_attributes_merge_and_reject_conflicts_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = NetworkContext::domain("example.com");
    store
        .write({
            let s = session.clone();
            move |ctx| network::set_cookie(ctx.conn(), &s, &cookie("sid"))
        })
        .unwrap();
    let mut draft = CookieDraft::new(&store, session.clone()).unwrap();
    let mut changed = draft.cookies[0].clone();
    changed.name = "renamed".into();
    changed.path = "/private".into();
    changed.value = Some("edited".into());
    changed.expires = Some(2_000_000_000);
    draft.edit(Some(0), changed.clone()).unwrap();
    assert_eq!(
        store.read(|c| network::cookies(c, &session)).unwrap()[0].name,
        "sid"
    );
    store
        .write({
            let s = session.clone();
            move |ctx| network::set_cookie(ctx.conn(), &s, &cookie("arrived"))
        })
        .unwrap();
    draft.apply(&store).unwrap();
    let values = store.read(|c| network::cookies(c, &session)).unwrap();
    assert!(values.contains(&changed));
    assert!(values.iter().any(|c| c.name == "arrived"));
    assert!(!values.iter().any(|c| c.name == "sid"));
    let mut draft = CookieDraft::new(&store, session.clone()).unwrap();
    draft.cookies.clear();
    store
        .write({
            let s = session.clone();
            move |ctx| {
                let mut c = cookie("arrived");
                c.value = Some("concurrent".into());
                network::set_cookie(ctx.conn(), &s, &c)
            }
        })
        .unwrap();
    assert!(draft.apply(&store).is_err());
    assert_eq!(
        store.read(|c| network::cookies(c, &session)).unwrap().len(),
        2
    );
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.read(|c| network::cookies(c, &session)).unwrap().len(),
        2
    );
    store
        .write({
            let s = session.clone();
            move |ctx| network::clear_session(ctx.conn(), &s)
        })
        .unwrap();
    assert!(store.read(network::sessions).unwrap().is_empty());
    store
        .write(move |ctx| network::create_session(ctx.conn(), &session))
        .unwrap();
    assert_eq!(store.read(network::sessions).unwrap().len(), 1);
}
#[test]
fn headers_merge_unrelated_writes_and_cancel_and_reject_same_key_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut draft = HeaderDraft::new(&store).unwrap();
    draft.edit(None, row("X-Token", "one")).unwrap();
    drop(draft);
    assert!(
        HeaderDraft::new(&store)
            .unwrap()
            .rows
            .iter()
            .all(|r| r.header.name != "X-Token")
    );
    let mut draft = HeaderDraft::new(&store).unwrap();
    draft.edit(None, row("X-Token", "one")).unwrap();
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::global(),
                "X-Unrelated",
                Some("kept"),
                None,
                None,
            )
        })
        .unwrap();
    draft.apply(&store).unwrap();
    let mut draft = HeaderDraft::new(&store).unwrap();
    let i = draft
        .rows
        .iter()
        .position(|r| r.header.name == "X-Token")
        .unwrap();
    draft.edit(Some(i), row("X-Renamed", "two")).unwrap();
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "X-Token",
                Some("concurrent"),
                None,
                None,
            )
        })
        .unwrap();
    assert!(draft.apply(&store).is_err());
    let rows = HeaderDraft::new(&store).unwrap().rows;
    assert!(rows.iter().any(|r| r.header.name == "X-Unrelated"));
    assert!(rows.iter().all(|r| r.header.name != "X-Renamed"));
}

#[test]
fn cleared_session_draft_preserves_a_later_request_cookie() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = NetworkContext::domain("example.com");
    store
        .write({
            let s = session.clone();
            move |ctx| network::set_cookie(ctx.conn(), &s, &cookie("original"))
        })
        .unwrap();
    let mut draft = CookieDraft::new(&store, session.clone()).unwrap();
    let mut edited = draft.cookies[0].clone();
    edited.value = Some("stale".into());
    draft.edit(Some(0), edited).unwrap();
    store
        .write({
            let s = session.clone();
            move |ctx| {
                network::clear_session(ctx.conn(), &s)?;
                network::set_cookie(ctx.conn(), &s, &cookie("after-clear"))
            }
        })
        .unwrap();
    assert!(draft.apply(&store).is_err());
    let cookies = store.read(|c| network::cookies(c, &session)).unwrap();
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name, "after-clear");
    let mut draft = HeaderDraft::new(&store).unwrap();
    draft.edit(None, row("X-Collision", "gui")).unwrap();
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "x-collision",
                Some("api"),
                None,
                None,
            )
        })
        .unwrap();
    assert!(draft.apply(&store).is_err());
}
#[test]
fn header_case_groups_delete_exact_rows_and_reject_concurrent_variant_insertions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|ctx| {
            let c = NetworkContext::domain("example.com");
            network::set_header(ctx.conn(), &c, "X-Token", Some("first"), None, None)?;
            network::set_header(ctx.conn(), &c, "x-token", Some("effective"), None, None)
        })
        .unwrap();
    let mut draft = HeaderDraft::new(&store).unwrap();
    draft.rows.retain(|r| r.header.name != "x-token");
    draft.apply(&store).unwrap();
    let headers = store
        .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].name, "X-Token");
    let mut draft = HeaderDraft::new(&store).unwrap();
    let i = draft
        .rows
        .iter()
        .position(|r| r.header.name == "X-Token")
        .unwrap();
    draft.edit(Some(i), row("X-Token", "gui")).unwrap();
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "x-token",
                Some("api"),
                None,
                None,
            )
        })
        .unwrap();
    assert!(draft.apply(&store).is_err());
    let headers = store
        .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].value, "first");
    assert_eq!(headers[1].value, "api");
    let mut draft = HeaderDraft::new(&store).unwrap();
    draft.rows.retain(|r| r.header.name != "x-token");
    let i = draft
        .rows
        .iter()
        .position(|r| r.header.name == "X-Token")
        .unwrap();
    draft.edit(Some(i), row("x-Token", "resolved")).unwrap();
    draft.apply(&store).unwrap();
    let headers = store
        .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].name, "x-Token");
    assert_eq!(headers[0].value, "resolved");
}
