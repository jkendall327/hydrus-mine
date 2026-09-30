//! Every primary table, read through hydrus-legacy's typed readers, must
//! match what the reference sees (`basic.expected.json`, written by
//! `oracle/dump_legacy_expectations.py`).

mod common;

use common::{basic, collect};
use hydrus_core::{ContentStatus, ServiceId};
use hydrus_legacy::LegacyDb;
use hydrus_legacy::readers::FileProperty;
use serde_json::{Value, json};

fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map_or(Value::Null, Into::into)
}

fn service_suffix(table: &str, prefix: &str) -> Option<ServiceId> {
    table
        .strip_prefix(prefix)
        .and_then(|rest| rest.parse::<u32>().ok())
        .map(ServiceId)
}

fn status_of(prefix: &str) -> ContentStatus {
    match prefix {
        "current" => ContentStatus::Current,
        "deleted" => ContentStatus::Deleted,
        "pending" => ContentStatus::Pending,
        "petitioned" => ContentStatus::Petitioned,
        other => panic!("status {other}"),
    }
}

/// The rows of a primary table as the typed readers see them, in the
/// reference's `SELECT *` column order.
#[allow(clippy::too_many_lines)]
fn reader_rows(db: &LegacyDb, qualified: &str) -> Option<Vec<Vec<Value>>> {
    let (schema, table) = qualified.split_once('.').unwrap();
    let rows = match (schema, table) {
        ("external_master", "hashes") => collect(db.hashes())
            .into_iter()
            .map(|(id, h)| vec![json!(id.get()), json!(h.to_hex())])
            .collect(),
        ("external_master", "local_hashes") => collect(db.local_hashes())
            .into_iter()
            .map(|h| {
                vec![
                    json!(h.hash_id.get()),
                    opt(h.md5.map(|x| x.to_hex())),
                    opt(h.sha1.map(|x| x.to_hex())),
                    opt(h.sha512.map(|x| x.to_hex())),
                ]
            })
            .collect(),
        ("external_master", "namespaces") => collect(db.namespaces())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "subtags") => collect(db.subtags())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "tags") => collect(db.tag_definitions())
            .into_iter()
            .map(|t| {
                vec![
                    json!(t.tag_id.get()),
                    json!(t.namespace_id.get()),
                    json!(t.subtag_id.get()),
                ]
            })
            .collect(),
        ("external_master", "url_domains") => collect(db.url_domains())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "urls") => collect(db.urls())
            .into_iter()
            .map(|u| {
                vec![
                    json!(u.url_id.get()),
                    json!(u.domain_id.get()),
                    json!(u.url),
                ]
            })
            .collect(),
        ("external_master", "texts") => collect(db.texts())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "labels") => collect(db.labels())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "notes") => collect(db.notes())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), json!(s)])
            .collect(),
        ("external_master", "blurhashes") => collect(db.blurhashes())
            .into_iter()
            .map(|(id, s)| vec![json!(id.get()), opt(s)])
            .collect(),
        ("external_master", "shape_perceptual_hashes") => collect(db.perceptual_hashes())
            .into_iter()
            .map(|(id, p)| vec![json!(id.get()), json!(p.to_hex())])
            .collect(),
        ("external_master", "shape_perceptual_hash_map") => collect(db.perceptual_hash_map())
            .into_iter()
            .map(|(p, h)| vec![json!(p.get()), json!(h.get())])
            .collect(),
        ("external_caches", "file_maintenance_jobs") => collect(db.file_maintenance_jobs())
            .into_iter()
            .map(|j| {
                vec![
                    json!(j.hash_id.get()),
                    json!(j.job_type),
                    opt(j.time_can_start),
                ]
            })
            .collect(),
        ("external_mappings", _) => {
            let (status, rest) = table.split_once('_').unwrap();
            let service = service_suffix(rest, "mappings_").unwrap();
            collect(db.mappings(service, status_of(status)))
                .into_iter()
                .map(|m| {
                    let mut row = vec![json!(m.tag_id.get()), json!(m.hash_id.get())];
                    if let Some(reason) = m.reason {
                        row.push(json!(reason.get()));
                    }
                    row
                })
                .collect()
        }
        ("main", _) => main_rows(db, table)?,
        _ => return None,
    };
    Some(rows)
}

