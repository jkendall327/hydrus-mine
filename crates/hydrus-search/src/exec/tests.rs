//! Executor tests against the reference fixture database.
//!
//! The main check is a property test: random searches (tags, negations,
//! namespaces, wildcards, OR groups and system predicates, over random file
//! and tag domains) are run by the executor under every planner strategy
//! and compared with a naive model that decides each file on its own, from
//! everything `hydrus_store::media` loads about it. The model computes
//! displayed tags forwards (stored tag -> ideal -> ancestors), while the
//! executor works backwards (displayed tag -> the stored tags that show as
//! it) through the indexes, so agreement checks the translation as well as
//! the planning.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use proptest::prelude::*;
use serde_json::{Value as Json, json};

use hydrus_core::service::builtin_keys;
use hydrus_core::{ContentStatus, HashId, ServiceKey, Sha256, Tag, TagId};
use hydrus_store::media::{self, FileFlags, MediaBatch, MediaResult};
use hydrus_store::{Snapshot, Store};

use super::context::Strategy as Planner;
use super::sort::{FileSort, SortBy, SortOrder};
use super::tags::{NamespacePattern, SubtagPattern};
use super::{Clock, search_with_strategy};
use crate::context::{FileSearchContext, LocationContext, TagContext};
use crate::filetype::FiletypeSet;
use crate::number::{Comparison, NumberOp};
use crate::predicate::{
    FileHashes, FileProperty, NamespaceFilter, NumericProperty, Predicate, SystemPredicate,
    UrlRule, Wildcard,
};
use crate::{api, parse_api_search};

struct Fixture {
    _legacy: tempfile::TempDir,
    _native: tempfile::TempDir,
    store: Arc<Store>,
}

fn import_fixture() -> Fixture {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    Fixture {
        _legacy: legacy,
        _native: native,
        store,
    }
}

static SHARED: LazyLock<Fixture> = LazyLock::new(import_fixture);

fn key(bytes: &[u8]) -> ServiceKey {
    ServiceKey::new(bytes.to_vec())
}

fn run(
    store: &Store,
    search: &FileSearchContext,
    sort: FileSort,
    strategy: Planner,
) -> Vec<HashId> {
    let snapshot = store.snapshot();
    let clock = Clock::system();
    store
        .read(|conn| {
            Ok(search_with_strategy(
                conn, &snapshot, search, sort, &clock, strategy,
            ))
        })
        .unwrap()
        .unwrap_or_else(|e| panic!("search failed: {e}"))
}

// the naive model -----------------------------------------------------------

/// Everything about every file, and the display rules.
struct Model {
    snapshot: Arc<Snapshot>,
    batch: MediaBatch,
    tag_ids: HashMap<Tag, TagId>,
}

impl Model {
    fn load(store: &Store) -> Model {
        let snapshot = store.snapshot();
        let batch = store
            .read(|c| {
                let ids: Vec<HashId> = c
                    .prepare("SELECT hash_id FROM hashes ORDER BY hash_id")?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                media::load(c, &snapshot.services, Some(&snapshot.display), &ids)
            })
            .unwrap();
        let all_tags: Vec<(TagId, Tag)> = store
            .read(|c| {
                let ids: Vec<TagId> = c
                    .prepare("SELECT tag_id FROM tags")?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(hydrus_store::master::tags(c, &ids)?.into_iter().collect())
            })
            .unwrap();
        Model {
            snapshot,
            batch,
            tag_ids: all_tags.into_iter().map(|(id, t)| (t, id)).collect(),
        }
    }

