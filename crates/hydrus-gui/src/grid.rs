//! The thumbnail grid's rows, made as the grid scrolls to them: the grid is
//! a list of rows (so only the visible ones exist). A row's thumbnails are
//! decoded off the UI thread when the grid first asks for them, shown as
//! they arrive ([`ThumbnailRows::receive`]), then kept.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::time::Duration;

use slint::{Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

use hydrus_core::HashId;

use crate::thumbnails::ThumbnailLoader;
use crate::{SearchPage, Thumbnail, ThumbnailRow};

/// Decoded thumbnails kept; past this, the cache starts afresh.
const CACHED: usize = 4000;

pub struct ThumbnailRows {
    page: RefCell<Rc<RefCell<SearchPage>>>,
    columns: Cell<usize>,
    cache: RefCell<HashMap<HashId, slint::Image>>,
    loader: ThumbnailLoader,
    /// The window's scale factor, which thumbnails are decoded for.
    scale: Cell<f32>,
    /// How many times the thumbnail settings have changed: thumbnails
    /// decoded under earlier ones are let go.
    generation: Cell<u64>,
    /// Thumbnails asked for and not yet decoded, with where the grid last
    /// showed each file.
    pending: RefCell<HashMap<HashId, usize>>,
    notify: ModelNotify,
}

impl std::fmt::Debug for ThumbnailRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailRows")
            .field("columns", &self.columns.get())
            .field("cached", &self.cache.borrow().len())
            .field("pending", &self.pending.borrow().len())
            .finish_non_exhaustive()
    }
}

impl ThumbnailRows {
    pub fn new(page: Rc<RefCell<SearchPage>>) -> Self {
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get().min(4));
        let loader = ThumbnailLoader::new(page.borrow().store(), workers);
        Self {
            page: RefCell::new(page),
            columns: Cell::new(1),
            cache: RefCell::default(),
            loader,
            scale: Cell::new(1.0),
            generation: Cell::new(0),
            pending: RefCell::default(),
            notify: ModelNotify::default(),
        }
    }

    /// Show the thumbnails decoded since last asked; how many.
    pub fn receive(&self) -> usize {
        let mut received = Vec::new();
        while let Some(result) = self.loader.try_receive() {
            received.push(result);
        }
        self.show(received)
    }

    /// Wait for every thumbnail asked for (for tests: the window's event
    /// loop otherwise collects them as they come).
    pub fn wait(&self) {
        while !self.pending.borrow().is_empty() {
            let Some(result) = self.loader.receive_timeout(Duration::from_secs(30)) else {
                return;
            };
            self.show(vec![result]);
        }
    }

    fn show(&self, received: Vec<crate::thumbnails::Loaded>) -> usize {
        let count = received.len();
        let mut rows = BTreeSet::new();
        {
            let mut cache = self.cache.borrow_mut();
            let mut pending = self.pending.borrow_mut();
            for (id, scale, generation, pixels) in received {
                // (decoded for a scale the window has since left, or under
                // thumbnail settings since changed)
                if scale.to_bits() != self.scale.get().to_bits()
                    || generation != self.generation.get()
                {
                    continue;
                }
                if cache.len() >= CACHED {
                    cache.clear();
                }
                cache.insert(
                    id,
                    pixels
                        .map(crate::thumbnails::Pixels::image)
                        .unwrap_or_default(),
                );
                if let Some(index) = pending.remove(&id) {
                    rows.insert(index / self.columns.get());
                }
            }
        }
        let row_count = self.row_count();
        for row in rows.into_iter().filter(|&row| row < row_count) {
            self.notify.row_changed(row);
        }
        count
    }

    /// The grid's width changed how many thumbnails fit in a row.
    pub fn set_columns(&self, columns: usize) {
        let columns = columns.max(1);
        if columns != self.columns.get() {
            self.columns.set(columns);
            self.notify.reset();
        }
    }

    /// The window's scale factor: thumbnails are decoded to show pixel for
    /// pixel at it, so a new one decodes them again.
    pub fn set_scale(&self, scale: f32) {
        if scale > 0.0 && scale.to_bits() != self.scale.get().to_bits() {
            self.scale.set(scale);
            self.cache.borrow_mut().clear();
            self.pending.borrow_mut().clear();
            self.notify.reset();
        }
    }

    /// The thumbnail settings changed (their size, say): every thumbnail is
    /// decoded again.
    pub fn thumbnails_changed(&self) {
        self.generation.set(self.generation.get() + 1);
        self.cache.borrow_mut().clear();
        self.pending.borrow_mut().clear();
        self.notify.reset();
    }

    /// How many thumbnails have been decoded (and are kept).
    pub fn cached(&self) -> usize {
        self.cache.borrow().len()
    }

    /// Show another page's files.
    pub fn set_page(&self, page: Rc<RefCell<SearchPage>>) {
        *self.page.borrow_mut() = page;
        self.notify.reset();
    }

    /// The page's files changed.
    pub fn reset(&self) {
        self.notify.reset();
    }

    /// The file at `index` changed (e.g. its selection).
    pub fn file_changed(&self, index: usize) {
        self.notify.row_changed(index / self.columns.get());
    }

    /// The files selected changed from `before` to `after` (by index):
    /// the rows whose files' selection changed are drawn again.
    pub fn selection_changed(&self, before: &BTreeSet<usize>, after: &BTreeSet<usize>) {
        let columns = self.columns.get();
        let rows: BTreeSet<usize> = before
            .symmetric_difference(after)
            .map(|i| i / columns)
            .collect();
        for row in rows {
            self.notify.row_changed(row);
        }
    }

    /// The file's thumbnail if decoded; otherwise a blank, and it is asked for.
    fn image(&self, id: HashId, index: usize) -> slint::Image {
        if let Some(image) = self.cache.borrow().get(&id) {
            return image.clone();
        }
        if self.pending.borrow_mut().insert(id, index).is_none() {
            self.loader
                .request(id, self.scale.get(), self.generation.get());
        }
        slint::Image::default()
    }
}

impl Model for ThumbnailRows {
    type Data = ThumbnailRow;

    fn row_count(&self) -> usize {
        self.page
            .borrow()
            .borrow()
            .results()
            .len()
            .div_ceil(self.columns.get())
    }

    fn row_data(&self, row: usize) -> Option<ThumbnailRow> {
        let page = self.page.borrow().clone();
        let page = page.borrow();
        let results = page.results();
        let columns = self.columns.get();
        let start = row.checked_mul(columns)?;
        if start >= results.len() {
            return None;
        }
        let end = (start + columns).min(results.len());
        let thumbnails: Vec<Thumbnail> = (start..end)
            .map(|i| Thumbnail {
                image: self.image(results[i], i),
                selected: page.is_selected(i),
                files: page
                    .collection(results[i])
                    .map_or_else(SharedString::new, |files| {
                        hydrus_core::numbers::human_int(files.len() as u64).into()
                    }),
            })
            .collect();
        Some(ThumbnailRow {
            first: i32::try_from(start).unwrap_or(i32::MAX),
            thumbnails: ModelRc::new(VecModel::from(thumbnails)),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
