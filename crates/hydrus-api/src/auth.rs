//! Access keys, session keys and permissions.
//!
//! A request authenticates with an access key (or a session key standing in
//! for one) in a header or query parameter. Each key has basic permissions
//! and, if it doesn't "permit everything", an optional tag filter limiting
//! which searches it may run. A restricted key may only fetch files its own
//! last search returned.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use parking_lot::RwLock;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use hydrus_core::HashId;

use crate::error::{ApiError, ApiResult, ErrorKind};
use crate::params::Params;

pub use hydrus_store::api_permissions::{
    AccessPermissions, Permission, delete_key, save_key, stored_keys,
};

/// HTTP permission refusals for persisted access permissions.
pub trait PermissionChecks {
    fn check(&self, permission: Permission) -> ApiResult<()>;
    fn check_any(&self, permissions: &[Permission]) -> ApiResult<()>;
    fn check_can_search_tags(&self, positive_tags: &[String]) -> ApiResult<()>;
    fn check_can_see_all_files(&self) -> ApiResult<()>;
}
impl PermissionChecks for AccessPermissions {
    fn check(&self, permission: Permission) -> ApiResult<()> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "You do not have permission to: {}",
                permission.description()
            )))
        }
    }
    fn check_any(&self, permissions: &[Permission]) -> ApiResult<()> {
        if permissions.iter().any(|p| self.has(*p)) {
            return Ok(());
        }
        let names: Vec<&str> = permissions.iter().map(|p| p.description()).collect();
        Err(ApiError::forbidden(format!(
            "You need at least one these permissions: {}",
            names.join(", ")
        )))
    }
    fn check_can_search_tags(&self, positive_tags: &[String]) -> ApiResult<()> {
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
    fn check_can_see_all_files(&self) -> ApiResult<()> {
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
        let mut keys = self.keys.write();
        // Previously returned files cannot survive a narrowed filter or revoke.
        self.last_search
            .write()
            .retain(|key, _| keys.get(key) == fresh.get(key) && fresh.contains_key(key));
        self.sessions
            .write()
            .retain(|_, (key, _)| fresh.contains_key(key));
        *keys = fresh;
        Ok(())
    }

    /// Load every access key from the database.
    pub fn load(conn: &Connection) -> hydrus_store::Result<Self> {
        let keys = stored_keys(conn)?
            .into_iter()
            .map(|key| (key.access_key.clone(), key))
            .collect();
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
        let keys = self.keys.read();
        if keys.get(&permissions.access_key) == Some(permissions)
            && !permissions.searches_unrestricted()
        {
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
