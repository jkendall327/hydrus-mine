//! A local import (the reference's "import" page, `HDDImport`) as the
//! daemon works it: each file imported from its path, in order, with its
//! modified time as its source time; one already in the database found
//! so; a missing one vetoed with the reference's note; and, if the import
//! says, each file that is in the database afterwards deleted from where
//! it was (only then), with the sidecars its routers might read. A file's
//! tags to add (an "import" page carried over from hydrus has them) are
//! added to it, and its routers' metadata (a .txt sidecar's tags, here).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::url::strings::{StringConverter, StringProcessor};
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_parse::sidecar::{Exporter, Importer, Router, SidecarNaming, Source};
use hydrus_store::queues::{self, LocalImport, SeedStatus};
use hydrus_store::settings::FolderSettings;
use hydrus_store::{Store, settings};

const OLD_MTIME: i64 = 1_600_000_000;

fn media(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/fixtures/media")
        .join(name)
}

/// A copy of a fixture file in `dir`, last modified long ago.
fn place(dir: &Path, name: &str) -> PathBuf {
    let to = dir.join(name);
    std::fs::copy(media(name), &to).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&to)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(OLD_MTIME as u64))
        .unwrap();
    to
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
        let pending = store
            .read(|conn| queues::next_file_seed(conn, queue))
            .unwrap();
        if pending.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the import did not finish");
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// leaf: import-existing-tags-filter
#[tokio::test(flavor = "multi_thread")]
async fn saved_existing_tag_filter_reaches_real_file_importer_for_parsed_and_additional_tags() {
    use hydrus_core::import_options::{ServiceTagImportOptions, TagImportOptions};
    use hydrus_core::tag_filter::{FilterRule, TagFilter};
    use hydrus_store::content::MappingAction;
    let fixture = hydrus_testkit::fixture_json("existing_tags_filter.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let work = tempfile::tempdir().unwrap();
    let known_path = place(work.path(), "bmp_24.bmp");
    let target_path = place(work.path(), "apng_rgba.png");
    let source = store
        .write(move |ctx| {
            queues::create_local_import(
                ctx.conn(),
                None,
                &ImportOptionsSlice::default(),
                &[(path_text(&known_path), None)],
                &queues::PathTags::new(),
                LocalImport::default(),
                0,
            )
        })
        .unwrap();
    let worker = runner(&store);
    worker.start_all().unwrap();
    wait_until_done(&store, source).await;
    let source_hash: hydrus_core::Sha256 =
        store.read(|conn| queues::file_seeds(conn, source)).unwrap()[0]
            .meta
            .hash("sha256")
            .unwrap()
            .parse()
            .unwrap();
    let key = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let my_tags = store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    let known: Vec<String> = serde_json::from_value(fixture["known"].clone()).unwrap();
    store
        .write_content(move |writer| {
            let hash = hydrus_store::master::hash_id(writer.conn(), &source_hash)?.unwrap();
            for text in known {
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &hydrus_core::Tag::new(&text).unwrap(),
                )?;
                writer.update_mappings(my_tags, &MappingAction::Add, tag, &[hash])?;
            }
            Ok(())
        })
        .unwrap();
    let mut filter = TagFilter::new();
    for pair in fixture["saved"]["rules"].as_array().unwrap() {
        filter.set_rule(
            pair[0].as_str().unwrap(),
            if pair[1] == 0 {
                FilterRule::Whitelist
            } else {
                FilterRule::Blacklist
            },
        );
    }
    let additional: Vec<String> = serde_json::from_value(fixture["additional"].clone()).unwrap();
    let own = ImportOptionsSlice {
        tags: Some(TagImportOptions {
            services: [
                key.clone(),
                hex::encode(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS),
            ]
            .into_iter()
            .map(|service| {
                (
                    service,
                    ServiceTagImportOptions {
                        get_tags: true,
                        additional_tags: additional.clone(),
                        only_add_existing_tags: true,
                        only_add_existing_tags_filter: filter.clone(),
                        ..Default::default()
                    },
                )
            })
            .collect(),
        }),
        ..Default::default()
    };
    // Save/reopen the same typed importer options the native editor accepts.
    let queue = store
        .write(move |ctx| {
            queues::create_local_import(
                ctx.conn(),
                None,
                &own,
                &[(path_text(&target_path), None)],
                &queues::PathTags::new(),
                LocalImport::default(),
                0,
            )
        })
        .unwrap();
    let parsed: BTreeSet<String> = serde_json::from_value(fixture["parsed"].clone()).unwrap();
    store
        .write(move |ctx| {
            let mut seed = queues::file_seeds(ctx.conn(), queue)?.remove(0);
            seed.meta.tags = parsed;
            queues::update_file_seed(ctx.conn(), &seed)
        })
        .unwrap();
    let reopened = Store::open(dir.path()).unwrap();
    let saved = reopened
        .read(|conn| queues::queue(conn, queue))
        .unwrap()
        .unwrap();
    assert!(
        saved
            .options
            .tags
            .as_ref()
            .unwrap()
            .service(&key)
            .unwrap()
            .only_add_existing_tags
    );
    worker.wake(queue);
    wait_until_done(&store, queue).await;
    let seed = store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .remove(0);
    assert_eq!(seed.status, SeedStatus::SuccessfulAndNew);
    let hash: hydrus_core::Sha256 = seed.meta.hash("sha256").unwrap().parse().unwrap();
    let results: Vec<BTreeSet<String>> = reopened
        .read(|conn| {
            let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
            let snapshot = reopened.snapshot();
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
            let second = snapshot
                .services
                .builtin(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS)?
                .id;
            Ok([my_tags, second]
                .into_iter()
                .map(|service| {
                    let tags = &batch.results[0].tags[&service].by_status
                        [&hydrus_core::ContentStatus::Current];
                    tags.iter()
                        .map(|tag| batch.tags[tag].as_str().to_owned())
                        .collect()
                })
                .collect())
        })
        .unwrap();
    assert_eq!(serde_json::json!(results[0]), fixture["consumer"]);
    assert_eq!(
        serde_json::json!(results[1]),
        fixture["second_service_consumer"]
    );
    // Disable the gate while retaining its saved filter, then re-import an
    // already-known file: previously blocked new tags must now be added.
    let mut disabled = saved.options;
    for (_, service) in &mut disabled.tags.as_mut().unwrap().services {
        service.only_add_existing_tags = false;
    }
    let path = seed.data;
    let parsed: BTreeSet<String> = serde_json::from_value(fixture["parsed"].clone()).unwrap();
    let again = store
        .write(move |ctx| {
            let queue = queues::create_local_import(
                ctx.conn(),
                None,
                &disabled,
                &[(path, None)],
                &queues::PathTags::new(),
                LocalImport::default(),
                0,
            )?;
            let mut seed = queues::file_seeds(ctx.conn(), queue)?.remove(0);
            seed.meta.tags = parsed;
            queues::update_file_seed(ctx.conn(), &seed)?;
            Ok(queue)
        })
        .unwrap();
    worker.wake(again);
    wait_until_done(&store, again).await;
    assert_eq!(
        store.read(|conn| queues::file_seeds(conn, again)).unwrap()[0].status,
        SeedStatus::SuccessfulButRedundant
    );
    let result: BTreeSet<String> = reopened
        .read(|conn| {
            let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
            let snapshot = reopened.snapshot();
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
            let tags =
                &batch.results[0].tags[&my_tags].by_status[&hydrus_core::ContentStatus::Current];
            Ok(tags
                .iter()
                .map(|tag| batch.tags[tag].as_str().to_owned())
                .collect())
        })
        .unwrap();
    assert_eq!(serde_json::json!(result), fixture["disabled_consumer"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_import_imports_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    // (deleted for good rather than recycled, here)
    store
        .write(|ctx| {
            let mut folders: FolderSettings = settings::get(ctx.conn())?;
            folders.delete_to_recycle_bin = false;
            settings::set(ctx.conn(), &folders)
        })
        .unwrap();
    let work = tempfile::tempdir().unwrap();
    let bmp = place(work.path(), "bmp_24.bmp");
    let png = place(work.path(), "apng_rgba.png");
    let gone = work.path().join("gone.png");
    let paths: Vec<(String, Option<i64>)> = [&bmp, &png, &gone]
        .iter()
        .map(|p| (path_text(p), (*p != &gone).then_some(OLD_MTIME)))
        .collect();
    // the bmp's tags in a .txt sidecar beside it, which a router reads
    let sidecar = work.path().join("bmp_24.bmp.txt");
    std::fs::write(&sidecar, "creator:samus\n").unwrap();
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let router = Router {
        importers: vec![Importer {
            source: Source::Txt {
                naming: SidecarNaming {
                    remove_actual_filename_ext: false,
                    suffix: String::new(),
                    filename_converter: StringConverter::default(),
                },
                separator: "\n".into(),
            },
            processor: StringProcessor::default(),
        }],
        processor: StringProcessor::default(),
        exporter: Exporter::MediaTags {
            service_key: my_tags.clone(),
        },
    };
    let create = |paths: Vec<(String, Option<i64>)>, delete: bool| {
        let routers = vec![router.clone()];
        store
            .write(move |ctx| {
                queues::create_local_import(
                    ctx.conn(),
                    None,
                    &ImportOptionsSlice::default(),
                    &paths,
                    &queues::PathTags::new(),
                    LocalImport {
                        delete_after_success: delete,
                        routers,
                    },
                    0,
                )
            })
            .unwrap()
    };
    let id = create(paths, true);
    let queue = store.read(|c| queues::queue(c, id)).unwrap().unwrap();
    assert_eq!(queue.kind, queues::QueueKind::LocalImport);
    assert_eq!(queue.name, "import");
    // (the png with a tag to add to "my tags")
    store
        .write({
            let my_tags = my_tags.clone();
            move |ctx| {
                let mut seed = queues::file_seeds(ctx.conn(), id)?.remove(1);
                seed.meta.external_additional_tags =
                    vec![(my_tags, BTreeSet::from(["series:metroid".to_owned()]))];
                queues::update_file_seed(ctx.conn(), &seed)
            }
        })
        .unwrap();
    let runner = runner(&store);
    runner.start_all().unwrap();
    wait_until_done(&store, id).await;

    let seeds = store.read(|c| queues::file_seeds(c, id)).unwrap();
    let found: Vec<(String, SeedStatus, &str, Option<i64>)> = seeds
        .iter()
        .map(|s| (s.data.clone(), s.status, s.note.as_str(), s.source_time))
        .collect();
    assert_eq!(
        found,
        [
            (
                path_text(&bmp),
                SeedStatus::SuccessfulAndNew,
                "",
                Some(OLD_MTIME)
            ),
            (
                path_text(&png),
                SeedStatus::SuccessfulAndNew,
                "",
                Some(OLD_MTIME)
            ),
            (
                path_text(&gone),
                SeedStatus::Vetoed,
                "Source file does not exist!",
                None
            ),
        ]
    );
    // the png with its tag, the bmp with its sidecar's
    let snapshot = store.snapshot();
    let my_tags = snapshot
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    for (seed, expected) in seeds[..2]
        .iter()
        .zip([vec!["creator:samus"], vec!["series:metroid"]])
    {
        let hash: hydrus_core::Sha256 = seed.meta.hash("sha256").unwrap().parse().unwrap();
        let tags: Vec<String> = store
            .read(|conn| {
                let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
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
        assert_eq!(tags, expected, "{}", seed.data);
    }
    // each in the database, and deleted from where it was
    for seed in &seeds[..2] {
        let hash = seed.meta.hash("sha256").expect("a hash");
        let hash: hydrus_core::Sha256 = hash.parse().unwrap();
        assert!(
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .is_some()
        );
        assert!(
            !Path::new(&seed.data).exists(),
            "{} is still there",
            seed.data
        );
    }

    assert!(!sidecar.exists(), "the sidecar is deleted with its file");

    // the same file again, not to be deleted: already in the database, kept
    let again = place(work.path(), "bmp_24.bmp");
    let id = create(vec![(path_text(&again), Some(OLD_MTIME))], false);
    runner.wake(id);
    wait_until_done(&store, id).await;
    let seeds = store.read(|c| queues::file_seeds(c, id)).unwrap();
    assert_eq!(seeds[0].status, SeedStatus::SuccessfulButRedundant);
    assert!(again.exists());
}

// leaf: audit-options-files-and-trash-test-import-local-files-directly-from-source-do-not-copy-to-temp-dir-beforehand
#[tokio::test(flavor = "multi_thread")]
async fn a_local_import_copies_to_a_temp_path_first_unless_the_option_says_not_to() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let net = Arc::new(
        NetEngine::new(
            Arc::clone(&store),
            NetOptions {
                obey_bandwidth: false,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let probe = importer.clone();
    let downloader = Arc::new(Downloader::new(Arc::clone(&store), net, importer).unwrap());
    let worker = QueueRunner::new(downloader, 60);
    let work = tempfile::tempdir().unwrap();
    let one = place(work.path(), "bmp_24.bmp");
    let two = place(work.path(), "apng_rgba.png");
    let hash_of = |queue: i64| -> String {
        store.read(|conn| queues::file_seeds(conn, queue)).unwrap()[0]
            .meta
            .hash("sha256")
            .unwrap()
            .to_owned()
    };
    let import = |path: &Path| {
        let path = path_text(path);
        store
            .write(move |ctx| {
                queues::create_local_import(
                    ctx.conn(),
                    None,
                    &ImportOptionsSlice::default(),
                    &[(path, None)],
                    &queues::PathTags::new(),
                    LocalImport::default(),
                    0,
                )
            })
            .unwrap()
    };

    // the reference's default: the file is copied to a temp path and imported
    // from there
    assert!(
        store
            .read(settings::get::<FolderSettings>)
            .unwrap()
            .copy_import_files_to_temp_dir
    );
    let first = import(&one);
    worker.start_all().unwrap();
    wait_until_done(&store, first).await;
    assert_eq!(probe.temp_copies_made(), 1);
    let leftovers = |dir: &Path| std::fs::read_dir(dir.join("tmp")).map_or(0, Iterator::count);
    assert_eq!(leftovers(dir.path()), 0, "the temp copy is gone afterwards");
    let copied = hash_of(first);

    // "import local files directly from source": the same file, no copy
    store
        .write(|ctx| {
            let mut folders: FolderSettings = settings::get(ctx.conn())?;
            folders.copy_import_files_to_temp_dir = false;
            settings::set(ctx.conn(), &folders)
        })
        .unwrap();
    let second = import(&two);
    worker.start_all().unwrap();
    wait_until_done(&store, second).await;
    assert_eq!(probe.temp_copies_made(), 1, "no second copy was made");
    let direct = hash_of(second);
    assert_ne!(copied, direct);
    // imported all the same: the file is in the database, from where it was
    for (queue, path) in [(first, &one), (second, &two)] {
        let seed = &store.read(|conn| queues::file_seeds(conn, queue)).unwrap()[0];
        assert!(seed.status.is_successful(), "{}", seed.note);
        assert!(path.exists(), "the source is left where it was");
    }
    // and a repeat of the first file, with the option back on, copies again
    store
        .write(|ctx| {
            let mut folders: FolderSettings = settings::get(ctx.conn())?;
            folders.copy_import_files_to_temp_dir = true;
            settings::set(ctx.conn(), &folders)
        })
        .unwrap();
    let third = import(&one);
    worker.start_all().unwrap();
    wait_until_done(&store, third).await;
    assert_eq!(probe.temp_copies_made(), 2);
}
