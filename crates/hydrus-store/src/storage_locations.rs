//! Database > locations (`MoveMediaFilesPanel` and the files manager's
//! rebalancing): the media locations with their weights, size limits and
//! usage, the thumbnail location override, and moving prefix folders
//! between locations until each holds its ideal share.
//!
//! As the reference does, a location's current usage is estimated from the
//! share of prefix folders it holds times the total size of the files in
//! local storage; a removed location that still holds folders stays with
//! weight 0 until they are moved out.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension as _, params};

use crate::error::{Result, StoreError};

/// One location as the list shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    /// Its ideal weight (0: to be emptied and removed).
    pub weight: i64,
    pub max_bytes: Option<i64>,
    /// Shares of the file and thumbnail prefix folders it holds (0 to 1).
    pub files_share: f64,
    pub thumbnails_share: f64,
    pub thumbnail_override: bool,
}

/// The locations, the local files' total size and count.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Review {
    pub locations: Vec<Location>,
    pub total_bytes: u64,
    pub total_files: u64,
}

fn local_storage(conn: &Connection) -> Result<hydrus_core::ServiceId> {
    let services = crate::services::ServiceRegistry::load(conn)?;
    Ok(services
        .builtin(hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)?
        .id)
}

/// A location's path, weight, limit, override flag and file and thumbnail
/// prefix counts.
type LocationRow = (String, Option<i64>, Option<i64>, bool, i64, i64);

/// Read the locations and what they hold.
pub fn review(conn: &Connection) -> Result<Review> {
    let local = local_storage(conn)?;
    let (total_bytes, total_files): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(f.size), 0), COUNT(*) FROM file_domain_current c
         JOIN files f USING (hash_id) WHERE c.service_id = ?",
        [local],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT l.location_id, l.path, l.ideal_weight, l.max_bytes, l.is_thumbnail_override,
                (SELECT COUNT(*) FROM storage_subfolders s WHERE s.location_id = l.location_id AND s.prefix LIKE 'f%'),
                (SELECT COUNT(*) FROM storage_subfolders s WHERE s.location_id = l.location_id AND s.prefix LIKE 't%')
         FROM storage_locations l ORDER BY l.path",
    )?;
    let rows: Vec<LocationRow> = stmt
        .query_map([], |r| {
            Ok((
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let (files, thumbs): (i64, i64) = rows.iter().fold((0, 0), |(f, t), r| (f + r.4, t + r.5));
    #[allow(clippy::cast_precision_loss)] // (prefix counts are small)
    let share = |n: i64, of: i64| if of == 0 { 0.0 } else { n as f64 / of as f64 };
    Ok(Review {
        locations: rows
            .into_iter()
            .map(
                |(path, weight, max_bytes, thumbnail_override, f, t)| Location {
                    path: PathBuf::from(path),
                    weight: weight.unwrap_or(0),
                    max_bytes,
                    files_share: share(f, files),
                    thumbnails_share: share(t, thumbs),
                    thumbnail_override,
                },
            )
            .collect(),
        total_bytes: u64::try_from(total_bytes).unwrap_or(0),
        total_files: u64::try_from(total_files).unwrap_or(0),
    })
}

/// Each location's ideal share of `total_bytes` (`STATICGetIdealWeights`):
/// limited locations that can't hold their share hold what they can, and
/// the rest is shared by weight. `locations` are (weight, max bytes).
pub fn ideal_shares(total_bytes: u64, locations: &[(i64, Option<i64>)]) -> Vec<f64> {
    #[allow(clippy::cast_precision_loss)] // (sizes and weights as fractions)
    let total = if total_bytes == 0 {
        1_048_576.0
    } else {
        total_bytes as f64
    };
    let mut result = vec![0.0; locations.len()];
    let mut remaining_weight: i64 = locations.iter().map(|l| l.0).sum();
    let mut remaining_share = 1.0;
    let mut limited: Vec<usize> = (0..locations.len())
        .filter(|&i| locations[i].1.is_some())
        .collect();
    limited.sort_by_key(|&i| locations[i].1);
    let mut unlimited: Vec<usize> = (0..locations.len())
        .filter(|&i| locations[i].1.is_none())
        .collect();
    let mut next_round = Vec::new();
    let mut eliminated = false;
    while !limited.is_empty() {
        let i = limited.remove(0);
        let (weight, max) = locations[i];
        #[allow(clippy::cast_precision_loss)]
        let wanted = if remaining_weight == 0 {
            0.0
        } else {
            weight as f64 / remaining_weight as f64 * remaining_share
        };
        #[allow(clippy::cast_precision_loss)]
        let can_hold = max.unwrap_or(0) as f64 / total;
        if can_hold < wanted {
            result[i] = can_hold;
            remaining_share -= can_hold;
            remaining_weight -= weight;
            eliminated = true;
        } else {
            next_round.push(i);
        }
        if limited.is_empty() {
            if eliminated {
                limited = std::mem::take(&mut next_round);
                eliminated = false;
            } else {
                unlimited.append(&mut next_round);
            }
        }
    }
    for i in unlimited {
        #[allow(clippy::cast_precision_loss)]
        if remaining_weight != 0 {
            result[i] = locations[i].0 as f64 / remaining_weight as f64 * remaining_share;
        }
    }
    result
}

fn location_id(conn: &Connection, path: &Path) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT location_id FROM storage_locations WHERE path = ?",
            [path.to_string_lossy()],
            |r| r.get(0),
        )
        .optional()?)
}

