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