    fn is_local(&self, m: &MediaResult) -> bool {
        m.is_current_in(self.service_id(&key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)))
    }

    fn tag(&self, id: TagId) -> &Tag {
        &self.batch.tags[&id]
    }

    fn service_id(&self, key: &ServiceKey) -> hydrus_core::ServiceId {
        self.snapshot.services.by_key(key).unwrap().id
    }

    /// Displayed tags of a file per searched service.
    fn displayed(&self, m: &MediaResult, tags: &TagContext) -> Vec<BTreeSet<TagId>> {
        let services: Vec<hydrus_core::ServiceId> = if tags.is_all_known_tags() {
            self.snapshot
                .services
                .tag_services()
                .map(|s| s.id)
                .collect()
        } else {
            vec![self.service_id(&tags.service)]
        };
        let mut statuses = Vec::new();
        if tags.include_current {
            statuses.push(ContentStatus::Current);
        }
        if tags.include_pending {
            statuses.push(ContentStatus::Pending);
        }
        services
            .into_iter()
            .map(|s| {
                let graph = self.snapshot.display.get(s);
                let mut out = BTreeSet::new();
                if let Some(service_tags) = m.tags.get(&s) {
                    for status in &statuses {
                        for &stored in service_tags.by_status.get(status).into_iter().flatten() {
                            out.extend(graph.display_tags(stored));
                        }
                    }
                }
                out
            })
            .collect()
    }

    fn in_domain(&self, m: &MediaResult, search: &FileSearchContext) -> bool {
        if search.location.is_all_known_files() {
            return self
                .displayed(m, &search.tags)
                .iter()
                .any(|d| !d.is_empty());
        }
        search
            .location
            .current()
            .iter()
            .any(|k| m.is_current_in(self.service_id(k)))
            || search
                .location
                .deleted()
                .iter()
                .any(|k| m.is_deleted_from(self.service_id(k)))
    }

    fn matches(&self, m: &MediaResult, p: &Predicate, tags: &TagContext) -> bool {
        let displayed = || self.displayed(m, tags);
        let services = || -> Vec<hydrus_core::ServiceId> {
            if tags.is_all_known_tags() {
                self.snapshot
                    .services
                    .tag_services()
                    .map(|s| s.id)
                    .collect()
            } else {
                vec![self.service_id(&tags.service)]
            }
        };
        match p {
            Predicate::Tag { tag, inclusive } => {
                let found = self.tag_ids.get(tag).is_some_and(|&id| {
                    services()
                        .iter()
                        .zip(displayed())
                        .any(|(s, d)| d.contains(&self.snapshot.display.get(*s).ideal(id)))
                });
                found == *inclusive
            }
            Predicate::Namespace {
                namespace,
                inclusive,
            } => {
                let pattern = NamespacePattern::parse(namespace);
                let found = displayed()
                    .iter()
                    .flatten()
                    .any(|d| namespace_matches(&pattern, self.tag(*d).namespace()));
                found == *inclusive
            }
            Predicate::Wildcard { pattern, inclusive } => {
                let ns = NamespacePattern::parse(pattern.namespace());
                let sub = SubtagPattern::parse(pattern.subtag());
                let found = displayed().iter().flatten().any(|d| {
                    let tag = self.tag(*d);
                    namespace_matches(&ns, tag.namespace()) && sub.matches(tag.subtag())
                });
                found == *inclusive
            }
            Predicate::Or(terms) => terms.iter().any(|t| self.matches(m, t, tags)),
            Predicate::System(s) => self.system(m, s, tags),
        }
    }

    fn system(&self, m: &MediaResult, p: &SystemPredicate, tags: &TagContext) -> bool {
        let info = m.info.as_ref();
        match p {
            SystemPredicate::Everything | SystemPredicate::Limit(_) => true,
            SystemPredicate::Inbox => m.inbox,
            SystemPredicate::Local => self.is_local(m),
            SystemPredicate::NotLocal => !self.is_local(m),
            // archived files are local files that are not in the inbox
            SystemPredicate::Archive => !m.inbox && self.is_local(m),
            SystemPredicate::Number { property, test } => {
                let Some(info) = info else { return false };
                let value = match property {
                    NumericProperty::Width => info.width.map(u64::from),
                    NumericProperty::Height => info.height.map(u64::from),
                    NumericProperty::Duration => info.duration_ms,
                    NumericProperty::NumWords => info.num_words,
                    other => panic!("model does not know {other:?}"),
                };
                super::numbers::Counts::from_number_test(*test).contains(value.unwrap_or(0))
            }
            SystemPredicate::FileProperty { property, has } => match property {
                FileProperty::Audio => info.is_some_and(|i| i.has_audio == *has),
                FileProperty::Exif => info.is_some_and(|i| i.flags.has(FileFlags::EXIF)) == *has,
                other => panic!("model does not know {other:?}"),
            },
            SystemPredicate::NumTags {
                namespace,
                op,
                count,
            } => {
                let pattern = match namespace {
                    NamespaceFilter::Any => NamespacePattern::Any,
                    NamespaceFilter::Unnamespaced => NamespacePattern::Exact(String::new()),
                    NamespaceFilter::Namespace(ns) => NamespacePattern::parse(ns),
                };
                let n = self
                    .displayed(m, tags)
                    .into_iter()
                    .flatten()
                    .filter(|d| namespace_matches(&pattern, self.tag(*d).namespace()))
                    .collect::<HashSet<_>>()
                    .len() as u64;
                match op {
                    Comparison::Less => n < *count,
                    Comparison::Greater => n > *count,
                    Comparison::Equal => n == *count,
                    Comparison::NotEqual => n != *count,
                    Comparison::Approx => {
                        let (lo, hi) = (*count as f64 * 0.85, *count as f64 * 1.15);
                        if lo <= 0.0 {
                            n as f64 <= hi
                        } else {
                            lo <= n as f64 && n as f64 <= hi
                        }
                    }
                }
            }
            SystemPredicate::FileSize { op, size, unit } => {
                let Some(info) = info else { return false };
                let bytes = size * unit.bytes();
                match op {
                    Comparison::Less => info.size < bytes,
                    Comparison::Greater => info.size > bytes,
                    other => panic!("model does not know {other:?}"),
                }
            }
            SystemPredicate::Filetype {
                filetypes,
                inclusive,
            } => {
                let Some(info) = info else { return false };
                let listed = filetypes.specific_mimes().contains(&info.mime);
                crate::filetype::is_searchable(info.mime) && listed == *inclusive
            }
            SystemPredicate::Hash { hashes, inclusive } => {
                let FileHashes::Sha256(hashes) = hashes else {
                    panic!("model only knows sha256")
                };
                hashes.contains(&m.hash) == *inclusive
            }
            SystemPredicate::KnownUrl {
                rule: UrlRule::Domain(domain),
                has,
            } => {
                let found = m.urls.iter().any(|u| {
                    let d = hydrus_store::master::url_domain(u);
                    d == domain || d.ends_with(&format!(".{domain}"))
                });
                found == *has
            }
            SystemPredicate::NoteName { name, has } => {
                m.notes.iter().any(|(n, _)| n == name) == *has
            }
            other => panic!("model does not know {other:?}"),
        }
    }

    /// The files a search should find, in id order.
    fn search(&self, search: &FileSearchContext) -> Vec<HashId> {
        self.batch
            .results
            .iter()
            .filter(|m| self.in_domain(m, search))
            .filter(|m| {
                search
                    .predicates
                    .iter()
                    .all(|p| self.matches(m, p, &search.tags))
            })
            .map(|m| m.hash_id)
            .collect()
    }
}

