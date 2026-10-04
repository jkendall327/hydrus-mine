//! Old-style import options, from before hydrus v670 split them into
//! today's per-kind options: file (type 7), tag (6) and note (82) import
//! options. They survive on subscriptions (and their queries) last saved
//! before then.
//!
//! Each is read in every version a subscription saved since hydrus
//! started keeping note import options on them can hold, applying the
//! reference's upgrades on the way, and [`convert`] turns them into an
//! import options slice as the reference does
//! (`ImportOptionsContainerMigration.ConvertLegacyOptionsToContainer`, with
//! no parent container: subscriptions load before the client has booted).
//!
//! | object | type | versions |
//! |---|---|---|
//! | file import options | 7 | 8-15 |
//! | tag import options | 6 | 6-9 |
//! | note import options | 82 | 1, 2 |

use std::collections::BTreeSet;

use hydrus_core::import_options::{
    FileFilteringOptions, ImportOptionsSlice, LocationOptions, NoteImportOptions, PrefetchCheck,
    PrefetchOptions, PresentationOptions, TagFilteringOptions, TagImportOptions,
};
use hydrus_core::mime::{Mime, specific_filetype_codes, summarise_filetype_codes};

use super::domain::expect;
use super::import_options::{
    check, core_tag_filter, deleted_keys, file_filtering, keys, locations, notes_from,
    opt_resolution, opt_u64, prefetch, presentation, service_tags, tag_filtering, tags,
};
use super::location::LocationContext;
use super::tag_filter::TagFilter;
use super::util::{DecodeResult, boolean, hex_bytes, int, list, malformed, nested, string, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const FILE: SerialisableType = SerialisableType(7);
const TAG: SerialisableType = SerialisableType(6);
const NOTE: SerialisableType = SerialisableType(82);
const PREDICATE: SerialisableType = SerialisableType(14);

/// File import options as the reference upgrades them (version 15).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyFileImportOptions {
    pub prefetch: PrefetchOptions,
    pub file_filtering: FileFilteringOptions,
    pub locations: LocationOptions,
    pub presentation: PresentationOptions,
    /// "Use the defaults", ignoring the rest.
    pub is_default: bool,
}

/// Tag import options as the reference upgrades them (version 9).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyTagImportOptions {
    pub fetch_even_if_url_known_and_file_already_in_db: bool,
    pub fetch_even_if_hash_known_and_file_already_in_db: bool,
    pub filtering: TagFilteringOptions,
    pub tags: TagImportOptions,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyNoteImportOptions {
    pub notes: NoteImportOptions,
    pub is_default: bool,
}

/// Take the next field of a stored tuple.
struct Fields<'a> {
    kind: SerialisableType,
    items: &'a [PyJson],
    at: usize,
}

impl<'a> Fields<'a> {
    fn new(kind: SerialisableType, value: &'a PyJson, what: &str) -> DecodeResult<Self> {
        Ok(Self {
            kind,
            items: list(kind, value, what)?,
            at: 0,
        })
    }

    fn next(&mut self, what: &str) -> DecodeResult<&'a PyJson> {
        let item = self
            .items
            .get(self.at)
            .ok_or_else(|| malformed(self.kind, format!("no {what}")))?;
        self.at += 1;
        Ok(item)
    }

    fn boolean(&mut self, what: &str) -> DecodeResult<bool> {
        let kind = self.kind;
        boolean(kind, self.next(what)?, what)
    }

    fn object(&mut self, what: &str) -> DecodeResult<SerialisableObject> {
        let kind = self.kind;
        nested(kind, self.next(what)?, what)
    }
}

const GENERAL_APPLICATION: u8 = Mime::GeneralApplication.code();

/// Add archives and image project files wherever "applications" were
/// allowed (they were split off from it).
fn split_applications(codes: &mut BTreeSet<u8>) {
    if codes.contains(&GENERAL_APPLICATION) {
        codes.insert(Mime::GeneralApplicationArchive.code());
        codes.insert(Mime::GeneralImageProject.code());
    }
}

/// The filetypes of a `system:filetype` predicate as it loads (applying
/// its own upgrades), by code.
fn predicate_filetypes(object: &SerialisableObject) -> DecodeResult<BTreeSet<u8>> {
    expect(object, PREDICATE, &[1, 2, 3, 4, 5, 6, 7, 8])?;
    let info = object.info();
    let [_, value, _] = tuple::<3>(PREDICATE, &info, "filetype predicate")?;
    let mut codes = list(PREDICATE, value, "filetypes")?
        .iter()
        .map(|m| {
            u8::try_from(int(PREDICATE, m, "filetype")?)
                .map_err(|_| malformed(PREDICATE, "filetype out of range"))
        })
        .collect::<DecodeResult<BTreeSet<u8>>>()?;
    if object.version < 5 {
        codes = summarise_filetype_codes(&codes, true);
    }
    if object.version < 7 {
        split_applications(&mut codes);
    }
    Ok(codes)
}

