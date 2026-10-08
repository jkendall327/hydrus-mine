//! Typed Client API permissions and native persistence shared by GUI and server.
use hydrus_core::TagFilter;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

    /// Whether searches are unrestricted.
    pub fn searches_unrestricted(&self) -> bool {
        self.permits_everything || self.search_filter.allows_everything()
    }

    /// Alphabetically ordered basic permission labels for service review.
    pub fn basic_string(&self) -> String {
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

/// Every access key in the database.
pub fn stored_keys(conn: &Connection) -> crate::Result<Vec<AccessPermissions>> {
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
    let mut keys = Vec::new();
    for row in rows {
        let (access_key, name, permits_everything, permissions, filter) = row?;
        let basic: BTreeSet<Permission> = serde_json::from_str(&permissions)?;
        let search_filter = match filter {
            Some(f) => serde_json::from_str(&f)?,
            None => TagFilter::default(),
        };
        keys.push(AccessPermissions {
            access_key,
            name,
            permits_everything,
            basic,
            search_filter,
        });
    }
    Ok(keys)
}

/// Add (or replace) an access key in the database.
pub fn save_key(conn: &Connection, key: &AccessPermissions) -> crate::Result<()> {
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
    advance_revision(conn)?;
    Ok(())
}

/// Remove an access key from the database; whether there was one.
pub fn delete_key(conn: &Connection, access_key: &[u8]) -> crate::Result<bool> {
    let deleted = conn.execute(
        "DELETE FROM api_permissions WHERE access_key = ?",
        [access_key],
    )? > 0;
    if deleted {
        advance_revision(conn)?;
    }
    Ok(deleted)
}

/// Durable revision changed in the same transaction as key edits.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Revision(pub u64);
impl crate::settings::Setting for Revision {
    const KEY: &'static str = "api_permissions_revision";
}
/// Read the committed permission revision without loading every key.
pub fn revision(conn: &Connection) -> crate::Result<u64> {
    Ok(crate::settings::get::<Revision>(conn)?.0)
}
fn advance_revision(conn: &Connection) -> crate::Result<()> {
    crate::settings::set(
        conn,
        &Revision(revision(conn)?.checked_add(1).ok_or_else(|| {
            crate::error::StoreError::Invalid("Permission revision exhausted.".into())
        })?),
    )
}
/// Commit detached key edits, rejecting concurrent changes and collisions atomically.
pub fn apply(
    store: &crate::Store,
    expected: Vec<AccessPermissions>,
    desired: Vec<AccessPermissions>,
) -> crate::Result<()> {
    store.write(move |ctx| {
        let conn=ctx.conn();
        let mut actual=stored_keys(conn)?;
        let mut expected=expected;
        actual.sort_by(|a,b| a.access_key.cmp(&b.access_key));
        expected.sort_by(|a,b| a.access_key.cmp(&b.access_key));
        if actual!=expected { return Err(crate::error::StoreError::Invalid("Access keys changed while this editor was open. Close and reopen it before applying.".into())); }
        let mut keys=std::collections::HashSet::new();
        for key in &desired {
            if key.access_key.len()!=32 || !keys.insert(&key.access_key) { return Err(crate::error::StoreError::Invalid("Access keys must be unique and contain 64 hexadecimal characters.".into())); }
        }
        for key in &actual { if !desired.iter().any(|k| k.access_key==key.access_key) { delete_key(conn,&key.access_key)?; } }
        for key in &desired { if !actual.contains(key) { save_key(conn,key)?; } }
        Ok(())
    })
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

impl crate::settings::Setting for Registration {
    const KEY: &'static str = "api_registration";
}
