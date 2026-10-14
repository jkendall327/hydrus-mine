//! What the importer does with the options the import options editor gives:
//! the tag service rules saved from the editor reach a real local-import
//! queue, whose file seed (carrying parsed tags and an additional tag) is
//! imported by the queue runner as a new file, one already in the inbox and
//! one already archived, each with a tag deleted from it before. The tags
//! the file ends up with are compared with what the reference's
//! `ServiceTagImportOptions.GetTags` gave for the same rules and inputs
//! (`oracle/fixtures/import_options_fields.json`, `record_import_options_fields.py`).
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_store::Store;
use hydrus_store::content::MappingAction;
use hydrus_store::queues::{self, LocalImport, SeedStatus};
use serde_json::Value;
use slint::Model as _;

use crate::import_options_fields::{TAGS, custom, setup};

const OLD_MTIME: i64 = 1_600_000_000;

fn fixture() -> Value {
    hydrus_testkit::fixture_json("import_options_fields.json")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn runner(store: &Arc<Store>) -> Arc<QueueRunner> {
    let net = Arc::new(
        NetEngine::new(
            Arc::clone(store),
            NetOptions {
                obey_bandwidth: false,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let importer = FileImporter::new(Arc::clone(store), MediaTools::new());
    let downloader = Arc::new(Downloader::new(Arc::clone(store), net, importer).unwrap());
    QueueRunner::new(downloader, 60)
}

async fn wait_until_done(store: &Store, queue: i64) {
    for _ in 0..400 {
        if store
            .read(|conn| queues::next_file_seed(conn, queue))
            .unwrap()
            .is_none()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the import did not finish");
}

/// A copy of a fixture file in `dir`, last modified long ago.
fn place(dir: &Path, name: &str) -> PathBuf {
    let to = dir.join(name);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/fixtures/media")
            .join(name),
        &to,
    )
    .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&to)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(OLD_MTIME as u64))
        .unwrap();
    to
}

fn sha256_of(path: &Path) -> hydrus_core::Sha256 {
    let out = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum runs");
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// Import `path` through a local-import queue with these options; its seed
/// carries the parsed tags and the additional tags a downloader would give.
async fn import(
    store: &Arc<Store>,
    worker: &Arc<QueueRunner>,
    options: ImportOptionsSlice,
    path: &Path,
    parsed: BTreeSet<String>,
    additional: (String, BTreeSet<String>),
) -> SeedStatus {
    let path = path.to_string_lossy().into_owned();
    let queue = store
        .write(move |ctx| {
            let queue = queues::create_local_import(
                ctx.conn(),
                None,
                &options,
                &[(path, None)],
                &queues::PathTags::new(),
                LocalImport::default(),
                0,
            )?;
            let mut seed = queues::file_seeds(ctx.conn(), queue)?.remove(0);
            seed.meta.tags = parsed;
            seed.meta.external_additional_tags = vec![additional];
            queues::update_file_seed(ctx.conn(), &seed)?;
            Ok(queue)
        })
        .unwrap();
    worker.wake(queue);
    wait_until_done(store, queue).await;
    store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .remove(0)
        .status
}

fn current_tags(store: &Store, hash: hydrus_core::Sha256) -> Vec<String> {
    let my_tags = store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    let tags: BTreeSet<String> = store
        .read(|conn| {
            let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
            let snapshot = store.snapshot();
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
            Ok(batch.results[0]
                .tags
                .get(&my_tags)
                .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                .into_iter()
                .flatten()
                .map(|t| batch.tags[t].as_str().to_owned())
                .collect())
        })
        .unwrap();
    tags.into_iter().collect()
}

// leaf: audit-network-options-tag-fetch
#[tokio::test(flavor = "multi_thread")]
async fn tag_service_rules_from_the_editor_decide_the_tags_an_imported_file_gets() {
    let _windows = hydrus_gui::headless::init();
    let fixture = fixture();
    let tags = &fixture["tags"];
    let key = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let toggles = [
        ("_to_new_files", "to-new"),
        ("_to_already_in_inbox", "to-inbox"),
        ("_to_already_in_archive", "to-archive"),
        ("_get_tags_overwrite_deleted", "get-overwrite"),
        ("_additional_tags_overwrite_deleted", "additional-overwrite"),
        ("_only_add_existing_tags", "only-existing"),
    ];
    let filterable: BTreeSet<String> = strings(&tags["filterable"]).into_iter().collect();
    let additional: BTreeSet<String> = strings(&tags["additional"]).into_iter().collect();
    let deleted = strings(&tags["deleted"]);
    // (a file of each kind: new, already in the inbox, already archived)
    let files = [
        ("new", "bmp_24.bmp"),
        ("inbox", "apng_rgba.png"),
        ("archive", "jpeg_420.jpg"),
    ];

    for scenario in tags["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let s = setup();
        let editor = s.editor();
        custom(&editor, TAGS);
        let mine = editor
            .get_tag_services()
            .iter()
            .position(|service| service.name == "my tags")
            .unwrap() as i32;
        for op in scenario["ops"].as_array().unwrap() {
            match op[0].as_str().unwrap() {
                "get_tags" => {
                    editor.invoke_tag_service_toggled(
                        mine,
                        "get-tags".into(),
                        op[1].as_bool().unwrap(),
                    );
                }
                "cog" => {
                    let which = toggles
                        .iter()
                        .find(|(a, _)| *a == op[1].as_str().unwrap())
                        .unwrap()
                        .1;
                    editor.invoke_tag_service_toggled(mine, which.into(), op[2].as_bool().unwrap());
                }
                other => panic!("unknown op {other}"),
            }
        }
        editor.invoke_apply();
        let saved = s.saved();
        assert!(saved.tags.is_some(), "{name}: the editor saved tag rules");

        let store = Arc::clone(&s.store);
        let worker = runner(&store);
        worker.start_all().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let my_tags = store
            .snapshot()
            .services
            .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
            .unwrap()
            .id;
        for (label, media) in files {
            let path = place(dir.path(), media);
            let hash = sha256_of(&path);
            // the file's history: a tag deleted from it
            let gone = deleted.clone();
            store
                .write_content(move |w| {
                    let id = hydrus_store::master::intern_hash(w.conn(), &hash)?;
                    for text in &gone {
                        let tag = hydrus_store::master::intern_tag(
                            w.conn(),
                            &hydrus_core::Tag::new(text).unwrap(),
                        )?;
                        w.update_mappings(my_tags, &MappingAction::Delete, tag, &[id])?;
                    }
                    Ok(())
                })
                .unwrap();
            if label != "new" {
                // already in the database (with no tags of its own), in the
                // inbox or archived
                let first = import(
                    &store,
                    &worker,
                    ImportOptionsSlice::default(),
                    &path,
                    BTreeSet::new(),
                    (key.clone(), BTreeSet::new()),
                )
                .await;
                assert_eq!(first, SeedStatus::SuccessfulAndNew, "{name}/{label}");
                if label == "archive" {
                    store
                        .write_content(move |w| {
                            let id = hydrus_store::master::hash_id(w.conn(), &hash)?.unwrap();
                            w.archive(&[id])
                        })
                        .unwrap();
                }
            }
            let status = import(
                &store,
                &worker,
                saved.clone(),
                &path,
                filterable.clone(),
                (key.clone(), additional.clone()),
            )
            .await;
            let want_status = if label == "new" {
                SeedStatus::SuccessfulAndNew
            } else {
                SeedStatus::SuccessfulButRedundant
            };
            assert_eq!(status, want_status, "{name}/{label}");
            assert_eq!(
                current_tags(&store, hash),
                strings(&scenario["outcomes"][label]),
                "{name}/{label}: the tags the file got"
            );
        }
    }
}

// --- prefetch ---------------------------------------------------------------
//
// The prefetch page's choices (hash check, URL check, neighbour spam, the two
// "fetch metadata even if known" boxes) and the file filtering page's
// "exclude previously deleted" box, set in the editor and applied, decide
// what a URL queue asks a site for. The reference ran the same steps on a
// real client against a loopback booru
// (`oracle/record_prefetch_outcomes.py` -> `fixtures/prefetch_outcomes.json`):
// which pages the site was asked for, and the file seed's status and note.

use axum::Router;
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use hydrus_core::url::strings::StringMatch;
use hydrus_core::url::{DomainMask, StringProcessor, UrlClass, UrlClassSettings, UrlType};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use std::sync::Mutex;

use crate::import_options_fields::{FILE_FILTERING, PREFETCH};

#[derive(Default)]
struct Site {
    hits: Mutex<Vec<String>>,
    /// Post 90 names this hash (the first post's file).
    hash_90: Mutex<String>,
}

fn media_bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/fixtures/media")
            .join(name),
    )
    .unwrap()
}

/// The recorded booru's images, by post.
fn image_of(post: u32) -> &'static str {
    match post {
        2 => "jpeg_422.jpg",
        3 => "jpeg_444_q95.jpg",
        _ => "jpeg_420.jpg",
    }
}

