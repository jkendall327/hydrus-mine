//! Favourite searches (`FavouriteSearchManager`, type 81) and the file
//! search contexts they hold (`FileSearchContext`, type 15).
//!
//! Search predicates (type 14) have a long upgrade chain of their own and
//! are kept as generic [`SerialisableObject`]s; decoding them is the search
//! crate's job.

use super::location::{LocationContext, TagContext};
use super::sort::{MediaCollect, MediaSort};
use super::util::{
    DecodeResult, boolean, int, list, malformed, nested, opt_string, service_key, string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

/// The user's saved searches.
#[derive(Debug, Clone, PartialEq)]
pub struct FavouriteSearchManager {
    /// In stored order.
    pub searches: Vec<FavouriteSearch>,
}

/// One saved search.
#[derive(Debug, Clone, PartialEq)]
pub struct FavouriteSearch {
    /// The menu folder, `None` for the top level.
    pub folder: Option<String>,
    pub name: String,
    pub file_search_context: FileSearchContext,
    /// Whether loading the search starts it immediately.
    pub synchronised: bool,
    pub media_sort: Option<MediaSort>,
    pub media_collect: Option<MediaCollect>,
}

impl FavouriteSearchManager {
    const KIND: SerialisableType = SerialisableType::FAVOURITE_SEARCH_MANAGER;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let searches = list(Self::KIND, &info, "favourite searches")?
            .iter()
            .map(|row| {
                let [folder, name, context, synchronised, sort, collect] =
                    tuple::<6>(Self::KIND, row, "favourite search")?;
                Ok(FavouriteSearch {
                    folder: opt_string(Self::KIND, folder, "folder")?,
                    name: string(Self::KIND, name, "name")?,
                    file_search_context: FileSearchContext::from_object(&nested(
                        Self::KIND,
                        context,
                        "file search context",
                    )?)?,
                    synchronised: boolean(Self::KIND, synchronised, "synchronised")?,
                    media_sort: match sort {
                        PyJson::Null => None,
                        sort => Some(MediaSort::from_tuple(sort)?),
                    },
                    media_collect: match collect {
                        PyJson::Null => None,
                        collect => Some(MediaCollect::from_tuple(collect)?),
                    },
                })
            })
            .collect::<DecodeResult<_>>()?;
        Ok(FavouriteSearchManager { searches })
    }
}

/// AND or OR (`SEARCH_TYPE_AND` = 0, `SEARCH_TYPE_OR` = 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    And,
    Or,
}

/// A file search: domains plus predicates.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSearchContext {
    pub location_context: LocationContext,
    pub tag_context: TagContext,
    pub search_type: SearchType,
    /// Predicate objects (type 14) as stored.
    pub predicates: Vec<SerialisableObject>,
    pub search_complete: bool,
}

impl FileSearchContext {
    const KIND: SerialisableType = SerialisableType::FILE_SEARCH_CONTEXT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        let k = Self::KIND;
        object.expect_kind(k)?;
        object.check_not_future()?;
        let info = object.info();
        let items = list(k, &info, "file search context")?;
        let file_key = |value: &PyJson| -> DecodeResult<LocationContext> {
            Ok(LocationContext::simple(service_key(
                k,
                value,
                "file service",
            )?))
        };
        let legacy_tags =
            |key: &PyJson, current: bool, pending: &PyJson| -> DecodeResult<TagContext> {
                let key = service_key(k, key, "tag service")?;
                Ok(TagContext {
                    display_service_key: key.clone(),
                    service_key: key,
                    include_current_tags: current,
                    include_pending_tags: boolean(k, pending, "include pending")?,
                })
            };
        // The reference's upgrade chain, collapsed: v2 added the search type,
        // v3 reset the search type and "include current" that v2 had swapped,
        // v4 made a tag context and v5 a location context.
        let (location_context, tag_context, search_type, predicates, complete) =
            match (object.version, items) {
                (1, [file, tags, _current, pending, predicates, complete]) => (
                    file_key(file)?,
                    legacy_tags(tags, true, pending)?,
                    SearchType::And,
                    predicates,
                    complete,
                ),
                (
                    2,
                    [
                        file,
                        tags,
                        _search_type,
                        _current,
                        pending,
                        predicates,
                        complete,
                    ],
                ) => (
                    file_key(file)?,
                    legacy_tags(tags, true, pending)?,
                    SearchType::And,
                    predicates,
                    complete,
                ),
                (
                    3,
                    [
                        file,
                        tags,
                        search_type,
                        current,
                        pending,
                        predicates,
                        complete,
                    ],
                ) => (
                    file_key(file)?,
                    legacy_tags(tags, boolean(k, current, "include current")?, pending)?,
                    decode_search_type(search_type)?,
                    predicates,
                    complete,
                ),
                (4, [file, tags, search_type, predicates, complete]) => (
                    file_key(file)?,
                    TagContext::from_tuple(tags)?,
                    decode_search_type(search_type)?,
                    predicates,
                    complete,
                ),
                (5, [location, tags, search_type, predicates, complete]) => (
                    LocationContext::from_tuple(location)?,
                    TagContext::from_tuple(tags)?,
                    decode_search_type(search_type)?,
                    predicates,
                    complete,
                ),
                (version, _) => {
                    return Err(SerialisableError::UnsupportedVersion {
                        kind: k,
                        version,
                        detail: "unexpected layout for this version",
                    });
                }
            };
        Ok(FileSearchContext {
            location_context,
            tag_context,
            search_type,
            predicates: list(k, predicates, "predicates")?
                .iter()
                .map(|p| nested(k, p, "predicate"))
                .collect::<DecodeResult<_>>()?,
            search_complete: boolean(k, complete, "search complete")?,
        })
    }
}

fn decode_search_type(value: &PyJson) -> DecodeResult<SearchType> {
    let kind = SerialisableType::FILE_SEARCH_CONTEXT;
    match int(kind, value, "search type")? {
        0 => Ok(SearchType::And),
        1 => Ok(SearchType::Or),
        other => Err(malformed(kind, format!("unknown search type {other}"))),
    }
}
