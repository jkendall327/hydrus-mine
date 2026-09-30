//! Access keys, session keys and permissions.
//!
//! A request authenticates with an access key (or a session key standing in
//! for one) in a header or query parameter. Each key has basic permissions
//! and, if it doesn't "permit everything", an optional tag filter limiting
//! which searches it may run. A restricted key may only fetch files its own
//! last search returned.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use parking_lot::RwLock;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use hydrus_core::{HashId, TagFilter};

use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::params::Params;

/// Basic permissions, with the reference's codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum Permission {
    AddUrls = 0,
    AddFiles = 1,
    AddTags = 2,
    SearchFiles = 3,
    ManagePages = 4,
    ManageHeaders = 5,
    ManageDatabase = 6,
    AddNotes = 7,
    ManageFileRelationships = 8,
    EditRatings = 9,
    ManagePopups = 10,
    EditTimes = 11,
    CommitPending = 12,
    SeeLocalPaths = 13,
}

impl Permission {
    pub const ALL: [Permission; 14] = [
        Permission::AddUrls,
        Permission::AddFiles,
        Permission::AddTags,
        Permission::SearchFiles,
        Permission::ManagePages,
        Permission::ManageHeaders,
        Permission::ManageDatabase,
        Permission::AddNotes,
        Permission::ManageFileRelationships,
        Permission::EditRatings,
        Permission::ManagePopups,
        Permission::EditTimes,
        Permission::CommitPending,
        Permission::SeeLocalPaths,
    ];

    pub fn description(self) -> &'static str {
        match self {
            Permission::AddUrls => "import and edit urls",
            Permission::AddFiles => "import and delete files",
            Permission::AddTags => "edit file tags",
            Permission::SearchFiles => "search for and fetch files",
            Permission::ManagePages => "manage pages",
            Permission::ManageHeaders => "manage cookies and headers",
            Permission::ManageDatabase => "manage database",
            Permission::AddNotes => "edit file notes",
            Permission::ManageFileRelationships => "edit file relationships",
            Permission::EditRatings => "edit file ratings",
            Permission::ManagePopups => "manage popups",
            Permission::EditTimes => "edit file times",
            Permission::CommitPending => "commit pending",
            Permission::SeeLocalPaths => "see local file paths",
        }
    }
}

impl From<Permission> for u8 {
    fn from(p: Permission) -> u8 {
        p as u8
    }
}

impl TryFrom<u8> for Permission {
    type Error = String;
    fn try_from(code: u8) -> Result<Self, String> {
        Permission::ALL
            .get(usize::from(code))
            .copied()
            .ok_or_else(|| format!("unknown permission {code}"))
    }
}

/// One access key's permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPermissions {
    pub access_key: Vec<u8>,
    pub name: String,
    pub permits_everything: bool,
    pub basic: BTreeSet<Permission>,
    pub search_filter: TagFilter,
}

impl AccessPermissions {
    pub fn has(&self, permission: Permission) -> bool {
        self.permits_everything || self.basic.contains(&permission)
    }

    pub fn check(&self, permission: Permission) -> ApiResult<()> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "You do not have permission to: {}",
                permission.description()
            )))
        }
    }

    pub fn check_any(&self, permissions: &[Permission]) -> ApiResult<()> {
        if permissions.iter().any(|p| self.has(*p)) {
            return Ok(());
        }
        let names: Vec<&str> = permissions.iter().map(|p| p.description()).collect();
        Err(ApiError::forbidden(format!(
            "You need at least one these permissions: {}",
            names.join(", ")
        )))
    }

    /// Whether searches are unrestricted.
    pub fn searches_unrestricted(&self) -> bool {
        self.permits_everything || self.search_filter.allows_everything()
    }

    /// A restricted key's search must include at least one positive tag its
    /// filter allows.
    pub fn check_can_search_tags(&self, positive_tags: &[String]) -> ApiResult<()> {
        if self.searches_unrestricted()
            || positive_tags
                .iter()
                .any(|t| self.search_filter.tag_ok(t, false))
        {
            return Ok(());
        }
        Err(ApiError::forbidden(format!(
            "You do not have permission to do this search. Your tag search permissions are: {}",
            self.search_filter.to_permitted_string()
        )))
    }

    pub fn check_can_see_all_files(&self) -> ApiResult<()> {
        if self.permits_everything
            || (self.has(Permission::SearchFiles) && self.search_filter.allows_everything())
        {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "You do not have permission to see all files, so you cannot do this.",
            ))
        }
    }

    fn basic_string(&self) -> String {
        if self.permits_everything {
            return "can do anything".into();
        }
        let mut names: Vec<&str> = self.basic.iter().map(|p| p.description()).collect();
        names.sort_unstable();
        names.join(", ")
    }

    /// e.g. "API Permissions (name): search for and fetch files: Can search: only allowing safe"
    pub fn human_description(&self) -> String {
        let mut s = format!("API Permissions ({}): ", self.name);
        s += &self.basic_string();
        if !self.permits_everything && self.has(Permission::SearchFiles) {
            s += &format!(": Can search: {}", self.search_filter.to_permitted_string());
        }
        s
    }
}

