//! Stills drawn at their zoom as the reference draws them. Slint's
//! software renderer scales images by their nearest pixel, which leaves a
//! photo shrunk to the window jagged and a small image grown blocky; the
//! reference resizes with OpenCV instead (`ResizeNumPyImageForMediaViewer`:
//! the file type's scale-up quality, Lanczos by default, growing, and its
//! scale-down quality, area, shrinking). So the part of a still that shows
//! is cut out and resized that way, off the UI thread, to exactly the
//! pixels it covers, and drawn over the quickly scaled still once ready.

use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use hydrus_core::media_viewer::ZoomRules;
use hydrus_media::Raster;
use hydrus_media::resample::{Interpolation, resize};

use crate::thumbnails::Pixels;
use crate::zoom::{Point, Rect};

/// What to draw of a still: the part of it (x, y, width, height, in its
/// pixels) that shows, the size to resize it to (in device pixels), how,
/// and where it goes (in logical pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub clip: (u32, u32, u32, u32),
    pub target: (u32, u32),
    pub interpolation: Interpolation,
    pub rect: (f32, f32, f32, f32),
}

/// The plan for a still of `source` pixels drawn at `media` (logical
/// pixels) in a canvas of `canvas` at `ratio`; `None` if none of it
/// shows, or if the reference wouldn't resize it (at 100%, or with
/// nearest-pixel quality), as Slint draws that as well.
pub fn plan(
    media: Rect,
    canvas: Point,
    ratio: f64,
    source: (u32, u32),
    rules: &ZoomRules,
) -> Option<Plan> {
    let (x, y, width, height) = media;
    let (left, top) = (f64::from(x) * ratio, f64::from(y) * ratio);
    let (width, height) = (f64::from(width) * ratio, f64::from(height) * ratio);
    let (canvas_width, canvas_height) = (f64::from(canvas.0) * ratio, f64::from(canvas.1) * ratio);
    if width <= 0.0 || height <= 0.0 || source.0 == 0 || source.1 == 0 {
        return None;
    }
    let (scale_x, scale_y) = (width / f64::from(source.0), height / f64::from(source.1));
    // the source's pixels that show, whole
    let span = |start: f64, length: f64, canvas: f64, scale: f64, size: u32| {
        let shown_start = start.max(0.0);
        let shown_end = (start + length).min(canvas);
        if shown_end <= shown_start {
            return None;
        }
        let first = (((shown_start - start) / scale).floor().max(0.0) as u32).min(size);
        let last = (((shown_end - start) / scale).ceil() as u32).min(size);
        (last > first).then_some((first, last - first))
    };
    let (clip_x, clip_width) = span(left, width, canvas_width, scale_x, source.0)?;
    let (clip_y, clip_height) = span(top, height, canvas_height, scale_y, source.1)?;
    let target = (
        ((f64::from(clip_width) * scale_x).round() as u32).max(1),
        ((f64::from(clip_height) * scale_y).round() as u32).max(1),
    );
    // (the reference's check, its sides crossed, as it has them)
    if target == (clip_width, clip_height) || target == (clip_height, clip_width) {
        return None;
    }
    let quality = if target.0 > clip_width || target.1 > clip_height {
        rules.scale_up_quality
    } else {
        rules.scale_down_quality
    };
    // (`CC.ZOOM_*`; no cubic here, so Lanczos, the closest)
    let interpolation = match quality {
        1 => Interpolation::Linear,
        2 => Interpolation::Area,
        3 | 4 => Interpolation::Lanczos4,
        _ => return None,
    };
    Some(Plan {
        clip: (clip_x, clip_y, clip_width, clip_height),
        target,
        interpolation,
        rect: (
            ((left + f64::from(clip_x) * scale_x) / ratio) as f32,
            ((top + f64::from(clip_y) * scale_y) / ratio) as f32,
            (f64::from(target.0) / ratio) as f32,
            (f64::from(target.1) / ratio) as f32,
        ),
    })
}

/// Cut out and resize the plan's part of `source`.
pub fn render(source: &Raster, plan: &Plan) -> Raster {
    let (x, y, width, height) = plan.clip;
    let channels = usize::from(source.channels());
    let stride = source.width() as usize * channels;
    let row = width as usize * channels;
    let mut data = Vec::with_capacity(row * height as usize);
    for line in y..y + height {
        let start = line as usize * stride + x as usize * channels;
        data.extend_from_slice(&source.data()[start..start + row]);
    }
    let clip = Raster::new(width, height, source.channels(), data).expect("a clip of the source");
    resize(&clip, plan.target.0, plan.target.1, plan.interpolation)
}

type Job = (u64, Arc<Raster>, Plan);

/// A thread rendering stills, the newest asked for first (older ones
/// waiting are dropped); it ends with its renderer.
pub(crate) struct Renderer {
    jobs: Sender<Job>,
    done: Receiver<(u64, Pixels)>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer").finish_non_exhaustive()
    }
}

impl Renderer {
    pub fn new() -> std::io::Result<Self> {
        let (jobs, queued) = crossbeam_channel::unbounded::<Job>();
        let (finished, done) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("stills".into())
            .spawn(move || {
                while let Ok(mut job) = queued.recv() {
                    while let Ok(newer) = queued.try_recv() {
                        job = newer;
                    }
                    let (id, source, plan) = job;
                    let pixels = Pixels::new(&render(&source, &plan));
                    if finished.send((id, pixels)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self { jobs, done })
    }

    pub fn request(&self, id: u64, source: Arc<Raster>, plan: Plan) {
        let _ = self.jobs.send((id, source, plan));
    }

    /// The newest render finished, if any.
    pub fn newest(&self) -> Option<(u64, Pixels)> {
        let mut newest = None;
        while let Ok(done) = self.done.try_recv() {
            newest = Some(done);
        }
        newest
    }
}
