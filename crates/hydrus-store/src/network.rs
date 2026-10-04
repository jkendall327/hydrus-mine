//! What the client sends when it talks to websites: the cookies of each
//! network session, and custom HTTP headers per network context.
//!
//! A *network context* is what a rule applies to: everything (the global
//! context) or one domain. The reference also has contexts for downloaders,
//! subscriptions and hydrus services; those are carried as their type code
//! and data. Cookies live in *sessions*, one per registrable domain (see
//! [`session_for`]), as the reference keeps them.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::url::psl;

use crate::error::{Result, StoreError};

pub use hydrus_core::network::{CONTEXT_DOMAIN, CONTEXT_GLOBAL, NetworkContext};

/// A cookie, as Python's cookie jar keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    /// `None` for a cookie without a value.
    pub value: Option<String>,
    pub domain: String,
    pub path: String,
    /// Seconds since the epoch; `None` for a session cookie.
    pub expires: Option<i64>,
    pub secure: bool,
    /// Other attributes (`HttpOnly`, `SameSite`, ...).
    pub rest: Vec<(String, Option<String>)>,
}

/// Whether the user has approved a custom header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Denied = 0,
    Approved = 1,
    Pending = 2,
}

impl Approval {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::Denied,
            1 => Self::Approved,
            2 => Self::Pending,
            _ => return None,
        })
    }

    /// The Client API's word for it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Approved => "approved",
            Self::Pending => "pending",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "denied" => Self::Denied,
            "approved" => Self::Approved,
            "pending" => Self::Pending,
            _ => return None,
        })
    }
}

/// A custom HTTP header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomHeader {
    pub name: String,
    pub value: String,
    pub approval: Approval,
    pub reason: String,
}

/// The User-Agent a new client sends.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (compatible; Hydrus Client)";

/// The headers a new client sends everywhere (the reference's defaults).
pub fn default_headers() -> Vec<CustomHeader> {
    let header = |name: &str, value: &str, reason: &str| CustomHeader {
        name: name.into(),
        value: value.into(),
        approval: Approval::Approved,
        reason: reason.into(),
    };
    vec![
        header(
            "User-Agent",
            DEFAULT_USER_AGENT,
            "This is the default User-Agent identifier for the client for all network connections.",
        ),
        header(
            "Accept",
            "image/jpeg,image/png,image/*;q=0.9,*/*;q=0.8",
            "Prefers jpeg/png over webp, but provides graceful fallback.",
        ),
        header(
            "Cache-Control",
            "no-transform",
            "Tells CDNs not to deliver \"optimised\" versions of files. May not be honoured.",
        ),
    ]
}

// sessions and cookies ----------------------------------------------------------

/// The session that holds a context's cookies: a domain's registrable
/// domain (`www.example.co.uk` -> `example.co.uk`), unless a session already
/// exists for a domain above that (sessions made before the public suffix
/// list was used), which is kept using.
pub fn session_for(conn: &Connection, context: &NetworkContext) -> Result<NetworkContext> {
    if context.kind != CONTEXT_DOMAIN {
        return Ok(context.clone());
    }
    let session = psl::second_level_domain(&context.data);
    let mut umbrella = session.as_str();
    while umbrella.matches('.').count() > 1 {
        umbrella = psl::next_level_domain(umbrella);
        if session_exists(conn, &NetworkContext::domain(umbrella))? {
            return Ok(NetworkContext::domain(umbrella));
        }
    }
    Ok(NetworkContext::domain(session))
}

fn session_exists(conn: &Connection, session: &NetworkContext) -> Result<bool> {
    Ok(conn
        .prepare_cached(
            "SELECT 1 FROM network_cookies WHERE session_type = ?1 AND session_key = ?2
             UNION SELECT 1 FROM network_sessions WHERE context_type = ?1 AND context_key = ?2 LIMIT 1",
        )?
        .query_row(params![session.kind, session.data], |_| Ok(()))
        .optional()?
        .is_some())
}

