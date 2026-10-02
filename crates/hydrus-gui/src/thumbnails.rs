//! Thumbnails read and decoded off the UI thread: the grid asks for the
//! ones it shows, a few worker threads read and decode them, and the UI
//! thread collects them as they arrive (a blank frame stands in until then).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use slint::{Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};

use hydrus_core::HashId;
use hydrus_core::thumbnail::ThumbnailSettings;
use hydrus_media::resample::{Interpolation, resize};
use hydrus_store::Store;

/// Decoded pixels, which (unlike an image) may cross threads.
pub enum Pixels {
    Rgba(SharedPixelBuffer<Rgba8Pixel>),
    Rgb(SharedPixelBuffer<Rgb8Pixel>),
}

impl Pixels {
    pub fn new(raster: &hydrus_media::Raster) -> Self {
        let (width, height) = (raster.width(), raster.height());
        match raster.channels() {
            4 => Pixels::Rgba(SharedPixelBuffer::clone_from_slice(
                raster.data(),
                width,
                height,
            )),
            3 => Pixels::Rgb(SharedPixelBuffer::clone_from_slice(
                raster.data(),
                width,
                height,
            )),
            _ => {
                let rgb: Vec<u8> = raster.data().iter().flat_map(|&v| [v, v, v]).collect();
                Pixels::Rgb(SharedPixelBuffer::clone_from_slice(&rgb, width, height))
            }
        }
    }

    pub fn image(self) -> slint::Image {
        match self {
            Pixels::Rgba(pixels) => slint::Image::from_rgba8(pixels),
            Pixels::Rgb(pixels) => slint::Image::from_rgb8(pixels),
        }
    }
}

/// A file's thumbnail, decoded; `None` if it has none. A thumbnail that
/// has gone missing is made again from its file, as the reference does.
pub fn thumbnail(store: &Arc<Store>, id: HashId) -> Option<hydrus_media::Raster> {
    let hash = store
        .read(|conn| hydrus_store::master::hash(conn, id))
        .ok()??;
    let path = store.snapshot().storage.thumbnail_path(&hash)?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => std::fs::read(regenerate(store, id)?).ok()?,
    };
    hydrus_media::decode_image(&bytes).ok()
}

/// The size, in the screen's pixels, to show a stored thumbnail of
/// `stored` pixels at: the reference draws it at its own size (over its
/// DPR), centred in the bounding box, and never stretches it to fill; a
/// screen scaled by `scale` takes that many pixels to each of those. (One
/// that has outgrown the box, for settings changed since it was made, is
/// shrunk into it.)
pub fn display_size(stored: (u32, u32), settings: &ThumbnailSettings, scale: f32) -> (u32, u32) {
    let dpr = f64::from(settings.dpr_percent.max(1)) / 100.0;
    let (mut width, mut height) = (f64::from(stored.0) / dpr, f64::from(stored.1) / dpr);
    let (bw, bh) = (
        f64::from(settings.bounding_width),
        f64::from(settings.bounding_height),
    );
    if width > bw || height > bh {
        let fit = (bw / width).min(bh / height);
        width *= fit;
        height *= fit;
    }
    let scale = f64::from(scale);
    let pixels = |v: f64| ((v * scale).round() as u32).max(1);
    (pixels(width), pixels(height))
}

/// A stored thumbnail, resampled (as the reference resizes them: area when
/// shrinking, Lanczos when growing) to show pixel for pixel on a screen
/// scaled by `scale`. The renderer would otherwise scale it by picking the
/// nearest pixels, which looks blocky.
pub fn for_display(
    raster: hydrus_media::Raster,
    settings: &ThumbnailSettings,
    scale: f32,
) -> hydrus_media::Raster {
    let (width, height) = display_size((raster.width(), raster.height()), settings, scale);
    if (width, height) == (raster.width(), raster.height()) {
        return raster;
    }
    let interpolation = if width < raster.width() || height < raster.height() {
        Interpolation::Area
    } else {
        Interpolation::Lanczos4
    };
    resize(&raster, width, height, interpolation)
}