/// "add new location for files": a location of weight 1 (or, if it is
/// there being emptied, its weight back to 1).
pub fn add_location(conn: &Connection, path: &Path) -> Result<()> {
    match location_id(conn, path)? {
        Some(id) => {
            conn.execute(
                "UPDATE storage_locations SET ideal_weight = 1 WHERE location_id = ?",
                [id],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO storage_locations (path, ideal_weight, max_bytes, is_thumbnail_override)
                 VALUES (?, 1, NULL, 0)",
                [path.to_string_lossy()],
            )?;
        }
    }
    Ok(())
}

/// Change a location's weight by `by` (not below 1, as the reference's
/// buttons allow).
pub fn adjust_weight(conn: &Connection, path: &Path, by: i64) -> Result<()> {
    let id = location_id(conn, path)?.ok_or_else(|| missing(path))?;
    conn.execute(
        "UPDATE storage_locations SET ideal_weight = MAX(1, COALESCE(ideal_weight, 0) + ?)
         WHERE location_id = ?",
        params![by, id],
    )?;
    Ok(())
}

/// Set (or clear) a location's size limit.
pub fn set_max_bytes(conn: &Connection, path: &Path, max: Option<i64>) -> Result<()> {
    let id = location_id(conn, path)?.ok_or_else(|| missing(path))?;
    conn.execute(
        "UPDATE storage_locations SET max_bytes = ? WHERE location_id = ?",
        params![max, id],
    )?;
    Ok(())
}

fn missing(path: &Path) -> StoreError {
    StoreError::Invalid(format!("There is no location \"{}\".", path.display()))
}

/// "remove location": gone if it holds nothing, else weight 0 so the next
/// rebalance empties it.
pub fn remove_location(conn: &Connection, path: &Path) -> Result<()> {
    let id = location_id(conn, path)?.ok_or_else(|| missing(path))?;
    let holds: i64 = conn.query_row(
        "SELECT COUNT(*) FROM storage_subfolders WHERE location_id = ?",
        [id],
        |r| r.get(0),
    )?;
    if holds == 0 {
        conn.execute("DELETE FROM storage_locations WHERE location_id = ?", [id])?;
    } else {
        conn.execute(
            "UPDATE storage_locations SET ideal_weight = 0, is_thumbnail_override = 0 WHERE location_id = ?",
            [id],
        )?;
    }
    Ok(())
}

/// Set (or clear) where all thumbnails go.
pub fn set_thumbnail_override(conn: &Connection, path: Option<&Path>) -> Result<()> {
    conn.execute("UPDATE storage_locations SET is_thumbnail_override = 0", [])?;
    if let Some(path) = path {
        let id = if let Some(id) = location_id(conn, path)? {
            id
        } else {
            conn.execute(
                "INSERT INTO storage_locations (path, ideal_weight, max_bytes, is_thumbnail_override)
                 VALUES (?, 0, NULL, 0)",
                [path.to_string_lossy()],
            )?;
            conn.last_insert_rowid()
        };
        conn.execute(
            "UPDATE storage_locations SET is_thumbnail_override = 1 WHERE location_id = ?",
            [id],
        )?;
    }
    Ok(())
}

/// One prefix folder to move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub prefix: String,
    pub from: PathBuf,
    pub to: PathBuf,
}

