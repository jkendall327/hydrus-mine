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
        // one not due yet, one for no file
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
        (files.len() * run.len() + 2) as u64
    );
    let store = Store::open(native.path()).unwrap();
    let before = facts(&store, &files);
    let info = || -> Vec<Vec<Option<i64>>> {
        store
            .read(|c| {
                Ok(c.prepare(
                    "SELECT size, mime, width, height, duration_ms, num_frames, has_audio, num_words
                     FROM files ORDER BY hash_id",
                )?
                .query_map([], |r| (0..8).map(|i| r.get(i)).collect())?
                .collect::<rusqlite::Result<_>>()?)
            })
            .unwrap()
    };
    let info_before = info();

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

    // what's left: the job not due. Files found to have EXIF (a possible
    // rotation) had their metadata read again, which hydrus had right.
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
    assert_eq!(left, BTreeSet::from([(files[1], JobType::FixPermissions)]));
    assert_eq!(
        done.done.get(&JobType::FileMetadata),
        Some(&(with_exif.len() as u64))
    );
    assert_eq!(info(), info_before);
}

/// Paths under `root`'s `kind` (`f` or `t`) subfolders whose names start
/// with `prefix`.
fn find(root: &std::path::Path, prefix: &str, kind: char) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, prefix, out);
            } else if entry.file_name().to_string_lossy().starts_with(prefix) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(kind) && entry.path().is_dir() {
            walk(&entry.path(), prefix, &mut out);
        }
    }
    out.sort();
    out
}

/// Damage the legacy install's files as `oracle/dump_file_maintenance.py`
/// does before the reference boots.
fn apply_edits(
    db_dir: &std::path::Path,
    scenarios: &[serde_json::Value],
    by_name: &BTreeMap<String, String>,
    garbage: &str,
) {
    let c = Connection::open(db_dir.join("client.db")).unwrap();
    c.execute(
        "ATTACH ?1 AS master",
        [db_dir.join("client.master.db").to_str().unwrap()],
    )
    .unwrap();
    let files = db_dir.join("client_files");
    for sc in scenarios {
        let hex = &by_name[sc["file"].as_str().unwrap()];
        let hash = hex.parse::<hydrus_core::Sha256>().unwrap();
        let id: i64 = c
            .query_row(
                "SELECT hash_id FROM master.hashes WHERE hash = ?1",
                [&hash.0[..]],
                |r| r.get(0),
            )
            .unwrap();
        let mut path = {
            let found = find(&files, hex, 'f');
            assert_eq!(found.len(), 1, "{found:?}");
            found[0].clone()
        };
        for edit in sc["edits"].as_array().unwrap() {
            match edit["op"].as_str().unwrap() {
                "set_info" => {
                    for column in ["width", "height", "mime"] {
                        if let Some(v) = edit.get(column) {
                            c.execute(
                                &format!("UPDATE files_info SET {column} = ?1 WHERE hash_id = ?2"),
                                params![v.as_i64().unwrap(), id],
                            )
                            .unwrap();
                        }
                    }
                }
                "force_mime" => {
                    c.execute(
                        "INSERT INTO files_info_forced_filetypes (hash_id, forced_mime) VALUES (?1, ?2)",
                        params![id, edit["mime"].as_i64().unwrap()],
                    )
                    .unwrap();
                }
                "forget_hashes" => {
                    c.execute("DELETE FROM master.local_hashes WHERE hash_id = ?1", [id])
                        .unwrap();
                }
                "forget_modified" => {
                    c.execute(
                        "DELETE FROM file_modified_timestamps WHERE hash_id = ?1",
                        [id],
                    )
                    .unwrap();
                }
                "rename_file" => {
                    let renamed = path.with_extension(&edit["ext"].as_str().unwrap()[1..]);
                    std::fs::rename(&path, &renamed).unwrap();
                    path = renamed;
                }
                "copy_file" => {
                    std::fs::copy(
                        &path,
                        path.with_extension(&edit["ext"].as_str().unwrap()[1..]),
                    )
                    .unwrap();
                }
                "garbage" => std::fs::write(&path, garbage).unwrap(),
                "append_byte" => {
                    let mut bytes = std::fs::read(&path).unwrap();
                    bytes.push(0);
                    std::fs::write(&path, bytes).unwrap();
                }
                "remove_file" => std::fs::remove_file(&path).unwrap(),
                "copy_thumbnail" => {
                    let source = &by_name[edit["from"].as_str().unwrap()];
                    let [from] = &find(&files, source, 't')[..] else {
                        panic!()
                    };
                    let [to] = &find(&files, hex, 't')[..] else {
                        panic!()
                    };
                    std::fs::copy(from, to).unwrap();
                }
                "remove_thumbnail" => {
                    let [to] = &find(&files, hex, 't')[..] else {
                        panic!()
                    };
                    std::fs::remove_file(to).unwrap();
                }
                other => panic!("{other}"),
            }
        }
    }
}

