//! Thumbnails read and decoded off the UI thread: the grid asks for the
//! ones it shows, a few worker threads read and decode them, and the UI
//! thread collects them as they arrive (a blank frame stands in until then).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use slint::{Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};

use hydrus_core::HashId;
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

/// A file's thumbnail, decoded; `None` if it has none on disk.
pub fn thumbnail(store: &Store, id: HashId) -> Option<hydrus_media::Raster> {
    let hash = store
        .read(|conn| hydrus_store::master::hash(conn, id))
        .ok()??;
    let path = store.snapshot().storage.thumbnail_path(&hash)?;
    let bytes = std::fs::read(path).ok()?;
    hydrus_media::decode_image(&bytes).ok()
}

pub struct ThumbnailLoader {
    requests: Sender<HashId>,
    results: Receiver<(HashId, Option<Pixels>)>,
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
        let (requests, jobs) = crossbeam_channel::unbounded::<HashId>();
        let (done, results) = crossbeam_channel::unbounded();
        for _ in 0..workers.max(1) {
            let (store, jobs, done) = (store.clone(), jobs.clone(), done.clone());
            thread::Builder::new()
                .name("thumbnails".into())
                .spawn(move || {
                    for id in jobs {
                        let pixels = thumbnail(&store, id).as_ref().map(Pixels::new);
                        if done.send((id, pixels)).is_err() {
                            break;
                        }
                    }
                })
                .expect("starting a thumbnail worker");
        }
        Self { requests, results }
    }

    pub fn request(&self, id: HashId) {
        // (the workers outlive every sender, so this can't fail)
        let _ = self.requests.send(id);
    }

    /// A thumbnail that is ready, if any.
    pub fn try_receive(&self) -> Option<(HashId, Option<Pixels>)> {
        self.results.try_recv().ok()
    }

    /// The next thumbnail, waiting up to `timeout` for it.
    pub fn receive_timeout(&self, timeout: Duration) -> Option<(HashId, Option<Pixels>)> {
        self.results.recv_timeout(timeout).ok()
    }
}
