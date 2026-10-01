//! Tag display filters (the reference's `TagDisplayManager` filters): per
//! tag service, the tags hidden from one file's tags (as the media viewer
//! shows them) and from a selection's (the tag list beside the thumbnails).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use hydrus_core::service::builtin_keys;
use hydrus_core::tag_filter::TagFilter;
use hydrus_core::{ServiceId, ServiceKey};

use crate::services::ServiceRegistry;

/// Where tags are shown (`ClientTags.TAG_DISPLAY_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagView {
    /// One file's tags (`TAG_DISPLAY_SINGLE_MEDIA`).
    SingleMedia,
    /// The tags of a selection of files (`TAG_DISPLAY_SELECTION_LIST`).
    SelectionList,
}

impl TagView {
    /// The reference's `TAG_DISPLAY_*` code.
    pub fn code(self) -> i64 {
        match self {
            Self::SingleMedia => 2,
            Self::SelectionList => 3,
        }
    }

    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            2 => Some(Self::SingleMedia),
            3 => Some(Self::SelectionList),
            _ => None,
        }
    }
}

/// The filters, by tag service key (hex); "all known tags" can have its
/// own, which applies to every service's tags as well.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagDisplayFilters {
    pub single_media: BTreeMap<String, TagFilter>,
    pub selection_list: BTreeMap<String, TagFilter>,
}

impl crate::settings::Setting for TagDisplayFilters {
    const KEY: &'static str = "tag_display_filters";
}

impl TagDisplayFilters {
    pub fn for_view(&self, view: TagView) -> &BTreeMap<String, TagFilter> {
        match view {
            TagView::SingleMedia => &self.single_media,
            TagView::SelectionList => &self.selection_list,
        }
    }

    pub fn for_view_mut(&mut self, view: TagView) -> &mut BTreeMap<String, TagFilter> {
        match view {
            TagView::SingleMedia => &mut self.single_media,
            TagView::SelectionList => &mut self.selection_list,
        }
    }

    /// The filters a `view` of each tag service's tags passes them through
    /// (`FilterTags`): the service's own, then "all known tags"'. Services
    /// with neither are left out.
    pub fn by_service(&self, view: TagView, registry: &ServiceRegistry) -> TagHider {
        let filters = self.for_view(view);
        let combined = filters
            .get(&ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()).to_hex())
            .filter(|f| !f.allows_everything());
        let mut out = HashMap::new();
        for service in registry.tag_services() {
            let own = filters
                .get(&service.key.to_hex())
                .filter(|f| !f.allows_everything());
            let list: Vec<TagFilter> = own.into_iter().chain(combined).cloned().collect();
            if !list.is_empty() {
                out.insert(service.id, list);
            }
        }
        TagHider(out)
    }
}

/// Which tags a view hides, by tag service.
#[derive(Debug, Clone, Default)]
pub struct TagHider(HashMap<ServiceId, Vec<TagFilter>>);

impl TagHider {
    /// Whether any of `service`'s tags may be hidden.
    pub fn filters(&self, service: ServiceId) -> bool {
        self.0.contains_key(&service)
    }

    /// Whether `tag`, from `service`, is shown.
    pub fn shows(&self, service: ServiceId, tag: &str) -> bool {
        self.0
            .get(&service)
            .is_none_or(|filters| filters.iter().all(|f| f.tag_ok(tag, false)))
    }
}

#[cfg(test)]
mod tests {
    use hydrus_core::tag_filter::FilterRule;

    use super::*;

    /// A service's tags pass its own filter and "all known tags"'; a
    /// service with neither hides nothing.
    #[test]
    fn a_service_s_tags_pass_its_filter_and_all_known_tags() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        for (key, name, kind) in crate::services::default_services() {
            crate::services::insert(&conn, &key, &name, &kind).unwrap();
        }
        let registry = ServiceRegistry::load(&conn).unwrap();
        let service = |k: &[u8]| registry.builtin(k).unwrap();
        let (my_tags, downloader) = (
            service(builtin_keys::MY_TAGS),
            service(builtin_keys::DOWNLOADER_TAGS),
        );
        let mut filters = TagDisplayFilters::default();
        let list = filters.for_view_mut(TagView::SelectionList);
        list.insert(
            my_tags.key.to_hex(),
            TagFilter::new().with_rule("meta:", FilterRule::Blacklist),
        );
        list.insert(
            ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()).to_hex(),
            TagFilter::new().with_rule("blue eyes", FilterRule::Blacklist),
        );
        let hider = filters.by_service(TagView::SelectionList, &registry);
        assert!(!hider.shows(my_tags.id, "meta:lowres"));
        assert!(!hider.shows(my_tags.id, "blue eyes"));
        assert!(hider.shows(my_tags.id, "character:blue eyes"));
        assert!(hider.shows(downloader.id, "meta:lowres"));
        assert!(!hider.shows(downloader.id, "blue eyes"));
        // the other view hides nothing
        let other = filters.by_service(TagView::SingleMedia, &registry);
        assert!(!other.filters(my_tags.id));
        assert!(other.shows(my_tags.id, "meta:lowres"));
    }
}