fn namespace_matches(pattern: &NamespacePattern, namespace: &str) -> bool {
    match pattern {
        NamespacePattern::Any => true,
        NamespacePattern::Exact(ns) => ns == namespace,
        NamespacePattern::Like(like) => super::tags::like(&like.replace('*', "%"), namespace),
    }
}

// the property test ------------------------------------------------------------

/// Search terms the model understands, in Client API syntax.
const TERMS: &[&str] = &[
    "blue eyes",
    "bleu eyes",
    "blue_eyes",
    "samus",
    "character:samus aran",
    "series:metroid",
    "series:the legend of zelda",
    "studio:nintendo",
    "character:link",
    "safe",
    "smile",
    "explicit",
    "source:fixture",
    "second:tag",
    "second:canonical",
    "meta:lowres",
    "meta:low resolution",
    "page:10",
    "::)",
    "no such tag",
    "character:*",
    "series:*",
    "studio:*",
    "page:*",
    "*",
    "*:*",
    "nonexistent namespace:*",
    "*eyes",
    "blue*",
    "*:sam*",
    "b*e*",
    "*:blue_eyes",
    "series:*zelda",
    "*:*d*",
    "s*",
    "system:everything",
    "system:inbox",
    "system:archive",
    "system:width > 500",
    "system:height < 200",
    "system:has audio",
    "system:no audio",
    "system:has duration",
    "system:no duration",
    "system:has tags",
    "system:no tags",
    "system:number of tags > 3",
    "system:number of tags < 5",
    "system:number of tags ~= 4",
    "system:number of character tags > 0",
    "system:number of series tags = 1",
    "system:number of unnamespaced tags ~= 3",
    "system:filesize > 10 kilobytes",
    "system:filesize < 5 KB",
    "system:filetype = image/png, apng",
    "system:filetype = image",
    "system:has domain gelbooru.com",
    "system:does not have domain gelbooru.com",
    "system:has note with name comment",
    "system:no note with name comment",
    "system:has exif",
    "system:no exif",
];

