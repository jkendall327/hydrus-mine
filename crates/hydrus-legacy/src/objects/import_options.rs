//! Import options (v688's import options manager, type 144, and the slices
//! it holds, type 143), decoded into `hydrus_core::import_options`.
//!
//! | object | type | version |
//! |---|---|---|
//! | import options manager / container (slice) | 144 / 143 | 1 / 1 |
//! | prefetch | 145 | 1, 2 |
//! | file filtering (with its filetype predicate, type 14) | 148 | 1 |
//! | locations (with a location context, type 103) | 149 | 1 |
//! | tag filtering (with a tag filter, type 44) | 150 | 1 |
//! | tags / per service | 151 / 65 | 1 / 4 |
//! | notes | 153 | 1 |
//! | presentation | 108 | 2 |
//! | external programs (kept as stored) | 162 | 1 |

use hydrus_core::import_options::{
    CallerType, ExternalProgramsOptions, FileFilteringOptions, ImportOptionsManager,
    ImportOptionsSlice, LocationOptions, NoteConflict, NoteImportOptions, PrefetchCheck,
    PrefetchOptions, PresentationInbox, PresentationOptions, PresentationStatus,
    ServiceTagImportOptions, TagFilteringOptions, TagImportOptions,
};
use hydrus_core::tag_filter::{FilterRule, TagFilter as CoreTagFilter};

use super::domain::expect;
use super::location::LocationContext;
use super::tag_filter::{TagFilter, TagRule};
use super::util::{
    DecodeResult, boolean, dictionary_pairs, hex_bytes, int, list, malformed, nested, opt_string,
    string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableObject, SerialisableType};

const MANAGER: SerialisableType = SerialisableType(144);
const CONTAINER: SerialisableType = SerialisableType(143);
const PREFETCH: SerialisableType = SerialisableType(145);
const FILE_FILTERING: SerialisableType = SerialisableType(148);
const LOCATIONS: SerialisableType = SerialisableType(149);
const TAG_FILTERING: SerialisableType = SerialisableType(150);
const TAGS: SerialisableType = SerialisableType(151);
const SERVICE_TAGS: SerialisableType = SerialisableType(65);
const NOTES: SerialisableType = SerialisableType(153);
const PRESENTATION: SerialisableType = SerialisableType(108);
const EXTERNAL_PROGRAMS: SerialisableType = SerialisableType(162);
const PREDICATE: SerialisableType = SerialisableType(14);

/// A legacy tag filter as a core one.
pub fn core_tag_filter(legacy: &TagFilter) -> CoreTagFilter {
    let mut filter = CoreTagFilter::new();
    for (slice, rule) in legacy.effective_rules() {
        filter.set_rule(
            slice,
            match rule {
                TagRule::Allow => FilterRule::Whitelist,
                TagRule::Block => FilterRule::Blacklist,
            },
        );
    }
    filter
}

fn object_meta(
    kind: SerialisableType,
    meta: &Meta,
    what: &str,
) -> DecodeResult<SerialisableObject> {
    match meta {
        Meta::Object(object) => Ok((**object).clone()),
        _ => Err(malformed(kind, format!("{what} is not an object"))),
    }
}

/// Decode the import options manager.
pub fn manager(object: &SerialisableObject) -> DecodeResult<ImportOptionsManager> {
    let k = MANAGER;
    expect(object, k, &[1])?;
    let info = object.info();
    let [callers, url_classes, favourites] = tuple::<3>(k, &info, "import options manager")?;
    let mut caller_defaults = Vec::new();
    for (key, value) in dictionary_pairs(&nested(k, callers, "caller defaults")?)?.iter() {
        let Meta::Json(code) = key else {
            return Err(malformed(k, "a caller type is not a number"));
        };
        let code = int(k, code, "caller type")?;
        let caller = CallerType::from_code(code)
            .ok_or_else(|| malformed(k, format!("unknown caller type {code}")))?;
        caller_defaults.push((caller, slice(&object_meta(k, value, "caller defaults")?)?));
    }
    let mut url_class_defaults = Vec::new();
    for (key, value) in dictionary_pairs(&nested(k, url_classes, "url class defaults")?)?.iter() {
        let Meta::Bytes(key) = key else {
            return Err(malformed(k, "a url class key is not bytes"));
        };
        url_class_defaults.push((
            hex::encode(key),
            slice(&object_meta(k, value, "url class defaults")?)?,
        ));
    }
    let mut named = Vec::new();
    for (key, value) in dictionary_pairs(&nested(k, favourites, "favourites")?)?.iter() {
        let name = key
            .as_str()
            .ok_or_else(|| malformed(k, "a favourite's name is not text"))?;
        named.push((
            name.to_owned(),
            slice(&object_meta(k, value, "favourite")?)?,
        ));
    }
    Ok(ImportOptionsManager {
        caller_defaults,
        url_class_defaults,
        favourites: named,
    })
}

