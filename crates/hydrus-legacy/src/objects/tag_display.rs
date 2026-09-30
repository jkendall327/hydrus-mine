//! Per-tag-service display settings: `TagDisplayManager` (type 79) with
//! its tag display filters and `TagAutocompleteOptions` (type 85).

use hydrus_core::ServiceKey;

use super::location::LocationContext;
use super::tag_filter::TagFilter;
use super::util::{
    DecodeResult, boolean, int, list, list_items, malformed, nested, opt_int, service_key, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableError, SerialisableObject, SerialisableType};

/// Tag display filters and autocomplete options for every tag service.
#[derive(Debug, Clone, PartialEq)]
pub struct TagDisplayManager {
    /// `(tag display type, [(tag service, filter)])` in stored order. The
    /// display type is a `ClientTags.TAG_DISPLAY_*` code (e.g. 2 = single
    /// media, 3 = selection list).
    pub tag_filters: Vec<(i64, Vec<(ServiceKey, TagFilter)>)>,
    /// One per tag service that has non-default options.
    pub autocomplete_options: Vec<TagAutocompleteOptions>,
}

impl TagDisplayManager {
    const KIND: SerialisableType = SerialisableType::TAG_DISPLAY_MANAGER;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        // v2 added autocomplete options; v3 added sibling/parent application,
        // which v4 moved to the database again.
        let (filters, autocomplete) = match object.version {
            1 => (&info, None),
            2 | 4 => {
                let [filters, ac] = tuple::<2>(Self::KIND, &info, "tag display manager")?;
                (filters, Some(ac))
            }
            3 => {
                let [filters, ac, _, _] = tuple::<4>(Self::KIND, &info, "tag display manager")?;
                (filters, Some(ac))
            }
            version => {
                return Err(SerialisableError::UnsupportedVersion {
                    kind: Self::KIND,
                    version,
                    detail: "unknown tag display manager version",
                });
            }
        };
        let tag_filters = list(Self::KIND, filters, "tag filters")?
            .iter()
            .map(|entry| {
                let [display_type, per_service] = tuple::<2>(Self::KIND, entry, "display type")?;
                let per_service = list(Self::KIND, per_service, "service filters")?
                    .iter()
                    .map(|pair| {
                        let [key, filter] = tuple::<2>(Self::KIND, pair, "service filter")?;
                        Ok((
                            service_key(Self::KIND, key, "tag service")?,
                            TagFilter::from_tuple(filter)?,
                        ))
                    })
                    .collect::<DecodeResult<_>>()?;
                Ok((int(Self::KIND, display_type, "display type")?, per_service))
            })
            .collect::<DecodeResult<_>>()?;
        let autocomplete_options = match autocomplete {
            None => Vec::new(),
            Some(ac) => {
                let ac = nested(Self::KIND, ac, "autocomplete options")?;
                list_items(&ac)?
                    .iter()
                    .map(|item| match item {
                        Meta::Object(object) => TagAutocompleteOptions::from_object(object),
                        _ => Err(malformed(
                            Self::KIND,
                            "autocomplete options entry is not an object",
                        )),
                    })
                    .collect::<DecodeResult<_>>()?
            }
        };
        Ok(TagDisplayManager {
            tag_filters,
            autocomplete_options,
        })
    }
}

/// Autocomplete behaviour for one tag service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagAutocompleteOptions {
    pub service_key: ServiceKey,
    pub write_autocomplete_tag_domain: ServiceKey,
    pub override_write_autocomplete_location_context: bool,
    pub write_autocomplete_location_context: LocationContext,
    pub search_namespaces_into_full_tags: bool,
    pub unnamespaced_search_gives_any_namespace_wildcards: bool,
    pub namespace_bare_fetch_all_allowed: bool,
    pub namespace_fetch_all_allowed: bool,
    pub fetch_all_allowed: bool,
    pub fetch_results_automatically: bool,
    pub exact_match_character_threshold: Option<i64>,
}