const SESSION_LIFETIME: Duration = Duration::from_secs(86_400);
const SEARCH_RESULTS_LIFETIME: Duration = Duration::from_secs(4 * 3600);

type Session = (Vec<u8>, Instant);
type LastSearch = (HashSet<HashId>, Instant);

/// Every access key, plus live sessions and restricted keys' last searches.
#[derive(Debug, Default)]
pub struct AccessRegistry {
    keys: RwLock<HashMap<Vec<u8>, AccessPermissions>>,
    /// session key -> (access key, created)
    sessions: RwLock<HashMap<Vec<u8>, Session>>,
    /// access key -> a restricted key's latest search results
    last_search: RwLock<HashMap<Vec<u8>, LastSearch>>,
}

/// The refusal of an access key that isn't known.
pub const UNKNOWN_KEY: &str = "Did not find an entry for that access key!";

/// Every access key in the database.
pub fn stored_keys(conn: &Connection) -> hydrus_store::Result<Vec<AccessPermissions>> {
    Ok(AccessRegistry::load(conn)?
        .keys
        .into_inner()
        .into_values()
        .collect())
}

/// Add (or replace) an access key in the database.
pub fn save_key(conn: &Connection, key: &AccessPermissions) -> hydrus_store::Result<()> {
    let filter = if key.search_filter == TagFilter::default() {
        None
    } else {
        Some(serde_json::to_string(&key.search_filter)?)
    };
    conn.execute(
        "INSERT OR REPLACE INTO api_permissions (access_key, name, permits_everything, permissions, search_tag_filter)
         VALUES (?, ?, ?, ?, ?)",
        rusqlite::params![
            key.access_key,
            key.name,
            key.permits_everything,
            serde_json::to_string(&key.basic)?,
            filter
        ],
    )?;
    Ok(())
}

/// Remove an access key from the database; whether there was one.
pub fn delete_key(conn: &Connection, access_key: &[u8]) -> hydrus_store::Result<bool> {
    Ok(conn.execute(
        "DELETE FROM api_permissions WHERE access_key = ?",
        [access_key],
    )? > 0)
}

/// Access keys asked for through `/request_new_permissions`, while
/// `hydrus api-keys <store> listen` (standing in for the reference's
/// registration dialog) is running.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    /// Requests are accepted until then (ms since the epoch).
    pub open_until_ms: Option<i64>,
    /// Keys asked for and not yet accepted or refused: access key (hex),
    /// name, everything, basic permissions.
    pub requests: Vec<(String, String, bool, Vec<Permission>)>,
}

impl hydrus_store::settings::Setting for Registration {
    const KEY: &'static str = "api_registration";
}

impl AccessRegistry {
    /// Re-read the access keys (another process may have changed them),
    /// keeping sessions.
    pub fn reload_keys(&self, conn: &Connection) -> hydrus_store::Result<()> {
        let fresh = Self::load(conn)?.keys.into_inner();
        *self.keys.write() = fresh;
        Ok(())
    }

    /// Load every access key from the database.
    pub fn load(conn: &Connection) -> hydrus_store::Result<Self> {
        let mut stmt =
            conn.prepare("SELECT access_key, name, permits_everything, permissions, search_tag_filter FROM api_permissions")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut keys = HashMap::new();
        for row in rows {
            let (access_key, name, permits_everything, permissions, filter) = row?;
            let basic: BTreeSet<Permission> = serde_json::from_str(&permissions)?;
            let search_filter = match filter {
                Some(f) => serde_json::from_str(&f)?,
                None => TagFilter::default(),
            };
            keys.insert(
                access_key.clone(),
                AccessPermissions {
                    access_key,
                    name,
                    permits_everything,
                    basic,
                    search_filter,
                },
            );
        }
        Ok(Self {
            keys: RwLock::new(keys),
            ..Self::default()
        })
    }

