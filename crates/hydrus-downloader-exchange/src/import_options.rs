//! Reference import-options containers for clipboard and PNG exchange.
use hydrus_core::import_options::{
    ImportOptionsSlice, PrefetchCheck, PresentationInbox, PresentationStatus,
};
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_legacy::{objects::import_options, serialisable::SerialisableObject};
use serde_json::{Value, json};

use crate::{Error, MAX_BYTES, Result, transport};

fn check(value: PrefetchCheck) -> u8 {
    match value {
        PrefetchCheck::DoNotCheck => 0,
        PrefetchCheck::Check => 1,
        PrefetchCheck::CheckAndMatchesAreDispositive => 2,
    }
}
fn filter(value: &TagFilter) -> Value {
    let rows = value
        .rules()
        .map(|(slice, rule)| {
            json!([
                slice,
                match rule {
                    FilterRule::Blacklist => 0,
                    FilterRule::Whitelist => 1,
                }
            ])
        })
        .collect::<Vec<_>>();
    json!([44, 1, rows])
}
fn location(keys: &[String]) -> Result<Value> {
    for key in keys {
        crate::encode::valid_key(key)?;
    }
    let mut keys = keys.to_vec();
    keys.sort();
    keys.dedup();
    Ok(json!([103, 1, [keys, []]]))
}

/// Encode all present kinds, leaving inherited kinds absent.
pub fn tuple(slice: &ImportOptionsSlice) -> Result<Value> {
    let mut rows = Vec::new();
    let mut add = |kind: u8, value: Value| rows.push(json!([[0, kind], [2, value]]));
    if let Some(o) = &slice.prefetch {
        add(
            0,
            json!([
                145,
                2,
                [
                    check(o.hash_check),
                    check(o.url_check),
                    o.url_check_looks_for_neighbour_spam,
                    o.fetch_metadata_even_if_url_recognised_and_file_already_in_db,
                    o.fetch_metadata_even_if_hash_recognised_and_file_already_in_db
                ]
            ]),
        );
    }
    if let Some(o) = &slice.file_filtering {
        add(
            1,
            json!([
                148,
                1,
                [
                    o.exclude_deleted,
                    o.allow_decompression_bombs,
                    [14, 8, [17, o.filetypes, true]],
                    o.min_size,
                    o.max_size,
                    o.max_gif_size,
                    o.min_resolution,
                    o.max_resolution
                ]
            ]),
        );
    }
    if let Some(o) = &slice.tag_filtering {
        add(2, json!([150, 1, [filter(&o.blacklist), o.whitelist]]));
    }
    if let Some(o) = &slice.locations {
        add(
            3,
            json!([
                149,
                1,
                [
                    location(&o.destinations)?,
                    o.automatically_archive,
                    o.associate_primary_urls,
                    o.associate_source_urls,
                    o.archive_already_in_db,
                    o.destinations_for_already_in_db
                ]
            ]),
        );
    }
    if let Some(o) = &slice.tags {
        let mut services = Vec::new();
        for (key, service) in &o.services {
            crate::encode::valid_key(key)?;
            let mut additional = service.additional_tags.clone();
            additional.sort();
            additional.dedup();
            services.push(json!([
                key,
                [
                    65,
                    4,
                    [
                        service.get_tags,
                        filter(&service.get_tags_filter),
                        additional,
                        service.to_new_files,
                        service.to_already_in_inbox,
                        service.to_already_in_archive,
                        service.only_add_existing_tags,
                        filter(&service.only_add_existing_tags_filter),
                        service.get_tags_overwrite_deleted,
                        service.additional_tags_overwrite_deleted
                    ]
                ]
            ]));
        }
        add(4, json!([151, 1, services]));
    }
    if let Some(o) = &slice.notes {
        add(
            5,
            json!([
                153,
                1,
                [
                    o.get_notes,
                    o.extend_existing_note_if_possible,
                    o.conflict as u8,
                    o.name_whitelist,
                    o.all_name_override,
                    o.name_overrides
                ]
            ]),
        );
    }
    if let Some(o) = &slice.presentation {
        let status = match o.status {
            PresentationStatus::AnyGood => 0,
            PresentationStatus::NewOnly => 1,
            PresentationStatus::None => 2,
        };
        let inbox = match o.inbox {
            PresentationInbox::Agnostic => 0,
            PresentationInbox::RequireInbox => 1,
            PresentationInbox::AndIncludeAllInbox => 2,
        };
        add(6, json!([108, 2, [location(&o.location)?, status, inbox]]));
    }
    if let Some(o) = &slice.external_programs {
        let entries: Value = o.stored.as_ref().map_or_else(
            || Ok(json!([26, 3, []])),
            |text| serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string())),
        )?;
        add(7, json!([162, 1, entries]));
    }
    Ok(json!([143, 1, [21, 2, rows]]))
}

/// Serialize as the reference's JSON clipboard text.
pub fn encode_text(slice: &ImportOptionsSlice) -> Result<String> {
    let text = hydrus_core::pyjson::PyJson::parse(&tuple(slice)?.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}

/// Decode atomically; reject location contexts the native importer cannot represent.
pub fn decode_text(text: &str) -> Result<ImportOptionsSlice> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let object =
        SerialisableObject::from_tuple_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let value: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    if let Some(rows) = value.pointer("/2/2").and_then(Value::as_array) {
        for row in rows {
            let context = match row.pointer("/0/1").and_then(Value::as_u64) {
                Some(3 | 6) => row.pointer("/1/1/2/0"),
                _ => None,
            };
            if context
                .and_then(|v| v.pointer("/2/1"))
                .and_then(Value::as_array)
                .is_some_and(|keys| !keys.is_empty())
            {
                return Err(Error::Unsupported(
                    "deleted file-domain import contexts are not yet supported".into(),
                ));
            }
        }
    }
    import_options::slice(&object).map_err(|e| Error::Unsupported(e.to_string()))
}

/// Export a compressed reference payload PNG.
pub fn encode_png(slice: &ImportOptionsSlice) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(slice)?)
}
/// Decode a compressed reference payload PNG.
pub fn decode_png(bytes: &[u8]) -> Result<ImportOptionsSlice> {
    decode_text(&transport::decode_payload(bytes)?)
}