async fn post_page(State(site): State<Arc<Site>>, AxPath(id): AxPath<u32>) -> Response {
    site.hits.lock().unwrap().push(format!("/post/{id}"));
    if ![1, 2, 3, 90].contains(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let hash = if id == 90 {
        format!(
            r#"<span class="hash">{}</span>"#,
            site.hash_90.lock().unwrap()
        )
    } else {
        String::new()
    };
    let html = format!(
        r#"<html><head><title>post {id}</title></head><body><ul id="tags"><li class="tag">post {id}</li></ul>{hash}<img id="image" src="/files/p{id}.jpg"></body></html>"#
    );
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn post_file(State(site): State<Arc<Site>>, AxPath(name): AxPath<String>) -> Response {
    site.hits.lock().unwrap().push(format!("/files/{name}"));
    let id: u32 = name
        .trim_start_matches('p')
        .trim_end_matches(".jpg")
        .parse()
        .unwrap_or(0);
    ([("content-type", "image/jpeg")], media_bytes(image_of(id))).into_response()
}

fn html_formula(tag: &str, attrs: &[(&str, &str)], content: HtmlContent) -> Formula {
    Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: FormulaKind::Html {
            rules: vec![HtmlRule {
                walk: HtmlWalk::Descendants(TagSearch {
                    attrs: attrs
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                        .collect(),
                    index: None,
                }),
                tag_name: Some(tag.to_owned()),
                text_match: None,
            }],
            content,
        },
        processor: StringProcessor::default(),
    }
}

fn booru_parser() -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: "local booru post parser".into(),
        key: "ab".into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![
            ContentParser {
                name: "tags".into(),
                kind: ContentKind::Tag { namespace: None },
                formula: html_formula("li", &[("class", "tag")], HtmlContent::Text),
            },
            ContentParser {
                name: "file".into(),
                kind: ContentKind::Url {
                    url_type: 7,
                    priority: 50,
                },
                formula: html_formula(
                    "img",
                    &[("id", "image")],
                    HtmlContent::Attribute("src".into()),
                ),
            },
            ContentParser {
                name: "hash".into(),
                kind: ContentKind::Hash {
                    hash_type: "sha256".into(),
                    encoding: "hex".into(),
                },
                formula: html_formula("span", &[("class", "hash")], HtmlContent::Text),
            },
        ],
        example_urls: Vec::new(),
    }
}

