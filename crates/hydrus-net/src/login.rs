//! Ordered login HTTP requests, cookie verification and VARIABLE/VETO responses.
use crate::{Job, Method, NetEngine, NetError, Request};
use hydrus_parse::{
    content::{ContentKind, ParseFailure},
    formula::ParsingContext,
    login::{CookieRequirement, DomainLogin, LoginScript, LoginStep, Validity},
};
use hydrus_store::{
    Store,
    network::{self, Cookie, NetworkContext},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

/// The raw reference URL/body preview alongside the HTTP request to send.
#[derive(Debug, Clone)]
pub struct Plan {
    pub request: Request,
    pub test_body: String,
}
fn query(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}
fn form_quote(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = String::new();
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                text.push(char::from(byte));
            }
            b' ' => text.push('+'),
            _ => {
                text.push('%');
                text.push(char::from(HEX[usize::from(byte >> 4)]));
                text.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
    }
    text
}
/// Static arguments are overwritten by credentials, then temporary variables.
pub fn plan(
    step: &LoginStep,
    domain: &str,
    credentials: &BTreeMap<String, String>,
    variables: &BTreeMap<String, String>,
    referral: Option<&str>,
) -> Result<Plan, String> {
    if !matches!(step.scheme.as_str(), "http" | "https")
        || !matches!(step.method.as_str(), "GET" | "POST")
    {
        return Err("Login steps support http/https and GET/POST.".into());
    }
    let host = step.subdomain.as_ref().map_or_else(
        || domain.to_owned(),
        |subdomain| {
            format!(
                "{subdomain}.{}",
                domain.strip_prefix("www.").unwrap_or(domain)
            )
        },
    );
    let mut values = step.static_args.clone();
    for (pretty, arg) in &step.credentials {
        let value = credentials
            .get(pretty)
            .ok_or_else(|| format!("Missing required credentials: {pretty}"))?;
        values.insert(arg.clone(), value.clone());
    }
    for (temporary, arg) in &step.temp_args {
        let value = variables
            .get(temporary)
            .ok_or_else(|| format!("The temporary variable '{temporary}' was not found!"))?;
        values.insert(arg.clone(), value.clone());
    }
    let origin = format!("{}://{host}", step.scheme);
    let mut url = format!("{origin}{}", step.path);
    let raw = query(&values);
    if step.method == "GET" && !raw.is_empty() {
        url.push('?');
        url.push_str(&raw);
    }
    let mut request = Request::get(url);
    request.for_login = true;
    request.referral_url = referral.map(str::to_owned);
    request.override_bandwidth_after = Some(0);
    let test_body = if step.method == "POST" {
        request.method = Method::Post;
        request.body = Some(
            values
                .iter()
                .map(|(key, value)| format!("{}={}", form_quote(key), form_quote(value)))
                .collect::<Vec<_>>()
                .join("&")
                .into_bytes(),
        );
        request.additional_headers.push((
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        ));
        if referral.is_some() {
            request.additional_headers.push(("origin".into(), origin));
        }
        raw
    } else {
        String::new()
    };
    Ok(Plan { request, test_body })
}
fn host(domain: &str) -> String {
    crate::cookies::CookieUrl::new("http", domain, "/", "").host
}
fn cookies(store: &Store, domain: &str) -> Result<Vec<Cookie>, String> {
    let domain = host(domain);
    store
        .read(move |conn| {
            let session = network::session_for(conn, &NetworkContext::domain(domain))?;
            let mut cookies = network::cookies(conn, &session)?;
            let now = hydrus_core::time::TimestampMs::now().secs();
            cookies.retain(|cookie| cookie.expires.is_none_or(|expiry| expiry > now));
            Ok(cookies)
        })
        .map_err(|error| error.to_string())
}
fn cookie_domain_matches(cookie: &Cookie, domain: &str) -> bool {
    cookie.domain == domain
        || cookie.domain == format!(".{domain}")
        || (cookie.domain.starts_with('.') && domain.ends_with(&cookie.domain))
}
/// The first matching cookie in cookie-jar order must satisfy the value test.
pub fn check_cookies(
    requirements: &[CookieRequirement],
    cookies: &[Cookie],
    domain: &str,
    step: Option<&str>,
) -> Result<(), String> {
    let domain = host(domain);
    for requirement in requirements {
        let name = requirement.name.describe(false, false);
        let cookie = cookies.iter().find(|cookie| {
            cookie_domain_matches(cookie, &domain) && requirement.name.matches(&cookie.name)
        });
        let Some(cookie) = cookie else {
            return Err(step.map_or_else(
                || format!("Missing cookie \"{name}\"!"),
                |step| format!("Missing cookie \"{name}\" on step \"{step}\"!"),
            ));
        };
        if let Err(error) = requirement
            .value
            .test(cookie.value.as_deref().unwrap_or_default())
        {
            return Err(step.map_or_else(
                || format!("Cookie \"{name}\" failed: {error}!"),
                |step| format!("Cookie \"{name}\" failed on step \"{step}\": {error}!"),
            ));
        }
    }
    Ok(())
}
/// Persisted session cookies determine logged-in state, independent of active status.
pub fn logged_in(store: &Store, script: &LoginScript, domain: &str) -> Result<bool, String> {
    Ok(session_state(store, script, domain)?.logged_in)
}
/// Required-cookie validity and the earliest expiry, with session-cookie precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionState {
    pub logged_in: bool,
    pub expires: Option<i64>,
}
/// Read the current shared cookie store, including changes made by open session editors.
pub fn session_state(
    store: &Store,
    script: &LoginScript,
    domain: &str,
) -> Result<SessionState, String> {
    let cookies = cookies(store, domain)?;
    let logged_in = check_cookies(&script.required_cookies, &cookies, domain, None).is_ok();
    let domain = host(domain);
    let mut expires = None;
    for requirement in &script.required_cookies {
        let Some(cookie) = cookies.iter().find(|cookie| {
            cookie_domain_matches(cookie, &domain) && requirement.name.matches(&cookie.name)
        }) else {
            return Ok(SessionState {
                logged_in,
                expires: None,
            });
        };
        let Some(expiry) = cookie.expires else {
            return Ok(SessionState {
                logged_in,
                expires: None,
            });
        };
        expires = Some(expires.map_or(expiry, |old: i64| old.min(expiry)));
    }
    Ok(SessionState { logged_in, expires })
}
/// Reference reset-login clears the resolved shared sessions immediately, outside the draft.
pub fn clear_sessions(store: &Store, domains: &[String]) -> Result<(), String> {
    let domains = domains.to_vec();
    store
        .write_and_refresh(move |ctx| {
            let sessions = domains
                .iter()
                .map(|domain| {
                    network::session_for(ctx.conn(), &NetworkContext::domain(host(domain)))
                })
                .collect::<hydrus_store::Result<BTreeSet<_>>>()?;
            for session in sessions {
                network::clear_session(ctx.conn(), &session)?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())
}
fn cookie_strings(cookies: &[Cookie]) -> BTreeSet<String> {
    cookies
        .iter()
        .map(|cookie| {
            format!(
                "{}: {} | {} | {}",
                cookie.name,
                cookie.value.as_deref().unwrap_or_default(),
                cookie.domain,
                cookie
                    .expires
                    .map_or_else(|| "session".to_owned(), |expires| expires.to_string())
            )
        })
        .collect()
}
/// One step's reference test-result fields; body is absent before a request plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    pub name: String,
    pub url: String,
    pub body: Option<String>,
    pub data: String,
    pub new_variables: Vec<String>,
    pub new_cookies: Vec<String>,
    pub result: String,
}
impl TestResult {
    fn initial(step: &LoginStep) -> Self {
        Self {
            name: step.name.clone(),
            url: "Did not make a url.".into(),
            body: None,
            data: "Did not download data.".into(),
            new_variables: Vec::new(),
            new_cookies: Vec::new(),
            result: "Did not start.".into(),
        }
    }
}
/// Final manager action, alongside the exact visible final result text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Verification(String),
    FinalCookies(String),
    Network(String),
    Unusual(String),
    Cancelled,
}
impl Outcome {
    pub fn text(&self) -> String {
        match self {
            Self::Success => "Login OK!".into(),
            Self::Verification(error) => format!("Verification error: {error}"),
            Self::FinalCookies(error) => format!("Final cookie check failed: {error}"),
            Self::Network(error) => format!("Network error: {error}"),
            Self::Unusual(error) => format!("Unusual error: {error}"),
            Self::Cancelled => "User cancelled the login process.".into(),
        }
    }
    /// Apply only while the domain still refers to the script that was run.
    pub fn update_domain(&self, login: &mut DomainLogin, key: &str, now: i64) -> bool {
        if login.script_key != key {
            return false;
        }
        match self {
            Self::Success => {
                login.validity = Validity::Valid;
                login.validity_error.clear();
            }
            Self::Verification(error) | Self::FinalCookies(error) | Self::Unusual(error) => {
                login.validity = Validity::Invalid;
                login.validity_error.clone_from(error);
            }
            Self::Network(error) => {
                login.no_work_until = now.saturating_add(4 * 3600);
                login.delay_reason.clone_from(error);
            }
            Self::Cancelled => {
                login.no_work_until = now.saturating_add(4 * 3600);
                login.delay_reason = "User cancelled the login process.".into();
            }
        }
        true
    }
}
/// Ordered results and temporary values retained even when a later step fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub results: Vec<TestResult>,
    pub variables: BTreeMap<String, String>,
    pub outcome: Outcome,
}
fn network_outcome(error: &NetError) -> Outcome {
    match error {
        NetError::Cancelled => Outcome::Cancelled,
        NetError::Status { code: 403, .. } => {
            Outcome::Network("403 - login script or credentials may be invalid".into())
        }
        NetError::Status { code: 401, .. } => {
            Outcome::Network("401 - login script or credentials may be invalid".into())
        }
        error => Outcome::Network(
            error
                .to_string()
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
        ),
    }
}
/// Execute through the ordinary HTTP engine/cookie store, with reference two-second
/// spacing after each successful step. A canceled control also stops pending waits.
pub async fn execute(
    engine: &NetEngine,
    store: &Store,
    script: &LoginScript,
    domain: &str,
    credentials: &BTreeMap<String, String>,
    control: &Job,
) -> Execution {
    execute_with_pause(
        engine,
        store,
        script,
        domain,
        credentials,
        control,
        Duration::from_secs(2),
    )
    .await
}
/// Explicit spacing for synthetic tests; production callers use [`execute`].
pub async fn execute_with_pause(
    engine: &NetEngine,
    store: &Store,
    script: &LoginScript,
    domain: &str,
    credentials: &BTreeMap<String, String>,
    control: &Job,
    pause: Duration,
) -> Execution {
    execute_with_observer(
        engine,
        store,
        script,
        domain,
        credentials,
        control,
        pause,
        |_| {},
    )
    .await
}
/// Production login with reference per-step result delivery before its two-second wait.
pub async fn execute_with_results(
    engine: &NetEngine,
    store: &Store,
    script: &LoginScript,
    domain: &str,
    credentials: &BTreeMap<String, String>,
    control: &Job,
    result_ready: impl FnMut(&TestResult),
) -> Execution {
    execute_with_observer(
        engine,
        store,
        script,
        domain,
        credentials,
        control,
        Duration::from_secs(2),
        result_ready,
    )
    .await
}
// Spacing and observation are orthogonal test/production concerns; retain the existing call shape.
#[allow(clippy::too_many_arguments)]
async fn execute_with_observer(
    engine: &NetEngine,
    store: &Store,
    script: &LoginScript,
    domain: &str,
    credentials: &BTreeMap<String, String>,
    control: &Job,
    pause: Duration,
    mut result_ready: impl FnMut(&TestResult),
) -> Execution {
    let mut execution = Execution {
        results: Vec::new(),
        variables: BTreeMap::new(),
        outcome: Outcome::Success,
    };
    for step in &script.steps {
        if control.is_cancelled() {
            execution.outcome = Outcome::Cancelled;
            return execution;
        }
        control.set_stage(step.name.clone());
        let mut result = TestResult::initial(step);
        let before = match cookies(store, domain) {
            Ok(cookies) => cookie_strings(&cookies),
            Err(error) => {
                execution.outcome = Outcome::Unusual(error);
                return execution;
            }
        };
        let planned = match plan(
            step,
            domain,
            credentials,
            &execution.variables,
            execution.results.last().map(|result| result.url.as_str()),
        ) {
            Ok(plan) => plan,
            Err(error) => {
                result.result.clone_from(&error);
                result_ready(&result);
                execution.results.push(result);
                execution.outcome = Outcome::Verification(error);
                return execution;
            }
        };
        result.url.clone_from(&planned.request.url);
        result.body = Some(planned.test_body);
        let response = engine.fetch(&planned.request, control).await;
        let after = match cookies(store, domain) {
            Ok(cookies) => cookies,
            Err(error) => {
                result.result.clone_from(&error);
                result_ready(&result);
                execution.results.push(result);
                execution.outcome = Outcome::Unusual(error);
                return execution;
            }
        };
        result.new_cookies = cookie_strings(&after)
            .difference(&before)
            .cloned()
            .collect();
        match response {
            Err(error) => {
                result.result = error.to_string();
                result_ready(&result);
                execution.results.push(result);
                execution.outcome = network_outcome(&error);
                return execution;
            }
            Ok(response) => {
                if let Err(error) = check_cookies(
                    &step.required_cookies,
                    &after,
                    if step.subdomain.is_some() {
                        domain.strip_prefix("www.").unwrap_or(domain)
                    } else {
                        domain
                    },
                    Some(&step.name),
                ) {
                    result.result.clone_from(&error);
                    result_ready(&result);
                    execution.results.push(result);
                    execution.outcome = Outcome::Verification(error);
                    return execution;
                }
                result.data = response.text();
                let context: ParsingContext = [("url".into(), result.url.clone())].into();
                let mut variables = BTreeMap::new();
                for parser in &step.content_parsers {
                    if !matches!(
                        parser.kind,
                        ContentKind::Variable { .. } | ContentKind::Veto { .. }
                    ) {
                        continue;
                    }
                    match parser.parse(&context, &result.data) {
                        Ok(parsed) => {
                            if let Some((name, value)) = parsed.variable() {
                                variables.insert(name, value);
                            }
                        }
                        Err(error) => {
                            result.new_variables = variables
                                .iter()
                                .map(|(name, value)| format!("{name}: {value}"))
                                .collect();
                            result.result = error.to_string();
                            let outcome = match &error {
                                ParseFailure::Veto(_) => Outcome::Verification(error.to_string()),
                                ParseFailure::Error(_) => Outcome::Unusual(error.to_string()),
                            };
                            result_ready(&result);
                            execution.results.push(result);
                            execution.outcome = outcome;
                            return execution;
                        }
                    }
                }
                result.new_variables = variables
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}"))
                    .collect();
                execution.variables.extend(variables);
                result.result = "OK!".into();
                result_ready(&result);
                execution.results.push(result);
            }
        }
        let until = tokio::time::Instant::now() + pause;
        while tokio::time::Instant::now() < until {
            if control.is_cancelled() {
                execution.outcome = Outcome::Cancelled;
                return execution;
            }
            tokio::time::sleep(
                until
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(Duration::from_millis(50)),
            )
            .await;
        }
    }
    match cookies(store, domain) {
        Ok(cookies) => {
            if let Err(error) = check_cookies(&script.required_cookies, &cookies, domain, None) {
                execution.outcome = Outcome::FinalCookies(error);
            }
        }
        Err(error) => execution.outcome = Outcome::Unusual(error),
    }
    execution
}

