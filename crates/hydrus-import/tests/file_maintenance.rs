//! File maintenance: the jobs hydrus had queued come across, and running
//! them puts back what they regenerate. The fixture's metadata flags,
//! hashes and perceptual hashes are the reference's own (and the import's
//! parity tests check ours against them), so wiping them and running the
//! jobs must give them back.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};

use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::file_maintenance::JobType;

/// md5, sha1 and sha512.
type Digests = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

/// (flags, pixel hash, md5, sha1, sha512, perceptual hashes, in the search)
type Facts = (
    i64,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    BTreeSet<Vec<u8>>,
    bool,
);

fn facts(store: &Store, files: &[i64]) -> BTreeMap<i64, Facts> {
    let files = files.to_vec();
    store
        .read(move |c| {
            let mut out = BTreeMap::new();
            for id in files {
                let (flags, pixel_hash): (i64, Option<Vec<u8>>) = c.query_row(
                    "SELECT flags, pixel_hash FROM files WHERE hash_id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let digests: Digests = c
                    .query_row(
                        "SELECT md5, sha1, sha512 FROM hash_digests WHERE hash_id = ?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .unwrap_or((None, None, None));
                let phashes: BTreeSet<Vec<u8>> = c
                    .prepare(
                        "SELECT phash FROM perceptual_hashes NATURAL JOIN file_perceptual_hashes WHERE hash_id = ?1",
                    )?
                    .query_map([id], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                let searched: bool = c.query_row(
                    "SELECT EXISTS (SELECT 1 FROM similar_search_status WHERE hash_id = ?1)",
                    [id],
                    |r| r.get(0),
                )?;
                out.insert(
                    id,
                    (flags, pixel_hash, digests.0, digests.1, digests.2, phashes, searched),
                );
            }
            Ok(out)
        })
        .unwrap()
}

#[test]
fn queued_jobs_come_across_and_put_back_what_they_regenerate() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let files: Vec<i64> = {
        let c = Connection::open(legacy.path().join("client.db")).unwrap();
        // (hydrus local file storage)
        let storage: i64 = c
            .query_row(
                "SELECT service_id FROM services WHERE service_type = 15",
                [],
                |r| r.get(0),
            )
            .unwrap();
        c.prepare(&format!(
            "SELECT hash_id FROM current_files_{storage} ORDER BY hash_id"
        ))
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    };
    assert!(files.len() > 20);
    let run = [
        JobType::HasTransparency,
        JobType::HasExif,
        JobType::HasXmp,
        JobType::HasIptc,
        JobType::HasHumanReadableEmbeddedMetadata,
        JobType::HasSoftwareSource,
        JobType::HasIccProfile,
        JobType::PixelHash,
        JobType::PerceptualHashes,
        JobType::CheckSimilarFilesMembership,
        JobType::OtherHashes,
    ];
    {
        let c = Connection::open(legacy.path().join("client.caches.db")).unwrap();
        let mut add = c
            .prepare("INSERT INTO file_maintenance_jobs (hash_id, job_type, time_can_start) VALUES (?1, ?2, ?3)")
            .unwrap();
        for &id in &files {
            for job in run {
                add.execute(params![id, job.code(), 0]).unwrap();
            }
        }
        // one this build doesn't run, one not due yet, one for no file
        add.execute(params![
            files[0],
            JobType::IntegrityPresenceLogOnly.code(),
            0
        ])
        .unwrap();
        add.execute(params![files[1], JobType::FixPermissions.code(), i64::MAX])
            .unwrap();
        add.execute(params![999_999, JobType::HasXmp.code(), 0])
            .unwrap();
    }
    let native = tempfile::tempdir().unwrap();
    let report = hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    assert_eq!(
        report.rows["file_maintenance_jobs"],
        (files.len() * run.len() + 3) as u64
    );
    let store = Store::open(native.path()).unwrap();
    let before = facts(&store, &files);

    // forget them all
    store
        .write(|ctx| {
            ctx.conn().execute_batch(
                "UPDATE files SET flags = 0, pixel_hash = NULL;
                 DELETE FROM hash_digests;
                 DELETE FROM file_perceptual_hashes;
                 DELETE FROM similar_search_status;",
            )?;
            Ok(())
        })
        .unwrap();

    let importer = FileImporter::new(store.clone(), MediaTools::new());
    // throttled, as `serve` runs it: a big job's worth (the first job type
    // in run order is perceptual hashes, each a big job)
    let throttled = importer.run_file_maintenance(u64::MAX, 100).unwrap();
    assert_eq!(throttled.total(), 1, "{throttled:?}");
    assert_eq!(throttled.weight, 100);
    let done = importer.run_file_maintenance(u64::MAX, u64::MAX).unwrap();
    assert!(
        throttled.total() + done.total() >= (files.len() * run.len()) as u64,
        "{done:?}"
    );
    let after = facts(&store, &files);
    let mut problems = Vec::new();
    for (id, want) in &before {
        let got = &after[id];
        if got != want {
            problems.push(format!("file {id}: before {want:?}\n  after {got:?}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    // what's left: the job this build doesn't run, the one not due, and
    // file metadata for files found to have EXIF (a possible rotation)
    let left: BTreeSet<(i64, JobType)> = store
        .read(|c| {
            Ok(
                c.prepare("SELECT hash_id, job_type FROM file_maintenance_jobs")?
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            )
        })
        .unwrap()
        .into_iter()
        .map(|(id, code)| (id, JobType::from_code(code).unwrap()))
        .collect();
    let with_exif: Vec<i64> = before
        .iter()
        .filter(|(_, f)| f.0 & i64::from(hydrus_store::media::FileFlags::EXIF) != 0)
        .map(|(id, _)| *id)
        .collect();
    assert!(!with_exif.is_empty());
    let mut expected: BTreeSet<(i64, JobType)> = BTreeSet::from([
        (files[0], JobType::IntegrityPresenceLogOnly),
        (files[1], JobType::FixPermissions),
    ]);
    expected.extend(with_exif.iter().map(|&id| (id, JobType::FileMetadata)));
    assert_eq!(left, expected);
}
