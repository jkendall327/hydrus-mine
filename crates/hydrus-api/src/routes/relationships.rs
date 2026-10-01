//! File relationships: duplicates, alternates and potential pairs.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::service::builtin_keys;
use hydrus_core::{DuplicateType, HashId, ServiceId, ServiceKey, Sha256};
use hydrus_search::{FileSearchContext, LocationContext, TagContext, parse_api_search};
use hydrus_store::delete_lock::Reinbox;
use hydrus_store::duplicates::{
    self, DuplicateFilterSettings, DuplicateMergeSettings, FileScope, PairDecision, PairOrder,
    PairRelationship, PairSearchKind, PairSelection, PixelDuplicates, PotentialsSearch,
    RelationshipWriter,
};
use hydrus_store::{Snapshot, master, settings};
use rusqlite::Connection;

use crate::AppState;
use crate::auth::Permission;
use crate::domains::{FileDomain, parse_file_domain, parse_tag_service};
use crate::error::{ApiError, ApiResult};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;
use crate::routes::search::search_error;
use hydrus_duplicates::potentials::PotentialsQuery;

fn scope_of(snapshot: &Snapshot, domain: FileDomain) -> FileScope {
    if domain.is_all_known_files(&snapshot.services) {
        FileScope::AllKnownFiles
    } else {
        FileScope::Domains {
            current: domain.current,
            deleted: domain.deleted,
        }
    }
}

fn parse_scope(snapshot: &Snapshot, params: &Params) -> ApiResult<FileScope> {
    let domain = parse_file_domain(
        &snapshot.services,
        params,
        builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
        true,
    )?;
    Ok(scope_of(snapshot, domain))
}

pub async fn get_file_relationships(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let scope = parse_scope(&snapshot, &params)?;
            let hashes = parse_hashes(app, &params)?.ok_or_else(|| {
                ApiError::bad_request(
                    "Please include some files in your request--file_id or hash based!",
                )
            })?;
            let local_storage = snapshot
                .services
                .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)?
                .id;
            let body = app.store.read(|conn| {
                let ids = master::hash_ids(conn, &hashes)?;
                let mut out = Map::new();
                let mut names: HashMap<HashId, Sha256> = HashMap::new();
                for hash in &hashes {
                    let entry = match ids.get(hash) {
                        // a file the client has never seen is its own king, nowhere
                        None => relationships_json(
                            hash,
                            &duplicates::FileRelationships {
                                king: HashId(0),
                                king_in_scope: matches!(scope, FileScope::AllKnownFiles),
                                king_is_local: false,
                                potentials: Vec::new(),
                                false_positives: Vec::new(),
                                alternates: Vec::new(),
                                duplicates: Vec::new(),
                            },
                            |_| Some(*hash),
                        ),
                        Some(&id) => {
                            let r =
                                duplicates::file_relationships(conn, &scope, local_storage, id)?;
                            let mut wanted: Vec<HashId> = vec![r.king];
                            for list in [
                                &r.potentials,
                                &r.false_positives,
                                &r.alternates,
                                &r.duplicates,
                            ] {
                                wanted.extend(list.iter().copied());
                            }
                            wanted.retain(|h| !names.contains_key(h));
                            names.extend(master::hashes(conn, &wanted)?);
                            relationships_json(hash, &r, |h| names.get(&h).copied())
                        }
                    };
                    out.insert(hash.to_hex(), entry);
                }
                Ok(out)
            })?;
            Ok(ApiResponse::Json(
                json!({ "file_relationships": body }),
                encoding,
            ))
        })
        .await
}

fn relationships_json(
    hash: &Sha256,
    r: &duplicates::FileRelationships,
    name: impl Fn(HashId) -> Option<Sha256>,
) -> Json {
    let king = name(r.king).unwrap_or(*hash);
    let list = |ids: &[HashId]| -> Vec<String> {
        ids.iter()
            .filter_map(|&h| name(h))
            .map(|h| h.to_hex())
            .collect()
    };
    let mut entry = Map::new();
    entry.insert("is_king".into(), json!(king == *hash));
    entry.insert("king".into(), json!(king.to_hex()));
    entry.insert("king_is_on_file_domain".into(), json!(r.king_in_scope));
    entry.insert("king_is_local".into(), json!(r.king_is_local));
    for (kind, ids) in [
        (DuplicateType::Potential, &r.potentials),
        (DuplicateType::FalsePositive, &r.false_positives),
        (DuplicateType::Alternate, &r.alternates),
        (DuplicateType::Member, &r.duplicates),
    ] {
        entry.insert(kind.code().to_string(), json!(list(ids)));
    }
    Json::Object(entry)
}