/// Decode old-style file import options (versions 8-15).
pub fn file_import_options(object: &SerialisableObject) -> DecodeResult<LegacyFileImportOptions> {
    let k = FILE;
    expect(object, k, &[8, 9, 10, 11, 12, 13, 14, 15])?;
    let v = object.version;
    let info = object.info();
    let mut fields = Fields::new(k, &info, "file import options")?;
    if v >= 14 {
        let prefetch = prefetch(&fields.object("prefetch options")?)?;
        let file_filtering = file_filtering(&fields.object("file filtering options")?)?;
        let locations = if v == 15 {
            locations(&fields.object("location options")?)?
        } else {
            let context = LocationContext::from_tuple(fields.next("destination")?)?;
            let post = fields.next("post-import options")?;
            location_options(&context, post, v)?
        };
        return Ok(LegacyFileImportOptions {
            prefetch,
            file_filtering,
            locations,
            presentation: presentation(&fields.object("presentation options")?)?,
            is_default: fields.boolean("is default")?,
        });
    }
    // version 13 had the prefetch options split off; before that they were
    // part of the pre-import options
    let prefetch_object = if v == 13 {
        Some(prefetch(&fields.object("prefetch options")?)?)
    } else {
        None
    };
    let pre = fields.next("pre-import options")?;
    let mut pre = Fields::new(k, pre, "pre-import options")?;
    let exclude_deleted = pre.boolean("exclude deleted")?;
    let prefetch = match prefetch_object {
        Some(p) => p,
        None if v >= 9 => PrefetchOptions {
            hash_check: check(k, pre.next("hash check type")?, "hash check type")?,
            url_check: check(k, pre.next("url check type")?, "url check type")?,
            url_check_looks_for_neighbour_spam: pre.boolean("neighbour spam check")?,
            ..PrefetchOptions::default()
        },
        None => {
            let skip_urls = pre.boolean("do not check known urls")?;
            let skip_hashes = pre.boolean("do not check hashes")?;
            PrefetchOptions {
                hash_check: if skip_hashes {
                    PrefetchCheck::DoNotCheck
                } else {
                    PrefetchCheck::CheckAndMatchesAreDispositive
                },
                url_check: if skip_urls {
                    PrefetchCheck::DoNotCheck
                } else {
                    PrefetchCheck::Check
                },
                url_check_looks_for_neighbour_spam: true,
                ..PrefetchOptions::default()
            }
        }
    };
    let allow_decompression_bombs = pre.boolean("allow decompression bombs")?;
    let mut filetypes = predicate_filetypes(&pre.object("filetype predicate")?)?;
    if v < 10 {
        split_applications(&mut filetypes);
        filetypes = summarise_filetype_codes(&filetypes, true);
    }
    // as version 14 set them: the specific types, summarised
    filetypes = summarise_filetype_codes(
        &summarise_filetype_codes(&specific_filetype_codes(&filetypes, false), false),
        true,
    );
    let file_filtering = FileFilteringOptions {
        exclude_deleted,
        allow_decompression_bombs,
        filetypes,
        min_size: opt_u64(k, pre.next("min size")?, "min size")?,
        max_size: opt_u64(k, pre.next("max size")?, "max size")?,
        max_gif_size: opt_u64(k, pre.next("max gif size")?, "max gif size")?,
        min_resolution: opt_resolution(k, pre.next("min resolution")?, "min resolution")?,
        max_resolution: opt_resolution(k, pre.next("max resolution")?, "max resolution")?,
    };
    let context = LocationContext::from_tuple(pre.next("destination")?)?;
    let locations = location_options(&context, fields.next("post-import options")?, v)?;
    Ok(LegacyFileImportOptions {
        prefetch,
        file_filtering,
        locations,
        presentation: presentation(&fields.object("presentation options")?)?,
        is_default: fields.boolean("is default")?,
    })
}

/// Location options from a destination and the post-import options of
/// file import options of version `v` (8-14).
fn location_options(
    context: &LocationContext,
    post: &PyJson,
    v: u32,
) -> DecodeResult<LocationOptions> {
    let k = FILE;
    let mut post = Fields::new(k, post, "post-import options")?;
    let automatically_archive = post.boolean("automatically archive")?;
    let associate_primary_urls = post.boolean("associate primary urls")?;
    let associate_source_urls = post.boolean("associate source urls")?;
    let (archive_already_in_db, destinations_for_already_in_db) = match v {
        // (version 11 added "do content updates on already in db files",
        // on; version 12 split it into archiving, and destinations, off)
        ..=10 => (true, false),
        11 => (post.boolean("content updates on already in db")?, false),
        _ => (
            post.boolean("archive already in db")?,
            post.boolean("destinations for already in db")?,
        ),
    };
    Ok(LocationOptions {
        destinations: keys(context),
        deleted_destinations: deleted_keys(context),
        automatically_archive,
        associate_primary_urls,
        associate_source_urls,
        archive_already_in_db,
        destinations_for_already_in_db,
    })
}

