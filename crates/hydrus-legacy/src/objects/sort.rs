//! File and tag sorting/collecting settings: `MediaSort` (type 49),
//! `TagSort` (type 101) and `MediaCollect` (type 78).

use hydrus_core::ServiceKey;

use super::location::TagContext;
use super::util::{
    DecodeResult, boolean, int, list, malformed, nested, service_key, service_keys, string,
    strings, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

/// Ascending or descending (`CC.SORT_ASC` = 0, `CC.SORT_DESC` = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortOrder {
    Ascending,
    Descending,
}

impl SortOrder {
    pub const fn code(self) -> i64 {
        match self {
            SortOrder::Ascending => 0,
            SortOrder::Descending => 1,
        }
    }

    fn decode(kind: SerialisableType, value: &PyJson) -> DecodeResult<Self> {
        match int(kind, value, "sort order")? {
            0 => Ok(SortOrder::Ascending),
            1 => Ok(SortOrder::Descending),
            other => Err(malformed(kind, format!("unknown sort order {other}"))),
        }
    }
}

/// What files are sorted by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaSortType {
    /// A built-in sort; the value is a `CC.SORT_FILES_BY_*` code.
    System(i64),
    /// By the tags in these namespaces, using this tag display type
    /// (`ClientTags.TAG_DISPLAY_*`).
    Namespaces {
        namespaces: Vec<String>,
        tag_display_type: i64,
    },
    /// By rating on this service.
    Rating(ServiceKey),
}

/// How a page's files are sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSort {
    pub sort_type: MediaSortType,
    pub sort_order: SortOrder,
    pub tag_context: TagContext,
}

/// `ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL`, which the v1->v2 upgrade assigns.
const TAG_DISPLAY_DISPLAY_ACTUAL: i64 = 1;

impl MediaSort {
    const KIND: SerialisableType = SerialisableType::MEDIA_SORT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let items = list(Self::KIND, &info, "media sort")?;
        let (metatype, data, order, tag_context) = match (object.version, items) {
            // v2 wrapped namespace sorts with a display type; v3 added the tag context
            (1 | 2, [metatype, data, order]) => (metatype, data, order, None),
            (3, [metatype, data, order, context]) => (metatype, data, order, Some(context)),
            (version, _) => {
                return Err(SerialisableError::UnsupportedVersion {
                    kind: Self::KIND,
                    version,
                    detail: "unexpected layout for this version",
                });
            }
        };
        let metatype = string(Self::KIND, metatype, "sort metatype")?;
        let sort_type = match metatype.as_str() {
            "system" => MediaSortType::System(int(Self::KIND, data, "system sort type")?),
            "namespaces" if object.version == 1 => MediaSortType::Namespaces {
                namespaces: strings(Self::KIND, data, "namespaces")?,
                tag_display_type: TAG_DISPLAY_DISPLAY_ACTUAL,
            },
            "namespaces" => {
                let [namespaces, display] = tuple::<2>(Self::KIND, data, "namespace sort")?;
                MediaSortType::Namespaces {
                    namespaces: strings(Self::KIND, namespaces, "namespaces")?,
                    tag_display_type: int(Self::KIND, display, "tag display type")?,
                }
            }
            "rating" => MediaSortType::Rating(service_key(Self::KIND, data, "rating service")?),
            other => {
                return Err(malformed(
                    Self::KIND,
                    format!("unknown sort metatype {other:?}"),
                ));
            }
        };
        Ok(MediaSort {
            sort_type,
            sort_order: SortOrder::decode(Self::KIND, order)?,
            tag_context: match tag_context {
                Some(context) => TagContext::from_tuple(context)?,
                None => TagContext::all_known_tags(),
            },
        })
    }

    pub fn from_tuple(value: &PyJson) -> DecodeResult<Self> {
        Self::from_object(&nested(Self::KIND, value, "media sort")?)
    }
}

/// How a tag list is sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSort {
    /// A `ClientTagSorting.SORT_BY_*` code.
    pub sort_type: i64,
    pub sort_order: SortOrder,
    pub use_siblings: bool,
    /// A `ClientTagSorting.GROUP_BY_*` code.
    pub group_by: i64,
}

impl TagSort {
    const KIND: SerialisableType = SerialisableType::TAG_SORT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let [sort_type, order, siblings, group_by] = tuple::<4>(Self::KIND, &info, "tag sort")?;
        Ok(TagSort {
            sort_type: int(Self::KIND, sort_type, "sort type")?,
            sort_order: SortOrder::decode(Self::KIND, order)?,
            use_siblings: boolean(Self::KIND, siblings, "use siblings")?,
            group_by: int(Self::KIND, group_by, "group by")?,
        })
    }
}

/// How a page's files are collected into groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCollect {
    pub namespaces: Vec<String>,
    pub rating_service_keys: Vec<ServiceKey>,
    pub collect_unmatched: bool,
    pub tag_context: TagContext,
}

impl MediaCollect {
    const KIND: SerialisableType = SerialisableType::MEDIA_COLLECT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let items = list(Self::KIND, &info, "media collect")?;
        let (namespaces, ratings, unmatched, context) = match (object.version, items) {
            (1, [namespaces, ratings, unmatched]) => (namespaces, ratings, unmatched, None),
            (2, [namespaces, ratings, unmatched, context]) => {
                (namespaces, ratings, unmatched, Some(context))
            }
            (version, _) => {
                return Err(SerialisableError::UnsupportedVersion {
                    kind: Self::KIND,
                    version,
                    detail: "unexpected layout for this version",
                });
            }
        };
        Ok(MediaCollect {
            namespaces: strings(Self::KIND, namespaces, "namespaces")?,
            rating_service_keys: service_keys(Self::KIND, ratings, "rating services")?,
            collect_unmatched: boolean(Self::KIND, unmatched, "collect unmatched")?,
            tag_context: match context {
                Some(context) => TagContext::from_tuple(context)?,
                None => TagContext::all_known_tags(),
            },
        })
    }

    pub fn from_tuple(value: &PyJson) -> DecodeResult<Self> {
        Self::from_object(&nested(Self::KIND, value, "media collect")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_sort_upgrades() {
        let v1 = MediaSort::from_tuple(
            &PyJson::parse(r#"[49, 1, ["namespaces", ["series", "page"], 1]]"#).unwrap(),
        )
        .unwrap();
        assert_eq!(
            v1.sort_type,
            MediaSortType::Namespaces {
                namespaces: vec!["series".into(), "page".into()],
                tag_display_type: 1
            }
        );
        assert_eq!(v1.sort_order, SortOrder::Descending);
        assert_eq!(v1.tag_context, TagContext::all_known_tags());

        let v3 = MediaSort::from_tuple(
            &PyJson::parse(
                r#"[49, 3, ["rating", "6661", 0, [80, 1, ["6c6f63616c2074616773", true, false]]]]"#,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            v3.sort_type,
            MediaSortType::Rating(ServiceKey::new(b"fa".to_vec()))
        );
        assert_eq!(v3.tag_context.display_service_key.as_bytes(), b"local tags");
        assert!(!v3.tag_context.include_pending_tags);
    }
}