/// An integer parameter that must be one of an enum's codes.
fn code_param<T>(
    params: &Params,
    name: &str,
    default: T,
    from_code: impl Fn(i64) -> Option<T>,
) -> ApiResult<T> {
    match params.optional::<i64>(name)? {
        None => Ok(default),
        Some(code) => from_code(code)
            .ok_or_else(|| ApiError::bad_request(format!("\"{name}\" cannot be {code}!"))),
    }
}

/// A parsed potential-duplicates search.
/// A potentials search, parsed from a request.
struct ParsedPotentials(PotentialsQuery);

impl ParsedPotentials {
    /// Run `f` on the search, inside one read. A bad file search is a 400.
    fn with_search<T>(
        &self,
        app: &AppState,
        snapshot: &Snapshot,
        f: impl FnOnce(&Connection, &PotentialsSearch<'_>) -> hydrus_store::Result<T>,
    ) -> ApiResult<T> {
        app.store
            .read(|conn| self.0.with_search(conn, snapshot, |search| f(conn, search)))?
            .map_err(search_error)
    }
}

fn parse_potentials(snapshot: &Snapshot, params: &Params) -> ApiResult<ParsedPotentials> {
    let domain = parse_file_domain(
        &snapshot.services,
        params,
        builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
        true,
    )?;
    let key =
        |id: ServiceId| -> ApiResult<ServiceKey> { Ok(snapshot.services.get(id)?.key.clone()) };
    let location = LocationContext::new(
        domain
            .current
            .iter()
            .map(|&id| key(id))
            .collect::<ApiResult<Vec<_>>>()?,
        domain
            .deleted
            .iter()
            .map(|&id| key(id))
            .collect::<ApiResult<Vec<_>>>()?,
    );
    let scope = scope_of(snapshot, domain);
    let mut searches = Vec::new();
    for (tags, service) in [
        ("tags_1", "tag_service_key_1"),
        ("tags_2", "tag_service_key_2"),
    ] {
        let tag_service = match parse_tag_service(&snapshot.services, params, service)? {
            Some(id) => key(id)?,
            None => ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()),
        };
        let predicates = match params.optional::<Json>(tags)? {
            None => Vec::new(),
            Some(list) => {
                parse_api_search(&list).map_err(|e| ApiError::bad_request(e.to_string()))?
            }
        };
        searches.push(FileSearchContext {
            location: location.clone(),
            tags: TagContext::new(tag_service, true, true),
            predicates,
        });
    }
    let search_2 = searches.pop().expect("two searches");
    let search_1 = searches.pop().expect("two searches");
    let kind = code_param(
        params,
        "potentials_search_type",
        PairSearchKind::OneFileMatchesOneSearch,
        |c| match c {
            0 => Some(PairSearchKind::OneFileMatchesOneSearch),
            1 => Some(PairSearchKind::BothFilesMatchOneSearch),
            2 => Some(PairSearchKind::BothFilesMatchDifferentSearches),
            _ => None,
        },
    )?;
    let pixel_duplicates =
        code_param(
            params,
            "pixel_duplicates",
            PixelDuplicates::Allowed,
            |c| match c {
                0 => Some(PixelDuplicates::Required),
                1 => Some(PixelDuplicates::Allowed),
                2 => Some(PixelDuplicates::Excluded),
                _ => None,
            },
        )?;
    let max_hamming_distance =
        code_param(params, "max_hamming_distance", 4, |c| u32::try_from(c).ok())?;
    Ok(ParsedPotentials(PotentialsQuery {
        scope,
        kind,
        pixel_duplicates,
        max_hamming_distance,
        search_1,
        search_2,
    }))
}

pub async fn get_potentials_count(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let parsed = parse_potentials(&snapshot, &params)?;
            let count = parsed.with_search(app, &snapshot, |conn, search| {
                Ok(duplicates::potential_pairs(conn, &snapshot, search)?.len())
            })?;
            Ok(ApiResponse::Json(
                json!({ "potential_duplicates_count": count }),
                encoding,
            ))
        })
        .await
}