/// Damaged, misnamed and missing files, wrong metadata, wrong-size
/// thumbnails and stray copies, each with its job, against what the
/// reference did with them (`oracle/dump_file_maintenance.py`): the
/// database, the files on disk, the jobs left queued, what was written to
/// `missing_and_invalid_files`, and the URLs sent to be downloaded again.
#[test]
fn damaged_files_are_dealt_with_as_the_reference_does() {
    use hydrus_store::file_maintenance::add_jobs;
    use serde_json::{Value, json};

    let fixture = hydrus_testkit::fixture_json("file_maintenance.json");
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let by_name: BTreeMap<String, String> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_owned(),
                f["hash"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let scenarios = fixture["scenarios"].as_array().unwrap();

    let legacy = hydrus_testkit::legacy_fixture("basic");
    apply_edits(
        legacy.path(),
        scenarios,
        &by_name,
        fixture["garbage"].as_str().unwrap(),
    );
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let ids: BTreeMap<String, hydrus_core::HashId> = store
        .read(|c| {
            let mut out = BTreeMap::new();
            for sc in scenarios {
                let hash = by_name[sc["file"].as_str().unwrap()]
                    .parse::<hydrus_core::Sha256>()
                    .unwrap();
                out.insert(
                    sc["name"].as_str().unwrap().to_owned(),
                    hydrus_store::master::hash_id(c, &hash)?.unwrap(),
                );
            }
            Ok(out)
        })
        .unwrap();
    let jobs: Vec<(hydrus_core::HashId, JobType)> = scenarios
        .iter()
        .map(|sc| {
            (
                ids[sc["name"].as_str().unwrap()],
                JobType::from_code(sc["job"].as_i64().unwrap()).unwrap(),
            )
        })
        .collect();
    let mandated: BTreeSet<JobType> = jobs.iter().map(|(_, job)| *job).collect();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::delete_lock::DeleteLock {
                    archived: true,
                    ..Default::default()
                },
            )?;
            for (id, job) in jobs {
                add_jobs(ctx.conn(), &[id], job, 0)?;
            }
            Ok(())
        })
        .unwrap();

    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let report = importer
        .run_file_maintenance_of(u64::MAX, u64::MAX, &|job| mandated.contains(&job))
        .unwrap();
    assert_eq!(report.total(), scenarios.len() as u64, "{report:?}");

    // each file
    let snap = store.snapshot();
    let locations: Vec<std::path::PathBuf> = snap
        .storage
        .locations()
        .iter()
        .map(|l| l.path.clone())
        .collect();
    let domains = [
        "hydrus local file storage",
        "combined local file domains",
        "my files",
        "trash",
        "art",
    ];
    let tools = MediaTools::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut problems = Vec::new();
    for sc in scenarios {
        let name = sc["name"].as_str().unwrap();
        let hex = &by_name[sc["file"].as_str().unwrap()];
        let id = ids[name];
        let want = &fixture["files"][name];
        let got = store
            .read(|c| {
                let info: Vec<Value> = c.query_row(
                    "SELECT size, mime, width, height, duration_ms, num_frames, has_audio, num_words, forced_mime
                     FROM files WHERE hash_id = ?1",
                    [id],
                    |r| (0..9).map(|i| r.get::<_, Option<i64>>(i).map(|v| json!(v))).collect(),
                )?;
                let in_domains = |table: &str| -> rusqlite::Result<Vec<&str>> {
                    let mut found: Vec<&str> = Vec::new();
                    for d in domains {
                        let service = snap.services.by_name(d).unwrap().id;
                        if c.query_row(
                            &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE service_id = ?1 AND hash_id = ?2)"),
                            params![service, id],
                            |r| r.get::<_, bool>(0),
                        )? {
                            found.push(d);
                        }
                    }
                    found.sort_unstable();
                    Ok(found)
                };
                let mut jobs: Vec<(i64, bool)> = c
                    .prepare("SELECT job_type, time_can_start <= ?2 FROM file_maintenance_jobs WHERE hash_id = ?1")?
                    .query_map(params![id, now], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                jobs.sort_unstable();
                let one = |sql: &str| c.query_row(sql, [id], |r| r.get::<_, bool>(0));
                Ok(json!({
                    "info": info[..8],
                    "forced_mime": info[8],
                    "jobs": jobs.iter().map(|(job, due)| json!({"job": job, "due": due})).collect::<Vec<_>>(),
                    "current": in_domains("file_domain_current")?,
                    "deleted": in_domains("file_domain_deleted")?,
                    "inbox": one("SELECT EXISTS (SELECT 1 FROM file_inbox WHERE hash_id = ?1)")?,
                    "other_hashes": one("SELECT EXISTS (SELECT 1 FROM hash_digests WHERE hash_id = ?1)")?,
                    "modified": one("SELECT file_modified_ms IS NOT NULL FROM files WHERE hash_id = ?1")?,
                }))
            })
            .unwrap();
        let mut got = got;
        let files: Vec<String> = locations
            .iter()
            .flat_map(|l| find(l, hex, 'f'))
            .map(|p| p.file_name().unwrap().to_string_lossy()[hex.len()..].to_owned())
            .collect();
        got["files"] = json!(files);
        let thumbnails: Vec<_> = locations.iter().flat_map(|l| find(l, hex, 't')).collect();
        got["thumbnail"] = match thumbnails.first() {
            Some(path) => {
                let image = tools
                    .load_image(path, tools.detect_mime(path).unwrap())
                    .unwrap();
                json!([image.width(), image.height()])
            }
            None => Value::Null,
        };
        let mut want = want.clone();
        // (a deleted file's thumbnail goes when the deletion is purged,
        // which the reference had or hadn't got to, and which never
        // happens to media shared with another install)
        if want["current"].as_array().unwrap().is_empty() {
            want["thumbnail"] = Value::Null;
            got["thumbnail"] = Value::Null;
        }
        if got != want {
            problems.push(format!("{name}:\n  want {want}\n  got  {got}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    // the error directory
    let error_dir = native
        .path()
        .join(hydrus_import::maintenance::ERROR_DIR_NAME);
    let mut missing_hashes = Vec::new();
    let mut text = serde_json::Map::new();
    let mut exported = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(&error_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let content = || std::fs::read_to_string(error_dir.join(&name)).unwrap();
        if name.ends_with(" missing hashes.txt") {
            missing_hashes.extend(content().split_whitespace().map(str::to_owned));
        } else if std::path::Path::new(&name)
            .extension()
            .is_some_and(|e| e == "txt")
        {
            let mut lines: Vec<String> = content().lines().map(str::to_owned).collect();
            lines.sort();
            text.insert(name, json!(lines));
        } else {
            exported.push(name);
        }
    }
    missing_hashes.sort();
    assert_eq!(
        json!({"missing_hashes": missing_hashes, "text": text, "files": exported}),
        fixture["error_dir"]
    );
    assert_eq!(report.bad_files, missing_hashes.len() as u64);

    // what was sent to be downloaded again
    let mut redownload = report.redownload.clone();
    redownload.sort();
    let want: Vec<&str> = fixture["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["url"].as_str().unwrap())
        .collect();
    assert_eq!(redownload, want);
}