fn term() -> impl Strategy<Value = Json> {
    (0..TERMS.len(), any::<bool>()).prop_map(|(i, negate)| {
        let term = TERMS[i];
        if negate && !term.starts_with("system:") {
            json!(format!("-{term}"))
        } else {
            json!(term)
        }
    })
}

fn item() -> impl Strategy<Value = Json> {
    prop_oneof![
        3 => term(),
        1 => proptest::collection::vec(term(), 1..4).prop_map(Json::Array),
        1 => (term(), proptest::collection::vec(term(), 1..3))
            .prop_map(|(a, inner)| json!([a, Json::Array(inner)])),
    ]
}

fn location() -> impl Strategy<Value = LocationContext> {
    prop_oneof![
        Just(LocationContext::default()),
        Just(LocationContext::single(key(builtin_keys::MY_FILES))),
        Just(LocationContext::single(key(builtin_keys::TRASH))),
        Just(LocationContext::single(key(
            builtin_keys::HYDRUS_LOCAL_FILE_STORAGE
        ))),
        Just(LocationContext::new([], [key(builtin_keys::MY_FILES)])),
        Just(LocationContext::new(
            [key(builtin_keys::MY_FILES), key(builtin_keys::TRASH)],
            []
        )),
        Just(LocationContext::single(key(builtin_keys::COMBINED_FILE))),
    ]
}

fn tag_context() -> impl Strategy<Value = TagContext> {
    let second_tags: Vec<u8> = hex_key("5ec0");
    (
        prop_oneof![
            Just(key(builtin_keys::COMBINED_TAG)),
            Just(key(builtin_keys::MY_TAGS)),
            Just(key(builtin_keys::DOWNLOADER_TAGS)),
            Just(ServiceKey::new(second_tags)),
        ],
        prop_oneof![4 => Just(true), 1 => Just(false)],
    )
        .prop_map(|(service, current)| TagContext::new(service, current, true))
}

/// A fixture service key made of a repeated two-byte pattern.
fn hex_key(pattern: &str) -> Vec<u8> {
    hex::decode(pattern.repeat(16)).unwrap()
}

static MODEL: LazyLock<Model> = LazyLock::new(|| Model::load(&SHARED.store));

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn executor_agrees_with_the_naive_model(
        items in proptest::collection::vec(item(), 1..4),
        location in location(),
        tags in tag_context(),
    ) {
        // "all known tags" over "all known files" is refused by the API
        prop_assume!(!(location.is_all_known_files() && tags.is_all_known_tags()));
        let predicates = parse_api_search(&Json::Array(items.clone())).unwrap();
        let search = FileSearchContext { location, tags, predicates };
        let expected = MODEL.search(&search);
        let sort = FileSort { by: SortBy::NumCollectionFiles, order: SortOrder::Ascending };
        for strategy in [Planner::Auto, Planner::AlwaysProbe, Planner::AlwaysScan] {
            let got = run(&SHARED.store, &search, sort, strategy);
            prop_assert_eq!(&got, &expected, "{:?} with {:?}", items, strategy);
        }
    }
}

#[test]
fn every_term_is_understood_by_the_model() {
    // the model panics on predicates it does not know; exercise each once
    for term in TERMS {
        let predicates = parse_api_search(&json!([term])).unwrap();
        let search = FileSearchContext {
            predicates,
            ..FileSearchContext::default()
        };
        let expected = MODEL.search(&search);
        let sort = FileSort {
            by: SortBy::NumCollectionFiles,
            order: SortOrder::Ascending,
        };
        assert_eq!(
            run(&SHARED.store, &search, sort, Planner::Auto),
            expected,
            "{term}"
        );
    }
}

