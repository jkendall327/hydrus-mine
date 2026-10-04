//! Atomic, service-keyed display/search configuration. Application queues are
//! ordered primary data; graphs and counts are published in the same commit.
use std::collections::{BTreeMap, HashSet};

use hydrus_core::search::context::LocationContext;
use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceId, ServiceKey, ServiceType};
use rusqlite::{OptionalExtension as _, params};
use serde::{Deserialize, Serialize};

use crate::autocomplete::{AutocompleteRules, AutocompleteSettings};
use crate::display::{DisplayGraphs, RelationKind};
use crate::settings::{self, Setting};
use crate::tag_display::{TagDisplayFilters, TagView};
use crate::{Result, Store, StoreError};

/// Widget behavior and initial write autocomplete domains for a tag service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutocompleteOptions {
    pub write_tag_service: ServiceKey,
    pub override_location: bool,
    pub write_location: LocationContext,
    pub fetch_automatically: bool,
    pub exact_match_threshold: Option<u16>,
}
impl AutocompleteOptions {
    /// GUI query gating; API tag search continues to use search rules directly.
    pub fn query(
        &self,
        input: &crate::autocomplete::AutocompleteInput,
        rules: &AutocompleteRules,
        manual: bool,
    ) -> Option<crate::autocomplete::TagQuery> {
        if !manual && !self.fetch_automatically {
            return None;
        }
        let mut query = input.tag_query(rules)?;
        let text = input.search_text();
        let subtag = hydrus_core::tag::split_tag(&text).1;
        if !text.contains('*')
            && (!rules.unnamespaced_search_gives_any_namespace_wildcards || text.contains(':'))
            && !subtag.is_empty()
            && self
                .exact_match_threshold
                .is_some_and(|n| subtag.chars().count() <= usize::from(n))
        {
            query.text = text;
            query.search_namespaces_into_full_tags = false;
        }
        Some(query)
    }
    /// Reference defaults depend on whether this is the default local service.
    pub fn for_service(service: &ServiceKey) -> Self {
        let location = if service.as_bytes() == builtin_keys::MY_TAGS {
            builtin_keys::COMBINED_LOCAL_FILE_DOMAINS
        } else {
            builtin_keys::COMBINED_FILE
        };
        Self {
            write_tag_service: service.clone(),
            override_location: true,
            write_location: LocationContext::single(ServiceKey::new(location.to_vec())),
            fetch_automatically: true,
            exact_match_threshold: Some(2),
        }
    }
}
/// Per-service widget options, separate from the shared search query rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutocompleteWidgetSettings {
    pub services: BTreeMap<String, AutocompleteOptions>,
}
impl Setting for AutocompleteWidgetSettings {
    const KEY: &'static str = "tag_autocomplete_widgets";
}
impl AutocompleteWidgetSettings {
    pub fn options(&self, key: &ServiceKey) -> AutocompleteOptions {
        self.services
            .get(&key.to_hex())
            .cloned()
            .unwrap_or_else(|| AutocompleteOptions::for_service(key))
    }
}
/// One changed service's settings. `None` means leave that area untouched.
#[derive(Debug, Clone)]
pub struct ServiceEdit {
    pub service: ServiceKey,
    pub single: Option<hydrus_core::tag_filter::TagFilter>,
    pub selection: Option<hydrus_core::tag_filter::TagFilter>,
    pub rules: Option<AutocompleteRules>,
    pub options: Option<AutocompleteOptions>,
    pub siblings: Option<Vec<ServiceKey>>,
    pub parents: Option<Vec<ServiceKey>>,
}