pub async fn get_potential_pairs(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let parsed = parse_potentials(&snapshot, &params)?;
            let order = code_param(
                &params,
                "duplicate_pair_sort_type",
                PairOrder::MaxFilesize,
                |c| match c {
                    0 => Some(PairOrder::MaxFilesize),
                    1 => Some(PairOrder::Similarity),
                    2 => Some(PairOrder::MinFilesize),
                    3 => Some(PairOrder::Random),
                    _ => None,
                },
            )?;
            let ascending = params.or("duplicate_pair_sort_asc", false)?;
            let group_mode = params.or("group_mode", false)?;
            let max_num_pairs = params.optional::<i64>("max_num_pairs")?;
            let pairs = parsed.with_search(app, &snapshot, |conn, search| {
                let selection = if group_mode {
                    PairSelection::Group
                } else {
                    let max = match max_num_pairs {
                        Some(n) => usize::try_from(n).unwrap_or(0),
                        None => {
                            settings::get::<DuplicateFilterSettings>(conn)?.max_batch_size as usize
                        }
                    };
                    PairSelection::Batch { max }
                };
                let pairs =
                    duplicates::select_pairs(conn, &snapshot, search, order, ascending, selection)?;
                let scores = settings::get::<DuplicateFilterSettings>(conn)?.scores;
                let pairs =
                    hydrus_duplicates::statements::ab_order(conn, &snapshot, pairs, &scores)?;
                let ids: Vec<HashId> = pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
                let hashes = master::hashes(conn, &ids)?;
                Ok(pairs
                    .into_iter()
                    .filter_map(|(a, b)| {
                        Some(json!([hashes.get(&a)?.to_hex(), hashes.get(&b)?.to_hex()]))
                    })
                    .collect::<Vec<Json>>())
            })?;
            Ok(ApiResponse::Json(
                json!({ "potential_duplicate_pairs": pairs }),
                encoding,
            ))
        })
        .await
}

pub async fn get_random_potentials(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    let encoding = req.response_encoding;
    app.clone()
        .blocking(move |app| {
            let snapshot = app.store.snapshot();
            let parsed = parse_potentials(&snapshot, &params)?;
            let hashes = parsed.with_search(app, &snapshot, |conn, search| {
                let group = duplicates::random_potential_group(conn, &snapshot, search)?;
                let hashes = master::hashes(conn, &group)?;
                Ok(group
                    .iter()
                    .filter_map(|id| Some(hashes.get(id)?.to_hex()))
                    .collect::<Vec<String>>())
            })?;
            Ok(ApiResponse::Json(
                json!({ "random_potential_duplicate_hashes": hashes }),
                encoding,
            ))
        })
        .await
}

// writing ---------------------------------------------------------------------

/// One pair of `set_file_relationships`, with WORSE already turned round.
struct PairRow {
    relationship: PairRelationship,
    a: Sha256,
    b: Sha256,
    merge: bool,
    delete_a: bool,
    delete_b: bool,
}

fn dict_field<T: serde::de::DeserializeOwned>(
    dict: &Map<String, Json>,
    key: &str,
    default: Option<T>,
) -> ApiResult<T> {
    match dict.get(key) {
        Some(value) if !value.is_null() => serde_json::from_value(value.clone()).map_err(|_| {
            ApiError::bad_request(format!(
                "The parameter \"{key}\", with value \"{value}\", was not the expected type!"
            ))
        }),
        _ => default.ok_or_else(|| {
            ApiError::bad_request(format!("The required parameter \"{key}\" was missing!"))
        }),
    }
}

