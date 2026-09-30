//! The thumbnail grid's rows, made as the grid scrolls to them: the grid is
//! a list of rows (so only the visible ones exist), and a row's thumbnails
//! are decoded when the grid first asks for it, then kept.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use hydrus_core::HashId;

use crate::{SearchPage, Thumbnail, ThumbnailRow};

/// Decoded thumbnails kept; past this, the cache starts afresh.
const CACHED: usize = 4000;

pub struct ThumbnailRows {
    page: Rc<RefCell<SearchPage>>,
    columns: Cell<usize>,
    cache: RefCell<HashMap<HashId, slint::Image>>,
    notify: ModelNotify,
}

impl std::fmt::Debug for ThumbnailRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailRows")
            .field("columns", &self.columns.get())
            .field("cached", &self.cache.borrow().len())
            .finish_non_exhaustive()
    }
}

impl ThumbnailRows {
    pub fn new(page: Rc<RefCell<SearchPage>>) -> Self {
        Self {
            page,
            columns: Cell::new(1),
            cache: RefCell::default(),
            notify: ModelNotify::default(),
        }
    }

    /// The grid's width changed how many thumbnails fit in a row.
    pub fn set_columns(&self, columns: usize) {
        let columns = columns.max(1);
        if columns != self.columns.get() {
            self.columns.set(columns);
            self.notify.reset();
        }
    }

    /// How many thumbnails have been decoded (and are kept).
    pub fn cached(&self) -> usize {
        self.cache.borrow().len()
    }

    /// The page's files changed.
    pub fn reset(&self) {
        self.notify.reset();
    }

    /// The file at `index` changed (e.g. its selection).
    pub fn file_changed(&self, index: usize) {
        self.notify.row_changed(index / self.columns.get());
    }

    fn image(&self, page: &SearchPage, id: HashId) -> slint::Image {
        if let Some(image) = self.cache.borrow().get(&id) {
            return image.clone();
        }
        let image = page
            .thumbnail(id)
            .as_ref()
            .map(crate::image)
            .unwrap_or_default();
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHED {
            cache.clear();
        }
        cache.insert(id, image.clone());
        image
    }
}

impl Model for ThumbnailRows {
    type Data = ThumbnailRow;

    fn row_count(&self) -> usize {
        self.page
            .borrow()
            .results()
            .len()
            .div_ceil(self.columns.get())
    }

    fn row_data(&self, row: usize) -> Option<ThumbnailRow> {
        let page = self.page.borrow();
        let results = page.results();
        let columns = self.columns.get();
        let start = row.checked_mul(columns)?;
        if start >= results.len() {
            return None;
        }
        let end = (start + columns).min(results.len());
        let thumbnails: Vec<Thumbnail> = (start..end)
            .map(|i| Thumbnail {
                image: self.image(&page, results[i]),
                selected: page.selected() == Some(i),
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