// the reference's recorded states after writes ------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A recorded search step and the file ids the reference returned.
fn recorded_search(scenario: &str, step: usize) -> (FileSearchContext, FileSort, Vec<HashId>) {
    let read = |dir: &str| -> Json {
        serde_json::from_str(
            &std::fs::read_to_string(repo_root().join(format!("oracle/{dir}/{scenario}.json")))
                .unwrap(),
        )
        .unwrap()
    };
    let steps = read("scenarios");
    let responses = read("recordings");
    let step_json = &steps["steps"][step];
    assert_eq!(step_json["path"], "/get_files/search_files");
    let query: HashMap<&str, &str> = step_json["query"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap(), p[1].as_str().unwrap()))
        .collect();
    let tags: Json = serde_json::from_str(query["tags"]).unwrap();
    let location = match query.get("file_service_key") {
        Some(k) => LocationContext::single(ServiceKey::new(hex::decode(k).unwrap())),
        None => LocationContext::default(),
    };
    let search = FileSearchContext {
        location,
        tags: TagContext::default(),
        predicates: api::parse_api_search(&tags).unwrap(),
    };
    let sort = FileSort {
        by: SortBy::from_code(query["file_sort_type"].parse().unwrap()).unwrap(),
        order: SortOrder::Ascending,
    };
    let expected = responses["responses"][step]["json"]["file_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| HashId(u32::try_from(v.as_u64().unwrap()).unwrap()))
        .collect();
    (search, sort, expected)
}

fn hash_id_of(store: &Store, name: &str) -> HashId {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hex = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let hash: Sha256 = hex.parse().unwrap();
    store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap()
}

/// The searches of the `add_tags` scenario, after the tags it adds (the
/// write endpoints are tested elsewhere; this applies the same change
/// directly and checks search sees it as the reference did).
#[test]
fn searches_see_added_tags_as_the_reference_did() {
    let fixture = import_fixture();
    let store = &fixture.store;
    let file = hash_id_of(store, "jpeg_00.jpg");
    let my_tags = store
        .snapshot()
        .services
        .builtin(builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    store
        .write(move |ctx| {
            let table = hydrus_store::schema::MappingTables::new(my_tags).current;
            for raw in ["new tag", "Blue_Eyes", "character:link", "  spaced  out  "] {
                let tag = Tag::new(raw).unwrap();
                let id = hydrus_store::master::intern_tag(ctx.conn(), &tag)?;
                ctx.conn().execute(
                    &format!("INSERT OR IGNORE INTO {table} (tag_id, hash_id) VALUES (?, ?)"),
                    rusqlite::params![id, file],
                )?;
            }
            Ok(())
        })
        .unwrap();
    for step in [2, 3, 4] {
        let (search, sort, expected) = recorded_search("add_tags", step);
        assert_eq!(
            run(store, &search, sort, Planner::Auto),
            expected,
            "add_tags step {step}"
        );
    }
}

/// The searches of the `file_lifecycle` scenario, after archiving,
/// inboxing and deleting files the way the reference does.
#[test]
fn searches_see_file_lifecycle_changes_as_the_reference_did() {
    let fixture = import_fixture();
    let store = &fixture.store;
    let archived = hash_id_of(store, "jpeg_00.jpg");
    let inboxed = hash_id_of(store, "jpeg_01.jpg");
    store
        .write(move |ctx| {
            let c = ctx.conn();
            c.execute("DELETE FROM file_inbox WHERE hash_id = ?", [archived])?;
            c.execute(
                "INSERT OR REPLACE INTO file_archived (hash_id, archived_ms) VALUES (?, 1)",
                [archived],
            )?;
            c.execute(
                "INSERT OR IGNORE INTO file_inbox (hash_id) VALUES (?)",
                [inboxed],
            )?;
            c.execute("DELETE FROM file_archived WHERE hash_id = ?", [inboxed])?;
            Ok(())
        })
        .unwrap();
    let (search, sort, expected) = recorded_search("file_lifecycle", 3);
    assert_eq!(run(store, &search, sort, Planner::Auto), expected);

    let snapshot = store.snapshot();
    let id = |k: &[u8]| snapshot.services.builtin(k).unwrap().id;
    let (my_files, media, trash) = (
        id(builtin_keys::MY_FILES),
        id(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS),
        id(builtin_keys::TRASH),
    );
    store
        .write(move |ctx| {
            let c = ctx.conn();
            c.execute(
                "DELETE FROM file_domain_current WHERE hash_id = ?1 AND service_id IN (?2, ?3)",
                rusqlite::params![archived, my_files, media],
            )?;
            c.execute(
                "INSERT INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?, ?, 1)",
                rusqlite::params![trash, archived],
            )?;
            c.execute(
                "INSERT INTO file_domain_deleted (service_id, hash_id, deleted_ms) VALUES (?, ?, 1)",
                rusqlite::params![my_files, archived],
            )?;
            hydrus_store::domains::changed(c)
        })
        .unwrap();
    let (search, sort, expected) = recorded_search("file_lifecycle", 6);
    assert_eq!(run(store, &search, sort, Planner::Auto), expected);
}

// smaller checks ---------------------------------------------------------------

#[test]
fn limits_apply_after_sorting() {
    let search = FileSearchContext {
        predicates: vec![
            Predicate::System(SystemPredicate::Everything),
            Predicate::System(SystemPredicate::Limit(5)),
            Predicate::System(SystemPredicate::Limit(3)),
        ],
        ..FileSearchContext::default()
    };
    let by_size = |order| FileSort {
        by: SortBy::FileSize,
        order,
    };
    let all = run(
        &SHARED.store,
        &FileSearchContext {
            predicates: vec![Predicate::System(SystemPredicate::Everything)],
            ..FileSearchContext::default()
        },
        by_size(SortOrder::Descending),
        Planner::Auto,
    );
    let limited = run(
        &SHARED.store,
        &search,
        by_size(SortOrder::Descending),
        Planner::Auto,
    );
    assert_eq!(limited, all[..3]);
}

#[test]
fn import_time_sorts_agree_whether_probed_or_from_the_cached_order() {
    for location in [
        LocationContext::default(),
        LocationContext::single(key(builtin_keys::TRASH)),
        LocationContext::new(
            [key(builtin_keys::MY_FILES)],
            [key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)],
        ),
    ] {
        for order in [SortOrder::Ascending, SortOrder::Descending] {
            let search = FileSearchContext {
                location: location.clone(),
                predicates: vec![Predicate::System(SystemPredicate::Everything)],
                tags: TagContext::default(),
            };
            let sort = FileSort {
                by: SortBy::ImportTime,
                order,
            };
            let probed = run(&SHARED.store, &search, sort, Planner::AlwaysProbe);
            assert!(!probed.is_empty(), "{location:?}");
            // the first scan fills the cache, the second uses it
            for _ in 0..2 {
                let scanned = run(&SHARED.store, &search, sort, Planner::AlwaysScan);
                assert_eq!(scanned, probed, "{location:?} {order:?}");
            }
        }
    }
}

