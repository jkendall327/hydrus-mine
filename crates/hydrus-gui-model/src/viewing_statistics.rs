//! Owned display intervals and the reference's cap-before-minimum view policy.
//! Settings are read when an interval ends. Filtering decisions do not roll back
//! time actually spent viewing; repeated display of the same media is one interval.
use hydrus_core::{CanvasType, HashId};
use hydrus_store::{Store, settings::FileViewingStatistics};
use std::sync::Arc;

/// One counted display interval, normalized to the persisted canvas category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completed {
    pub canvas: CanvasType,
    pub elapsed_ms: u64,
}

/// The actual viewer/filter policy. Explicit Client API viewtime writes use the
/// API's independent existing path; they do not call this timed-display helper.
pub fn completed(
    settings: &FileViewingStatistics,
    canvas: CanvasType,
    duration_ms: Option<u64>,
    elapsed_ms: u64,
) -> Option<Completed> {
    if !settings.active {
        return None;
    }
    let enabled = match canvas {
        CanvasType::Preview | CanvasType::MediaViewer => true,
        CanvasType::ArchiveDeleteFilter => settings.archive_delete,
        CanvasType::DuplicatesFilter => settings.duplicates,
        _ => false,
    };
    if !enabled {
        return None;
    }
    let (minimum, maximum, recorded_canvas) = if canvas == CanvasType::Preview {
        (
            settings.preview_min_ms,
            settings.preview_max_ms,
            CanvasType::Preview,
        )
    } else {
        (
            settings.media_min_ms,
            settings.media_max_ms,
            CanvasType::MediaViewer,
        )
    };
    let cap = maximum.map(|cap| cap.max(duration_ms.unwrap_or(0).saturating_mul(5)));
    let elapsed_ms = cap.map_or(elapsed_ms, |cap| elapsed_ms.min(cap));
    if minimum.is_some_and(|minimum| elapsed_ms < minimum) {
        return None;
    }
    Some(Completed {
        canvas: recorded_canvas,
        elapsed_ms,
    })
}

#[derive(Debug)]
struct Interval {
    file: HashId,
    started_ms: i64,
    duration_ms: Option<u64>,
}

/// A single canvas's interval owner. Close is terminal and idempotent so stale
/// callbacks cannot count again or restart tracking after its window closes.
pub struct Tracker {
    store: Arc<Store>,
    canvas: CanvasType,
    interval: Option<Interval>,
    closed: bool,
}
impl std::fmt::Debug for Tracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tracker")
            .field("canvas", &self.canvas)
            .field("interval", &self.interval)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
impl Tracker {
    /// Start an empty tracker; showing a file begins its timed interval.
    pub fn new(store: Arc<Store>, canvas: CanvasType) -> Self {
        Self {
            store,
            canvas,
            interval: None,
            closed: false,
        }
    }
    fn finish(&mut self, now_ms: i64) -> hydrus_store::Result<()> {
        let Some(interval) = self.interval.take() else {
            return Ok(());
        };
        let settings: FileViewingStatistics = self.store.read(hydrus_store::settings::get)?;
        let elapsed = u64::try_from(now_ms.saturating_sub(interval.started_ms)).unwrap_or(0);
        let Some(recorded) = completed(&settings, self.canvas, interval.duration_ms, elapsed)
        else {
            return Ok(());
        };
        self.store.write_content(move |w| {
            let viewed = hydrus_store::media::viewing_stats(w.conn(), &[interval.file])?
                .into_iter()
                .filter(|stats| stats.canvas == recorded.canvas)
                .filter_map(|stats| stats.last_viewed)
                .map(|time| time.0)
                .fold(interval.started_ms, i64::max);
            w.add_views(
                interval.file,
                recorded.canvas,
                Some(viewed),
                1,
                i64::try_from(recorded.elapsed_ms).unwrap_or(i64::MAX),
            )
        })
    }
    /// Finish the previous file and begin the next. Same-file redraws retain the
    /// start time, matching Canvas.SetMedia's unchanged-media guard.
    pub fn show(&mut self, file: Option<HashId>, now_ms: i64) -> hydrus_store::Result<()> {
        if self.closed || self.interval.as_ref().map(|i| i.file) == file {
            return Ok(());
        }
        self.finish(now_ms)?;
        if let Some(file) = file {
            let duration_ms = self
                .store
                .read(|conn| hydrus_store::media::load_basic(conn, &[file]))?
                .into_iter()
                .next()
                .and_then(|basic| basic.info)
                .and_then(|info| info.duration_ms);
            self.interval = Some(Interval {
                file,
                started_ms: now_ms,
                duration_ms,
            });
        }
        Ok(())
    }
    /// Finish this canvas even when edits/decisions are cancelled. A second
    /// close or a later callback cannot save another interval.
    pub fn close(&mut self, now_ms: i64) -> hydrus_store::Result<()> {
        self.closed = true;
        self.finish(now_ms)
    }
}
impl Drop for Tracker {
    fn drop(&mut self) {
        if let Err(error) = self.close(hydrus_core::TimestampMs::now().0) {
            eprintln!("could not save viewing statistics: {error}");
        }
    }
}
