//! Changing file storage granularity (`RegranulariseFileStorage`,
//! `EstimateBaseLocationGranularity`, the files manager's `Granularise`):
//! moving every file from its prefix folder of one length to the folder of
//! the other (`f3a` to `f3a/b` and back), for the store's locations or an
//! offline folder such as a backup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, StoreError};
use crate::storage::prefix_dir;

const HEX: &[u8] = b"0123456789abcdef";

/// Every prefix of `kind` (`f`, `t`) with `length` hex characters.
pub fn prefixes(kind: char, length: usize) -> Vec<String> {
    let mut out = vec![kind.to_string()];
    for _ in 0..length {
        out = out
            .into_iter()
            .flat_map(|p| HEX.iter().map(move |&c| format!("{p}{}", c as char)))
            .collect();
    }
    out
}

/// A folder's granularity, judged from the prefix folders in it
/// (`EstimateBaseLocationGranularity`): the most common depth of hex
/// folders, if any.
pub fn estimate(path: &Path) -> Option<usize> {
    let is_hex = |s: &str| s.bytes().all(|b| HEX.contains(&b));
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut weird = 0;
    for entry in std::fs::read_dir(path).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let length = match name.len() {
                3 if is_hex(&name[1..]) => 2,
                1 | 2 if is_hex(&name) => name.len(),
                _ => continue,
            };
            let deeper = estimate(&entry.path()).unwrap_or(0);
            *counts.entry(length + deeper).or_default() += 1;
        } else {
            weird += 1;
            if weird > 16 {
                return None;
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(length, count)| (count, std::cmp::Reverse(length)))
        .map(|(length, _)| length)
}

/// What a regranularisation did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Each new prefix and the location its folder is in.
    pub prefixes: BTreeMap<String, PathBuf>,
    pub moved: usize,
    pub weird_dirs: usize,
    pub weird_files: usize,
}

/// How a regranularisation goes: `say` takes its two lines of progress,
/// and `paused`/`cancelled` are asked between chunks of files.
#[allow(missing_debug_implementations)] // (closures)
pub struct Progress<'a> {
    pub say: &'a mut dyn FnMut(Option<String>, Option<String>),
    pub paused: &'a dyn Fn() -> bool,
    pub cancelled: &'a dyn Fn() -> bool,
}

/// Move every file of `kinds` under `bases` from prefix folders of
/// `from` hex characters to those of `to` (`RegranulariseFileStorage`).
pub fn regranularise(
    bases: &[PathBuf],
    kinds: &[char],
    from: usize,
    to: usize,
    progress: &mut Progress<'_>,
) -> Result<Outcome> {
    if from == to {
        return Err(StoreError::Invalid(format!(
            "Called to granularise a file storage folder, but the starting and ending granularisation was the same ({from})!!"
        )));
    }
    let starting: Vec<String> = kinds.iter().flat_map(|&k| prefixes(k, from)).collect();
    let ending: Vec<String> = kinds.iter().flat_map(|&k| prefixes(k, to)).collect();
    (progress.say)(Some("checking sources".into()), None);
    let mut sources: Vec<(String, PathBuf)> = Vec::new();
    for base in bases {
        for prefix in &starting {
            if prefix_dir(base, prefix).is_dir() {
                sources.push((prefix.clone(), base.clone()));
            }
        }
    }
    let mut outcome = Outcome::default();
    let total = sources.len();
    for (done, (prefix, base)) in sources.iter().enumerate() {
        (progress.say)(
            Some(format!(
                "Working \"{prefix}\" ({})\u{2026}",
                hydrus_core::numbers::value_range(done as u64, total as u64)
            )),
            None,
        );
        let targets: Vec<&String> = if from < to {
            ending
                .iter()
                .filter(|e| e.starts_with(prefix.as_str()))
                .collect()
        } else {
            ending
                .iter()
                .filter(|e| prefix.starts_with(e.as_str()))
                .collect()
        };
        for target in targets {
            outcome
                .prefixes
                .entry(target.clone())
                .or_insert_with(|| base.clone());
            std::fs::create_dir_all(prefix_dir(&outcome.prefixes[target], target))?;
        }
        let source = prefix_dir(base, prefix);
        let mut jobs: Vec<(PathBuf, PathBuf)> = Vec::new();
        for entry in std::fs::read_dir(&source)?.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                // (the new deeper folders are expected; anything else isn't)
                let expected = outcome
                    .prefixes
                    .iter()
                    .any(|(p, b)| prefix_dir(b, p).starts_with(&path));
                if !expected {
                    outcome.weird_dirs += 1;
                }
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = &prefix[..1];
            let wanted = format!("{kind}{}", name.get(..to).unwrap_or(""));
            match outcome.prefixes.get(&wanted) {
                Some(dest_base)
                    if name.len() >= to && name[..to].bytes().all(|b| HEX.contains(&b)) =>
                {
                    jobs.push((path, prefix_dir(dest_base, &wanted).join(&name)));
                }
                _ => outcome.weird_files += 1,
            }
        }
        for (i, chunk) in jobs.chunks(100).enumerate() {
            (progress.say)(
                None,
                Some(format!(
                    "File {}",
                    hydrus_core::numbers::value_range((i * 100) as u64, jobs.len() as u64)
                )),
            );
            while (progress.paused)() && !(progress.cancelled)() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if (progress.cancelled)() {
                return Err(StoreError::Invalid("Cancelled by user".into()));
            }
            for (from_path, to_path) in chunk {
                if std::fs::rename(from_path, to_path).is_err() {
                    std::fs::copy(from_path, to_path)?;
                    std::fs::remove_file(from_path)?;
                }
            }
            outcome.moved += chunk.len();
        }
        if from > to && std::fs::read_dir(&source).is_ok_and(|mut d| d.next().is_none()) {
            std::fs::remove_dir(&source)?;
        }
    }
    Ok(outcome)
}