#[allow(clippy::too_many_lines)]
fn main_rows(db: &LegacyDb, table: &str) -> Option<Vec<Vec<Value>>> {
    let hash_ids = |rows: Vec<hydrus_core::HashId>| -> Vec<Vec<Value>> {
        rows.into_iter().map(|h| vec![json!(h.get())]).collect()
    };
    let pairs = |rows: Vec<(u32, u32)>| -> Vec<Vec<Value>> {
        rows.into_iter()
            .map(|(a, b)| vec![json!(a), json!(b)])
            .collect()
    };
    if let Some(property) = FileProperty::ALL.iter().find(|p| p.table() == table) {
        return Some(hash_ids(collect(db.files_with_property(*property))));
    }
    for (prefix, status) in [
        ("current_tag_siblings_", ContentStatus::Current),
        ("deleted_tag_siblings_", ContentStatus::Deleted),
        ("pending_tag_siblings_", ContentStatus::Pending),
        ("petitioned_tag_siblings_", ContentStatus::Petitioned),
        ("current_tag_parents_", ContentStatus::Current),
        ("deleted_tag_parents_", ContentStatus::Deleted),
        ("pending_tag_parents_", ContentStatus::Pending),
        ("petitioned_tag_parents_", ContentStatus::Petitioned),
    ] {
        if let Some(service) = service_suffix(table, prefix) {
            let rows = if prefix.contains("siblings") {
                db.tag_siblings(service, status)
            } else {
                db.tag_parents(service, status)
            };
            return Some(
                collect(rows)
                    .into_iter()
                    .map(|p| {
                        let mut row = vec![json!(p.from.get()), json!(p.to.get())];
                        if let Some(reason) = p.reason {
                            row.push(json!(reason.get()));
                        }
                        row
                    })
                    .collect(),
            );
        }
    }
    if let Some(service) = service_suffix(table, "current_files_") {
        return Some(
            collect(db.current_files(service))
                .into_iter()
                .map(|f| {
                    vec![
                        json!(f.hash_id.get()),
                        opt(f.added.map(hydrus_core::TimestampMs::millis)),
                    ]
                })
                .collect(),
        );
    }
    if let Some(service) = service_suffix(table, "deleted_files_") {
        return Some(
            collect(db.deleted_files(service))
                .into_iter()
                .map(|f| {
                    vec![
                        json!(f.hash_id.get()),
                        opt(f.deleted.map(hydrus_core::TimestampMs::millis)),
                        opt(f.originally_added.map(hydrus_core::TimestampMs::millis)),
                    ]
                })
                .collect(),
        );
    }
    if let Some(service) = service_suffix(table, "pending_files_") {
        return Some(hash_ids(collect(db.pending_files(service))));
    }
    if let Some(service) = service_suffix(table, "petitioned_files_") {
        return Some(
            collect(db.petitioned_files(service))
                .into_iter()
                .map(|(h, r)| vec![json!(h.get()), json!(r.get())])
                .collect(),
        );
    }
    let rows = match table {
        "version" => vec![vec![json!(hydrus_core::REFERENCE_VERSION)]],
        "services" => db
            .services()
            .unwrap()
            .into_iter()
            .map(|s| {
                vec![
                    json!(s.id.get()),
                    json!(s.key.to_hex()),
                    json!(s.service_type.code()),
                    json!(s.name),
                    // re-serialised: must be byte-identical to what is stored
                    json!(s.dictionary.to_tuple_string()),
                ]
            })
            .collect(),
        "files_info" => collect(db.files_info())
            .into_iter()
            .map(|f| {
                vec![
                    json!(f.hash_id.get()),
                    json!(f.size),
                    json!(f.mime.code()),
                    opt(f.width),
                    opt(f.height),
                    opt(f.duration_ms),
                    opt(f.num_frames),
                    opt(f.has_audio.map(i64::from)),
                    opt(f.num_words),
                ]
            })
            .collect(),
        "files_info_forced_filetypes" => collect(db.forced_filetypes())
            .into_iter()
            .map(|(h, m)| vec![json!(h.get()), json!(m.code())])
            .collect(),
        "local_file_deletion_reasons" => collect(db.deletion_reasons())
            .into_iter()
            .map(|(h, r)| vec![json!(h.get()), json!(r.get())])
            .collect(),
        "file_inbox" => hash_ids(collect(db.inbox())),
        "deferred_physical_file_deletes" => hash_ids(collect(db.deferred_physical_file_deletes())),
        "deferred_physical_thumbnail_deletes" => {
            hash_ids(collect(db.deferred_physical_thumbnail_deletes()))
        }
        "archive_timestamps" => collect(db.archive_timestamps())
            .into_iter()
            .map(|(h, t)| vec![json!(h.get()), json!(t.millis())])
            .collect(),
        "file_modified_timestamps" => collect(db.file_modified_timestamps())
            .into_iter()
            .map(|(h, t)| vec![json!(h.get()), json!(t.millis())])
            .collect(),
        "file_domain_modified_timestamps" => collect(db.domain_modified_timestamps())
            .into_iter()
            .map(|(h, d, t)| vec![json!(h.get()), json!(d.get()), json!(t.millis())])
            .collect(),
        "file_viewing_stats" => collect(db.viewing_stats())
            .into_iter()
            .map(|v| {
                vec![
                    json!(v.hash_id.get()),
                    json!(v.canvas.code()),
                    opt(v.last_viewed.map(hydrus_core::TimestampMs::millis)),
                    json!(v.views),
                    json!(v.viewtime_ms),
                ]
            })
            .collect(),
        "url_map" => pairs(
            collect(db.url_map())
                .into_iter()
                .map(|(h, u)| (h.get(), u.get()))
                .collect(),
        ),
        "file_notes" => collect(db.file_notes())
            .into_iter()
            .map(|n| {
                vec![
                    json!(n.hash_id.get()),
                    json!(n.name_id.get()),
                    json!(n.note_id.get()),
                ]
            })
            .collect(),
        "local_ratings" => collect(db.ratings())
            .into_iter()
            .map(|r| {
                vec![
                    json!(r.service_id.get()),
                    json!(r.hash_id.get()),
                    json!(r.value),
                ]
            })
            .collect(),
        "local_incdec_ratings" => collect(db.incdec_ratings())
            .into_iter()
            .map(|r| {
                vec![
                    json!(r.service_id.get()),
                    json!(r.hash_id.get()),
                    json!(r.value),
                ]
            })
            .collect(),
        "recent_tags" => collect(db.recent_tags())
            .into_iter()
            .map(|r| {
                vec![
                    json!(r.service_id.get()),
                    json!(r.tag_id.get()),
                    opt(r.last_used.map(hydrus_core::TimestampMs::millis)),
                ]
            })
            .collect(),
        "tag_sibling_application" | "tag_parent_application" => {
            let map = if table == "tag_sibling_application" {
                db.sibling_application().unwrap()
            } else {
                db.parent_application().unwrap()
            };
            map.into_iter()
                .flat_map(|(master, applied)| {
                    applied
                        .into_iter()
                        .enumerate()
                        .map(move |(i, a)| vec![json!(master.get()), json!(i), json!(a.get())])
                })
                .collect()
        }
        "duplicate_files" => collect(db.duplicate_kings())
            .into_iter()
            .map(|(m, h)| vec![json!(m.0), json!(h.get())])
            .collect(),
        "duplicate_file_members" => collect(db.duplicate_members())
            .into_iter()
            .map(|(m, h)| vec![json!(m.0), json!(h.get())])
            .collect(),
        "alternate_file_groups" => collect(db.alternates_groups())
            .into_iter()
            .map(|a| vec![json!(a.0)])
            .collect(),
        "alternate_file_group_members" => pairs(
            collect(db.alternates_members())
                .into_iter()
                .map(|(a, m)| (a.0, m.0))
                .collect(),
        ),
        "confirmed_alternate_pairs" => pairs(
            collect(db.confirmed_alternate_pairs())
                .into_iter()
                .map(|(a, b)| (a.0, b.0))
                .collect(),
        ),
        "duplicate_false_positives" => pairs(
            collect(db.false_positive_pairs())
                .into_iter()
                .map(|(a, b)| (a.0, b.0))
                .collect(),
        ),
        "potential_duplicate_pairs" => collect(db.potential_duplicate_pairs())
            .into_iter()
            .map(|p| {
                vec![
                    json!(p.smaller_media_id.0),
                    json!(p.larger_media_id.0),
                    json!(p.distance),
                ]
            })
            .collect(),
        "shape_search_cache" => collect(db.similar_files_search_progress())
            .into_iter()
            .map(|(h, d)| vec![json!(h.get()), opt(d)])
            .collect(),
        "pixel_hash_map" => pairs(
            collect(db.pixel_hash_map())
                .into_iter()
                .map(|(a, b)| (a.get(), b.get()))
                .collect(),
        ),
        "duplicate_files_auto_resolution_rules" => db
            .auto_resolution_rule_ids()
            .unwrap()
            .into_iter()
            .map(|id| vec![json!(id)])
            .collect(),
        "remote_thumbnails" => pairs(
            collect(db.remote_thumbnails())
                .into_iter()
                .map(|(s, h)| (s.get(), h.get()))
                .collect(),
        ),
        "service_filenames" => collect(db.service_filenames())
            .into_iter()
            .map(|(s, h, f)| vec![json!(s.get()), json!(h.get()), json!(f)])
            .collect(),
        "service_directories" => collect(db.service_directories())
            .into_iter()
            .map(|d| {
                vec![
                    json!(d.service_id.get()),
                    json!(d.directory_id.get()),
                    json!(d.num_files),
                    json!(d.total_size),
                    json!(d.note),
                ]
            })
            .collect(),
        "service_directory_file_map" => collect(db.service_directory_files())
            .into_iter()
            .map(|(s, d, h)| vec![json!(s.get()), json!(d.get()), json!(h.get())])
            .collect(),
        "current_storage_granularity" => {
            vec![vec![json!(db.file_storage().unwrap().granularity)]]
        }
        "current_client_files_locations" => db
            .file_storage()
            .unwrap()
            .locations
            .into_iter()
            .map(|(id, location)| vec![json!(id.0), json!(location)])
            .collect(),
        "client_files_subfolders" => db
            .file_storage()
            .unwrap()
            .subfolders
            .into_iter()
            .map(|(prefix, id)| vec![json!(prefix), json!(id.0)])
            .collect(),
        "ideal_client_files_locations" => db
            .file_storage()
            .unwrap()
            .ideal_locations
            .into_iter()
            .map(|l| vec![json!(l.location.0), json!(l.weight), opt(l.max_num_bytes)])
            .collect(),
        "ideal_thumbnail_override_location" => db
            .file_storage()
            .unwrap()
            .ideal_thumbnail_override
            .into_iter()
            .map(|id| vec![json!(id.0)])
            .collect(),
        "json_dict" => db
            .json_dict()
            .unwrap()
            .into_iter()
            .map(|(name, value)| vec![json!(name), json!(value.to_python_string())])
            .collect(),
        // checked by the serialised object tests; the expectations keep only counts
        "json_dumps" | "json_dumps_named" | "json_dumps_hashed" | "options" => Vec::new(),
        _ => return None,
    };
    Some(rows)
}

