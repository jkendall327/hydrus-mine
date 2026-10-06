//! Global clear and the reference's minimum-before-maximum statistics cull.
use crate::{Result, StoreError, settings::FileViewingStatistics};
use hydrus_core::CanvasType;
use rusqlite::{Connection, params};

fn integer_bound(value: u64) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| StoreError::Invalid("Viewing time exceeds the database integer range.".into()))
}

/// Delete counts, durations and last-viewed timestamps for every canvas.
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM file_viewing_stats", [])?;
    Ok(())
}

/// Delete the viewing records of these files only (the thumbnail menu's
/// manage > viewing stats > clear).
pub fn clear_files(conn: &Connection, files: &[hydrus_core::HashId]) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM file_viewing_stats WHERE hash_id IN rarray(?1)",
        [crate::master::id_array(files)],
    )?)
}

/// Read the current rules at acceptance and cull both viewers atomically in the
/// caller's writer transaction. Other canvas categories and timestamps survive.
pub fn cull(conn: &Connection) -> Result<()> {
    let settings: FileViewingStatistics = crate::settings::get(conn)?;
    let limits = [
        (
            "Media",
            CanvasType::MediaViewer,
            settings.media_min_ms,
            settings.media_max_ms,
        ),
        (
            "Preview",
            CanvasType::Preview,
            settings.preview_min_ms,
            settings.preview_max_ms,
        ),
    ];
    // Validate all bounds before the first write, as the reference does.
    for (name, _, minimum, maximum) in limits {
        if minimum.zip(maximum).is_some_and(|(min, max)| min > max) {
            return Err(StoreError::Invalid(format!(
                "{name} min was greater than {} max! Abandoning cull now!",
                name.to_ascii_lowercase()
            )));
        }
        for value in [minimum, maximum].into_iter().flatten() {
            integer_bound(value)?;
        }
    }
    for (_, canvas, minimum, maximum) in limits {
        if let Some(minimum) = minimum {
            let minimum = integer_bound(minimum)?;
            conn.execute(
                "UPDATE file_viewing_stats SET views = CAST(viewtime_ms / ?1 AS INTEGER)
                 WHERE views * ?1 > viewtime_ms AND canvas_type = ?2",
                params![minimum, canvas.code()],
            )?;
        }
        if let Some(maximum) = maximum {
            let maximum = integer_bound(maximum)?;
            conn.execute(
                "UPDATE file_viewing_stats SET viewtime_ms = views * ?1
                 WHERE viewtime_ms > views * ?1 AND canvas_type = ?2",
                params![maximum, canvas.code()],
            )?;
        }
    }
    Ok(())
}