#[test]
fn constructed_predicates_without_text_syntax_work() {
    let sort = FileSort {
        by: SortBy::NumCollectionFiles,
        order: SortOrder::Ascending,
    };
    let check = |search: FileSearchContext| {
        let expected = MODEL.search(&search);
        assert!(!expected.is_empty(), "{search:?} should find something");
        assert_eq!(run(&SHARED.store, &search, sort, Planner::Auto), expected);
    };
    // system:local and system:not local have no text syntax
    for location in [
        LocationContext::single(key(builtin_keys::MY_FILES)),
        LocationContext::new([], [key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)]),
        LocationContext::new(
            [key(builtin_keys::TRASH)],
            [key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)],
        ),
    ] {
        for predicate in [SystemPredicate::Local, SystemPredicate::NotLocal] {
            let search = FileSearchContext {
                location: location.clone(),
                predicates: vec![Predicate::System(predicate.clone())],
                tags: TagContext::default(),
            };
            let expected = MODEL.search(&search);
            assert_eq!(
                run(&SHARED.store, &search, sort, Planner::Auto),
                expected,
                "{search:?}"
            );
        }
    }
    // a wildcard built directly, an absolute number test and a filetype group
    check(FileSearchContext {
        predicates: vec![
            Predicate::Wildcard {
                pattern: Wildcard::from_clean("*:*aran"),
                inclusive: true,
            },
            Predicate::System(SystemPredicate::Number {
                property: NumericProperty::Width,
                test: crate::NumberTest::new(NumberOp::ApproxAbsolute { tolerance: 1000 }, 0),
            }),
            Predicate::System(SystemPredicate::Filetype {
                filetypes: FiletypeSet::new([hydrus_core::Mime::GeneralImage]),
                inclusive: true,
            }),
        ],
        ..FileSearchContext::default()
    });
}
