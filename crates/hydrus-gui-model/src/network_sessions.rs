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
/// The note under the context type choice (`CC.network_context_type_description_lookup`)
/// for the new-session editor's two types: 0 web domain, 1 hydrus service.
pub fn context_type_info(choice: i32) -> &'static str {
    if choice == 1 {
        "Network traffic going to or from a hydrus service."
    } else {
        "Network traffic going to or from a web domain (or a subdomain)."
    }
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
    expiry_text_with_format(
        expires,
        now,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn expiry_text_with_format(
    expires: Option<i64>,
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> String {
    match expires {
        None => "session".into(),
        Some(0) => "unknown expiration".into(),
        Some(time) if time.checked_sub(now).and_then(i64::checked_abs).is_none() => {
            format!("unparseable time {time}")
        }
        Some(time) => format!(
            "{} {}",
            if time < now { "expired" } else { "expires" },
            crate::gui_format::timestamp(formatting, Some(time), now)
        ),
    }
}
/// Reference session row, summarising the latest persistent expiry.
pub fn session_cells(context: &NetworkContext, cookies: &[Cookie], now: i64) -> Vec<String> {
    session_cells_with_format(
        context,
        cookies,
        now,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn session_cells_with_format(
    context: &NetworkContext,
    cookies: &[Cookie],
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> Vec<String> {
    vec![
        context.to_human_string(),
        cookies.len().to_string(),
        if cookies.is_empty() {
            String::new()
        } else {
            expiry_text_with_format(
                cookies.iter().filter_map(|c| c.expires).max(),
                now,
                formatting,
            )
        },
    ]
}
/// Cookie display fields, including a secure flag beyond the reference's columns.
pub fn cookie_cells(cookie: &Cookie, now: i64) -> Vec<String> {
    cookie_cells_with_format(
        cookie,
        now,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn cookie_cells_with_format(
    cookie: &Cookie,
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> Vec<String> {
    vec![
        cookie.name.clone(),
        cookie.value.clone().unwrap_or_default(),
        cookie.domain.clone(),
        cookie.path.clone(),
        expiry_text_with_format(cookie.expires, now, formatting),
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
/// Keep clipboard/file imports bounded before decoding or changing a draft.
pub const COOKIE_EXCHANGE_LIMIT: usize = 8 * 1024 * 1024;
const COOKIE_COUNT_LIMIT: usize = 10_000;
type FlatCookie = (String, Option<String>, String, String, Option<i64>);

/// The reference clipboard format deliberately carries five fields only.
/// Secure and other attributes belong to Netscape file exchange instead.
pub fn export_cookies(cookies: &[Cookie]) -> Result<String, String> {
    let flat: Vec<FlatCookie> = cookies
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                c.value.clone(),
                c.domain.clone(),
                c.path.clone(),
                c.expires,
            )
        })
        .collect();
    serde_json::to_string(&flat).map_err(|e| e.to_string())
}

fn check_exchange_size(text: &str) -> Result<(), String> {
    if text.len() > COOKIE_EXCHANGE_LIMIT {
        return Err("Cookie imports are limited to 8 MiB.".into());
    }
    Ok(())
}
fn check_import_cookie(cookie: Cookie) -> Result<Cookie, String> {
    // Check raw fields before trimming so a leading/trailing control cannot hide.
    if [
        cookie.name.as_str(),
        cookie.value.as_deref().unwrap_or_default(),
        cookie.domain.as_str(),
        cookie.path.as_str(),
    ]
    .iter()
    .any(|field| field.chars().any(char::is_control))
    {
        return Err("Cookie fields cannot contain control characters.".into());
    }
    let mut checked = validate_cookie(cookie.clone())?;
    // Imported values may contain intentional leading/trailing spaces.
    checked.value = cookie.value;
    Ok(checked)
}

/// Decode the actual reference five-field JSON, rejecting the entire malformed batch.
/// An expiry of zero means a session cookie, as in the reference import handlers.
pub fn import_cookie_clipboard(text: &str) -> Result<Vec<Cookie>, String> {
    check_exchange_size(text)?;
    let flat: Vec<FlatCookie> = serde_json::from_str(text)
        .map_err(|e| format!("Did not understand what was in the clipboard: {e}"))?;
    if flat.len() > COOKIE_COUNT_LIMIT {
        return Err("Cookie imports are limited to 10,000 cookies.".into());
    }
    flat.into_iter()
        .map(|(name, value, domain, path, expires)| {
            check_import_cookie(Cookie {
                name,
                value,
                domain: domain.to_ascii_lowercase(),
                path,
                expires: expires.filter(|&e| e != 0),
                secure: false,
                rest: Vec::new(),
            })
        })
        .collect()
}

/// Load Netscape/Mozilla cookies.txt including expired, session and HttpOnly cookies.
/// The leading dot and TRUE/FALSE domain flag must agree, as MozillaCookieJar requires.
pub fn import_netscape_cookies(text: &str) -> Result<Vec<Cookie>, String> {
    check_exchange_size(text)?;
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    if !(first.starts_with("# Netscape HTTP Cookie File")
        || first.starts_with("# HTTP Cookie File"))
    {
        return Err("It looks like that cookies.txt failed to load. Unfortunately, not all formats are supported.".into());
    }
    let mut cookies = Vec::new();
    for (line_number, line) in lines.enumerate() {
        let (line, http_only) = line
            .strip_prefix("#HttpOnly_")
            .map_or((line, false), |line| (line, true));
        if line.trim().is_empty() || line.starts_with(['#', '$']) {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        let invalid = || format!("Invalid Netscape cookie on line {}.", line_number + 2);
        if fields.len() != 7
            || !["TRUE", "FALSE"].contains(&fields[1])
            || !["TRUE", "FALSE"].contains(&fields[3])
            || fields[0].starts_with('.') != (fields[1] == "TRUE")
        {
            return Err(invalid());
        }
        let expires = if fields[4].is_empty() || fields[4] == "0" {
            None
        } else {
            Some(fields[4].parse::<i64>().map_err(|_| invalid())?)
        };
        let (name, value) = if fields[5].is_empty() {
            (fields[6].to_owned(), None)
        } else {
            (fields[5].to_owned(), Some(fields[6].to_owned()))
        };
        let cookie = check_import_cookie(Cookie {
            name,
            value,
            domain: fields[0].to_ascii_lowercase(),
            path: fields[2].into(),
            expires,
            secure: fields[3] == "TRUE",
            rest: if http_only {
                vec![("HTTPOnly".into(), Some(String::new()))]
            } else {
                Vec::new()
            },
        })
        .map_err(|e| format!("{} {e}", invalid()))?;
        // MozillaCookieJar resolves duplicate identities to the last value in a file.
        if let Some(index) = cookies
            .iter()
            .position(|c| cookie_key(c) == cookie_key(&cookie))
        {
            cookies[index] = cookie;
        } else {
            cookies.push(cookie);
        }
        if cookies.len() > COOKIE_COUNT_LIMIT {
            return Err("Cookie imports are limited to 10,000 cookies.".into());
        }
    }
    Ok(cookies)
}

/// The domain suffix filter used by the reference session clipboard import choice.
pub fn matching_cookies(cookies: &[Cookie], session: &NetworkContext) -> Vec<Cookie> {
    cookies
        .iter()
        .filter(|c| session.kind != network::CONTEXT_DOMAIN || c.domain.ends_with(&session.data))
        .cloned()
        .collect()
}

/// Describe the exact import batch before changing any cookies.
pub fn cookie_import_question(cookies: &[Cookie], browser: bool) -> String {
    let domains: BTreeSet<_> = cookies.iter().map(|c| c.domain.as_str()).collect();
    let summary = if domains.len() == 1 {
        format!(
            " \"{}\"{}",
            domains.first().copied().unwrap_or_default(),
            if browser { " " } else { "" }
        )
    } else {
        format!(
            "\n\n{}{}",
            domains.into_iter().collect::<Vec<_>>().join("\n"),
            if browser { "\n\n" } else { "" }
        )
    };
    format!(
        "About to import {} cookies for the domains {summary}{} Is that ok?",
        cookies.len(),
        if browser { "" } else { "." }
    )
}

/// Import cookies into their domain silos in one transaction, replacing matching identities.
pub fn import_cookie_sessions(store: &Store, cookies: Vec<Cookie>) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        for cookie in cookies {
            let session = network::session_for(
                ctx.conn(),
                &NetworkContext::domain(cookie.domain.trim_start_matches('.')),
            )?;
            network::set_cookie(ctx.conn(), &session, &cookie)?;
        }
        Ok(())
    })
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
    /// Stage a validated import batch; the last matching identity replaces the old value.
    /// Cancelling this editor still cancels all imported cookies.
    pub fn import(&mut self, cookies: Vec<Cookie>) -> Result<usize, String> {
        let cookies = cookies
            .into_iter()
            .map(check_import_cookie)
            .collect::<Result<Vec<_>, _>>()?;
        let count = cookies.len();
        for cookie in cookies {
            if let Some(index) = self
                .cookies
                .iter()
                .position(|c| cookie_key(c) == cookie_key(&cookie))
            {
                self.cookies[index] = cookie;
            } else {
                self.cookies.push(cookie);
            }
        }
        Ok(count)
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
/// A pending header currently blocking one or more requests in a daemon epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderQuestion {
    pub row: HeaderRow,
    pub jobs: Vec<(String, u64)>,
}
impl HeaderQuestion {
    /// The exact question text produced by the reference validation popup process.
    pub fn text(&self) -> String {
        format!(
            "For the network context {}, can the client set this header?\n\n{}: {}\n\n{}",
            self.row.context.to_human_string(),
            self.row.header.name,
            self.row.header.value,
            self.row.header.reason
        )
    }
}

/// Questions are triggered only by fresh, live requests waiting on these contexts.
/// Multiple requests for the same header share one decision.
pub fn pending_header_questions(
    store: &Store,
    now: i64,
) -> hydrus_store::Result<Vec<HeaderQuestion>> {
    store.read(|conn| {
        let snapshot =
            hydrus_store::settings::get::<hydrus_store::network_runtime::Snapshot>(conn)?;
        if !snapshot.fresh(now) {
            return Ok(Vec::new());
        }
        let mut questions: Vec<HeaderQuestion> = Vec::new();
        for job in snapshot
            .jobs
            .iter()
            .filter(|j| j.wait == hydrus_store::network_runtime::WaitReason::Headers)
        {
            for context in &job.contexts {
                for header in network::headers(conn, context)?
                    .into_iter()
                    .filter(|h| h.approval == Approval::Pending)
                {
                    let row = HeaderRow {
                        context: context.clone(),
                        header,
                    };
                    let identity = (snapshot.epoch.clone(), job.id);
                    if let Some(question) = questions.iter_mut().find(|q| q.row == row) {
                        if !question.jobs.contains(&identity) {
                            question.jobs.push(identity);
                        }
                    } else {
                        questions.push(HeaderQuestion {
                            row,
                            jobs: vec![identity],
                        });
                    }
                }
            }
        }
        for question in &mut questions {
            question.jobs.sort();
        }
        Ok(questions)
    })
}

/// Approve or deny precisely the header that was shown, preserving its value/reason.
/// A changed or removed header cannot inherit an answer to an old question.
pub fn answer_header_question(
    store: &Store,
    question: HeaderQuestion,
    approved: bool,
) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        let row = question.row;
        let current = network::headers(ctx.conn(), &row.context)?.into_iter().find(|h| h.name == row.header.name);
        if row.header.approval != Approval::Pending || current.as_ref() != Some(&row.header) {
            return Err(StoreError::Invalid("This header changed while the question was open. Review its new value before approving it.".into()));
        }
        network::set_header(ctx.conn(), &row.context, &row.header.name, None,
            Some(if approved { Approval::Approved } else { Approval::Denied }), None)
    })
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
