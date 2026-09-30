//! The similar-files search on generated near-duplicates, against the
//! reference's (`oracle/fixtures/similar_files.json`, made by
//! `oracle/record_similar_files.py`): the same files imported in the same
//! order, searched at distance 2 and then 4, must give the same perceptual
//! hashes, the same potential duplicate pairs at the same distances, and the
//! same files searched.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::similar::{self, SimilarFilesSettings};

fn pairs(store: &Store) -> BTreeSet<(String, String, u32)> {
    store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT lower(hex(a.sha256)), lower(hex(b.sha256)), p.distance FROM potential_pairs AS p
                 JOIN dup_groups AS ga ON ga.group_id = p.smaller_group_id
                 JOIN dup_groups AS gb ON gb.group_id = p.larger_group_id
                 JOIN hashes AS a ON a.hash_id = ga.king_hash_id
                 JOIN hashes AS b ON b.hash_id = gb.king_hash_id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    let (a, b): (String, String) = (r.get(0)?, r.get(1)?);
                    let (a, b) = if a <= b { (a, b) } else { (b, a) };
                    Ok((a, b, r.get(2)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn searched(store: &Store) -> BTreeMap<String, i64> {
    store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT lower(hex(h.sha256)), s.searched_distance FROM similar_search_status AS s
                 JOIN hashes AS h USING (hash_id)",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(-1)))
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
        .unwrap()
}

fn phashes(store: &Store) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let rows: Vec<(String, String)> = store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT lower(hex(h.sha256)), lower(hex(p.phash)) FROM file_perceptual_hashes
                 JOIN perceptual_hashes AS p USING (phash_id) JOIN hashes AS h USING (hash_id)",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
        .unwrap();
    for (hash, phash) in rows {
        out.entry(hash).or_default().push(phash);
    }
    for list in out.values_mut() {
        list.sort();
    }
    out
}

#[test]
fn the_search_finds_what_the_reference_found() {
    let recorded = hydrus_testkit::fixture_json("similar_files.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    for file in recorded["files"].as_array().unwrap() {
        let name = file[0].as_str().unwrap();
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("similar_files/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
        assert_eq!(
            result.hash.unwrap().to_hex(),
            file[1].as_str().unwrap(),
            "{name}"
        );
    }
    for distance in [2u32, 4] {
        let expected = &recorded["searches"][distance.to_string()];
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &SimilarFilesSettings {
                        search_distance: distance,
                        ..SimilarFilesSettings::default()
                    },
                )
            })
            .unwrap();
        while similar::run_search(&store, 10).unwrap() > 0 {}

        let want_phashes: BTreeMap<String, Vec<String>> =
            serde_json::from_value(expected["phashes"].clone()).unwrap();
        assert_eq!(phashes(&store), want_phashes, "perceptual hashes");
        let want_searched: BTreeMap<String, i64> =
            serde_json::from_value(expected["searched"].clone()).unwrap();
        assert_eq!(searched(&store), want_searched, "searched at {distance}");
        let want_pairs: BTreeSet<(String, String, u32)> = expected["pairs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p[0].as_str().unwrap().to_owned(),
                    p[1].as_str().unwrap().to_owned(),
                    p[2].as_u64().unwrap() as u32,
                )
            })
            .collect();
        assert_eq!(pairs(&store), want_pairs, "pairs at {distance}");
    }
}