/// The real status of a seed, by the reference's code, with the time taken
/// out of its note as the recording does.
fn normalised(note: &str) -> String {
    let note = regex::Regex::new(r"at \d{4}-\d\d-\d\d \d\d:\d\d:\d\d")
        .unwrap()
        .replace_all(note, "at TIME");
    regex::Regex::new(r"which was .*? before this check")
        .unwrap()
        .replace_all(&note, "which was AGO before this check")
        .into_owned()
}

fn set_checks(editor: &hydrus_gui::ImportOptionsWindow, options: &Value) {
    editor.set_hash_index(options["hash_check"].as_i64().unwrap() as i32);
    editor.invoke_changed();
    editor.set_url_index(options["url_check"].as_i64().unwrap() as i32);
    editor.invoke_changed();
    editor.set_neighbour_spam(options["neighbour_spam"].as_bool().unwrap());
    editor.set_fetch_hash(options["fetch_hash"].as_bool().unwrap());
    editor.set_fetch_url(options["fetch_url"].as_bool().unwrap());
    editor.invoke_changed();
}

// leaf: audit-network-options-prefetch
#[tokio::test(flavor = "multi_thread")]
async fn prefetch_choices_from_the_editor_decide_what_a_url_queue_fetches() {
    use hydrus_store::queues::QueueKind;
    let _windows = hydrus_gui::headless::init();
    let fixture = hydrus_testkit::fixture_json("prefetch_outcomes.json");

    for scenario in fixture["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let options = &scenario["options"];
        let s = setup();
        let store = Arc::clone(&s.store);

        // the booru, and its downloader in the client
        let site = Arc::new(Site::default());
        let app = Router::new()
            .route("/post/{id}", get(post_page))
            .route("/files/{name}", get(post_file))
            .with_state(Arc::clone(&site));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let class = UrlClass {
            name: "local booru post".into(),
            key: vec![0xcd],
            url_type: UrlType::Post,
            preferred_scheme: "http".into(),
            domain_mask: DomainMask::new(vec![host.clone()], vec![], false, false),
            path_components: vec![
                (StringMatch::fixed("post"), None),
                (StringMatch::any(), None),
            ],
            example_url: format!("http://{host}/post/1"),
            ..UrlClass::default()
        };
        let classes = UrlClassSettings {
            parser_links: vec![(hex::encode(&class.key), Some("ab".into()))],
            parser_keys: vec!["ab".into()],
            url_classes: vec![class],
            collapse_leading_slashes: false,
        };
        let downloaders = Downloaders {
            parsers: vec![booru_parser()],
            ..Downloaders::default()
        };
        store
            .write_and_refresh(move |ctx| {
                hydrus_store::settings::set(ctx.conn(), &classes)?;
                hydrus_store::settings::set(ctx.conn(), &downloaders)
            })
            .unwrap();
        // (the imported client has all new network traffic paused)
        store
            .write(|ctx| {
                let mut pauses: hydrus_store::settings::Pauses =
                    hydrus_store::settings::get(ctx.conn())?;
                pauses.network_traffic = false;
                hydrus_store::settings::set(ctx.conn(), &pauses)
            })
            .unwrap();
        let worker = runner(&store);
        worker.start_all().unwrap();
        let base = format!("http://{host}");

        // posts 1 to 3 imported with the client's own options
        let mut hashes = Vec::new();
        for post in 1..=3 {
            let queue = store
                .write(move |ctx| {
                    queues::create_queue(
                        ctx.conn(),
                        QueueKind::Urls,
                        &format!("seed {post}"),
                        None,
                        &ImportOptionsSlice::default(),
                        0,
                    )
                })
                .unwrap();
            worker
                .pend_urls(
                    queue,
                    &[format!("{base}/post/{post}")],
                    &BTreeSet::new(),
                    &[],
                )
                .unwrap();
            wait_until_done(&store, queue).await;
            let seed = store
                .read(|conn| queues::file_seeds(conn, queue))
                .unwrap()
                .remove(0);
            assert_eq!(seed.status, SeedStatus::SuccessfulAndNew, "{name}: seeding");
            hashes.push(seed.meta.hash("sha256").unwrap().to_owned());
        }
        *site.hash_90.lock().unwrap() = hashes[0].clone();

        // the file's history, as the recording staged it
        let file_1: hydrus_core::Sha256 = hashes[0].parse().unwrap();
        let file_2: hydrus_core::Sha256 = hashes[1].parse().unwrap();
        match scenario["setup"].as_str() {
            Some("extra_url") => {
                let url = format!("{base}/post/50");
                store
                    .write_content(move |w| {
                        let id = hydrus_store::master::hash_id(w.conn(), &file_1)?.unwrap();
                        w.add_urls(&[id], &[url])
                    })
                    .unwrap();
            }
            Some("delete") => {
                let local = store.snapshot().services.by_name("my files").unwrap().id;
                store
                    .write_content(move |w| {
                        let id = hydrus_store::master::hash_id(w.conn(), &file_2)?.unwrap();
                        w.delete_files(local, &[id], None)
                    })
                    .unwrap();
            }
            _ => {}
        }

        // the editor on the importer's page, then Apply
        let editor = s.editor();
        custom(&editor, PREFETCH);
        custom(&editor, FILE_FILTERING);
        editor.invoke_kind_clicked(PREFETCH);
        set_checks(&editor, options);
        editor.invoke_kind_clicked(FILE_FILTERING);
        editor.set_exclude_deleted(options["exclude_deleted"].as_bool().unwrap());
        editor.invoke_changed();
        editor.invoke_apply();
        let saved = s.saved();
        assert!(
            saved.prefetch.is_some(),
            "{name}: the editor saved prefetch"
        );

        // a queue with the saved options is given the recorded URL
        let queue = store
            .write(move |ctx| {
                queues::create_queue(ctx.conn(), QueueKind::Urls, "step", None, &saved, 0)
            })
            .unwrap();
        site.hits.lock().unwrap().clear();
        let url = format!("{base}{}", scenario["path"].as_str().unwrap());
        if scenario["setup"].as_str() == Some("preset_hash") {
            // the seed is queued already knowing the first file's sha256
            let hash = hashes[0].clone();
            store
                .write(move |ctx| {
                    queues::add_file_seeds(
                        ctx.conn(),
                        queue,
                        &[queues::NewFileSeed {
                            seed_type: queues::SeedType::Url,
                            data: url.clone(),
                            data_for_comparison: url,
                            source_time: None,
                            referral_url: None,
                            meta: queues::FileSeedMeta {
                                hashes: vec![("sha256".into(), hash)],
                                ..queues::FileSeedMeta::default()
                            },
                        }],
                        true,
                        0,
                    )
                })
                .unwrap();
            worker.wake(queue);
        } else {
            worker
                .pend_urls(queue, &[url], &BTreeSet::new(), &[])
                .unwrap();
        }
        wait_until_done(&store, queue).await;
        // (a moment for any fetch still to come to show)
        tokio::time::sleep(Duration::from_millis(300)).await;
        let seed = store
            .read(|conn| queues::file_seeds(conn, queue))
            .unwrap()
            .remove(0);
        let mut hits = site.hits.lock().unwrap().clone();
        hits.sort();
        let want = &scenario["seed"];
        assert_eq!(
            hits,
            strings(&scenario["hits"]),
            "{name}: what the site was asked for"
        );
        assert_eq!(
            seed.status.code(),
            want["status"].as_i64().unwrap(),
            "{name}"
        );
        assert_eq!(
            normalised(&seed.note),
            want["note"].as_str().unwrap(),
            "{name}: the seed's note"
        );
        server.abort();
    }
}
