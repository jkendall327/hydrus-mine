//! Native login drafts load preserved reference data until their first Apply.
use crate::{
    Result, StoreError, legacy,
    settings::{self, Setting},
};
use hydrus_legacy::serialisable::{SerialisableObject, SerialisableType};
pub use hydrus_parse::login::LoginManager;
use rusqlite::Connection;
impl Setting for LoginManager {
    const KEY: &'static str = "login_manager";
}

/// Read full login preferences, including imported script and domain credentials.
pub fn load(conn: &Connection) -> Result<LoginManager> {
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?)",
        [LoginManager::KEY],
        |row| row.get(0),
    )?;
    if saved {
        return settings::get(conn);
    }
    let Some((version, info)) = legacy::singleton(conn, 48)? else {
        return Ok(LoginManager::default());
    };
    let object = SerialisableObject::from_stored(SerialisableType(48), None, version, &info)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    hydrus_legacy::objects::logins::manager(&object)
        .map_err(|error| StoreError::Corrupt(error.to_string()))
}
/// Save preferences and refresh the domains used by downloader login warnings.
pub fn save(conn: &Connection, value: &LoginManager) -> Result<()> {
    settings::set(conn, value)?;
    let domains = crate::network::LoginDomains(
        value
            .domains
            .iter()
            .filter(|(_, login)| login.active)
            .map(|(domain, _)| domain.clone())
            .collect(),
    );
    settings::set(conn, &domains)
}
