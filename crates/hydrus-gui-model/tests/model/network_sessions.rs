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

#[test]
fn cookie_exchange_replays_reference_fields_questions_and_collisions() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/cookie_exchange.json"
    ))
    .unwrap();
    let imported = model::import_netscape_cookies(f["netscape_text"].as_str().unwrap()).unwrap();
    assert_eq!(imported.len(), 4);
    // Qt clears expired cookies during its list refresh; native drafts retain them.
    let mut live = imported
        .iter()
        .filter(|c| c.expires.is_none_or(|e| e > 1_700_000_000))
        .map(|c| {
            serde_json::json!({"name": c.name, "value": c.value, "domain": c.domain,
            "path": c.path, "expires": c.expires, "secure": c.secure, "rest": c.rest})
        })
        .collect::<Vec<_>>();
    live.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    assert_eq!(serde_json::json!(live), f["netscape_cookies"]);
    let mut exported: Vec<serde_json::Value> = serde_json::from_str(
        &model::export_cookies(
            &imported
                .iter()
                .filter(|c| c.expires != Some(1))
                .cloned()
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut reference = f["exports"][0].as_array().unwrap().clone();
    exported.sort_by_key(ToString::to_string);
    reference.sort_by_key(ToString::to_string);
    assert_eq!(exported, reference);
    let cookies = model::import_cookie_clipboard(f["clipboard_text"].as_str().unwrap()).unwrap();
    let session = NetworkContext::domain("example.com");
    let matching = model::matching_cookies(&cookies, &session);
    assert_eq!(matching.len(), 1);
    assert_eq!(
        model::cookie_import_question(&matching, false),
        f["questions"][1]["text"]
    );
    assert_eq!(
        model::cookie_import_question(&cookies, false),
        f["questions"][3]["text"]
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut draft = CookieDraft::new(&store, session.clone()).unwrap();
    draft.import(imported).unwrap();
    draft.import(matching).unwrap();
    let sid = draft.cookies.iter().find(|c| c.name == "sid").unwrap();
    assert_eq!(sid.value.as_deref(), Some("replacement"));
    assert_eq!(sid.expires, None);
    assert!(!sid.secure);
    assert!(sid.rest.is_empty());
    assert_eq!(draft.cookies.iter().filter(|c| c.name == "sid").count(), 1);
    assert!(
        store
            .read(|c| network::cookies(c, &session))
            .unwrap()
            .is_empty()
    );
    draft.apply(&store).unwrap();
    let mut stale = CookieDraft::new(&store, session.clone()).unwrap();
    stale.import(cookies).unwrap();
    let sid_index = stale.cookies.iter().position(|c| c.name == "sid").unwrap();
    // Re-importing the identical sid is a no-op, so it must preserve a
    // website response. The stale conflict case must actually edit that key.
    let mut unchanged_import = CookieDraft::new(&store, session.clone()).unwrap();
    unchanged_import
        .import(vec![stale.cookies[sid_index].clone()])
        .unwrap();
    let mut edited_sid = stale.cookies[sid_index].clone();
    edited_sid.value = Some("local-choice".into());
    stale.edit(Some(sid_index), edited_sid).unwrap();
    store
        .write(move |ctx| {
            let mut c = cookie("sid");
            c.path = "/private".into();
            c.value = Some("website-response".into());
            network::set_cookie(ctx.conn(), &session, &c)
        })
        .unwrap();
    unchanged_import.apply(&store).unwrap();
    assert!(stale.apply(&store).is_err());
    let reloaded = CookieDraft::new(&store, NetworkContext::domain("example.com")).unwrap();
    assert_eq!(
        reloaded
            .cookies
            .iter()
            .find(|c| c.name == "sid")
            .unwrap()
            .value
            .as_deref(),
        Some("website-response")
    );
    assert!(!reloaded.cookies.iter().any(|c| c.name == "other"));
}

#[test]
fn malformed_cookie_batches_never_partially_change_the_draft_and_browser_routes_silos() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut draft = CookieDraft::new(&store, NetworkContext::domain("example.com")).unwrap();
    draft.import(vec![cookie("original")]).unwrap();
    let original = draft.cookies.clone();
    let mut bad = cookie("bad");
    bad.path = "relative".into();
    assert!(draft.import(vec![cookie("first"), bad]).is_err());
    assert_eq!(draft.cookies, original);
    for text in [
        "not json",
        "{}",
        "[[\"n\",\"v\",\"example.com\",\"/\"]]",
        "[[\"n\",\"v\",\"example.com\",\"/\",null],[1,2,3,4,5]]",
    ] {
        assert!(model::import_cookie_clipboard(text).is_err(), "{text}");
    }
    assert!(model::import_cookie_clipboard(&" ".repeat(model::COOKIE_EXCHANGE_LIMIT + 1)).is_err());
    for text in [
        "example.com\tFALSE\t/\tFALSE\t0\tn\tv",
        "# Netscape HTTP Cookie File\n.example.com\tFALSE\t/\tTRUE\t0\tn\tv",
        "# Netscape HTTP Cookie File\nexample.com\tFALSE\t/\tFALSE\twrong\tn\tv",
    ] {
        assert!(model::import_netscape_cookies(text).is_err(), "{text}");
    }
    let mut cookies = model::import_cookie_clipboard(
        "[[\"a\",\" v \",\".sub.example.com\",\"/\",0],[\"b\",null,\"other.example.test\",\"/\",null]]"
    ).unwrap();
    assert_eq!(cookies[0].value.as_deref(), Some(" v "));
    let mut replacement = cookies[0].clone();
    replacement.value = Some("last".into());
    cookies.push(replacement);
    model::import_cookie_sessions(&store, cookies).unwrap();
    let sessions = store.read(network::sessions).unwrap();
    assert!(sessions.contains(&NetworkContext::domain("example.com")));
    let routed = store
        .read(|c| network::session_for(c, &NetworkContext::domain("other.example.test")))
        .unwrap();
    assert!(sessions.contains(&routed));
    assert_eq!(
        store
            .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
            .unwrap()[0]
            .value
            .as_deref(),
        Some("last")
    );
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.read(network::sessions).unwrap().len(), 2);
}

