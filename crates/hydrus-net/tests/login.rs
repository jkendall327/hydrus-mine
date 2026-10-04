//! Real loopback HTTP consumers replay the executed Python login engine.
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request as HttpRequest, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_net::{
    Job, NetEngine, NetOptions,
    login::{self, Execution, Outcome},
};
use hydrus_parse::login::{LoginScript, Validity};
use hydrus_store::{
    Store,
    network::{self, NetworkContext},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
fn script(case: &Value) -> LoginScript {
    legacy::login_script(&SerialisableObject::from_tuple_str(&case["script"].to_string()).unwrap())
        .unwrap()
}
struct Site {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    engine: NetEngine,
    domain: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Site {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn handler(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    request: HttpRequest,
) -> impl IntoResponse {
    let method = request.method().to_string();
    let path = request.uri().path_and_query().unwrap().to_string();
    let route = request.uri().path().to_owned();
    let (referer, origin, cookie) = {
        let header_value = |name| {
            request
                .headers()
                .get(name)
                .map(|value| value.to_str().unwrap().to_owned())
        };
        (
            header_value("referer"),
            header_value("origin"),
            header_value("cookie"),
        )
    };
    let body = String::from_utf8(
        to_bytes(request.into_body(), 1_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    {
        let mut recorded = requests.lock().unwrap();
        recorded.push(json!({"method":method,"path":path,"body":body,"referer":referer,"origin":origin,"cookie":cookie}));
    }
    let (status, data, cookie) = if route == "/unauthorised" {
        (StatusCode::UNAUTHORIZED, "credentials denied", None)
    } else if route == "/login" && method == "GET" {
        (StatusCode::OK, "login response", Some("session=ok; Path=/"))
    } else if route == "/stall" {
        tokio::time::sleep(Duration::from_secs(5)).await;
        (StatusCode::OK, "late", None)
    } else if method == "GET" {
        (
            StatusCode::OK,
            "<input name=\"csrf\" value=\"loop-token\"><p>start</p>",
            Some("preflight=ready; Path=/"),
        )
    } else if route == "/missing-cookie" {
        (StatusCode::OK, "no cookie", None)
    } else {
        let mut parts = body.split('&').collect::<Vec<_>>();
        parts.sort_unstable();
        assert_eq!(
            parts,
            [
                "mode=login",
                "pass=dummy+%2B+pass%26%3D",
                "token=loop-token",
                "user=alice"
            ]
        );
        (StatusCode::OK, "login response", Some("session=ok; Path=/"))
    };
    let mut response = (status, Body::from(data)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    if let Some(cookie) = cookie {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, cookie.parse().unwrap());
    }
    response
}
async fn site() -> Site {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let engine = NetEngine::new(
        store.clone(),
        NetOptions {
            obey_bandwidth: false,
            detect_sleep: false,
            network_timeout: 2,
            max_connection_attempts: 1,
            max_get_attempts: 1,
            ..NetOptions::default()
        },
    )
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let router = Router::new().fallback(handler).with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let domain = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Site {
        dir,
        store,
        engine,
        domain,
        requests,
        task,
    }
}
fn results(execution: &Execution) -> Value {
    json!(
        execution
            .results
            .iter()
            .map(|result| json!([
                result.name,
                result.url,
                result.body,
                result.data,
                result.new_variables,
                result.new_cookies,
                result.result
            ]))
            .collect::<Vec<_>>()
    )
}
fn normalise(value: &Value, domain: &str) -> Value {
    let mut value = value.clone();
    if let Value::Array(rows) = &mut value {
        for row in rows {
            if let Value::Array(fields) = row {
                fields[1] = json!(fields[1].as_str().unwrap().replace(domain, "127.0.0.1"));
                let mut cookies = fields[5].as_array().unwrap().clone();
                cookies.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                fields[5] = json!(cookies);
            } else if let Value::Object(fields) = row {
                for name in ["referer", "origin"] {
                    if let Some(Value::String(url)) = fields.get_mut(name) {
                        *url = url.replace(domain, "127.0.0.1");
                    }
                }
                if let Some(Value::String(body)) = fields.get_mut("body") {
                    let mut pieces = body.split('&').collect::<Vec<_>>();
                    pieces.sort_unstable();
                    *body = pieces.join("&");
                }
            }
        }
    }
    value
}
#[tokio::test]
async fn real_http_steps_replay_reference_tokens_cookie_checks_veto_401_and_cancel() {
    let cases = hydrus_testkit::fixture_json("login_execution.json");
    for case in cases.as_array().unwrap() {
        let site = site().await;
        let script = script(case);
        let credentials = serde_json::from_value(case["credentials"].clone()).unwrap();
        let control = Job::new();
        if case["name"] == "cancel_before_start" {
            control.cancel();
        }
        let execution = login::execute_with_pause(
            &site.engine,
            &site.store,
            &script,
            &site.domain,
            &credentials,
            &control,
            Duration::ZERO,
        )
        .await;
        assert_eq!(
            execution.outcome.text(),
            case["outcome"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            normalise(&results(&execution), &site.domain),
            normalise(&case["results"], "127.0.0.1"),
            "{}",
            case["name"]
        );
        let actual = json!(*site.requests.lock().unwrap());
        assert_eq!(
            normalise(&actual, &site.domain),
            normalise(&case["requests"], "127.0.0.1"),
            "{}",
            case["name"]
        );
        assert_eq!(
            login::logged_in(&site.store, &script, &site.domain).unwrap(),
            case["logged_in"].as_bool().unwrap()
        );
        if case["name"] == "success" {
            let cookies = site
                .store
                .read(|conn| {
                    let context = network::session_for(conn, &NetworkContext::domain("127.0.0.1"))?;
                    network::cookies(conn, &context)
                })
                .unwrap();
            assert!(cookies.iter().any(|cookie|cookie.name=="session"&&cookie.value.as_deref()==Some("ok")));
            let reopened = Store::open(site.dir.path()).unwrap();
            assert!(login::logged_in(&reopened, &script, &site.domain).unwrap());
            assert_eq!(execution.variables["csrf"], "loop-token");
        }
    }
}
#[tokio::test]
async fn cancellation_stops_an_active_request_and_keeps_partial_results() {
    let site = site().await;
    let cases = hydrus_testkit::fixture_json("login_execution.json");
    let mut script = script(&cases[0]);
    script.steps[1].path = "/stall".into();
    let credentials = serde_json::from_value(cases[0]["credentials"].clone()).unwrap();
    let control = Job::new();
    let cancel = control.clone();
    let requests = site.requests.clone();
    let task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if requests.lock().unwrap().len() >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
    });
    let execution = login::execute_with_pause(
        &site.engine,
        &site.store,
        &script,
        &site.domain,
        &credentials,
        &control,
        Duration::ZERO,
    )
    .await;
    task.await.unwrap();
    assert_eq!(execution.outcome, Outcome::Cancelled);
    assert_eq!(execution.results.len(), 2);
    assert_eq!(execution.results[0].result, "OK!");
    assert_eq!(execution.variables["csrf"], "loop-token");
    assert!(!login::logged_in(&site.store, &script, &site.domain).unwrap());
}
#[test]
fn final_outcomes_guard_script_identity_and_preserve_domain_preferences() {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let manager = legacy::manager(
        &SerialisableObject::from_tuple_str(&fixture["manager"].to_string()).unwrap(),
    )
    .unwrap();
    let original = &manager.domains["login.example"];
    for outcome in [
        Outcome::Success,
        Outcome::Verification("synthetic veto".into()),
        Outcome::FinalCookies("missing".into()),
        Outcome::Network("denied".into()),
        Outcome::Cancelled,
    ] {
        let mut domain = original.clone();
        assert!(!outcome.update_domain(&mut domain, "different key", 1000));
        assert_eq!(&domain, original);
        assert!(outcome.update_domain(&mut domain, &original.script_key, 1000));
        assert_eq!(domain.credentials, original.credentials);
        assert_eq!(domain.active, original.active);
        match outcome {
            Outcome::Success => assert_eq!(domain.validity, Validity::Valid),
            Outcome::Network(_) | Outcome::Cancelled => {
                assert_eq!(domain.no_work_until, 1000 + 4 * 3600);
            }
            _ => assert_eq!(domain.validity, Validity::Invalid),
        }
    }
}
#[test]
fn request_plan_replaces_www_and_orders_static_credentials_then_temporary_values() {
    let step = hydrus_parse::login::LoginStep {
        subdomain: Some("auth".into()),
        static_args: BTreeMap::from([
            ("same".into(), "static".into()),
            ("z".into(), "last".into()),
        ]),
        credentials: BTreeMap::from([("user".into(), "same".into())]),
        temp_args: BTreeMap::from([("csrf".into(), "same".into())]),
        ..hydrus_parse::login::LoginStep::default()
    };
    let credentials = [("user".into(), "credential".into())].into();
    let variables = [("csrf".into(), "temporary".into())].into();
    let planned = login::plan(&step, "www.login.example", &credentials, &variables, None).unwrap();
    assert_eq!(
        planned.request.url,
        "https://auth.login.example/?same=temporary&z=last"
    );
    assert_eq!(planned.test_body, "");
    assert_eq!(
        login::plan(&step, "login.example", &credentials, &BTreeMap::new(), None).unwrap_err(),
        "The temporary variable 'csrf' was not found!"
    );
}

#[test]
fn shared_login_session_state_replays_required_cookie_expiry_and_reset() {
    let fixture = hydrus_testkit::fixture_json("login_sessions.json");
    let script = script(&fixture);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .read(|conn| network::session_for(conn, &NetworkContext::domain("login.example")))
            .unwrap()
            .data,
        fixture["resolved_session"].as_str().unwrap()
    );
    for case in fixture["states"].as_array().unwrap().iter().take(4) {
        let input = case["input"].as_array().unwrap().clone();
        store
            .write_and_refresh(move |ctx| {
                let session =
                    network::session_for(ctx.conn(), &NetworkContext::domain("login.example"))?;
                network::clear_session(ctx.conn(), &session)?;
                for row in input {
                    network::set_cookie(
                        ctx.conn(),
                        &session,
                        &network::Cookie {
                            name: row[0].as_str().unwrap().into(),
                            value: Some(row[1].as_str().unwrap().into()),
                            domain: "login.example".into(),
                            path: "/".into(),
                            expires: row[2].as_i64(),
                            secure: false,
                            rest: Vec::new(),
                        },
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let state = login::session_state(&store, &script, "login.example").unwrap();
        assert_eq!(
            state.logged_in,
            case["state"]["logged_in"].as_bool().unwrap()
        );
        assert_eq!(state.expires, case["state"]["expiry"].as_i64());
    }
}

#[tokio::test]
async fn confirmed_login_reset_reaches_an_existing_http_engine_and_keeps_other_sessions() {
    let site = site().await;
    site.store
        .write_and_refresh(move |ctx| {
            for domain in ["127.0.0.1", "other.example.net"] {
                network::set_cookie(
                    ctx.conn(),
                    &network::session_for(ctx.conn(), &NetworkContext::domain(domain))?,
                    &network::Cookie {
                        name: "session".into(),
                        value: Some("ok".into()),
                        domain: domain.into(),
                        path: "/".into(),
                        expires: None,
                        secure: false,
                        rest: Vec::new(),
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let request = hydrus_net::Request::get(format!("http://{}/start", site.domain));
    site.engine.fetch(&request, &Job::new()).await.unwrap();
    assert_eq!(site.requests.lock().unwrap()[0]["cookie"], "session=ok");
    login::clear_sessions(&site.store, std::slice::from_ref(&site.domain)).unwrap();
    site.engine.fetch(&request, &Job::new()).await.unwrap();
    assert!(site.requests.lock().unwrap()[1]["cookie"].is_null());
    assert_eq!(
        site.store
            .read(|conn| network::cookies(
                conn,
                &network::session_for(conn, &NetworkContext::domain("other.example.net"))?
            ))
            .unwrap()
            .len(),
        1
    );
    let reopened = Store::open(site.dir.path()).unwrap();
    assert_eq!(
        reopened
            .read(|conn| network::cookies(
                conn,
                &network::session_for(conn, &NetworkContext::domain("other.example.net"))?
            ))
            .unwrap()
            .len(),
        1
    );
    assert!(
        !reopened
            .read(|conn| network::cookies(
                conn,
                &network::session_for(conn, &NetworkContext::domain("127.0.0.1"))?
            ))
            .unwrap()
            .iter()
            .any(|cookie| cookie.name == "session")
    );
}

#[tokio::test]
async fn completed_step_is_delivered_before_later_http_and_survives_wait_cancellation() {
    let fixture = hydrus_testkit::fixture_json("login_execution.json");
    let script = script(&fixture[0]);
    let credentials = serde_json::from_value(fixture[0]["credentials"].clone()).unwrap();
    let site = site().await;
    let control = Job::new();
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let execution = login::execute_with_results(
        &site.engine,
        &site.store,
        &script,
        &site.domain,
        &credentials,
        &control,
        |result| {
            send.send(result.clone()).unwrap();
        },
    );
    let observe = async {
        let result = tokio::time::timeout(Duration::from_secs(1), receive.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            result.name,
            fixture[0]["stream"][0]["name"].as_str().unwrap()
        );
        assert_eq!(
            site.requests.lock().unwrap().len(),
            usize::try_from(fixture[0]["stream"][0]["request_count"].as_u64().unwrap()).unwrap()
        );
        control.cancel();
        result
    };
    let (execution, result) = tokio::join!(execution, observe);
    assert_eq!(execution.outcome, Outcome::Cancelled);
    assert_eq!(execution.results, [result]);
    assert_eq!(site.requests.lock().unwrap().len(), 1);
    assert!(receive.try_recv().is_err());
}

fn demand_manager(domain: &str, active: bool) -> hydrus_parse::login::LoginManager {
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let ready = fixture["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|state| state["name"] == "ready")
        .unwrap();
    let mut manager =
        legacy::manager(&SerialisableObject::from_tuple_str(&ready["before"].to_string()).unwrap())
            .unwrap();
    let mut login = manager.domains.remove("login.example.net").unwrap();
    login.active = active;
    manager.domains.insert(domain.to_owned(), login);
    manager
}
fn save_demand(store: &Store, domain: &str, active: bool) {
    let manager = demand_manager(domain, active);
    let domain = domain.to_owned();
    store
        .write_and_refresh(move |ctx| {
            let session = network::session_for(ctx.conn(), &NetworkContext::domain(domain))?;
            network::clear_session(ctx.conn(), &session)?;
            hydrus_store::logins::save(ctx.conn(), &manager)
        })
        .unwrap();
}
#[test]
fn demand_eligibility_replays_actual_reference_domains_cookies_errors_and_key_fallback() {
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    for state in fixture["states"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let manager = legacy::manager(
            &SerialisableObject::from_tuple_str(&state["before"].to_string()).unwrap(),
        )
        .unwrap();
        let expected = legacy::manager(
            &SerialisableObject::from_tuple_str(&state["after"].to_string()).unwrap(),
        )
        .unwrap();
        let domain = state["requested"].as_str().unwrap().to_owned();
        let cookies = state["cookies"].as_array().unwrap().clone();
        store
            .write_and_refresh({
                let domain = domain.clone();
                move |ctx| {
                    hydrus_store::logins::save(ctx.conn(), &manager)?;
                    let session =
                        network::session_for(ctx.conn(), &NetworkContext::domain(domain))?;
                    for cookie in cookies {
                        network::set_cookie(
                            ctx.conn(),
                            &session,
                            &network::Cookie {
                                name: cookie[0].as_str().unwrap().into(),
                                value: Some(cookie[1].as_str().unwrap().into()),
                                domain: "login.example.net".into(),
                                path: "/".into(),
                                expires: None,
                                secure: false,
                                rest: Vec::new(),
                            },
                        )?;
                    }
                    Ok(())
                }
            })
            .unwrap();
        let demand = login::demand(&store, &domain, fixture["now"].as_i64().unwrap()).unwrap();
        if !state["needs"].as_bool().unwrap() {
            assert_eq!(demand, login::Demand::None, "{}", state["name"]);
        } else if let Some(error) = state["error"].as_str() {
            assert_eq!(
                demand,
                login::Demand::Blocked {
                    domain: state["status"][0].as_str().unwrap().into(),
                    error: error.into()
                },
                "{}",
                state["name"]
            );
        } else {
            let login::Demand::Ready(login) = demand else {
                panic!("{}", state["name"]);
            };
            assert_eq!(login.domain, state["status"][0].as_str().unwrap());
            assert_eq!(login.script.name, "demand fixture");
        }
        assert_eq!(
            store.read(hydrus_store::logins::load).unwrap(),
            expected,
            "{}",
            state["name"]
        );
    }
}
#[tokio::test]
async fn automatic_demand_replays_reference_http_and_reuses_one_connection_slot() {
    use hydrus_net::Request;
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let site = site().await;
    site.engine
        .set_options(NetOptions {
            max_jobs: 1,
            max_jobs_per_domain: 1,
            ..site.engine.options()
        })
        .unwrap();
    for case in fixture["runtime"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["trigger_cancelled"] == false)
    {
        save_demand(&site.store, &site.domain, case["active"].as_bool().unwrap());
        if case["initial_cookie"] == true {
            site.store
                .write_and_refresh(|ctx| {
                    let session =
                        network::session_for(ctx.conn(), &NetworkContext::domain("127.0.0.1"))?;
                    network::set_cookie(
                        ctx.conn(),
                        &session,
                        &network::Cookie {
                            name: "session".into(),
                            value: Some("ok".into()),
                            domain: "127.0.0.1".into(),
                            path: "/".into(),
                            expires: None,
                            secure: false,
                            rest: Vec::new(),
                        },
                    )
                })
                .unwrap();
        }
        site.requests.lock().unwrap().clear();
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            site.engine.fetch(
                &Request::get(format!("http://{}/data", site.domain)),
                &Job::new(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status, 200);
        let actual = json!(
            site.requests
                .lock()
                .unwrap()
                .iter()
                .map(|request| json!({"path":request["path"],"cookie":request["cookie"]}))
                .collect::<Vec<_>>()
        );
        assert_eq!(actual, case["requests"], "{}", case["name"]);
        let manager = site.store.read(hydrus_store::logins::load).unwrap();
        assert_eq!(
            login::logged_in(&site.store, &manager.scripts[0], &site.domain).unwrap(),
            case["logged_in"].as_bool().unwrap()
        );
        if case["name"] == "automatic" {
            assert_eq!(manager.domains[&site.domain].validity, Validity::Valid);
        }
        assert!(site.engine.runtime_snapshot().login.is_none());
    }
}
async fn wait_for_login_request(site: &Site) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if site
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request["path"] == "/login")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn cancelled_trigger_keeps_reference_global_login_alive_for_another_engine() {
    use hydrus_net::{NetError, Request};
    let site = site().await;
    save_demand(&site.store, &site.domain, true);
    let sibling = NetEngine::new(
        Store::open(site.store.dir()).unwrap(),
        site.engine.options(),
    )
    .unwrap();
    assert_ne!(
        site.engine.runtime_snapshot().epoch,
        sibling.runtime_snapshot().epoch
    );
    let trigger = Job::new();
    let url = format!("http://{}/data", site.domain);
    let request = Request::get(&url);
    let first = site.engine.fetch(&request, &trigger);
    tokio::pin!(first);
    tokio::select! {
        result = &mut first => panic!("trigger completed before cancellation: {result:?}"),
        () = wait_for_login_request(&site) => {}
    }
    assert!(site.engine.runtime_snapshot().login.is_some());
    trigger.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), &mut first)
            .await
            .unwrap()
            .unwrap_err(),
        NetError::Cancelled
    );
    assert!(
        site.engine.runtime_snapshot().login.is_some(),
        "the reference engine owns the process independently of its trigger"
    );
    let next = Job::new();
    let next_request = Request::get(&url);
    tokio::time::timeout(Duration::from_secs(5), sibling.fetch(&next_request, &next))
        .await
        .unwrap()
        .unwrap();
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let case = fixture["runtime"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "cancel-trigger")
        .unwrap();
    assert_eq!(
        json!(
            site.requests
                .lock()
                .unwrap()
                .iter()
                .map(|request| json!({"path":request["path"],"cookie":request["cookie"]}))
                .collect::<Vec<_>>()
        ),
        case["requests"]
    );
    assert_eq!(
        site.store.read(hydrus_store::logins::load).unwrap().domains[&site.domain].validity,
        Validity::Valid
    );
    assert!(sibling.runtime_snapshot().login.is_none());
}
#[tokio::test]
async fn invalid_demand_waits_for_ordinary_jobs_and_cancels_subscription_with_recorded_note() {
    use hydrus_net::{NetError, Request};
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let site = site().await;
    save_demand(&site.store, &site.domain, true);
    let domain = site.domain.clone();
    site.store
        .write_and_refresh(move |ctx| {
            let mut manager = hydrus_store::logins::load(ctx.conn())?;
            let login = manager.domains.get_mut(&domain).unwrap();
            login.validity = Validity::Invalid;
            login.validity_error = "synthetic invalid".into();
            hydrus_store::logins::save(ctx.conn(), &manager)
        })
        .unwrap();
    let request = Request::get(format!("http://{}/data", site.domain));
    let ordinary = Job::new();
    let first = site.engine.fetch(&request, &ordinary);
    tokio::pin!(first);
    tokio::select! {
        result=&mut first=>panic!("invalid ordinary demand unexpectedly finished: {result:?}"),
        result=tokio::time::timeout(Duration::from_secs(1),async { loop { if ordinary.state().status.contains("synthetic invalid"){break;}tokio::task::yield_now().await; } })=>{ result.unwrap(); }
    }
    assert_eq!(
        ordinary.state().status,
        fixture["blocked"][0]["before_cancel"]
            .as_str()
            .unwrap()
            .replace("127.0.0.1", &site.domain)
    );
    ordinary.cancel();
    assert_eq!(first.await.unwrap_err(), NetError::Cancelled);
    assert_eq!(
        ordinary.cancelled_note(),
        fixture["blocked"][0]["error"].as_str().unwrap()
    );
    let mut subscription = request.clone();
    subscription
        .extra_contexts
        .push(NetworkContext::subscription("synthetic query", "fixture"));
    let job = Job::new();
    assert_eq!(
        site.engine.fetch(&subscription, &job).await.unwrap_err(),
        NetError::Cancelled
    );
    assert_eq!(
        job.cancelled_note(),
        fixture["blocked"][1]["error"]
            .as_str()
            .unwrap()
            .replace("127.0.0.1", &site.domain)
    );
    assert!(site.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn global_process_cancel_uses_its_owner_id_and_persists_reference_delay_without_a_second_step()
 {
    use hydrus_net::Request;
    use hydrus_store::network_runtime::{Command, JobAction};
    let site = site().await;
    let sibling = NetEngine::new(
        Store::open(site.store.dir()).unwrap(),
        site.engine.options(),
    )
    .unwrap();
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let case = &fixture["cancelled_process"];
    let mut manager = demand_manager(&site.domain, true);
    manager.scripts[0] = legacy::login_script(
        &SerialisableObject::from_tuple_str(&case["script"].to_string()).unwrap(),
    )
    .unwrap();
    site.store
        .write_and_refresh(move |ctx| hydrus_store::logins::save(ctx.conn(), &manager))
        .unwrap();
    let request = Request::get(format!("http://{}/data", site.domain));
    let job = Job::new();
    let fetch = site.engine.fetch(&request, &job);
    tokio::pin!(fetch);
    tokio::select! {
        result=&mut fetch=>panic!("demand completed before process cancellation: {result:?}"),
        ()=wait_for_login_request(&site)=>{}
    }
    let snapshot = site.engine.runtime_snapshot();
    let process = snapshot.login.unwrap();
    assert_eq!(process.status, case["status"].as_str().unwrap());
    assert!(!site.engine.runtime_command(&Command {
        epoch: "retired owner".into(),
        job: process.id,
        action: JobAction::CancelLogin
    }));
    assert!(!site.engine.runtime_command(&Command {
        epoch: process.epoch.clone(),
        job: process.id + 100,
        action: JobAction::CancelLogin
    }));
    assert!(sibling.runtime_command(&Command {
        epoch: process.epoch.clone(),
        job: process.id,
        action: JobAction::CancelLogin
    }));
    tokio::select! {
        result=&mut fetch=>panic!("failed login demand unexpectedly completed: {result:?}"),
        result=tokio::time::timeout(Duration::from_secs(1),async { loop { if job.state().status.contains("User cancelled the login process."){break;}tokio::task::yield_now().await; } })=>{ result.unwrap(); }
    }
    assert_eq!(
        job.state().status,
        case["queued_status"]
            .as_str()
            .unwrap()
            .replace("127.0.0.1", &site.domain)
    );
    job.cancel();
    assert_eq!(fetch.await.unwrap_err(), hydrus_net::NetError::Cancelled);
    assert_eq!(job.cancelled_note(), case["queued_error"].as_str().unwrap());
    assert_eq!(
        json!(
            site.requests
                .lock()
                .unwrap()
                .iter()
                .map(|request| json!({"path":request["path"],"cookie":request["cookie"]}))
                .collect::<Vec<_>>()
        ),
        case["requests"]
    );
    let manager = site.store.read(hydrus_store::logins::load).unwrap();
    let login = &manager.domains[&site.domain];
    assert_eq!(login.delay_reason, "User cancelled the login process.");
    assert!(login.no_work_until > hydrus_core::time::TimestampMs::now().secs());
    assert_eq!(login.validity, Validity::Untested);
    assert!(site.engine.runtime_snapshot().login.is_none());
    assert!(!site.engine.runtime_command(&Command {
        epoch: process.epoch,
        job: process.id,
        action: JobAction::CancelLogin
    }));
}

#[test]
fn abandoned_unpolled_login_worker_releases_metadata_and_the_shared_admission_gate() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    save_demand(&store, "127.0.0.1:9", true);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let engine = runtime.block_on(async {
        let engine = NetEngine::new(
            store,
            NetOptions {
                obey_bandwidth: false,
                detect_sleep: false,
                ..Default::default()
            },
        )
        .unwrap();
        let job = Job::new();
        let request = hydrus_net::Request::get("http://127.0.0.1:9/data");
        let mut fetch = Box::pin(engine.fetch(&request, &job));
        // Poll admission once on the current-thread runtime, then drop its outer
        // request and the runtime before its detached login worker can be polled.
        std::future::poll_fn(|cx| match std::future::Future::poll(fetch.as_mut(), cx) {
            std::task::Poll::Pending => std::task::Poll::Ready(()),
            std::task::Poll::Ready(result) => {
                panic!("admission ended before spawning login: {result:?}")
            }
        })
        .await;
        assert!(engine.runtime_snapshot().login.is_some());
        drop(fetch);
        engine
    });
    drop(runtime);
    assert!(engine.runtime_snapshot().login.is_none());
    assert!(engine.runtime_snapshot().jobs.is_empty());
    let command = hydrus_store::network_runtime::Command {
        epoch: engine.runtime_snapshot().epoch,
        job: 2,
        action: hydrus_store::network_runtime::JobAction::CancelLogin,
    };
    assert!(!engine.runtime_command(&command));
}

#[tokio::test]
async fn forced_manual_login_serializes_demand_across_independently_opened_engines() {
    use hydrus_net::{NetError, Request};
    let site = site().await;
    save_demand(&site.store, &site.domain, true);
    let manager = site.store.read(hydrus_store::logins::load).unwrap();
    let script = &manager.scripts[0];
    let control = Job::new();
    let manual_engine = NetEngine::new(
        Store::open(site.store.dir()).unwrap(),
        site.engine.options(),
    )
    .unwrap();
    let credentials = BTreeMap::new();
    let manual =
        manual_engine.run_login_with_results(script, &site.domain, &credentials, &control, |_| {});
    tokio::pin!(manual);
    tokio::select! {
        result = &mut manual => panic!("manual login completed before its first HTTP step: {result:?}"),
        () = wait_for_login_request(&site) => {},
    }
    let owner = site.engine.runtime_snapshot().login.unwrap();
    assert_eq!(owner.epoch, manual_engine.runtime_snapshot().epoch);
    // A forced second manual attempt waits on the same lease and cancellation
    // retires only that queued attempt, leaving the current global process alive.
    let queued = Job::new();
    let second =
        site.engine
            .run_login_with_results(script, &site.domain, &credentials, &queued, |_| {});
    tokio::pin!(second);
    tokio::select! {
        result = &mut second => panic!("a second manual process bypassed admission: {result:?}"),
        () = tokio::time::sleep(Duration::from_millis(75)) => {},
    }
    assert_eq!(queued.state().status, "waiting in login queue…");
    queued.cancel();
    assert_eq!(second.await.unwrap_err(), NetError::Cancelled);
    assert_eq!(site.engine.runtime_snapshot().login.unwrap().id, owner.id);
    let download = Job::new();
    let request = Request::get(format!("http://{}/data", site.domain));
    let fetch = site.engine.fetch(&request, &download);
    tokio::pin!(fetch);
    tokio::select! {
        result = &mut fetch => panic!("demand bypassed the active same-domain manual login: {result:?}"),
        () = tokio::time::sleep(Duration::from_millis(75)) => {},
    }
    assert_eq!(download.state().status, "waiting in login queue…");
    assert_eq!(site.requests.lock().unwrap().len(), 1);
    let (execution, downloaded) = tokio::join!(manual, fetch);
    assert_eq!(execution.unwrap().outcome, Outcome::Success);
    downloaded.unwrap();
    let fixture = hydrus_testkit::fixture_json("login_demand.json");
    let recorded = |requests: &[Value]| {
        json!(
            requests
                .iter()
                .map(|request| json!({"path":request["path"], "cookie":request["cookie"]}))
                .collect::<Vec<_>>()
        )
    };
    assert_eq!(
        recorded(&site.requests.lock().unwrap()),
        fixture["forced"]["requests"]
    );
    assert!(site.engine.runtime_snapshot().login.is_none());
    assert_eq!(
        site.store.read(hydrus_store::logins::load).unwrap().domains[&site.domain].validity,
        Validity::Valid
    );
    // Forced execution still runs with existing cookies, rather than being
    // silently swallowed by automatic demand's logged-in eligibility check.
    let forced = Job::new();
    assert_eq!(
        manual_engine
            .run_login_with_results(script, &site.domain, &credentials, &forced, |_| {})
            .await
            .unwrap()
            .outcome,
        Outcome::Success
    );
    assert_eq!(
        recorded(&site.requests.lock().unwrap()[2..]),
        fixture["forced"]["repeat_requests"]
    );
}