/// The next folder to move (`_GetRebalanceTuple`), if any: a file folder
/// from a location over its share (or its limit, or being removed) to one
/// under its share with room; else a thumbnail folder to where its file
/// folder (or the thumbnail override) is.
pub fn next_move(conn: &Connection) -> Result<Option<Move>> {
    let review = review(conn)?;
    let mut subfolders: Vec<(String, PathBuf)> = conn
        .prepare(
            "SELECT s.prefix, l.path FROM storage_subfolders s JOIN storage_locations l USING (location_id)
             ORDER BY s.prefix",
        )?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, PathBuf::from(r.get::<_, String>(1)?))))?
        .collect::<rusqlite::Result<_>>()?;
    subfolders.sort();
    let file_prefixes: Vec<&(String, PathBuf)> = subfolders
        .iter()
        .filter(|(p, _)| p.starts_with('f'))
        .collect();
    if file_prefixes.is_empty() {
        return Ok(None);
    }
    #[allow(clippy::cast_precision_loss)]
    let total = review.total_bytes.max(1) as f64;
    #[allow(clippy::cast_precision_loss)]
    let one = 1.0 / file_prefixes.len() as f64;
    let one_bytes = one * total;
    let media: Vec<&Location> = review
        .locations
        .iter()
        .filter(|l| l.weight > 0 || l.files_share > 0.0)
        .collect();
    let total_weight: i64 = media.iter().map(|l| l.weight).sum();
    if total_weight == 0 {
        return Ok(None);
    }
    let able = |l: &Location, bytes: f64| {
        #[allow(clippy::cast_precision_loss)]
        let fits = l
            .max_bytes
            .is_none_or(|max| bytes + one_bytes <= max as f64);
        fits && l.weight != 0
    };
    // first round: who can't take more (and must shed, if over a limit or
    // being removed)
    let mut lost = 0.0;
    let mut desperate = Vec::new();
    let mut second = Vec::new();
    for &l in &media {
        let bytes = l.files_share * total;
        if able(l, bytes) {
            second.push(l);
        } else {
            #[allow(clippy::cast_precision_loss)]
            match l.max_bytes {
                None => lost = l.weight as f64 / total_weight as f64,
                Some(max) => lost += max as f64 / total,
            }
            #[allow(clippy::cast_precision_loss)]
            let over_limit = l.max_bytes.is_some_and(|max| bytes > max as f64);
            if l.weight == 0 || over_limit {
                desperate.push(l);
            }
        }
    }
    let second_weight: i64 = second.iter().map(|l| l.weight).sum();
    let (mut overweight, mut starving, mut available) = (Vec::new(), Vec::new(), Vec::new());
    for &l in &second {
        let bytes = l.files_share * total;
        let share = if lost < 1.0 {
            l.files_share / (1.0 - lost)
        } else {
            l.files_share
        };
        #[allow(clippy::cast_precision_loss)]
        let ideal = if second_weight == 0 {
            0.0
        } else {
            l.weight as f64 / second_weight as f64
        };
        if share - one > ideal {
            overweight.push(l);
        }
        if share + one <= ideal && able(l, bytes) {
            starving.push(l);
        } else if able(l, bytes) {
            available.push(l);
        }
    }
    let free = |l: &&Location| fs4::available_space(&l.path).unwrap_or(0);
    desperate.sort_by_key(|l| free(l));
    overweight.sort_by_key(|l| free(l));
    starving.sort_by_key(|l| std::cmp::Reverse(free(l)));
    available.sort_by_key(|l| std::cmp::Reverse(free(l)));
    let (sources, destinations): (Vec<&Location>, Vec<&Location>) = if !desperate.is_empty() {
        (desperate, starving.into_iter().chain(available).collect())
    } else if !overweight.is_empty() {
        (overweight, starving)
    } else {
        (Vec::new(), Vec::new())
    };
    if let (Some(source), Some(destination)) = (sources.first(), destinations.first()) {
        if let Some((prefix, _)) = file_prefixes.iter().find(|(_, path)| *path == source.path) {
            return Ok(Some(Move {
                prefix: prefix.clone(),
                from: source.path.clone(),
                to: destination.path.clone(),
            }));
        }
        return Ok(None);
    }
    // thumbnails follow their files, or go to the override
    let override_path = review
        .locations
        .iter()
        .find(|l| l.thumbnail_override)
        .map(|l| l.path.clone());
    for (prefix, path) in subfolders.iter().filter(|(p, _)| p.starts_with('t')) {
        let correct = if let Some(path) = &override_path {
            Some(path.clone())
        } else {
            let file_prefix = format!("f{}", &prefix[1..]);
            subfolders
                .iter()
                .find(|(p, _)| *p == file_prefix)
                .map(|(_, path)| path.clone())
        };
        if let Some(correct) = correct
            && correct != *path
        {
            return Ok(Some(Move {
                prefix: prefix.clone(),
                from: path.clone(),
                to: correct,
            }));
        }
    }
    Ok(None)
}

