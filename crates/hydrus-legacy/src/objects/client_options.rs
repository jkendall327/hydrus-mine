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
use hydrus_core::media_viewer::{
    InfoLineSettings, MediaView, MediaViewerSettings, ScaleAction, ShowAction, ZoomCentre,
    ZoomRules, ZoomType,
};
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
    /// How each file type (or general class), by `HC` mime code, is shown
    /// in the media viewer and the preview (`media_view`); `None` if not
    /// stored.
    pub media_view: Option<BTreeMap<i64, MediaView>>,
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
            media_view: media_view(&settings)?,
            dictionary: dictionary.clone(),
        };
        Ok(options)
    }
}

impl ClientOptions {
    /// The options of a new reference client
    /// (`oracle/dump_client_options_defaults.py`).
    pub fn defaults() -> DecodeResult<Self> {
        let object =
            SerialisableObject::from_tuple_str(include_str!("client_options_defaults.json"))?;
        Self::from_object(&object)
    }

    /// Fill in what isn't stored from `defaults`, as the reference does when
    /// it loads options: each group that is a dictionary gains the keys it
    /// lacks, and a missing group is taken whole. (An empty list here counts
    /// as missing.)
    pub fn fill_defaults(&mut self, defaults: &ClientOptions) {
        fn keys<V: Clone>(stored: &mut BTreeMap<String, V>, defaults: &BTreeMap<String, V>) {
            for (k, v) in defaults {
                stored.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        fn whole<T: Clone>(stored: &mut Option<T>, default: Option<&T>) {
            if stored.is_none() {
                *stored = default.cloned();
            }
        }
        fn list<T: Clone>(stored: &mut Vec<T>, default: &[T]) {
            if stored.is_empty() {
                stored.extend_from_slice(default);
            }
        }
        keys(&mut self.booleans, &defaults.booleans);
        keys(&mut self.integers, &defaults.integers);
        keys(&mut self.noneable_integers, &defaults.noneable_integers);
        keys(&mut self.floats, &defaults.floats);
        keys(&mut self.strings, &defaults.strings);
        keys(&mut self.noneable_strings, &defaults.noneable_strings);
        keys(&mut self.keys, &defaults.keys);
        keys(&mut self.key_lists, &defaults.key_lists);
        keys(&mut self.string_lists, &defaults.string_lists);
        keys(&mut self.integer_lists, &defaults.integer_lists);
        keys(&mut self.colours, &defaults.colours);
        keys(
            &mut self.favourite_tag_filters,
            &defaults.favourite_tag_filters,
        );
        for (k, v) in &defaults.default_tag_sorts {
            self.default_tag_sorts
                .entry(*k)
                .or_insert_with(|| v.clone());
        }
        if self.suggested_tags_favourites.is_empty() {
            self.suggested_tags_favourites
                .clone_from(&defaults.suggested_tags_favourites);
        }
        list(&mut self.media_zooms, &defaults.media_zooms);
        list(&mut self.slideshow_durations, &defaults.slideshow_durations);
        list(
            &mut self.default_namespace_sorts,
            &defaults.default_namespace_sorts,
        );
        whole(&mut self.default_sort, defaults.default_sort.as_ref());
        whole(&mut self.fallback_sort, defaults.fallback_sort.as_ref());
        whole(&mut self.default_collect, defaults.default_collect.as_ref());
        whole(
            &mut self.default_local_location_context,
            defaults.default_local_location_context.as_ref(),
        );
        whole(
            &mut self.duplicate_action_options,
            defaults.duplicate_action_options.as_ref(),
        );
        whole(
            &mut self.default_subscription_checker_options,
            defaults.default_subscription_checker_options.as_ref(),
        );
        whole(
            &mut self.default_watcher_checker_options,
            defaults.default_watcher_checker_options.as_ref(),
        );
        whole(&mut self.media_view, defaults.media_view.as_ref());
    }

    /// How a file's info lines read, and which are interesting.
    pub fn info_line_settings(&self) -> InfoLineSettings {
        let mut out = InfoLineSettings::default();
        for (key, field) in [
            (
                "file_info_line_consider_archived_interesting",
                &mut out.archived_interesting,
            ),
            (
                "file_info_line_consider_archived_time_interesting",
                &mut out.archived_time_interesting,
            ),
            (
                "file_info_line_consider_file_services_interesting",
                &mut out.file_services_interesting,
            ),
            (
                "file_info_line_consider_file_services_import_times_interesting",
                &mut out.file_services_import_times_interesting,
            ),
            (
                "file_info_line_consider_trash_time_interesting",
                &mut out.trash_time_interesting,
            ),
            (
                "file_info_line_consider_trash_reason_interesting",
                &mut out.trash_reason_interesting,
            ),
            (
                "hide_uninteresting_modified_time",
                &mut out.hide_uninteresting_modified_time,
            ),
            ("use_nice_resolution_strings", &mut out.nice_resolutions),
        ] {
            if let Some(&value) = self.booleans.get(key) {
                *field = value;
            }
        }
        if let Some(label) = self.strings.get("has_audio_label") {
            out.has_audio_label.clone_from(label);
        }
        out
    }

    /// The media viewer's options: the zoom steps, where zooming centres,
    /// the default zoom and how each file type is shown.
    pub fn media_viewer_settings(&self) -> MediaViewerSettings {
        let mut out = MediaViewerSettings::default();
        if !self.media_zooms.is_empty() {
            out.media_zooms.clone_from(&self.media_zooms);
        }
        let integer = |key: &str| self.integers.get(key).copied();
        if let Some(centre) = integer("media_viewer_zoom_center").and_then(ZoomCentre::from_code) {
            out.zoom_centre = centre;
        }
        if let Some(zoom) =
            integer("media_viewer_default_zoom_type_override").and_then(ZoomType::from_code)
        {
            out.default_zoom_type = zoom;
        }
        let float = |key: &str| self.floats.get(key).copied().filter(|v| *v > 0.0);
        if let Some(size) = float("media_viewer_rating_icon_size_px") {
            out.rating_icon_size = size;
        }
        if let Some(height) = float("media_viewer_rating_incdec_height_px") {
            out.rating_incdec_height = height;
        }
        if let Some(view) = &self.media_view {
            out.media_view = view
                .iter()
                .filter_map(|(&mime, view)| Some((u8::try_from(mime).ok()?, *view)))
                .collect();
        }
        out
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

/// The `media_view` dictionary: mime code to (media show action, start
/// paused, start with embed, preview show action, start paused, start with
/// embed, (media scale up, scale down, preview scale up, scale down, exact
/// zooms only, scale up quality, scale down quality)).
fn media_view(settings: &Settings<'_>) -> DecodeResult<Option<BTreeMap<i64, MediaView>>> {
    let Some(meta) = settings.get("media_view") else {
        return Ok(None);
    };
    let code = |value: &PyJson, what: &str| int(KIND, value, what);
    let action = |value: &PyJson| {
        ShowAction::from_code(code(value, "show action")?)
            .ok_or_else(|| malformed(KIND, "unknown media viewer show action"))
    };
    let scale = |value: &PyJson| {
        ScaleAction::from_code(code(value, "scale action")?)
            .ok_or_else(|| malformed(KIND, "unknown media viewer scale action"))
    };
    let quality = |value: &PyJson| {
        u8::try_from(code(value, "zoom quality")?)
            .map_err(|_| malformed(KIND, "zoom quality out of range"))
    };
    dictionary_pairs(expect_object(meta, "media_view")?)?
        .iter()
        .map(|(mime, view)| {
            let mime = mime
                .as_json()
                .and_then(PyJson::as_i64)
                .ok_or_else(|| malformed(KIND, "media_view mime is not an integer"))?;
            let view = plain(KIND, view, "media view options")?;
            let [show, paused, embed, p_show, p_paused, p_embed, zoom] =
                tuple::<7>(KIND, &view, "media view options")?;
            let [up, down, p_up, p_down, exact, up_quality, down_quality] =
                tuple::<7>(KIND, zoom, "zoom options")?;
            Ok((
                mime,
                MediaView {
                    media_show_action: action(show)?,
                    media_start_paused: boolean(KIND, paused, "start paused")?,
                    media_start_with_embed: boolean(KIND, embed, "start with embed")?,
                    preview_show_action: action(p_show)?,
                    preview_start_paused: boolean(KIND, p_paused, "start paused")?,
                    preview_start_with_embed: boolean(KIND, p_embed, "start with embed")?,
                    zoom: ZoomRules {
                        media_scale_up: scale(up)?,
                        media_scale_down: scale(down)?,
                        preview_scale_up: scale(p_up)?,
                        preview_scale_down: scale(p_down)?,
                        exact_zooms_only: boolean(KIND, exact, "exact zooms only")?,
                        scale_up_quality: quality(up_quality)?,
                        scale_down_quality: quality(down_quality)?,
                    },
                },
            ))
        })
        .collect::<DecodeResult<_>>()
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::ClientOptions;

    #[test]
    fn the_new_client_defaults_decode() {
        let defaults = ClientOptions::defaults().unwrap();
        assert!(defaults.booleans.len() > 200);
        assert!(defaults.default_sort.is_some());
        assert_eq!(defaults.default_tag_sorts.len(), 4);
        // (a new client's media_view, where mpv plays video)
        assert_eq!(
            defaults.media_viewer_settings(),
            hydrus_core::media_viewer::MediaViewerSettings::default()
        );
        assert_eq!(
            defaults.info_line_settings(),
            hydrus_core::media_viewer::InfoLineSettings::default()
        );
    }
}