    pub fn insert(&self, permissions: AccessPermissions) {
        self.keys
            .write()
            .insert(permissions.access_key.clone(), permissions);
    }

    /// Establish who a request is from.
    pub fn authenticate(
        &self,
        headers: &HeaderMap,
        params: &Params,
    ) -> ApiResult<AccessPermissions> {
        let access_key = if let Some(key) = header_key(headers, ACCESS_KEY_HEADER)? {
            Some(key)
        } else if let Some(session) = header_key(headers, SESSION_KEY_HEADER)? {
            Some(self.session_access_key(&session)?)
        } else {
            self.key_from_params(params)?
        };
        let Some(access_key) = access_key else {
            return Err(ApiError::new(
                ErrorKind::MissingCredentials,
                "No access key or session key provided!",
            ));
        };
        self.keys
            .read()
            .get(&access_key)
            .cloned()
            .ok_or_else(|| ApiError::forbidden(UNKNOWN_KEY))
    }

    fn key_from_params(&self, params: &Params) -> ApiResult<Option<Vec<u8>>> {
        if let Some(key) = params.optional::<Vec<u8>>(ACCESS_KEY_HEADER)? {
            return Ok(Some(key));
        }
        match params.optional::<Vec<u8>>(SESSION_KEY_HEADER)? {
            Some(session) => self.session_access_key(&session).map(Some),
            None => Ok(None),
        }
    }

    fn session_access_key(&self, session_key: &[u8]) -> ApiResult<Vec<u8>> {
        let sessions = self.sessions.read();
        match sessions.get(session_key) {
            Some((access_key, created)) if created.elapsed() < SESSION_LIFETIME => {
                Ok(access_key.clone())
            }
            _ => Err(ApiError::new(
                ErrorKind::Session,
                "Did not find an entry for that session key!",
            )),
        }
    }

    /// Mint a session key for an access key.
    pub fn new_session(&self, access_key: &[u8]) -> Vec<u8> {
        let session = rand::random::<[u8; 32]>().to_vec();
        self.sessions
            .write()
            .insert(session.clone(), (access_key.to_vec(), Instant::now()));
        session
    }

    /// Remember a restricted key's latest search results.
    pub fn record_search(&self, permissions: &AccessPermissions, results: &[HashId]) {
        if !permissions.searches_unrestricted() {
            self.last_search.write().insert(
                permissions.access_key.clone(),
                (results.iter().copied().collect(), Instant::now()),
            );
        }
    }

    /// A restricted key may only see files its last search returned.
    pub fn check_can_see(
        &self,
        permissions: &AccessPermissions,
        hash_ids: &[HashId],
    ) -> ApiResult<()> {
        if permissions.searches_unrestricted() {
            return Ok(());
        }
        let searches = self.last_search.read();
        let Some((allowed, _)) = searches
            .get(&permissions.access_key)
            .filter(|(_, when)| when.elapsed() < SEARCH_RESULTS_LIFETIME)
        else {
            return Err(ApiError::bad_request(
                "It looks like those search results are no longer available--please run the search again!",
            ));
        };
        let visible = hash_ids.iter().filter(|h| allowed.contains(h)).count();
        if visible == hash_ids.len() {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "You do not seem to have access to all those files! You asked to see {} files, but you were only authorised to see {} of them!",
                hash_ids.len(),
                visible
            )))
        }
    }
}

pub const ACCESS_KEY_HEADER: &str = "Hydrus-Client-API-Access-Key";
pub const SESSION_KEY_HEADER: &str = "Hydrus-Client-API-Session-Key";

fn header_key(headers: &HeaderMap, name: &str) -> ApiResult<Option<Vec<u8>>> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    value
        .to_str()
        .ok()
        .and_then(|v| hex::decode(v.trim()).ok())
        .map(Some)
        .ok_or_else(|| ApiError::bad_request(format!("Problem parsing {name}!")))
}
