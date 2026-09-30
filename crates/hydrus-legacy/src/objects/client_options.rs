//! The big client options object (`ClientOptions.ClientOptions`, type 22).
//!
//! Its `info` is one `SerialisableDictionary` of named groups: typed maps
//! (`booleans`, `integers`, `strings`, ...) plus assorted settings objects.
//! The typed maps and the settings the Client API exposes
//! (`/manage_database/get_client_options`) are decoded here; everything
//! else stays available, losslessly, in [`ClientOptions::dictionary`].
//!
//! Only values that are stored are reported. The reference fills in its
//! current defaults for missing keys when it loads options; since the v686
//! -> v687 database update re-saved every client's options, a v688 database
//! is only missing keys added by v687/v688 that the user has not saved since.

use std::collections::BTreeMap;

use hydrus_core::ServiceKey;
use hydrus_core::subscriptions::CheckerOptions;

use super::duplicates::DuplicateMergeOptions;
use super::location::LocationContext;
use super::services::Rgb;
use super::sort::{MediaCollect, MediaSort, TagSort};
use super::tag_filter::TagFilter;
use super::util::{
    DecodeResult, Settings, boolean, dictionary_pairs, float, hex_bytes, int, list, list_items,
    malformed, opt_int, opt_string, plain, service_key, string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType::CLIENT_OPTIONS;

/// The decoded client options.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientOptions {
    pub booleans: BTreeMap<String, bool>,
    pub integers: BTreeMap<String, i64>,
    pub noneable_integers: BTreeMap<String, Option<i64>>,
    /// Stored as Python numbers; some are integers.
    pub floats: BTreeMap<String, f64>,
    pub strings: BTreeMap<String, String>,
    pub noneable_strings: BTreeMap<String, Option<String>>,
    /// Named keys (tag service keys, a downloader key, ...).
    pub keys: BTreeMap<String, Vec<u8>>,
    pub key_lists: BTreeMap<String, Vec<Vec<u8>>>,
    /// Includes `favourite_tags`.
    pub string_lists: BTreeMap<String, Vec<String>>,
    pub integer_lists: BTreeMap<String, Vec<i64>>,
    /// Colour set name (`"default"`, `"darkmode"`) to `CC.COLOUR_*` code to colour.
    pub colours: BTreeMap<String, BTreeMap<i64, Rgb>>,
    pub media_zooms: Vec<f64>,
    pub slideshow_durations: Vec<f64>,
    /// Favourite tags shown in the "suggested tags" panel, per tag service.
    pub suggested_tags_favourites: BTreeMap<ServiceKey, Vec<String>>,
    pub favourite_tag_filters: BTreeMap<String, TagFilter>,
    pub default_sort: Option<MediaSort>,
    pub fallback_sort: Option<MediaSort>,
    pub default_namespace_sorts: Vec<MediaSort>,
    /// `CC.TAG_PRESENTATION_*` code to tag sort.
    pub default_tag_sorts: BTreeMap<i64, TagSort>,
    pub default_collect: Option<MediaCollect>,
    pub default_local_location_context: Option<LocationContext>,
    /// Duplicate metadata merge options by `HC.DUPLICATE_*` relationship
    /// (better, same quality, alternate); `None` if not stored.
    pub duplicate_action_options: Option<BTreeMap<i64, DuplicateMergeOptions>>,
    /// New subscriptions' and new watchers' checker timings (`misc`'s
    /// `default_subscription_checker_options` and
    /// `default_thread_watcher_options`).
    pub default_subscription_checker_options: Option<CheckerOptions>,
    pub default_watcher_checker_options: Option<CheckerOptions>,
    /// The whole stored options dictionary (type 21), including everything
    /// not decoded above.
    pub dictionary: SerialisableObject,
}

impl ClientOptions {
    /// Decode the client options object.
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(KIND)?;
        object.check_not_future()?;
        if object.version != 8 {
            // The v686 -> v687 database update re-saved everyone's options
            // at version 8, so older versions cannot occur in a v688 database.
            return Err(crate::serialisable::SerialisableError::UnsupportedVersion {
                kind: KIND,
                version: object.version,
                detail: "a v688 database always has version 8 options",
            });
        }
        let dictionary = SerialisableObject::from_tuple(&object.info())?;
        let settings = Settings::new(KIND, &dictionary)?;