impl TagAutocompleteOptions {
    const KIND: SerialisableType = SerialisableType::TAG_AUTOCOMPLETE_OPTIONS;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let items = list(Self::KIND, &info, "autocomplete options")?;
        let k = Self::KIND;
        let b = |value: &PyJson, what: &str| boolean(k, value, what);
        // Normalise every version to v5's layout, as the reference's upgrade chain does.
        let (
            key,
            domain,
            override_location,
            location,
            into_full,
            any_ns,
            bare,
            ns_all,
            all,
            auto,
            threshold,
        ) = match (object.version, items) {
            (1, [key, domain, o, file, into_full, ns_all, all]) => (
                key,
                domain,
                o,
                LocationSource::FileService(file),
                into_full,
                None,
                None,
                ns_all,
                all,
                None,
                None,
            ),
            (2, [key, domain, o, file, into_full, bare, ns_all, all]) => (
                key,
                domain,
                o,
                LocationSource::FileService(file),
                into_full,
                None,
                Some(bare),
                ns_all,
                all,
                None,
                None,
            ),
            (
                3,
                [
                    key,
                    domain,
                    o,
                    file,
                    into_full,
                    bare,
                    ns_all,
                    all,
                    auto,
                    threshold,
                ],
            ) => (
                key,
                domain,
                o,
                LocationSource::FileService(file),
                into_full,
                None,
                Some(bare),
                ns_all,
                all,
                Some(auto),
                Some(threshold),
            ),
            (
                4,
                [
                    key,
                    domain,
                    o,
                    location,
                    into_full,
                    bare,
                    ns_all,
                    all,
                    auto,
                    threshold,
                ],
            ) => (
                key,
                domain,
                o,
                LocationSource::Context(location),
                into_full,
                None,
                Some(bare),
                ns_all,
                all,
                Some(auto),
                Some(threshold),
            ),
            (
                5,
                [
                    key,
                    domain,
                    o,
                    location,
                    into_full,
                    any_ns,
                    bare,
                    ns_all,
                    all,
                    auto,
                    threshold,
                ],
            ) => (
                key,
                domain,
                o,
                LocationSource::Context(location),
                into_full,
                Some(any_ns),
                Some(bare),
                ns_all,
                all,
                Some(auto),
                Some(threshold),
            ),
            (version, _) => {
                return Err(SerialisableError::UnsupportedVersion {
                    kind: k,
                    version,
                    detail: "unexpected layout for this version",
                });
            }
        };
        Ok(TagAutocompleteOptions {
            service_key: service_key(k, key, "service key")?,
            write_autocomplete_tag_domain: service_key(k, domain, "write tag domain")?,
            override_write_autocomplete_location_context: b(
                override_location,
                "override location",
            )?,
            write_autocomplete_location_context: match location {
                LocationSource::FileService(file) => {
                    LocationContext::simple(service_key(k, file, "write file domain")?)
                }
                LocationSource::Context(context) => LocationContext::from_tuple(context)?,
            },
            search_namespaces_into_full_tags: b(into_full, "search namespaces into full tags")?,
            unnamespaced_search_gives_any_namespace_wildcards: any_ns
                .map_or(Ok(false), |v| b(v, "unnamespaced search wildcards"))?,
            namespace_bare_fetch_all_allowed: bare
                .map_or(Ok(false), |v| b(v, "namespace bare fetch all"))?,
            namespace_fetch_all_allowed: b(ns_all, "namespace fetch all")?,
            fetch_all_allowed: b(all, "fetch all")?,
            fetch_results_automatically: auto.map_or(Ok(true), |v| b(v, "fetch automatically"))?,
            exact_match_character_threshold: match threshold {
                Some(v) => opt_int(k, v, "exact match threshold")?,
                None => Some(2),
            },
        })
    }
}

/// Before v4 the write location was a single file service key.
enum LocationSource<'a> {
    FileService(&'a PyJson),
    Context(&'a PyJson),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_v1_upgrade() {
        let v1 = r#"[85, 1, ["6c6f63616c2074616773", "6c6f63616c2074616773", true, "6c6f63616c2066696c6573", false, true, false]]"#;
        let object = SerialisableObject::from_tuple_str(v1).unwrap();
        let options = TagAutocompleteOptions::from_object(&object).unwrap();
        assert_eq!(
            options.write_autocomplete_location_context,
            LocationContext::simple(ServiceKey::new(b"local files".to_vec()))
        );
        assert!(options.namespace_fetch_all_allowed);
        assert!(!options.namespace_bare_fetch_all_allowed);
        assert!(options.fetch_results_automatically);
        assert_eq!(options.exact_match_character_threshold, Some(2));
    }

    #[test]
    fn manager_versions() {
        let v1 = r#"[79, 1, [[2, [["6c6f63616c2074616773", [44, 1, [["meta:", 1]]]]]]]]"#;
        let object = SerialisableObject::from_tuple_str(v1).unwrap();
        let manager = TagDisplayManager::from_object(&object).unwrap();
        assert_eq!(manager.tag_filters.len(), 1);
        assert!(manager.autocomplete_options.is_empty());
        assert!(!manager.tag_filters[0].1[0].1.allows("meta:x", false));

        let v4 = r"[79, 4, [[], [26, 3, []]]]";
        let object = SerialisableObject::from_tuple_str(v4).unwrap();
        assert!(
            TagDisplayManager::from_object(&object)
                .unwrap()
                .tag_filters
                .is_empty()
        );
    }
}