/// One active domain login selected by the reference's most-specific domain lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct DemandLogin {
    pub domain: String,
    pub script: Box<LoginScript>,
    pub credentials: BTreeMap<String, String>,
}
/// Request admission before any downloader worker/connection slot is acquired.
#[derive(Debug, Clone, PartialEq)]
pub enum Demand {
    None,
    Blocked { domain: String, error: String },
    Ready(DemandLogin),
}
fn save_resolution(
    store: &Store,
    domain: &str,
    old: &DomainLogin,
    replacement: DomainLogin,
) -> Result<(), String> {
    let domain = domain.to_owned();
    let old = old.clone();
    store
        .write_and_refresh(move |ctx| {
            let mut manager = hydrus_store::logins::load(ctx.conn())?;
            if manager.domains.get(&domain) == Some(&old) {
                manager.domains.insert(domain, replacement);
                hydrus_store::logins::save(ctx.conn(), &manager)?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())
}
/// Read current scripts/credentials/cookies on every admission retry. Inactive
/// specific domains mask active parent domains, and www remains a possible owner.
pub fn demand(store: &Store, requested: &str, now: i64) -> Result<Demand, String> {
    let manager = store
        .read(hydrus_store::logins::load)
        .map_err(|error| error.to_string())?;
    let Some((domain, login)) = std::iter::once(requested.to_owned())
        .chain(hydrus_core::url::psl::all_applicable_domains(requested))
        .find_map(|domain| manager.domains.get(&domain).map(|login| (domain, login)))
    else {
        return Ok(Demand::None);
    };
    if !login.active {
        return Ok(Demand::None);
    }
    let blocked = |error: &str| Demand::Blocked {
        domain: domain.clone(),
        error: format!("The domain \"{domain}\" cannot log in: {error}"),
    };
    let Some(script) = manager.script(login) else {
        let error = format!("Could not find the login script for \"{domain}\"!");
        let mut replacement = login.clone();
        replacement.validity = Validity::Invalid;
        replacement.validity_error.clone_from(&error);
        save_resolution(store, &domain, login, replacement)?;
        return Ok(blocked(&error));
    };
    let mut replacement = login.clone();
    replacement.script_key.clone_from(&script.key);
    replacement.script_name.clone_from(&script.name);
    if let Err(error) = script
        .check_valid()
        .and_then(|()| script.check_credentials(&login.credentials))
    {
        replacement.validity = Validity::Invalid;
        replacement.validity_error.clone_from(&error);
        save_resolution(store, &domain, login, replacement)?;
        return Ok(blocked(&error));
    }
    if replacement.validity == Validity::Untested {
        replacement.validity_error.clear();
    }
    if &replacement != login {
        save_resolution(store, &domain, login, replacement)?;
    }
    if logged_in(store, script, &domain)? {
        return Ok(Demand::None);
    }
    if login.validity == Validity::Invalid {
        return Ok(blocked(&login.validity_error));
    }
    if login.no_work_until >= now && login.no_work_until != 0 {
        return Ok(blocked(&login.delay_reason));
    }
    Ok(Demand::Ready(DemandLogin {
        domain,
        script: Box::new(script.clone()),
        credentials: login.credentials.clone(),
    }))
}