/// Move a prefix folder (`relocate_client_files`): its files to the new
/// location (renamed if the filesystems allow, else copied then removed),
/// then the record.
pub fn apply_move(store: &crate::Store, step: &Move) -> Result<()> {
    let source = step.from.join(&step.prefix);
    let dest = step.to.join(&step.prefix);
    std::fs::create_dir_all(&step.to)?;
    if source.exists() {
        if std::fs::rename(&source, &dest).is_err() {
            std::fs::create_dir_all(&dest)?;
            crate::backup::mirror_tree(&source, &dest, &mut |_| {}, &|| false)?;
            std::fs::remove_dir_all(&source)?;
        }
    } else {
        std::fs::create_dir_all(&dest)?;
    }
    let (prefix, to) = (step.prefix.clone(), step.to.clone());
    store.write_and_refresh(move |ctx| {
        let id = location_id(ctx.conn(), &to)?.ok_or_else(|| missing(&to))?;
        ctx.conn().execute(
            "UPDATE storage_subfolders SET location_id = ? WHERE prefix = ?",
            params![id, prefix],
        )?;
        // a removed location that is now empty goes
        ctx.conn().execute(
            "DELETE FROM storage_locations WHERE ideal_weight = 0 AND is_thumbnail_override = 0
             AND location_id NOT IN (SELECT location_id FROM storage_subfolders)",
            [],
        )?;
        Ok(())
    })
}

/// "move files now" (`Rebalance`): move folders until none need to,
/// saying each move, stopping if `cancelled`; how many moved.
pub fn rebalance(
    store: &crate::Store,
    say: &mut dyn FnMut(String),
    cancelled: &dyn Fn() -> bool,
) -> Result<usize> {
    let mut moved = 0;
    while !cancelled() {
        let Some(step) = store.read(next_move)? else {
            break;
        };
        say(format!(
            "Moving \"{}\" to \"{}\".",
            step.from.join(&step.prefix).display(),
            step.to.join(&step.prefix).display()
        ));
        apply_move(store, &step)?;
        moved += 1;
        // (a guard against a move that changes nothing)
        if moved > 100_000 {
            break;
        }
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_follow_weights_and_limits() {
        let close = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(close(
            &ideal_shares(1000, &[(1, None), (1, None)]),
            &[0.5, 0.5]
        ));
        assert!(close(
            &ideal_shares(1000, &[(3, None), (1, None)]),
            &[0.75, 0.25]
        ));
        // a limit below its share holds what it can; the rest share the rest
        assert!(close(
            &ideal_shares(1000, &[(1, Some(100)), (1, None), (2, None)]),
            &[0.1, 0.3, 0.6]
        ));
        assert!(close(
            &ideal_shares(1000, &[(1, Some(900)), (1, None)]),
            &[0.5, 0.5]
        ));
    }

    #[test]
    fn a_second_location_takes_half_and_thumbnails_follow() {
        let (home, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let store = crate::Store::open(home.path()).unwrap();
        assert_eq!(store.read(next_move).unwrap(), None);
        let second = other.path().to_path_buf();
        store
            .write_and_refresh(move |ctx| add_location(ctx.conn(), &second))
            .unwrap();
        let first = store.read(next_move).unwrap().unwrap();
        assert!(first.prefix.starts_with('f'));
        assert_eq!(first.to, other.path());
        let mut said = 0;
        let moved = rebalance(&store, &mut |_| said += 1, &|| false).unwrap();
        assert_eq!(moved, said);
        let review = store.read(review).unwrap();
        for location in &review.locations {
            assert!(
                (location.files_share - 0.5).abs() <= 1.0 / 256.0,
                "{location:?}"
            );
            assert!((location.thumbnails_share - location.files_share).abs() < 1e-9);
        }
        assert_eq!(store.read(next_move).unwrap(), None);
        // removing the second moves everything back, and it goes
        let second = other.path().to_path_buf();
        store
            .write_and_refresh(move |ctx| remove_location(ctx.conn(), &second))
            .unwrap();
        rebalance(&store, &mut |_| {}, &|| false).unwrap();
        let review = store.read(super::review).unwrap();
        assert_eq!(review.locations.len(), 1);
        assert!((review.locations[0].files_share - 1.0).abs() < 1e-9);
    }
}