/// Decode a slice (an import options container).
pub fn slice(object: &SerialisableObject) -> DecodeResult<ImportOptionsSlice> {
    let k = CONTAINER;
    expect(object, k, &[1])?;
    let mut slice = ImportOptionsSlice::default();
    let dictionary = SerialisableObject::from_tuple(&object.info())
        .map_err(|e| malformed(k, format!("options: {e}")))?;
    for (key, value) in dictionary_pairs(&dictionary)?.iter() {
        let Meta::Json(code) = key else {
            return Err(malformed(k, "an options kind is not a number"));
        };
        let options = object_meta(k, value, "options")?;
        match int(k, code, "options kind")? {
            0 => slice.prefetch = Some(prefetch(&options)?),
            1 => slice.file_filtering = Some(file_filtering(&options)?),
            2 => slice.tag_filtering = Some(tag_filtering(&options)?),
            3 => slice.locations = Some(locations(&options)?),
            4 => slice.tags = Some(tags(&options)?),
            5 => slice.notes = Some(notes(&options)?),
            6 => slice.presentation = Some(presentation(&options)?),
            7 => slice.external_programs = Some(external_programs(&options)?),
            other => return Err(malformed(k, format!("unknown options kind {other}"))),
        }
    }
    Ok(slice)
}

fn check(k: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<PrefetchCheck> {
    let code = int(k, value, what)?;
    PrefetchCheck::from_code(code).ok_or_else(|| malformed(k, format!("unknown {what} {code}")))
}

fn prefetch(object: &SerialisableObject) -> DecodeResult<PrefetchOptions> {
    let k = PREFETCH;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let items = list(k, &info, "prefetch options")?;
    let (hash, url, spam, even_url, even_hash) = match items {
        [hash, url, spam] => (hash, url, spam, None, None),
        [hash, url, spam, even_url, even_hash] => {
            (hash, url, spam, Some(even_url), Some(even_hash))
        }
        _ => return Err(malformed(k, "prefetch options have the wrong length")),
    };
    let flag = |v: Option<&PyJson>, what: &str| v.map_or(Ok(false), |v| boolean(k, v, what));
    Ok(PrefetchOptions {
        hash_check: check(k, hash, "hash check type")?,
        url_check: check(k, url, "url check type")?,
        url_check_looks_for_neighbour_spam: boolean(k, spam, "neighbour spam check")?,
        fetch_metadata_even_if_url_recognised_and_file_already_in_db: flag(
            even_url,
            "fetch if url recognised",
        )?,
        fetch_metadata_even_if_hash_recognised_and_file_already_in_db: flag(
            even_hash,
            "fetch if hash recognised",
        )?,
    })
}

fn opt_u64(k: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    let n = int(k, value, what)?;
    u64::try_from(n)
        .map(Some)
        .map_err(|_| malformed(k, format!("{what} is negative")))
}

fn opt_resolution(
    k: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Option<(u32, u32)>> {
    if value.is_null() {
        return Ok(None);
    }
    let [w, h] = tuple::<2>(k, value, what)?;
    let dimension = |v: &PyJson| {
        u32::try_from(int(k, v, what)?).map_err(|_| malformed(k, format!("{what} out of range")))
    };
    Ok(Some((dimension(w)?, dimension(h)?)))
}

fn file_filtering(object: &SerialisableObject) -> DecodeResult<FileFilteringOptions> {
    let k = FILE_FILTERING;
    expect(object, k, &[1])?;
    let info = object.info();
    let [
        exclude_deleted,
        bombs,
        predicate,
        min_size,
        max_size,
        max_gif_size,
        min_resolution,
        max_resolution,
    ] = tuple::<8>(k, &info, "file filtering options")?;
    let predicate = nested(k, predicate, "filetype predicate")?;
    predicate.expect_kind(PREDICATE)?;
    let predicate_info = predicate.info();
    let [_, value, _] = tuple::<3>(PREDICATE, &predicate_info, "filetype predicate")?;
    let filetypes = list(PREDICATE, value, "filetypes")?
        .iter()
        .map(|m| {
            u8::try_from(int(PREDICATE, m, "filetype")?)
                .map_err(|_| malformed(PREDICATE, "filetype out of range"))
        })
        .collect::<DecodeResult<_>>()?;
    Ok(FileFilteringOptions {
        exclude_deleted: boolean(k, exclude_deleted, "exclude deleted")?,
        allow_decompression_bombs: boolean(k, bombs, "allow decompression bombs")?,
        filetypes,
        min_size: opt_u64(k, min_size, "min size")?,
        max_size: opt_u64(k, max_size, "max size")?,
        max_gif_size: opt_u64(k, max_gif_size, "max gif size")?,
        min_resolution: opt_resolution(k, min_resolution, "min resolution")?,
        max_resolution: opt_resolution(k, max_resolution, "max resolution")?,
    })
}

fn tag_filtering(object: &SerialisableObject) -> DecodeResult<TagFilteringOptions> {
    let k = TAG_FILTERING;
    expect(object, k, &[1])?;
    let info = object.info();
    let [blacklist, whitelist] = tuple::<2>(k, &info, "tag filtering options")?;
    Ok(TagFilteringOptions {
        blacklist: core_tag_filter(&TagFilter::from_tuple(blacklist)?),
        whitelist: list(k, whitelist, "tag whitelist")?
            .iter()
            .map(|t| string(k, t, "whitelisted tag"))
            .collect::<DecodeResult<_>>()?,
    })
}

/// A location context's file domains (a set, so sorted).
fn keys(context: &LocationContext) -> Vec<String> {
    let mut keys: Vec<String> = context
        .current
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    keys.sort();
    keys
}

/// Texts the reference keeps as a set (stored in arbitrary order), sorted.
fn string_set(k: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<Vec<String>> {
    let mut items: Vec<String> = list(k, value, what)?
        .iter()
        .map(|t| string(k, t, what))
        .collect::<DecodeResult<_>>()?;
    items.sort();
    items.dedup();
    Ok(items)
}

fn locations(object: &SerialisableObject) -> DecodeResult<LocationOptions> {
    let k = LOCATIONS;
    expect(object, k, &[1])?;
    let info = object.info();
    let [
        destination,
        archive,
        primary,
        source,
        archive_already,
        destinations_already,
    ] = tuple::<6>(k, &info, "location options")?;
    Ok(LocationOptions {
        destinations: keys(&LocationContext::from_tuple(destination)?),
        automatically_archive: boolean(k, archive, "automatically archive")?,
        associate_primary_urls: boolean(k, primary, "associate primary urls")?,
        associate_source_urls: boolean(k, source, "associate source urls")?,
        archive_already_in_db: boolean(k, archive_already, "archive already in db")?,
        destinations_for_already_in_db: boolean(
            k,
            destinations_already,
            "destinations for already in db",
        )?,
    })
}

pub fn tags(object: &SerialisableObject) -> DecodeResult<TagImportOptions> {
    let k = TAGS;
    expect(object, k, &[1])?;
    let info = object.info();
    let services = list(k, &info, "tag import options")?
        .iter()
        .map(|pair| {
            let [key, options] = tuple::<2>(k, pair, "service tag import options")?;
            Ok((
                hex::encode(hex_bytes(k, key, "tag service key")?),
                service_tags(&nested(k, options, "service tag import options")?)?,
            ))
        })
        .collect::<DecodeResult<_>>()?;
    Ok(TagImportOptions { services })
}

fn service_tags(object: &SerialisableObject) -> DecodeResult<ServiceTagImportOptions> {
    let k = SERVICE_TAGS;
    expect(object, k, &[4])?;
    let info = object.info();
    let [
        get_tags,
        get_tags_filter,
        additional,
        to_new,
        to_inbox,
        to_archive,
        only_existing,
        only_existing_filter,
        overwrite_deleted,
        additional_overwrite_deleted,
    ] = tuple::<10>(k, &info, "service tag import options")?;
    let b = |v: &PyJson, what: &str| boolean(k, v, what);
    Ok(ServiceTagImportOptions {
        get_tags: b(get_tags, "get tags")?,
        get_tags_filter: core_tag_filter(&TagFilter::from_tuple(get_tags_filter)?),
        additional_tags: string_set(k, additional, "additional tags")?,
        to_new_files: b(to_new, "to new files")?,
        to_already_in_inbox: b(to_inbox, "to already in inbox")?,
        to_already_in_archive: b(to_archive, "to already in archive")?,
        only_add_existing_tags: b(only_existing, "only add existing tags")?,
        only_add_existing_tags_filter: core_tag_filter(&TagFilter::from_tuple(
            only_existing_filter,
        )?),
        get_tags_overwrite_deleted: b(overwrite_deleted, "overwrite deleted")?,
        additional_tags_overwrite_deleted: b(
            additional_overwrite_deleted,
            "additional overwrite deleted",
        )?,
    })
}

fn notes(object: &SerialisableObject) -> DecodeResult<NoteImportOptions> {
    let k = NOTES;
    expect(object, k, &[1])?;
    let info = object.info();
    let [get, extend, conflict, whitelist, all_override, overrides] =
        tuple::<6>(k, &info, "note import options")?;
    let code = int(k, conflict, "conflict resolution")?;
    Ok(NoteImportOptions {
        get_notes: boolean(k, get, "get notes")?,
        extend_existing_note_if_possible: boolean(k, extend, "extend")?,
        conflict: NoteConflict::from_code(code)
            .ok_or_else(|| malformed(k, format!("unknown conflict resolution {code}")))?,
        name_whitelist: string_set(k, whitelist, "name whitelist")?,
        all_name_override: opt_string(k, all_override, "name override")?,
        name_overrides: list(k, overrides, "name overrides")?
            .iter()
            .map(|pair| {
                let [from, to] = tuple::<2>(k, pair, "name override")?;
                Ok((string(k, from, "name")?, string(k, to, "name")?))
            })
            .collect::<DecodeResult<_>>()?,
    })
}

fn presentation(object: &SerialisableObject) -> DecodeResult<PresentationOptions> {
    let k = PRESENTATION;
    expect(object, k, &[2])?;
    let info = object.info();
    let [location, status, inbox] = tuple::<3>(k, &info, "presentation options")?;
    Ok(PresentationOptions {
        location: keys(&LocationContext::from_tuple(location)?),
        status: match int(k, status, "presentation status")? {
            0 => PresentationStatus::AnyGood,
            1 => PresentationStatus::NewOnly,
            2 => PresentationStatus::None,
            other => return Err(malformed(k, format!("unknown presentation status {other}"))),
        },
        inbox: match int(k, inbox, "presentation inbox")? {
            0 => PresentationInbox::Agnostic,
            1 => PresentationInbox::RequireInbox,
            2 => PresentationInbox::AndIncludeAllInbox,
            other => return Err(malformed(k, format!("unknown presentation inbox {other}"))),
        },
    })
}

fn external_programs(object: &SerialisableObject) -> DecodeResult<ExternalProgramsOptions> {
    let k = EXTERNAL_PROGRAMS;
    expect(object, k, &[1])?;
    let entries = nested(k, &object.info(), "entries")?;
    let empty = matches!(&entries.info(), PyJson::List(items) if items.is_empty());
    Ok(ExternalProgramsOptions {
        stored: (!empty).then(|| object.info().to_python_string()),
    })
}