/// Regranularise the store's own file storage (`Granularise`), then record
/// where each new prefix folder is.
pub fn granularise_store(
    store: &crate::Store,
    from: usize,
    to: usize,
    progress: &mut Progress<'_>,
) -> Result<Outcome> {
    let storage = store.read(crate::storage::FileStorage::load)?;
    if storage.granularity() != from {
        return Err(StoreError::Invalid(format!(
            "Hey, your current granularity is {}, but it needs to be {from} to start this process! Something went wrong, please tell hydev.",
            storage.granularity()
        )));
    }
    let bases: Vec<PathBuf> = storage.locations().iter().map(|l| l.path.clone()).collect();
    let outcome = match regranularise(&bases, &['f', 't'], from, to, progress) {
        Ok(outcome) => outcome,
        Err(e) => {
            // (cancelled or failed: put every file back where the records
            // say, as the reference undoes the job)
            let (no, say) = (|| false, &mut *progress.say);
            let mut undo = Progress {
                say,
                paused: &no,
                cancelled: &no,
            };
            regranularise(&bases, &['f', 't'], to, from, &mut undo)?;
            return Err(e);
        }
    };
    // each new prefix where its old one was (the first, going down), as
    // the records say, whether or not its folder had files
    let old: Vec<(String, PathBuf)> = store.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT s.prefix, l.path FROM storage_subfolders s JOIN storage_locations l USING (location_id)
             ORDER BY s.prefix",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, PathBuf::from(r.get::<_, String>(1)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    })?;
    let mut map: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (prefix, base) in old {
        let kind = prefix.chars().next().unwrap_or('f');
        if from < to {
            for new in prefixes(kind, to)
                .into_iter()
                .filter(|p| p.starts_with(&prefix))
            {
                map.entry(new).or_insert_with(|| base.clone());
            }
        } else {
            map.entry(prefix[..=to].to_owned()).or_insert(base);
        }
    }
    store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        conn.execute("DELETE FROM storage_subfolders", [])?;
        let mut insert = conn.prepare(
            "INSERT INTO storage_subfolders (prefix, location_id)
             SELECT ?1, location_id FROM storage_locations WHERE path = ?2",
        )?;
        for (prefix, base) in &map {
            insert.execute(rusqlite::params![prefix, base.to_string_lossy()])?;
        }
        Ok(())
    })?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_goes_to_three_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        for prefix in prefixes('f', 2) {
            std::fs::create_dir_all(prefix_dir(&base, &prefix)).unwrap();
        }
        let file = "3ab4567.png";
        std::fs::write(prefix_dir(&base, "f3a").join(file), b"x").unwrap();
        std::fs::write(prefix_dir(&base, "f3a").join("desktop.ini"), b"?").unwrap();
        assert_eq!(estimate(&base), Some(2));
        let (mut said, paused, cancelled) = (0, || false, || false);
        let mut say = |_: Option<String>, _: Option<String>| said += 1;
        let mut progress = Progress {
            say: &mut say,
            paused: &paused,
            cancelled: &cancelled,
        };
        let up = regranularise(std::slice::from_ref(&base), &['f'], 2, 3, &mut progress).unwrap();
        assert_eq!(up.moved, 1);
        assert_eq!(up.weird_files, 1);
        assert_eq!(up.prefixes.len(), 4096);
        assert!(base.join("f3a/b").join(file).is_file());
        assert_eq!(estimate(&base), Some(3));
        let down = regranularise(std::slice::from_ref(&base), &['f'], 3, 2, &mut progress).unwrap();
        assert_eq!(down.moved, 1);
        assert!(base.join("f3a").join(file).is_file());
    }

    #[test]
    fn the_store_records_its_new_folders() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        let (paused, cancelled) = (|| false, || false);
        let mut say = |_: Option<String>, _: Option<String>| {};
        let mut progress = Progress {
            say: &mut say,
            paused: &paused,
            cancelled: &cancelled,
        };
        granularise_store(&store, 2, 3, &mut progress).unwrap();
        let storage = store.read(crate::storage::FileStorage::load).unwrap();
        assert_eq!(storage.granularity(), 3);
        assert!(granularise_store(&store, 2, 3, &mut progress).is_err());
        granularise_store(&store, 3, 2, &mut progress).unwrap();
        assert_eq!(
            store
                .read(crate::storage::FileStorage::load)
                .unwrap()
                .granularity(),
            2
        );
        // a cancelled job is undone
        let file =
            crate::storage::prefix_dir(&dir.path().join("client_files"), "f3a").join("3abc.png");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"x").unwrap();
        let stop = || true;
        let mut quiet = |_: Option<String>, _: Option<String>| {};
        let mut cancelling = Progress {
            say: &mut quiet,
            paused: &paused,
            cancelled: &stop,
        };
        assert!(granularise_store(&store, 2, 3, &mut cancelling).is_err());
        assert!(file.is_file());
        assert_eq!(
            store
                .read(crate::storage::FileStorage::load)
                .unwrap()
                .granularity(),
            2
        );
    }
}
