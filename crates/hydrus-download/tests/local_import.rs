//! A local import (the reference's "import" page, `HDDImport`) as the
//! daemon works it: each file imported from its path, in order, with its
//! modified time as its source time; one already in the database found
//! so; a missing one vetoed with the reference's note; and, if the import
//! says, each file that is in the database afterwards deleted from where
//! it was (only then). A file's tags to add (an "import" page carried over
//! from hydrus has them) are added to it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
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
    let create = |paths: Vec<(String, Option<i64>)>, delete: bool| {
        store
            .write(move |ctx| {
                queues::create_local_import(
                    ctx.conn(),
                    None,
                    &ImportOptionsSlice::default(),
                    &paths,
                    LocalImport {
                        delete_after_success: delete,
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
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
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
    // the png with its tag, the bmp with none
    let snapshot = store.snapshot();
    let my_tags = snapshot
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_TAGS)
        .unwrap()
        .id;
    for (seed, expected) in seeds[..2].iter().zip([vec![], vec!["series:metroid"]]) {
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

    // the same file again, not to be deleted: already in the database, kept
    let again = place(work.path(), "bmp_24.bmp");
    let id = create(vec![(path_text(&again), Some(OLD_MTIME))], false);
    runner.wake(id);
    wait_until_done(&store, id).await;
    let seeds = store.read(|c| queues::file_seeds(c, id)).unwrap();
    assert_eq!(seeds[0].status, SeedStatus::SuccessfulButRedundant);
    assert!(again.exists());
}