#[test]
fn every_table_is_classified() {
    let fixture = basic();
    let classification = fixture.expected["table_classification"]
        .as_object()
        .unwrap();
    for (schema, _) in hydrus_legacy::db::DATABASE_FILES {
        for table in fixture.db.table_names(schema).unwrap() {
            let name = format!("{schema}.{table}");
            assert!(
                classification.contains_key(&name),
                "{name} is not classified"
            );
        }
    }
}

#[test]
fn every_primary_table_matches_the_reference() {
    let fixture = basic();
    let tables = fixture.expected["tables"].as_object().unwrap();
    let mut checked = 0;
    for (name, expected) in tables {
        let actual = reader_rows(&fixture.db, name)
            .unwrap_or_else(|| panic!("no reader covers primary table {name}"));
        let (schema, table) = name.split_once('.').unwrap();
        assert_eq!(
            fixture.db.count_rows(schema, table).unwrap(),
            expected["count"].as_u64().unwrap(),
            "{name} count"
        );
        let expected_rows: Vec<Vec<Value>> =
            serde_json::from_value(expected["rows"].clone()).unwrap();
        if name.ends_with("json_dumps")
            || name.ends_with("json_dumps_named")
            || name.ends_with("json_dumps_hashed")
            || name.ends_with(".options")
        {
            continue;
        }
        assert_eq!(actual, expected_rows, "{name}");
        checked += 1;
    }
    assert!(checked > 100, "only {checked} tables checked");
}

#[test]
fn tag_text_matches_definitions() {
    let fixture = basic();
    let tables = &fixture.expected["tables"];
    let lookup = |table: &str| -> std::collections::HashMap<u64, String> {
        tables[table]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r[0].as_u64().unwrap(), r[1].as_str().unwrap().to_owned()))
            .collect()
    };
    let namespaces = lookup("external_master.namespaces");
    let subtags = lookup("external_master.subtags");
    let definitions = tables["external_master.tags"]["rows"].as_array().unwrap();
    let tags = collect(fixture.db.tags());
    assert_eq!(tags.len(), definitions.len());
    for ((tag_id, tag), def) in tags.iter().zip(definitions) {
        assert_eq!(u64::from(tag_id.get()), def[0].as_u64().unwrap());
        let namespace = &namespaces[&def[1].as_u64().unwrap()];
        let subtag = &subtags[&def[2].as_u64().unwrap()];
        assert_eq!(tag.split(), (namespace.as_str(), subtag.as_str()));
    }
    // the reference stores ":)" as namespace "" and subtag ":)", shown as "::)"
    assert!(tags.iter().any(|(_, t)| t.as_str() == "::)"));
}
