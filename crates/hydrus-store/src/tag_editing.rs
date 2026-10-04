//! Tag-dialog service preferences, shared by options and manage-tags consumers.

use hydrus_core::{ServiceKey, service::builtin_keys};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// The reference's default service tab and whether changing tabs remembers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagEditingSettings {
    pub remember_service: bool,
    pub default_service: ServiceKey,
}

impl Default for TagEditingSettings {
    fn default() -> Self {
        Self {
            remember_service: true,
            default_service: ServiceKey::new(builtin_keys::MY_TAGS.to_vec()),
        }
    }
}

impl crate::settings::Setting for TagEditingSettings {
    const KEY: &'static str = "tag_editing";
}

/// Remember a service immediately on a tab change, independently of staged tags.
/// Read the preference in this transaction so an open dialog honours later options changes.
pub fn remember_service(conn: &Connection, service: &ServiceKey) -> crate::Result<()> {
    let mut options: TagEditingSettings = crate::settings::get(conn)?;
    if options.remember_service && options.default_service != *service {
        options.default_service = service.clone();
        crate::settings::set(conn, &options)?;
    }
    Ok(())
}
