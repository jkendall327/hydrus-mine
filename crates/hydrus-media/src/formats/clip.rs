//! Clip Studio Paint files (`HydrusClipHandling`): an SQLite database
//! embedded after a proprietary header holds the canvas size, timeline and
//! a PNG preview.

use std::io::Write;

use crate::error::{MediaError, Result};

/// Canvas size in pixels and, for animations, frames and duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClipProperties {
    pub width: u32,
    pub height: u32,
    /// The reference computes this as a float (`num_frames / fps * 1000`).
    pub duration_ms: Option<f64>,
    pub num_frames: Option<u64>,
}

fn with_db<T>(data: &[u8], f: impl FnOnce(&rusqlite::Connection) -> Result<T>) -> Result<T> {
    let start = data
        .windows(15)
        .position(|w| w == b"SQLite format 3")
        .ok_or_else(|| MediaError::damaged("This clip file had no internal SQLite file!"))?;
    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(&data[start..])?;
    tmp.flush()?;
    let db = rusqlite::Connection::open_with_flags(
        tmp.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| {
        MediaError::damaged("This clip file seemed to have an invalid internal SQLite file!")
    })?;
    f(&db)
}

fn sql_err(e: &rusqlite::Error) -> MediaError {
    MediaError::damaged(format!("could not read this clip file's database: {e}"))
}

/// `GetClipProperties`.
pub(crate) fn properties(data: &[u8]) -> Result<ClipProperties> {
    with_db(data, |db| {
        let (w, h, unit, dpi): (f64, f64, f64, f64) = db
            .query_row(
                "SELECT CanvasWidth, CanvasHeight, CanvasUnit, CanvasResolution FROM Canvas;",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|e| sql_err(&e))?;
        let has_timeline = db
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE name = 'TimeLine';",
                [],
                |_| Ok(()),
            )
            .is_ok();
        let (mut duration_ms, mut num_frames) = (None, None);
        if has_timeline {
            let row: rusqlite::Result<(f64, f64, f64)> = db.query_row(
                "SELECT StartFrame, FrameRate, EndFrame from TimeLine;",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            );
            if let Ok((start, mut rate, end)) = row {
                let frames = (end - start).trunc();
                if rate == 0.0 {
                    rate = 24.0;
                }
                num_frames = Some(frames.max(0.0) as u64);
                duration_ms = Some(frames / rate * 1000.0);
            }
        }
        #[allow(clippy::float_cmp)]
        let multiplier = if unit == 1.0 {
            dpi / 2.54
        } else if unit == 2.0 {
            dpi / 25.4
        } else if unit == 3.0 {
            dpi
        } else if unit == 5.0 {
            dpi / 72.0
        } else {
            1.0
        };
        let px = |v: f64| (v * multiplier).round_ties_even().max(0.0) as u32;
        Ok(ClipProperties {
            width: px(w),
            height: px(h),
            duration_ms,
            num_frames,
        })
    })
}

/// `ExtractDBPNGToPath`: the stored canvas preview (a PNG).
pub(crate) fn preview_png(data: &[u8]) -> Result<Vec<u8>> {
    with_db(data, |db| {
        db.query_row("SELECT ImageData FROM CanvasPreview;", [], |r| r.get(0))
            .map_err(|e| sql_err(&e))
    })
}