fn regenerate(store: &Arc<Store>, id: HashId) -> Option<std::path::PathBuf> {
    let snapshot = store.snapshot();
    let media = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[id]))
        .ok()?
        .results
        .pop()?;
    let importer =
        hydrus_import::FileImporter::new(Arc::clone(store), hydrus_media::MediaTools::new());
    importer
        .regenerate_thumbnail(&media)
        .map_err(|e| eprintln!("regenerating a thumbnail failed: {e}"))
        .ok()?
}

/// A thumbnail decoded for a screen scaled by `.1`.
pub type Loaded = (HashId, f32, Option<Pixels>);

pub struct ThumbnailLoader {
    requests: Sender<(HashId, f32)>,
    results: Receiver<Loaded>,
}

impl std::fmt::Debug for ThumbnailLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailLoader")
            .field("queued", &self.requests.len())
            .field("ready", &self.results.len())
            .finish()
    }
}

impl ThumbnailLoader {
    /// Workers for `store`'s thumbnails; they end when the loader does.
    pub fn new(store: &Arc<Store>, workers: usize) -> Self {
        let (requests, jobs) = crossbeam_channel::unbounded::<(HashId, f32)>();
        let (done, results) = crossbeam_channel::unbounded();
        for _ in 0..workers.max(1) {
            let (store, jobs, done) = (store.clone(), jobs.clone(), done.clone());
            thread::Builder::new()
                .name("thumbnails".into())
                .spawn(move || {
                    for (id, scale) in jobs {
                        let settings = store.snapshot().thumbnails;
                        let pixels = thumbnail(&store, id)
                            .map(|raster| Pixels::new(&for_display(raster, &settings, scale)));
                        if done.send((id, scale, pixels)).is_err() {
                            break;
                        }
                    }
                })
                .expect("starting a thumbnail worker");
        }
        Self { requests, results }
    }

    /// Decode `id`'s thumbnail for a screen scaled by `scale`.
    pub fn request(&self, id: HashId, scale: f32) {
        // (the workers outlive every sender, so this can't fail)
        let _ = self.requests.send((id, scale));
    }

    /// A thumbnail that is ready, if any.
    pub fn try_receive(&self) -> Option<Loaded> {
        self.results.try_recv().ok()
    }

    /// The next thumbnail, waiting up to `timeout` for it.
    pub fn receive_timeout(&self, timeout: Duration) -> Option<Loaded> {
        self.results.recv_timeout(timeout).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_show_at_their_own_size_in_the_screens_pixels() {
        let settings = ThumbnailSettings::default();
        // pixel for pixel, as the reference draws them
        assert_eq!(display_size((150, 125), &settings, 1.0), (150, 125));
        // a small file's small thumbnail is not stretched to the cell
        assert_eq!(display_size((64, 40), &settings, 1.0), (64, 40));
        // a screen scaled by 1.5 takes half as many pixels again
        assert_eq!(display_size((150, 125), &settings, 1.5), (225, 188));
        // one made for a bigger box is shrunk into this one
        assert_eq!(display_size((300, 300), &settings, 1.0), (125, 125));
    }

    #[test]
    fn a_thumbnail_dpr_makes_them_show_smaller() {
        let settings = ThumbnailSettings {
            dpr_percent: 200,
            ..ThumbnailSettings::default()
        };
        // made at twice the size, for a screen that shows them so
        assert_eq!(display_size((300, 250), &settings, 2.0), (300, 250));
        assert_eq!(display_size((300, 250), &settings, 1.0), (150, 125));
        // (a small file's, too, which the box does not shrink)
        assert_eq!(display_size((100, 80), &settings, 2.0), (100, 80));
        assert_eq!(display_size((100, 80), &settings, 1.0), (50, 40));
    }

    #[test]
    fn thumbnails_are_resampled_only_when_the_screen_needs_it() {
        let raster = hydrus_media::Raster::new(2, 2, 3, vec![10; 12]).unwrap();
        let settings = ThumbnailSettings::default();
        let same = for_display(raster.clone(), &settings, 1.0);
        assert_eq!((same.width(), same.height()), (2, 2));
        let grown = for_display(raster, &settings, 2.0);
        assert_eq!((grown.width(), grown.height()), (4, 4));
        // (a flat image stays flat)
        assert!(grown.data().iter().all(|&v| v == 10));
    }
}