fn parse_pair(raw: &Json) -> ApiResult<PairRow> {
    let dict = raw.as_object().ok_or_else(|| {
        ApiError::bad_request(format!(
            "The list parameter \"relationships\" held an item, \"{raw}\" that was not the expected type: Object!"
        ))
    })?;
    let code: i64 = dict_field(dict, "relationship", None)?;
    let hash_a: String = dict_field(dict, "hash_a", None)?;
    let hash_b: String = dict_field(dict, "hash_b", None)?;
    let merge: bool = dict_field(dict, "do_default_content_merge", None)?;
    let delete_a: bool = dict_field(dict, "delete_a", Some(false))?;
    let delete_b: bool = dict_field(dict, "delete_b", Some(false))?;
    let kind = u8::try_from(code).ok().and_then(DuplicateType::from_code);
    let relationship = match kind {
        Some(DuplicateType::FalsePositive) => PairRelationship::FalsePositive,
        Some(DuplicateType::Alternate) => PairRelationship::Alternate,
        Some(DuplicateType::Better | DuplicateType::Worse) => PairRelationship::Better,
        Some(DuplicateType::SameQuality) => PairRelationship::SameQuality,
        Some(DuplicateType::Potential) => PairRelationship::Potential,
        _ => {
            return Err(ApiError::bad_request(format!(
                "The parameter \"relationship\", with value \"{code}\", was not in the allowed values: {{0, 1, 2, 3, 4, 7}}!"
            )));
        }
    };
    let parse = |text: &str| -> ApiResult<Sha256> {
        let bytes = hex::decode(text).map_err(|_| {
            ApiError::bad_request(format!(
                "Sorry, did not understand one of the hashes {hash_a} or {hash_b}!"
            ))
        })?;
        Sha256::from_slice(&bytes).map_err(|_| {
            ApiError::bad_request(format!(
                "Sorry, one of the given hashes was the wrong length! sha256 hashes should be 32 bytes long, but {text} is {} bytes long!",
                bytes.len()
            ))
        })
    };
    let (a, b) = (parse(&hash_a)?, parse(&hash_b)?);
    Ok(if kind == Some(DuplicateType::Worse) {
        PairRow {
            relationship,
            a: b,
            b: a,
            merge,
            delete_a: delete_b,
            delete_b: delete_a,
        }
    } else {
        PairRow {
            relationship,
            a,
            b,
            merge,
            delete_a,
            delete_b,
        }
    })
}

/// The files a request names; there must be some.
fn required_hashes(app: &AppState, params: &Params) -> ApiResult<Vec<Sha256>> {
    parse_hashes(app, params)?.ok_or_else(|| {
        ApiError::bad_request("Please include some files in your request--file_id or hash based!")
    })
}

pub async fn set_file_relationships(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let raw: Vec<Json> = req.params.or("relationships", Vec::new())?;
    let rows = raw.iter().map(parse_pair).collect::<ApiResult<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(ApiResponse::Empty);
    }
    app.blocking(move |app| {
        app.store.write_content(move |w| {
            let merge_settings: DuplicateMergeSettings = settings::get(w.conn())?;
            for row in &rows {
                let a = master::intern_hash(w.conn(), &row.a)?;
                let b = master::intern_hash(w.conn(), &row.b)?;
                let merge = if row.merge {
                    merge_settings.for_relationship(row.relationship)
                } else {
                    None
                };
                duplicates::apply_decision(
                    w,
                    &PairDecision {
                        relationship: row.relationship,
                        a,
                        b,
                        merge,
                        delete_a: row.delete_a,
                        delete_b: row.delete_b,
                        deletion_reason: "From Client API (duplicates processing).",
                        // (only the merge path inboxes, as in the reference)
                        reinbox: if row.merge {
                            Reinbox::AfterDuplicateFilter
                        } else {
                            Reinbox::Never
                        },
                    },
                )?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

pub async fn set_kings(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let hashes = required_hashes(app, &params)?;
        app.store.write_content(move |w| {
            let local_storage = w.roles().local_file_storage;
            for hash in &hashes {
                let id = master::intern_hash(w.conn(), hash)?;
                RelationshipWriter::new(w.conn(), local_storage).set_king(id)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}

pub async fn remove_potentials(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?
        .check(Permission::ManageFileRelationships)?;
    let params = req.params.clone();
    app.blocking(move |app| {
        let hashes = required_hashes(app, &params)?;
        app.store.write_content(move |w| {
            let local_storage = w.roles().local_file_storage;
            let ids = master::hash_ids(w.conn(), &hashes)?;
            for id in ids.into_values() {
                RelationshipWriter::new(w.conn(), local_storage).remove_potentials(id)?;
            }
            Ok(())
        })?;
        Ok(())
    })
    .await?;
    Ok(ApiResponse::Empty)
}