/// Apply staged edits atomically, validating against the services at commit
/// time. Unknown JSON fields and unedited service settings remain intact.
pub fn apply(store: &Store, edits: Vec<ServiceEdit>) -> Result<()> {
    if edits.is_empty() {
        return Ok(());
    }
    store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        let registry = crate::services::ServiceRegistry::load(conn)?;
        let old_graphs = DisplayGraphs::load(conn, &registry)?;
        let mut filters: TagDisplayFilters = settings::get(conn)?;
        let mut rules: AutocompleteSettings = settings::get(conn)?;
        let mut widgets: AutocompleteWidgetSettings = settings::get(conn)?;
        for edit in edits {
            let service = registry.by_key(&edit.service)?;
            let real = matches!(
                service.service_type(),
                ServiceType::LocalTag | ServiceType::TagRepository
            );
            if !real && service.service_type() != ServiceType::CombinedTag {
                return Err(StoreError::Invalid(
                    "display settings require a tag service".into(),
                ));
            }
            let key = edit.service.to_hex();
            if let Some(filter) = edit.single {
                filters
                    .for_view_mut(TagView::SingleMedia)
                    .insert(key.clone(), filter);
            }
            if let Some(filter) = edit.selection {
                filters
                    .for_view_mut(TagView::SelectionList)
                    .insert(key.clone(), filter);
            }
            if let Some(value) = edit.rules {
                rules.services.insert(key.clone(), value);
            }
            if let Some(value) = edit.options {
                if value
                    .exact_match_threshold
                    .is_some_and(|n| !(1..=256).contains(&n))
                {
                    return Err(StoreError::Invalid(
                        "character threshold must be between 1 and 256".into(),
                    ));
                }
                let tag_domain = registry.by_key(&value.write_tag_service)?;
                if !matches!(
                    tag_domain.service_type(),
                    ServiceType::CombinedTag | ServiceType::LocalTag | ServiceType::TagRepository
                ) {
                    return Err(StoreError::Invalid(
                        "autocomplete requires a tag domain".into(),
                    ));
                }
                for key in value
                    .write_location
                    .current()
                    .iter()
                    .chain(value.write_location.deleted())
                {
                    if !registry.by_key(key)?.service_type().is_file_service() {
                        return Err(StoreError::Invalid(
                            "autocomplete location requires file domains".into(),
                        ));
                    }
                }
                widgets.services.insert(key, value);
            }
            for (kind, sources) in [
                (RelationKind::Siblings, edit.siblings),
                (RelationKind::Parents, edit.parents),
            ] {
                let Some(sources) = sources else { continue };
                if !real {
                    return Err(StoreError::Invalid(
                        "combined tags cannot own application queues".into(),
                    ));
                }
                let mut seen = HashSet::new();
                let mut ids = Vec::new();
                for key in sources {
                    let source = registry.by_key(&key)?;
                    if !matches!(
                        source.service_type(),
                        ServiceType::LocalTag | ServiceType::TagRepository
                    ) || !seen.insert(source.id)
                    {
                        return Err(StoreError::Invalid(
                            "application sources must be distinct real tag services".into(),
                        ));
                    }
                    ids.push(source.id);
                }
                conn.execute(
                    "DELETE FROM tag_display_application WHERE display_service_id=?1 AND kind=?2",
                    params![service.id, kind as u8],
                )?;
                // Service ids are positive. Zero is an explicit empty queue,
                // distinguishable from an absent queue (default: self).
                if ids.is_empty() {
                    ids.push(ServiceId(0));
                }
                for (position, source) in ids.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO tag_display_application VALUES (?1,?2,?3,?4)",
                        params![
                            service.id,
                            kind as u8,
                            i64::try_from(position)
                                .map_err(|_| StoreError::Invalid("too many sources".into()))?,
                            source
                        ],
                    )?;
                }
            }
        }
        merge_setting(conn, &filters)?;
        merge_setting(conn, &rules)?;
        merge_setting(conn, &widgets)?;
        let graphs = DisplayGraphs::load(conn, &registry)?;
        for service in registry.tag_services() {
            if old_graphs.get(service.id) != graphs.get(service.id) {
                crate::counts::rebuild_display_service(conn, &registry, service.id, &graphs)?;
            }
        }
        Ok(())
    })
}
fn merge_setting<S: Setting>(conn: &rusqlite::Connection, value: &S) -> Result<()> {
    fn merge(old: &mut serde_json::Value, new: serde_json::Value) {
        if let (Some(old), serde_json::Value::Object(new)) = (old.as_object_mut(), &new) {
            for (key, value) in new {
                if key == "rules" {
                    old.insert(key.clone(), value.clone());
                } else {
                    merge(
                        old.entry(key).or_insert(serde_json::Value::Null),
                        value.clone(),
                    );
                }
            }
        } else {
            *old = new;
        }
    }
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", [S::KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let mut raw = raw
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or(serde_json::Value::Null);
    merge(&mut raw, serde_json::to_value(value)?);
    conn.execute(
        "INSERT OR REPLACE INTO settings VALUES (?1,?2)",
        params![S::KEY, serde_json::to_string(&raw)?],
    )?;
    Ok(())
}
