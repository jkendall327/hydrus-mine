//! Reading file relationships: duplicates, alternates and potential pairs.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::service::builtin_keys;
use hydrus_core::{DuplicateType, HashId, Sha256};
use hydrus_search::parse_api_search;
use hydrus_store::duplicates::{
    self, DuplicateFilterSettings, FileScope, PairOrder, PairSearchKind, PairSelection,
    PixelDuplicates, PotentialsSearch,
};
use hydrus_store::{Snapshot, master, settings};

use crate::AppState;
use crate::auth::Permission;
use crate::domains::{FileDomain, parse_file_domain, parse_tag_service};
use crate::error::{ApiError, ApiResult};
use crate::file_filter::{OwnedFileFilter, interim_filter};
use crate::params::Params;
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

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

/// A parsed potential-duplicates search, owning its file filters.
struct ParsedPotentials {
    scope: FileScope,
    kind: PairSearchKind,
    pixel_duplicates: PixelDuplicates,
    max_hamming_distance: u32,
    filter_1: Option<OwnedFileFilter>,
    filter_2: Option<OwnedFileFilter>,
}

impl ParsedPotentials {
    fn search(&self) -> PotentialsSearch<'_> {
        PotentialsSearch {
            scope: self.scope.clone(),
            kind: self.kind,
            pixel_duplicates: self.pixel_duplicates,
            max_hamming_distance: self.max_hamming_distance,
            search_1: self.filter_1.as_deref(),
            search_2: self.filter_2.as_deref(),
        }
    }
}

fn parse_potentials(snapshot: &Arc<Snapshot>, params: &Params) -> ApiResult<ParsedPotentials> {
    let scope = parse_scope(snapshot, params)?;
    let mut filters = Vec::new();
    for (tags, service) in [
        ("tags_1", "tag_service_key_1"),
        ("tags_2", "tag_service_key_2"),
    ] {
        let tag_service = parse_tag_service(&snapshot.services, params, service)?;
        let predicates = match params.optional::<Json>(tags)? {
            None => Vec::new(),
            Some(list) => {
                parse_api_search(&list).map_err(|e| ApiError::bad_request(e.to_string()))?
            }
        };
        filters.push(interim_filter(
            Arc::clone(snapshot),
            predicates,
            tag_service,
        )?);
    }
    let filter_2 = filters.pop().flatten();
    let filter_1 = filters.pop().flatten();
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
    Ok(ParsedPotentials {
        scope,
        kind,
        pixel_duplicates,
        max_hamming_distance,
        filter_1,
        filter_2,
    })
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
            let count = app
                .store
                .read(|conn| Ok(duplicates::potential_pairs(conn, &parsed.search())?.len()))?;
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
            let pairs = app.store.read(|conn| {
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
                    duplicates::select_pairs(conn, &parsed.search(), order, ascending, selection)?;
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