        let map = |group: &str| named_group(&settings, group);
        let options = ClientOptions {
            booleans: map("booleans")?.decode(|v| boolean(KIND, v, "boolean"))?,
            integers: map("integers")?.decode(|v| int(KIND, v, "integer"))?,
            noneable_integers: map("noneable_integers")?
                .decode(|v| opt_int(KIND, v, "noneable integer"))?,
            floats: map("floats")?.decode(|v| float(KIND, v, "float"))?,
            strings: map("strings")?.decode(|v| string(KIND, v, "string"))?,
            noneable_strings: map("noneable_strings")?
                .decode(|v| opt_string(KIND, v, "noneable string"))?,
            keys: map("keys")?.decode(|v| hex_bytes(KIND, v, "key"))?,
            key_lists: map("key_list")?.decode(|v| {
                list(KIND, v, "key list")?
                    .iter()
                    .map(|k| hex_bytes(KIND, k, "key"))
                    .collect()
            })?,
            string_lists: map("string_list")?.decode(|v| {
                list(KIND, v, "string list")?
                    .iter()
                    .map(|s| string(KIND, s, "string"))
                    .collect()
            })?,
            integer_lists: map("integer_list")?.decode(|v| {
                list(KIND, v, "integer list")?
                    .iter()
                    .map(|i| int(KIND, i, "integer"))
                    .collect()
            })?,
            colours: colours(&settings)?,
            media_zooms: floats(&settings, "media_zooms")?,
            slideshow_durations: floats(&settings, "slideshow_durations")?,
            suggested_tags_favourites: suggested_tags_favourites(&settings)?,
            favourite_tag_filters: favourite_tag_filters(&settings)?,
            default_sort: optional_object(&settings, "default_sort", MediaSort::from_object)?,
            fallback_sort: optional_object(&settings, "fallback_sort", MediaSort::from_object)?,
            default_namespace_sorts: object_list(
                &settings,
                "default_namespace_sorts",
                MediaSort::from_object,
            )?,
            default_tag_sorts: default_tag_sorts(&settings)?,
            default_collect: optional_object(
                &settings,
                "default_collect",
                MediaCollect::from_object,
            )?,
            default_local_location_context: optional_object(
                &settings,
                "default_local_location_context",
                LocationContext::from_object,
            )?,
            duplicate_action_options: duplicate_action_options(&settings)?,
            default_subscription_checker_options: misc_checker(
                &settings,
                "default_subscription_checker_options",
            )?,
            default_watcher_checker_options: misc_checker(
                &settings,
                "default_thread_watcher_options",
            )?,
            dictionary: dictionary.clone(),
        };
        Ok(options)
    }
}

/// A group of named plain values, e.g. `booleans`.
struct NamedGroup {
    entries: Vec<(String, PyJson)>,
}

impl NamedGroup {
    fn decode<T>(
        self,
        f: impl Fn(&PyJson) -> DecodeResult<T>,
    ) -> DecodeResult<BTreeMap<String, T>> {
        self.entries
            .into_iter()
            .map(|(name, value)| {
                f(&value)
                    .map(|v| (name.clone(), v))
                    .map_err(|e| malformed(KIND, format!("option {name:?}: {e}")))
            })
            .collect()
    }
}

/// The entries of a string-keyed sub-dictionary (empty if absent).
fn named_group(settings: &Settings<'_>, group: &str) -> DecodeResult<NamedGroup> {
    let Some(meta) = settings.get(group) else {
        return Ok(NamedGroup {
            entries: Vec::new(),
        });
    };
    let object = expect_object(meta, group)?;
    let entries = dictionary_pairs(object)?
        .iter()
        .map(|(k, v)| {
            let name = k
                .as_str()
                .ok_or_else(|| malformed(KIND, format!("{group} has a non-string key")))?
                .to_owned();
            let value = plain(KIND, v, &name)?;
            Ok((name, value))
        })
        .collect::<DecodeResult<_>>()?;
    Ok(NamedGroup { entries })
}

fn expect_object<'a>(meta: &'a Meta, what: &str) -> DecodeResult<&'a SerialisableObject> {
    meta.as_object()
        .ok_or_else(|| malformed(KIND, format!("{what} should be an object")))
}

fn optional_object<T>(
    settings: &Settings<'_>,
    key: &str,
    decode: impl Fn(&SerialisableObject) -> DecodeResult<T>,
) -> DecodeResult<Option<T>> {
    settings
        .get(key)
        .map(|meta| decode(expect_object(meta, key)?))
        .transpose()
}

fn object_list<T>(
    settings: &Settings<'_>,
    key: &str,
    decode: impl Fn(&SerialisableObject) -> DecodeResult<T>,
) -> DecodeResult<Vec<T>> {
    let Some(meta) = settings.get(key) else {
        return Ok(Vec::new());
    };
    list_items(expect_object(meta, key)?)?
        .iter()
        .map(|item| decode(expect_object(item, key)?))
        .collect()
}

fn floats(settings: &Settings<'_>, key: &str) -> DecodeResult<Vec<f64>> {
    match settings.plain(key)? {
        None => Ok(Vec::new()),
        Some(value) => list(KIND, &value, key)?
            .iter()
            .map(|v| float(KIND, v, key))
            .collect(),
    }
}

