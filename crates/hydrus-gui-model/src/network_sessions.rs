//! Detached cookie and HTTP-header drafts. Apply merges changed keys only;
//! a concurrent write to a changed key rejects the whole draft.
use hydrus_store::{
    Store, StoreError,
    network::{self, Approval, Cookie, CustomHeader, NetworkContext},
};
use std::collections::BTreeSet;

/// The session-clear question shown by the reference.
pub const CLEAR_QUESTION: &str = "Clear these sessions? This will delete them completely.";
/// The cookie-removal question shown by the reference.
pub const DELETE_QUESTION: &str = "Delete all selected cookies?";
/// A cookie's identity within a session.
pub fn cookie_key(cookie: &Cookie) -> (String, String, String) {
    (
        cookie.domain.clone(),
        cookie.path.clone(),
        cookie.name.clone(),
    )
}
/// Reject whitespace/newlines and delimiters that cannot safely become HTTP fields.
pub fn validate_cookie(mut cookie: Cookie) -> Result<Cookie, String> {
    cookie.name = cookie.name.trim().into();
    cookie.value = cookie.value.map(|v| v.trim().into());
    cookie.domain = cookie.domain.trim().to_ascii_lowercase();
    cookie.path = cookie.path.trim().into();
    for (text, label) in [
        (cookie.name.as_str(), "name"),
        (cookie.value.as_deref().unwrap_or(""), "value"),
        (cookie.domain.as_str(), "domain"),
        (cookie.path.as_str(), "path"),
    ] {
        if text.chars().any(char::is_control) {
            return Err(format!("Hey, it looks like the \"{label}\" has a newline!"));
        }
    }
    if cookie.name.is_empty() || cookie.name.contains([';', '=', ' ', ',']) {
        return Err("A cookie needs a valid name.".into());
    }
    if cookie.value.as_ref().is_some_and(|v| v.contains(';')) {
        return Err("Cookie values cannot contain a semicolon.".into());
    }
    validate_domain(cookie.domain.trim_start_matches('.'))?;
    if !cookie.path.starts_with('/') {
        return Err("Cookie paths must start with /.".into());
    }
    if cookie
        .expires
        .is_some_and(|e| !(0..=253_402_300_799).contains(&e))
    {
        return Err("Cookie expiry must be UTC seconds from 1970 through year 9999.".into());
    }
    Ok(cookie)
}
/// Parse an explicit domain rather than a URL or a wildcard.
pub fn validate_domain(domain: &str) -> Result<(), String> {
    if domain.is_empty()
        || domain.contains(['/', ':', '?', '#', '*', '@', ';', '=', ','])
        || domain.chars().any(char::is_whitespace)
    {
        return Err("Enter a hostname without a scheme, path or wildcard.".into());
    }
    Ok(())
}
/// Hostname with an optional numeric port, as request contexts retain the authority.
pub fn validate_context_domain(domain: &str) -> Result<(), String> {
    if let Some((host, port)) = domain.rsplit_once(':') {
        port.parse::<u16>()
            .map_err(|_| "Enter a valid numeric port.")?;
        validate_domain(host)
    } else {
        validate_domain(domain)
    }
}
/// Trim HTTP key/value input and reject header injection and invalid field names.
pub fn validate_header(
    context: &NetworkContext,
    mut header: CustomHeader,
) -> Result<CustomHeader, String> {
    if context.kind == network::CONTEXT_DOMAIN {
        validate_context_domain(&context.data)?;
    } else if context.kind != network::CONTEXT_GLOBAL {
        return Err("Headers require a global or web domain context.".into());
    }
    header.name = header.name.trim().into();
    header.value = header.value.trim().into();
    for (text, label) in [(&header.name, "key"), (&header.value, "value")] {
        if text.chars().any(char::is_control) {
            return Err(format!("Hey, it looks like the \"{label}\" has a newline!"));
        }
    }
    if header.name.is_empty()
        || !header
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    {
        return Err("Enter a valid HTTP header name.".into());
    }
    Ok(header)
}
/// Human text in the header approval column (the reference calls pending unknown).
pub fn approval_text(approval: Approval) -> &'static str {
    match approval {
        Approval::Approved => "approved - will use it",
        Approval::Denied => "denied - will not use it",
        Approval::Pending => "pending - will ask the user on next use",
    }
}
/// Human expiry text, shared by session and cookie rows.
pub fn expiry_text(expires: Option<i64>, now: i64) -> String {
    match expires {
        None => "session".into(),
        Some(0) => "unknown expiration".into(),
        Some(time) if time.checked_sub(now).and_then(i64::checked_abs).is_none() => {
            format!("unparseable time {time}")
        }
        Some(time) => format!(
            "{} {}",
            if time <= now { "expired" } else { "expires" },
            hydrus_core::time::timestamp_to_pretty_time_delta(time, now, " ago")
        ),
    }
}
/// Reference session row, summarising the latest persistent expiry.
pub fn session_cells(context: &NetworkContext, cookies: &[Cookie], now: i64) -> Vec<String> {
    vec![
        context.to_human_string(),
        cookies.len().to_string(),
        if cookies.is_empty() {
            String::new()
        } else {
            expiry_text(cookies.iter().filter_map(|c| c.expires).max(), now)
        },
    ]
}
/// Cookie display fields, including a secure flag beyond the reference's columns.
pub fn cookie_cells(cookie: &Cookie, now: i64) -> Vec<String> {
    vec![
        cookie.name.clone(),
        cookie.value.clone().unwrap_or_default(),
        cookie.domain.clone(),
        cookie.path.clone(),
        expiry_text(cookie.expires, now),
        if cookie.secure { "yes" } else { "no" }.into(),
    ]
}
/// Reference context/header/state/reason row.
pub fn header_cells(row: &HeaderRow) -> Vec<String> {
    vec![
        row.context.to_human_string(),
        format!("{}: {}", row.header.name, row.header.value),
        approval_text(row.header.approval).into(),
        row.header.reason.clone(),
    ]
}
/// Cookies staged for one session; dropping the draft cancels every change.
#[derive(Debug, Clone)]
pub struct CookieDraft {
    pub session: NetworkContext,
    pub cookies: Vec<Cookie>,
    original: Vec<Cookie>,
    existed: bool,
}
impl CookieDraft {
    /// Read the current cookie jar.
    pub fn new(store: &Store, session: NetworkContext) -> hydrus_store::Result<Self> {
        let (cookies, existed) = store.read(|c| {
            Ok((
                network::cookies(c, &session)?,
                network::sessions(c)?.contains(&session),
            ))
        })?;
        Ok(Self {
            session,
            original: cookies.clone(),
            existed,
            cookies,
        })
    }
    /// Replace the old identity when domain/path/name changes, preserving attributes.
    pub fn edit(&mut self, index: Option<usize>, cookie: Cookie) -> Result<(), String> {
        let cookie = validate_cookie(cookie)?;
        if self
            .cookies
            .iter()
            .enumerate()
            .any(|(i, c)| Some(i) != index && cookie_key(c) == cookie_key(&cookie))
        {
            return Err("A cookie with this domain, path and name already exists.".into());
        }
        if let Some(i) = index {
            *self.cookies.get_mut(i).ok_or("Cookie no longer exists.")? = cookie;
        } else {
            self.cookies.push(cookie);
        }
        Ok(())
    }
    /// Apply changed identities atomically without replacing cookies arriving meanwhile.
    pub fn apply(&self, store: &Store) -> hydrus_store::Result<()> {
        let draft = self.clone();
        store.write(move |ctx| {
            let conn = ctx.conn(); let current = network::cookies(conn, &draft.session)?;
            if draft.existed && !network::sessions(conn)?.contains(&draft.session) && draft.cookies != draft.original { return Err(StoreError::Invalid("This session was cleared elsewhere. Reopen the session browser.".into())); }
            let keys: BTreeSet<_> = draft.original.iter().chain(&draft.cookies).map(cookie_key).collect();
            for key in keys {
                let old = draft.original.iter().find(|c| cookie_key(c) == key);
                let new = draft.cookies.iter().find(|c| cookie_key(c) == key);
                if old == new { continue; }
                if current.iter().find(|c| cookie_key(c) == key) != old { return Err(StoreError::Invalid("A changed cookie was modified by another request or editor. Reopen this session.".into())); }
                if let Some(cookie) = new { network::set_cookie(conn, &draft.session, cookie)?; }
                else { network::clear_cookie(conn, &draft.session, &key.0, &key.1, &key.2)?; }
            }
            Ok(())
        })
    }
}
/// A header in a specific context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRow {
    pub context: NetworkContext,
    pub header: CustomHeader,
}
/// All staged custom headers.
#[derive(Debug, Clone)]
pub struct HeaderDraft {
    pub rows: Vec<HeaderRow>,
    original: Vec<HeaderRow>,
}
impl HeaderDraft {
    /// Read every configured context.
    pub fn new(store: &Store) -> hydrus_store::Result<Self> {
        let rows = store.read(|c| {
            let mut rows = Vec::new();
            for context in network::header_contexts(c)? {
                for header in network::headers(c, &context)? {
                    rows.push(HeaderRow {
                        context: context.clone(),
                        header,
                    });
                }
            }
            Ok(rows)
        })?;
        Ok(Self {
            original: rows.clone(),
            rows,
        })
    }
    /// Accept a child dialog without touching persistent headers.
    pub fn edit(&mut self, index: Option<usize>, mut row: HeaderRow) -> Result<(), String> {
        row.header = validate_header(&row.context, row.header)?;
        if self.rows.iter().enumerate().any(|(i, r)| {
            Some(i) != index
                && r.context == row.context
                && r.header.name.eq_ignore_ascii_case(&row.header.name)
        }) {
            return Err("This context already has a header with that name.".into());
        }
        if let Some(i) = index {
            *self.rows.get_mut(i).ok_or("Header no longer exists.")? = row;
        } else {
            self.rows.push(row);
        }
        Ok(())
    }
    /// Merge changed header keys, preserving unrelated concurrent writes.
    pub fn apply(&self, store: &Store) -> hydrus_store::Result<()> {
        let draft = self.clone();
        store.write(move |ctx| {
            let conn = ctx.conn();
            let keys: BTreeSet<_> = draft
                .original
                .iter()
                .chain(&draft.rows)
                .map(|r| (r.context.clone(), r.header.name.to_ascii_lowercase()))
                .collect();
            for (context, name) in keys {
                let old: Vec<_> = draft
                    .original
                    .iter()
                    .filter(|r| r.context == context && r.header.name.eq_ignore_ascii_case(&name))
                    .collect();
                let new: Vec<_> = draft
                    .rows
                    .iter()
                    .filter(|r| r.context == context && r.header.name.eq_ignore_ascii_case(&name))
                    .collect();
                if old == new {
                    continue;
                }
                let current: Vec<_> = network::headers(conn, &context)?
                    .into_iter()
                    .filter(|h| h.name.eq_ignore_ascii_case(&name))
                    .collect();
                if current.iter().collect::<Vec<_>>()
                    != old.iter().map(|r| &r.header).collect::<Vec<_>>()
                {
                    return Err(StoreError::Invalid(
                        "A changed header was modified by another editor. Reopen HTTP headers."
                            .into(),
                    ));
                }
                for row in &old {
                    if !new.iter().any(|r| r.header.name == row.header.name) {
                        network::delete_header(conn, &context, &row.header.name)?;
                    }
                }
                for row in new {
                    if old.iter().any(|r| r.header == row.header) {
                        continue;
                    }
                    network::set_header(
                        conn,
                        &context,
                        &row.header.name,
                        Some(&row.header.value),
                        Some(row.header.approval),
                        Some(&row.header.reason),
                    )?;
                }
            }
            Ok(())
        })
    }
}
