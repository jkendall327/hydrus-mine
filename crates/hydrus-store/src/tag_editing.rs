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
    pub use_listbook: bool,
    pub tag_list_show_parents: bool,
    pub tag_list_expand_parents: bool,
    pub tag_list_show_siblings: bool,
    pub select_first_with_count: bool,
    pub skip_multiline_paste_confirmation: bool,
    pub autocomplete_list_height: u32,
    pub autocomplete_show_parents: bool,
    pub autocomplete_expand_parents: bool,
    pub autocomplete_show_siblings: bool,
}

impl Default for TagEditingSettings {
    fn default() -> Self {
        Self {
            remember_service: true,
            default_service: ServiceKey::new(builtin_keys::MY_TAGS.to_vec()),
            use_listbook: false,
            tag_list_show_parents: true,
            tag_list_expand_parents: true,
            tag_list_show_siblings: true,
            select_first_with_count: false,
            skip_multiline_paste_confirmation: false,
            autocomplete_list_height: 11,
            autocomplete_show_parents: true,
            autocomplete_expand_parents: true,
            autocomplete_show_siblings: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_preferences_get_write_defaults_and_tab_memory_preserves_them() {
        let old: TagEditingSettings = serde_json::from_value(serde_json::json!({
            "remember_service": false,
            "default_service": ServiceKey::new(builtin_keys::MY_TAGS.to_vec())
        }))
        .unwrap();
        assert!(!old.use_listbook);
        assert!(old.tag_list_show_parents);
        assert!(old.tag_list_expand_parents);
        assert!(old.tag_list_show_siblings);
        assert!(!old.select_first_with_count);
        assert!(!old.skip_multiline_paste_confirmation);
        assert!(old.autocomplete_show_parents);
        assert!(old.autocomplete_expand_parents);
        assert!(old.autocomplete_show_siblings);
        assert_eq!(old.autocomplete_list_height, 11);
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        let changed = TagEditingSettings {
            use_listbook: true,
            tag_list_show_parents: false,
            tag_list_expand_parents: false,
            tag_list_show_siblings: false,
            select_first_with_count: true,
            skip_multiline_paste_confirmation: true,
            autocomplete_list_height: 3,
            autocomplete_show_parents: false,
            autocomplete_expand_parents: false,
            autocomplete_show_siblings: false,
            ..TagEditingSettings::default()
        };
        let expected = changed.clone();
        store
            .write(move |ctx| crate::settings::set(ctx.conn(), &changed))
            .unwrap();
        let key = ServiceKey::new(b"remember another service".to_vec());
        let remembered = key.clone();
        store
            .write(move |ctx| remember_service(ctx.conn(), &remembered))
            .unwrap();
        let actual: TagEditingSettings = store.read(crate::settings::get).unwrap();
        assert_eq!(
            actual,
            TagEditingSettings {
                default_service: key,
                ..expected
            }
        );
    }
}
