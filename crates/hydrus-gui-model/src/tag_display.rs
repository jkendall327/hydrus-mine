//! Staged display filters, autocomplete controls and ordered graph sources.
use hydrus_core::tag_filter::TagFilter;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_store::autocomplete::{AutocompleteRules, AutocompleteSettings};
use hydrus_store::display::RelationKind;
use hydrus_store::tag_display::{TagDisplayFilters, TagView};
use hydrus_store::tag_display_config::{
    AutocompleteOptions, AutocompleteWidgetSettings, ServiceEdit,
};
use hydrus_store::{Result, Store};
use std::sync::Arc;

/// An editable service tab. Queues retain priority order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceOptions {
    pub key: ServiceKey,
    pub name: String,
    pub real: bool,
    pub single: TagFilter,
    pub selection: TagFilter,
    pub rules: AutocompleteRules,
    pub autocomplete: AutocompleteOptions,
    pub siblings: Vec<ServiceKey>,
    pub parents: Vec<ServiceKey>,
}
/// A cancel-safe editor: only Apply writes, and only changed areas are written.
#[derive(Debug)]
pub struct TagDisplayEditor {
    store: Arc<Store>,
    original: Vec<ServiceOptions>,
    services: Vec<ServiceOptions>,
    selected: usize,
    /// Whether "Allow namespace:" and "Allow namespace:*" are enabled, per
    /// service tab, where a namespace search change has set them (the
    /// reference's tabs start with both enabled).
    namespace_enabled: std::collections::HashMap<ServiceKey, (bool, bool)>,
}
impl TagDisplayEditor {
    pub fn new(store: Arc<Store>) -> Result<Self> {
        let snapshot = store.snapshot();
        let (filters, rules, widgets, app) = store.read(|conn| {
            Ok((
                hydrus_store::settings::get::<TagDisplayFilters>(conn)?,
                hydrus_store::settings::get::<AutocompleteSettings>(conn)?,
                hydrus_store::settings::get::<AutocompleteWidgetSettings>(conn)?,
                hydrus_store::display::load_application(conn, &snapshot.services)?,
            ))
        })?;
        let services: Vec<_> = snapshot
            .services
            .all()
            .filter(|s| {
                matches!(
                    s.service_type(),
                    ServiceType::CombinedTag | ServiceType::LocalTag | ServiceType::TagRepository
                )
            })
            .map(|s| {
                let sources = |kind| {
                    app.sources(kind, s.id)
                        .into_iter()
                        .filter_map(|id| snapshot.services.get(id).ok().map(|s| s.key.clone()))
                        .collect()
                };
                ServiceOptions {
                    key: s.key.clone(),
                    name: s.name.clone(),
                    real: s.service_type() != ServiceType::CombinedTag,
                    single: filters
                        .single_media
                        .get(&s.key.to_hex())
                        .cloned()
                        .unwrap_or_default(),
                    selection: filters
                        .selection_list
                        .get(&s.key.to_hex())
                        .cloned()
                        .unwrap_or_default(),
                    rules: rules.rules(&s.key),
                    autocomplete: widgets.options(&s.key),
                    siblings: sources(RelationKind::Siblings),
                    parents: sources(RelationKind::Parents),
                }
            })
            .collect();
        let selected = services.iter().position(|s| !s.real).unwrap_or(0);
        Ok(Self {
            store,
            original: services.clone(),
            services,
            selected,
            namespace_enabled: std::collections::HashMap::new(),
        })
    }
    /// Application dialogs show only real tag services.
    pub fn application_only(&mut self) {
        self.services.retain(|s| s.real);
        self.original.retain(|s| s.real);
        self.selected = 0;
    }
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
    pub fn services(&self) -> &[ServiceOptions] {
        &self.services
    }
    pub fn services_mut(&mut self) -> &mut [ServiceOptions] {
        &mut self.services
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn choose(&mut self, index: usize) {
        if index < self.services.len() {
            self.selected = index;
        }
    }
    pub fn current(&self) -> &ServiceOptions {
        &self.services[self.selected]
    }
    pub fn current_mut(&mut self) -> &mut ServiceOptions {
        &mut self.services[self.selected]
    }
    pub fn set_filter(&mut self, key: &ServiceKey, view: TagView, value: TagFilter) {
        if let Some(s) = self.services.iter_mut().find(|s| &s.key == key) {
            match view {
                TagView::SingleMedia => s.single = value,
                TagView::SelectionList => s.selection = value,
            }
        }
    }
    /// Checkbox interlocks from the reference: namespace search and any-
    /// namespace input exclude one another; namespace search enables both fetches.
    pub fn set_rule(&mut self, index: usize, on: bool) {
        let (bare_enabled, star_enabled) = self.namespace_enabled();
        let searched = self.current().rules.search_namespaces_into_full_tags;
        let r = &mut self.current_mut().rules;
        match index {
            0 => {
                r.search_namespaces_into_full_tags = on;
                if on {
                    r.unnamespaced_search_gives_any_namespace_wildcards = false;
                }
            }
            1 => {
                r.unnamespaced_search_gives_any_namespace_wildcards = on;
                if on {
                    r.search_namespaces_into_full_tags = false;
                }
            }
            2 if bare_enabled => r.namespace_bare_fetch_all_allowed = on,
            3 if star_enabled => r.namespace_fetch_all_allowed = on,
            4 => r.fetch_all_allowed = on,
            _ => {}
        }
        // (`_UpdateControlsFromSearchNamespacesIntoFullTags`, on a change)
        let search = r.search_namespaces_into_full_tags;
        if search != searched {
            let enabled = (!search, !search && !r.namespace_bare_fetch_all_allowed);
            let key = self.current().key.clone();
            self.namespace_enabled.insert(key, enabled);
        }
        // (`_UpdateControls`: a disabled box is ticked)
        let (bare_enabled, star_enabled) = self.namespace_enabled();
        let r = &mut self.current_mut().rules;
        if !bare_enabled {
            r.namespace_bare_fetch_all_allowed = true;
        }
        if !star_enabled {
            r.namespace_fetch_all_allowed = true;
        }
    }
    /// Whether the shown tab's "Allow namespace:" and "Allow namespace:*"
    /// boxes are enabled.
    pub fn namespace_enabled(&self) -> (bool, bool) {
        self.namespace_enabled
            .get(&self.current().key)
            .copied()
            .unwrap_or((true, true))
    }
    pub fn add_source(&mut self, parents: bool, key: ServiceKey) -> bool {
        if !self.current().real || !self.services.iter().any(|s| s.real && s.key == key) {
            return false;
        }
        let s = self.current_mut();
        let list = if parents {
            &mut s.parents
        } else {
            &mut s.siblings
        };
        if list.contains(&key) {
            return false;
        }
        list.push(key);
        true
    }
    pub fn change_source(&mut self, parents: bool, index: usize, movement: Option<isize>) {
        let s = self.current_mut();
        let list = if parents {
            &mut s.parents
        } else {
            &mut s.siblings
        };
        if index >= list.len() {
            return;
        }
        match movement {
            None => {
                list.remove(index);
            }
            Some(by) => {
                let to = index.saturating_add_signed(by).min(list.len() - 1);
                list.swap(index, to);
            }
        }
    }
    pub fn apply(&self) -> Result<()> {
        let edits = self
            .services
            .iter()
            .zip(&self.original)
            .filter(|(s, o)| s != o)
            .map(|(s, o)| ServiceEdit {
                service: s.key.clone(),
                single: (s.single != o.single).then(|| s.single.clone()),
                selection: (s.selection != o.selection).then(|| s.selection.clone()),
                rules: (s.rules != o.rules).then_some(s.rules),
                options: (s.autocomplete != o.autocomplete).then(|| s.autocomplete.clone()),
                siblings: (s.siblings != o.siblings).then(|| s.siblings.clone()),
                parents: (s.parents != o.parents).then(|| s.parents.clone()),
            })
            .collect();
        hydrus_store::tag_display_config::apply(&self.store, edits)
    }
}