/// All persisted sessions, including explicitly created empty ones.
pub fn sessions(conn: &Connection) -> Result<Vec<NetworkContext>> {
    let mut stmt = conn.prepare_cached(
        "SELECT context_type, context_key FROM network_sessions
         UNION SELECT session_type, session_key FROM network_cookies ORDER BY 1, 2",
    )?;
    Ok(stmt
        .query_map([], |r| {
            Ok(NetworkContext {
                kind: r.get(0)?,
                data: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Retain an empty session until the user explicitly clears it.
pub fn create_session(conn: &Connection, session: &NetworkContext) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO network_sessions VALUES (?, ?)",
        params![session.kind, session.data],
    )?;
    Ok(())
}

/// Delete the selected session and all its cookies.
pub fn clear_session(conn: &Connection, session: &NetworkContext) -> Result<()> {
    conn.execute(
        "DELETE FROM network_cookies WHERE session_type = ? AND session_key = ?",
        params![session.kind, session.data],
    )?;
    conn.execute(
        "DELETE FROM network_sessions WHERE context_type = ? AND context_key = ?",
        params![session.kind, session.data],
    )?;
    Ok(())
}

/// Every context with custom HTTP headers.
pub fn header_contexts(conn: &Connection) -> Result<Vec<NetworkContext>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT context_type, context_key FROM network_headers ORDER BY 1, 2",
    )?;
    Ok(stmt
        .query_map([], |r| {
            Ok(NetworkContext {
                kind: r.get(0)?,
                data: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A session's cookies, in the order Python's cookie jar would list them:
/// by domain, then path, then name, each in the order first added.
pub fn cookies(conn: &Connection, session: &NetworkContext) -> Result<Vec<Cookie>> {
    let mut stmt = conn.prepare_cached(
        "SELECT rowid, name, value, domain, path, expires, secure, rest FROM network_cookies
         WHERE session_type = ? AND session_key = ? ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map(params![session.kind, session.data], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Cookie {
                    name: r.get(1)?,
                    value: r.get(2)?,
                    domain: r.get(3)?,
                    path: r.get(4)?,
                    expires: r.get(5)?,
                    secure: r.get(6)?,
                    rest: Vec::new(),
                },
                r.get::<_, String>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // where each domain, and each path within it, first appeared
    let mut domain_first: HashMap<&str, i64> = HashMap::new();
    let mut path_first: HashMap<(&str, &str), i64> = HashMap::new();
    for (rowid, c, _) in &rows {
        domain_first.entry(&c.domain).or_insert(*rowid);
        path_first.entry((&c.domain, &c.path)).or_insert(*rowid);
    }
    let mut keyed: Vec<((i64, i64, i64), &Cookie, &String)> = rows
        .iter()
        .map(|(rowid, c, rest)| {
            let key = (
                domain_first[c.domain.as_str()],
                path_first[&(c.domain.as_str(), c.path.as_str())],
                *rowid,
            );
            (key, c, rest)
        })
        .collect();
    keyed.sort_by_key(|(key, _, _)| *key);
    keyed
        .into_iter()
        .map(|(_, cookie, rest)| {
            let rest = serde_json::from_str(rest)
                .map_err(|e| StoreError::Corrupt(format!("cookie attributes: {e}")))?;
            Ok(Cookie {
                rest,
                ..cookie.clone()
            })
        })
        .collect()
}

/// Add a cookie to a session, replacing one with the same domain, path and
/// name.
pub fn set_cookie(conn: &Connection, session: &NetworkContext, cookie: &Cookie) -> Result<()> {
    create_session(conn, session)?;
    let rest = serde_json::to_string(&cookie.rest).expect("strings serialise");
    let updated = conn
        .prepare_cached(
            "UPDATE network_cookies SET value = ?, expires = ?, secure = ?, rest = ?
             WHERE session_type = ? AND session_key = ? AND domain = ? AND path = ? AND name = ?",
        )?
        .execute(params![
            cookie.value,
            cookie.expires,
            cookie.secure,
            rest,
            session.kind,
            session.data,
            cookie.domain,
            cookie.path,
            cookie.name
        ])?;
    if updated == 0 {
        conn.prepare_cached(
            "INSERT INTO network_cookies (session_type, session_key, domain, path, name, value, expires, secure, rest)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?
        .execute(params![
            session.kind,
            session.data,
            cookie.domain,
            cookie.path,
            cookie.name,
            cookie.value,
            cookie.expires,
            cookie.secure,
            rest
        ])?;
    }
    Ok(())
}

/// Remove a cookie from a session; whether there was one.
pub fn clear_cookie(
    conn: &Connection,
    session: &NetworkContext,
    domain: &str,
    path: &str,
    name: &str,
) -> Result<bool> {
    Ok(conn
        .prepare_cached(
            "DELETE FROM network_cookies
             WHERE session_type = ? AND session_key = ? AND domain = ? AND path = ? AND name = ?",
        )?
        .execute(params![session.kind, session.data, domain, path, name])?
        > 0)
}

// custom headers ------------------------------------------------------------------

/// A context's custom headers, in the order they were first set.
pub fn headers(conn: &Connection, context: &NetworkContext) -> Result<Vec<CustomHeader>> {
    let mut stmt = conn.prepare_cached(
        "SELECT name, value, approval, reason FROM network_headers
         WHERE context_type = ? AND context_key = ? ORDER BY rowid",
    )?;
    let rows = stmt.query_map(params![context.kind, context.data], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (name, value, approval, reason) = row?;
        Ok(CustomHeader {
            name,
            value,
            approval: Approval::from_code(approval)
                .ok_or_else(|| StoreError::Corrupt(format!("header approval {approval}")))?,
            reason,
        })
    })
    .collect()
}

/// Set a custom header. What isn't given keeps its current value; a new
/// header needs a value, and is approved "by the Client API" unless said
/// otherwise.
pub fn set_header(
    conn: &Connection,
    context: &NetworkContext,
    name: &str,
    value: Option<&str>,
    approval: Option<Approval>,
    reason: Option<&str>,
) -> Result<()> {
    let existing = headers(conn, context)?.into_iter().find(|h| h.name == name);
    let value = match (value, &existing) {
        (Some(v), _) => v.to_owned(),
        (None, Some(h)) => h.value.clone(),
        (None, None) => {
            return Err(StoreError::Invalid(format!(
                "there is no \"{name}\" header to change; give it a value"
            )));
        }
    };
    let approval = approval
        .or(existing.as_ref().map(|h| h.approval))
        .unwrap_or(Approval::Approved);
    let reason = reason
        .map(str::to_owned)
        .or(existing.as_ref().map(|h| h.reason.clone()))
        .unwrap_or_else(|| "Set by Client API".to_owned());
    if existing.is_some() {
        conn.prepare_cached(
            "UPDATE network_headers SET value = ?, approval = ?, reason = ?
             WHERE context_type = ? AND context_key = ? AND name = ?",
        )?
        .execute(params![
            value,
            approval as i64,
            reason,
            context.kind,
            context.data,
            name
        ])?;
    } else {
        conn.prepare_cached(
            "INSERT INTO network_headers (context_type, context_key, name, value, approval, reason)
             VALUES (?, ?, ?, ?, ?, ?)",
        )?
        .execute(params![
            context.kind,
            context.data,
            name,
            value,
            approval as i64,
            reason
        ])?;
    }
    Ok(())
}

/// Remove a custom header; whether there was one.
pub fn delete_header(conn: &Connection, context: &NetworkContext, name: &str) -> Result<bool> {
    Ok(conn
        .prepare_cached(
            "DELETE FROM network_headers WHERE context_type = ? AND context_key = ? AND name = ?",
        )?
        .execute(params![context.kind, context.data, name])?
        > 0)
}

/// Give a new client the default headers.
pub(crate) fn create_defaults(conn: &Connection) -> Result<()> {
    for header in default_headers() {
        set_header(
            conn,
            &NetworkContext::global(),
            &header.name,
            Some(&header.value),
            Some(header.approval),
            Some(&header.reason),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn
    }

    fn cookie(name: &str, domain: &str, path: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: Some("v".into()),
            domain: domain.into(),
            path: path.into(),
            expires: None,
            secure: false,
            rest: vec![("HttpOnly".into(), None)],
        }
    }

    #[test]
    fn cookies_list_in_jar_order() {
        let conn = conn();
        let s = NetworkContext::domain("example.com");
        for (name, domain, path) in [
            ("a", "example.com", "/x"),
            ("b", ".example.com", "/"),
            ("c", "example.com", "/"),
            ("d", "example.com", "/x"),
        ] {
            set_cookie(&conn, &s, &cookie(name, domain, path)).unwrap();
        }
        // replacing keeps the position
        set_cookie(&conn, &s, &cookie("a", "example.com", "/x")).unwrap();
        let names: Vec<String> = cookies(&conn, &s)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["a", "d", "c", "b"]);
        assert!(clear_cookie(&conn, &s, "example.com", "/x", "a").unwrap());
        assert!(!clear_cookie(&conn, &s, "example.com", "/x", "a").unwrap());
        assert_eq!(
            cookies(&conn, &s).unwrap()[0].rest,
            [("HttpOnly".to_owned(), None)]
        );
    }

    #[test]
    fn sessions_are_per_registrable_domain() {
        let conn = conn();
        let session = |d: &str| session_for(&conn, &NetworkContext::domain(d)).unwrap().data;
        // Keep the real multi-label public suffix to exercise its session boundary.
        assert_eq!(session("www.example.co.uk"), "example.co.uk");
        assert_eq!(session("example.com"), "example.com");
        // an older, wider session keeps being used
        set_cookie(
            &conn,
            &NetworkContext::domain("co.uk"),
            &cookie("a", "x.co.uk", "/"),
        )
        .unwrap();
        assert_eq!(session("www.example.co.uk"), "co.uk");
    }

    #[test]
    fn headers_keep_what_is_not_given() {
        let conn = conn();
        create_defaults(&conn).unwrap();
        let global = NetworkContext::global();
        set_header(
            &conn,
            &global,
            "User-Agent",
            None,
            Some(Approval::Denied),
            None,
        )
        .unwrap();
        let ua = headers(&conn, &global).unwrap().remove(0);
        assert_eq!(ua.value, DEFAULT_USER_AGENT);
        assert_eq!(ua.approval, Approval::Denied);
        assert!(set_header(&conn, &global, "X-New", None, None, None).is_err());
        set_header(&conn, &global, "X-New", Some("1"), None, None).unwrap();
        let new = headers(&conn, &global).unwrap().pop().unwrap();
        assert_eq!(
            (new.approval, new.reason.as_str()),
            (Approval::Approved, "Set by Client API")
        );
    }
}

/// The client options for talking to websites and pacing the downloaders,
/// under the reference's names.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Seconds to wait for a connection (`network_timeout`).
    pub network_timeout: u64,
    pub connection_error_wait_time: u64,
    pub serverside_bandwidth_wait_time: u64,
    /// `max_connection_attempts_allowed`.
    pub max_connection_attempts: u32,
    /// `max_request_attempts_allowed_get`.
    pub max_get_attempts: u32,
    /// `max_network_jobs`.
    pub max_jobs: usize,
    /// `max_network_jobs_per_domain`.
    pub max_jobs_per_domain: usize,
    /// `verify_regular_https`.
    pub verify_https: bool,
    /// `domain_network_infrastructure_error_number` and `_time_delta`.
    pub domain_error_number: usize,
    pub domain_error_window: i64,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
    /// Seconds a downloader queue waits after a gallery page or watcher
    /// check fails on the network.
    pub downloader_network_error_delay: u64,
    pub subscription_network_error_delay: i64,
    pub subscription_other_error_delay: i64,
    pub process_subs_in_random_order: bool,
    pub max_simultaneous_subscriptions: u32,
    /// `replace_percent_twenty_with_space_in_gug_input`.
    pub gug_percent_twenty_is_space: bool,
    /// Seconds requests wait after the computer wakes from sleep.
    pub wake_delay_period: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            network_timeout: 10,
            connection_error_wait_time: 15,
            serverside_bandwidth_wait_time: 60,
            max_connection_attempts: 5,
            max_get_attempts: 5,
            max_jobs: 15,
            max_jobs_per_domain: 3,
            verify_https: true,
            domain_error_number: 3,
            domain_error_window: 600,
            http_proxy: None,
            https_proxy: None,
            no_proxy: Some("127.0.0.1".into()),
            downloader_network_error_delay: 90 * 60,
            subscription_network_error_delay: 12 * 3600,
            subscription_other_error_delay: 36 * 3600,
            process_subs_in_random_order: true,
            max_simultaneous_subscriptions: 1,
            gug_percent_twenty_is_space: false,
            wake_delay_period: 15,
        }
    }
}

impl crate::settings::Setting for NetworkSettings {
    const KEY: &'static str = "network";
}

/// The domains the reference logged in to with a login script (which
/// hydrus-rs doesn't run), so a refusal from one can say why.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoginDomains(pub Vec<String>);

impl crate::settings::Setting for LoginDomains {
    const KEY: &'static str = "login_domains";
}

impl LoginDomains {
    /// Whether `url`'s domain, or one it is under, had a login.
    pub fn covers(&self, url: &str) -> bool {
        let Ok(domain) = hydrus_core::url::url_domain(url) else {
            return false;
        };
        psl::all_applicable_domains(&domain)
            .iter()
            .any(|d| self.0.iter().any(|login| login == d))
    }
}

#[cfg(test)]
mod login_tests {
    use super::LoginDomains;

    #[test]
    fn a_login_covers_its_domain_and_subdomains() {
        let logins = LoginDomains(vec!["example.com".into()]);
        assert!(logins.covers("https://example.com/post/1"));
        assert!(logins.covers("https://img.example.com/file.jpg"));
        assert!(!logins.covers("https://other.example/post/1"));
        assert!(!logins.covers("not a url"));
    }
}