fn rgb(value: &PyJson) -> DecodeResult<Rgb> {
    let channels = tuple::<3>(KIND, value, "colour")?;
    let mut out = [0u8; 3];
    for (slot, channel) in out.iter_mut().zip(channels) {
        let c = int(KIND, channel, "colour channel")?;
        *slot = u8::try_from(c).map_err(|_| malformed(KIND, format!("colour channel {c}")))?;
    }
    Ok(out)
}

fn colours(settings: &Settings<'_>) -> DecodeResult<BTreeMap<String, BTreeMap<i64, Rgb>>> {
    let Some(meta) = settings.get("colours") else {
        return Ok(BTreeMap::new());
    };
    dictionary_pairs(expect_object(meta, "colours")?)?
        .iter()
        .map(|(set, colours)| {
            let set = set
                .as_str()
                .ok_or_else(|| malformed(KIND, "colour set name is not a string"))?;
            let colours = dictionary_pairs(expect_object(colours, "colour set")?)?
                .iter()
                .map(|(code, colour)| {
                    let code = code
                        .as_json()
                        .and_then(PyJson::as_i64)
                        .ok_or_else(|| malformed(KIND, "colour type is not an integer"))?;
                    Ok((code, rgb(&plain(KIND, colour, "colour")?)?))
                })
                .collect::<DecodeResult<_>>()?;
            Ok((set.to_owned(), colours))
        })
        .collect()
}

fn suggested_tags_favourites(
    settings: &Settings<'_>,
) -> DecodeResult<BTreeMap<ServiceKey, Vec<String>>> {
    let Some(meta) = settings.get("suggested_tags") else {
        return Ok(BTreeMap::new());
    };
    let suggested = Settings::new(KIND, expect_object(meta, "suggested_tags")?)?;
    let Some(favourites) = suggested.get("favourites") else {
        return Ok(BTreeMap::new());
    };
    dictionary_pairs(expect_object(favourites, "suggested tag favourites")?)?
        .iter()
        .map(|(key, tags)| {
            let key = service_key(
                KIND,
                key.as_json()
                    .ok_or_else(|| malformed(KIND, "favourites key is not a string"))?,
                "favourites service",
            )?;
            let tags = plain(KIND, tags, "favourite tags")?;
            let tags = list(KIND, &tags, "favourite tags")?
                .iter()
                .map(|t| string(KIND, t, "tag"))
                .collect::<DecodeResult<_>>()?;
            Ok((key, tags))
        })
        .collect()
}

fn favourite_tag_filters(settings: &Settings<'_>) -> DecodeResult<BTreeMap<String, TagFilter>> {
    let Some(meta) = settings.get("favourite_tag_filters") else {
        return Ok(BTreeMap::new());
    };
    dictionary_pairs(expect_object(meta, "favourite_tag_filters")?)?
        .iter()
        .map(|(name, filter)| {
            let name = name
                .as_str()
                .ok_or_else(|| malformed(KIND, "tag filter name is not a string"))?;
            Ok((
                name.to_owned(),
                TagFilter::from_object(expect_object(filter, "tag filter")?)?,
            ))
        })
        .collect()
}

fn default_tag_sorts(settings: &Settings<'_>) -> DecodeResult<BTreeMap<i64, TagSort>> {
    let Some(meta) = settings.get("default_tag_sorts") else {
        return Ok(BTreeMap::new());
    };
    dictionary_pairs(expect_object(meta, "default_tag_sorts")?)?
        .iter()
        .map(|(location, sort)| {
            let location = location
                .as_json()
                .and_then(PyJson::as_i64)
                .ok_or_else(|| malformed(KIND, "tag presentation is not an integer"))?;
            Ok((
                location,
                TagSort::from_object(expect_object(sort, "tag sort")?)?,
            ))
        })
        .collect()
}

/// A checker options object in the `misc` group.
fn misc_checker(settings: &Settings<'_>, name: &str) -> DecodeResult<Option<CheckerOptions>> {
    let Some(meta) = settings.get("misc") else {
        return Ok(None);
    };
    for (key, value) in dictionary_pairs(expect_object(meta, "misc")?)?.iter() {
        if key.as_str() == Some(name) {
            return super::subscriptions::checker_options(expect_object(value, name)?).map(Some);
        }
    }
    Ok(None)
}

fn duplicate_action_options(
    settings: &Settings<'_>,
) -> DecodeResult<Option<BTreeMap<i64, DuplicateMergeOptions>>> {
    let Some(meta) = settings.get("duplicate_action_options") else {
        return Ok(None);
    };
    dictionary_pairs(expect_object(meta, "duplicate_action_options")?)?
        .iter()
        .map(|(kind, options)| {
            let kind = kind
                .as_json()
                .and_then(PyJson::as_i64)
                .ok_or_else(|| malformed(KIND, "duplicate type is not an integer"))?;
            Ok((
                kind,
                DuplicateMergeOptions::from_object(expect_object(options, "merge options")?)?,
            ))
        })
        .collect::<DecodeResult<_>>()
        .map(Some)
}