/// Decode old-style tag import options (versions 6-9).
pub fn tag_import_options(object: &SerialisableObject) -> DecodeResult<LegacyTagImportOptions> {
    let k = TAG;
    expect(object, k, &[6, 7, 8, 9])?;
    let v = object.version;
    let info = object.info();
    let mut fields = Fields::new(k, &info, "tag import options")?;
    let fetch_url = fields.boolean("fetch if url known")?;
    let fetch_hash = fields.boolean("fetch if hash known")?;
    let (filtering, tags) = if v == 9 {
        (
            tag_filtering(&fields.object("tag filtering options")?)?,
            tags(&fields.object("tag import options")?)?,
        )
    } else {
        let blacklist = core_tag_filter(&TagFilter::from_tuple(fields.next("tag blacklist")?)?);
        let whitelist = if v == 8 {
            list(k, fields.next("tag whitelist")?, "tag whitelist")?
                .iter()
                .map(|t| string(k, t, "whitelisted tag"))
                .collect::<DecodeResult<_>>()?
        } else {
            Vec::new()
        };
        let services = list(
            k,
            fields.next("service tag options")?,
            "service tag options",
        )?
        .iter()
        .map(|pair| {
            let [key, options] = tuple::<2>(k, pair, "service tag import options")?;
            Ok((
                hex::encode(hex_bytes(k, key, "tag service key")?),
                service_tags(&nested(k, options, "service tag import options")?)?,
            ))
        })
        .collect::<DecodeResult<_>>()?;
        (
            TagFilteringOptions {
                blacklist,
                whitelist,
            },
            TagImportOptions { services },
        )
    };
    let is_default = if v >= 7 {
        fields.boolean("is default")?
    } else {
        false
    };
    Ok(LegacyTagImportOptions {
        fetch_even_if_url_known_and_file_already_in_db: fetch_url,
        fetch_even_if_hash_known_and_file_already_in_db: fetch_hash,
        filtering,
        tags,
        is_default,
    })
}

/// Decode old-style note import options (versions 1, 2).
pub fn note_import_options(object: &SerialisableObject) -> DecodeResult<LegacyNoteImportOptions> {
    let k = NOTE;
    expect(object, k, &[1, 2])?;
    let info = object.info();
    if object.version == 2 {
        let [notes, is_default] = tuple::<2>(k, &info, "note import options")?;
        let notes = nested(k, notes, "note import options")?;
        return Ok(LegacyNoteImportOptions {
            notes: super::import_options::notes(&notes)?,
            is_default: boolean(k, is_default, "is default")?,
        });
    }
    let [first @ .., is_default] = tuple::<7>(k, &info, "note import options")?;
    Ok(LegacyNoteImportOptions {
        notes: notes_from(k, first)?,
        is_default: boolean(k, is_default, "is default")?,
    })
}

/// The import options slice the reference makes of old-style options.
pub fn convert(
    file: Option<&LegacyFileImportOptions>,
    tags: Option<&LegacyTagImportOptions>,
    notes: Option<&LegacyNoteImportOptions>,
) -> ImportOptionsSlice {
    let file = file.filter(|f| !f.is_default);
    let tags = tags.filter(|t| !t.is_default);
    let notes = notes.filter(|n| !n.is_default);
    let mut slice = ImportOptionsSlice::default();
    if let Some(t) = tags {
        // the "fetch anyway" switches moved from tag to prefetch options
        let mut prefetch = file.map(|f| f.prefetch.clone()).unwrap_or_default();
        prefetch.fetch_metadata_even_if_url_recognised_and_file_already_in_db =
            t.fetch_even_if_url_known_and_file_already_in_db;
        prefetch.fetch_metadata_even_if_hash_recognised_and_file_already_in_db =
            t.fetch_even_if_hash_known_and_file_already_in_db;
        if prefetch != PrefetchOptions::default() {
            slice.prefetch = Some(prefetch);
        }
    }
    if let Some(f) = file {
        slice.locations = Some(f.locations.clone());
        slice.file_filtering = Some(f.file_filtering.clone());
        slice.presentation = Some(f.presentation.clone());
    }
    if let Some(t) = tags {
        slice.tags = Some(t.tags.clone());
        slice.tag_filtering = Some(t.filtering.clone());
    }
    if let Some(n) = notes {
        slice.notes = Some(n.notes.clone());
    }
    slice
}