#[test]
fn automatic_header_questions_replay_reference_deduplicate_and_reject_changed_payloads() {
    use hydrus_store::{
        network_runtime::{NetworkJob, Snapshot, WaitReason},
        settings,
    };
    let fixture = hydrus_testkit::fixture_json("header_approval.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let pending = |name: &str, value: &str, reason: &str| CustomHeader {
        name: name.into(),
        value: value.into(),
        reason: reason.into(),
        approval: Approval::Pending,
    };
    let headers = [
        (
            NetworkContext::global(),
            pending("X-Test", "global-value", "global reason"),
        ),
        (
            NetworkContext::domain("example.com"),
            pending("Authorization", "token", "login reason"),
        ),
    ];
    store
        .write(move |ctx| {
            for (context, header) in headers {
                network::set_header(
                    ctx.conn(),
                    &context,
                    &header.name,
                    Some(&header.value),
                    Some(header.approval),
                    Some(&header.reason),
                )?;
            }
            let job = |id| NetworkJob {
                id,
                url: "https://example.com/file".into(),
                status: "header approval".into(),
                wait: WaitReason::Headers,
                bytes_read: 0,
                bytes_total: None,
                speed: 0,
                contexts: vec![
                    NetworkContext::global(),
                    NetworkContext::domain("example.com"),
                ],
                obeys_bandwidth: true,
            };
            settings::set(
                ctx.conn(),
                &Snapshot {
                    epoch: "header test".into(),
                    at: 100,
                    jobs: vec![job(1), job(2)],
                    usage: Vec::new(),
                },
            )
        })
        .unwrap();
    let questions = model::pending_header_questions(&store, 100).unwrap();
    assert_eq!(questions.len(), 2);
    assert_eq!(
        serde_json::json!(
            questions
                .iter()
                .map(model::HeaderQuestion::text)
                .collect::<Vec<_>>()
        ),
        fixture["questions"]
    );
    assert!(questions.iter().all(|q| q.jobs.len() == 2));
    model::answer_header_question(&store, questions[0].clone(), true).unwrap();
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::global()))
            .unwrap()
            .iter()
            .find(|h| h.name == "X-Test")
            .unwrap()
            .approval as i64,
        fixture["validations"][0][2].as_i64().unwrap()
    );
    assert_eq!(
        model::pending_header_questions(&store, 100).unwrap().len(),
        1
    );
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "Authorization",
                Some("changed token"),
                None,
                None,
            )
        })
        .unwrap();
    assert!(model::answer_header_question(&store, questions[1].clone(), true).is_err());
    let fresh = model::pending_header_questions(&store, 100)
        .unwrap()
        .remove(0);
    assert_eq!(fresh.row.header.value, "changed token");
    model::answer_header_question(&store, fresh, false).unwrap();
    let header = store
        .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
        .unwrap()
        .remove(0);
    assert_eq!(
        header.approval as i64,
        fixture["validations"][1][2].as_i64().unwrap()
    );
    assert_eq!(header.value, "changed token");
    assert_eq!(header.reason, "login reason");
    assert!(
        model::pending_header_questions(&store, 100)
            .unwrap()
            .is_empty()
    );
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "Authorization",
                None,
                Some(Approval::Pending),
                None,
            )
        })
        .unwrap();
    assert!(
        model::pending_header_questions(&store, 106)
            .unwrap()
            .is_empty()
    );
    store
        .write(|ctx| {
            let mut snapshot = settings::get::<Snapshot>(ctx.conn())?;
            for job in &mut snapshot.jobs {
                job.wait = WaitReason::Downloading;
            }
            settings::set(ctx.conn(), &snapshot)
        })
        .unwrap();
    assert!(
        model::pending_header_questions(&store, 100)
            .unwrap()
            .is_empty()
    );
}
