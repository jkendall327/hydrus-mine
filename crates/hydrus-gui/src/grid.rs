//! The thumbnail grid's rows, made as the grid scrolls to them: the grid is
//! a list of rows (so only the visible ones exist). A row's thumbnails are
//! decoded off the UI thread when the grid first asks for them, shown as
//! they arrive ([`ThumbnailRows::receive`]), then kept.

use hydrus_gui_model::thumbnail_cache::Cache;
use hydrus_store::settings::{self, ThumbnailCacheSettings};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

use hydrus_core::HashId;

use hydrus_core::thumbnail::ThumbnailRatingSettings;

use crate::thumbnail_icons::{self, IconFacts, Ratings};
use crate::thumbnail_ratings::{self, Look};
use crate::thumbnails::ThumbnailLoader;
use crate::{RatingShape, SearchPage, ThumbBox, ThumbIcon, ThumbRating, Thumbnail, ThumbnailRow};

pub struct ThumbnailRows {
    page: RefCell<Rc<RefCell<SearchPage>>>,
    columns: Cell<usize>,
    cache: RefCell<Cache<slint::Image>>,
    clock: Instant,
    active: Cell<bool>,
    paint_owner: Rc<()>,
    admissions: RefCell<HashMap<HashId, u64>>,
    next_admission: Cell<u64>,
    paint_clock: RefCell<Rc<dyn Fn() -> Duration>>,
    loader: ThumbnailLoader,
    /// The window's scale factor, which thumbnails are decoded for.
    scale: Cell<f32>,
    /// How many times the thumbnail settings have changed: thumbnails
    /// decoded under earlier ones are let go.
    generation: Cell<u64>,
    /// Thumbnails asked for and not yet decoded, with where the grid last
    /// showed each file.
    pending: RefCell<HashMap<HashId, usize>>,
    /// A thumbnail's border, width and height (the border included), for
    /// the icons over it.
    cell: Cell<(i32, i32, i32)>,
    /// What the icons over each file's thumbnail say, and its ratings,
    /// read as its row is first shown (and again after the files change).
    icon_facts: RefCell<HashMap<HashId, (IconFacts, Ratings)>>,
    /// How ratings are drawn over thumbnails (the options').
    rating_settings: Cell<ThumbnailRatingSettings>,
    /// The tag summaries drawn over thumbnails, and what each file's (or
    /// collection's) say, read as its row is first shown.
    summaries: RefCell<hydrus_core::tag_summary::TagSummaries>,
    banners: RefCell<HashMap<HashId, (SharedString, SharedString)>>,
    paints: RefCell<crate::thumbnail_paint::Paints>,
    palette: RefCell<crate::thumbnail_paint::Palette>,
    appearance: RefCell<hydrus_store::thumbnail_appearance::Preferences>,
    visible: Cell<bool>,
    background: RefCell<crate::thumbnail_background::Background>,
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
        let policy = page
            .borrow()
            .store()
            .read(settings::get)
            .unwrap_or_default();
        let appearance = page
            .borrow()
            .store()
            .read(hydrus_store::thumbnail_appearance::load)
            .unwrap_or_default();
        let paint_start = Instant::now();
        Self {
            paint_owner: Rc::new(()),
            admissions: RefCell::default(),
            next_admission: Cell::new(0),
            paint_clock: RefCell::new(Rc::new(move || paint_start.elapsed())),
            page: RefCell::new(page),
            columns: Cell::new(1),
            cache: RefCell::new(Cache::new(policy)),
            clock: Instant::now(),
            active: Cell::new(true),
            loader,
            scale: Cell::new(1.0),
            generation: Cell::new(0),
            pending: RefCell::default(),
            cell: Cell::new((1, 152, 127)),
            icon_facts: RefCell::default(),
            rating_settings: Cell::new(ThumbnailRatingSettings::default()),
            summaries: RefCell::default(),
            banners: RefCell::default(),
            paints: RefCell::default(),
            palette: RefCell::default(),
            appearance: RefCell::new(appearance),
            visible: Cell::new(false),
            background: RefCell::default(),
            notify: ModelNotify::default(),
        }
    }

    /// Show the thumbnails decoded since last asked; how many.
    pub fn receive(&self) -> usize {
        if !self.active.get() {
            // Release late decoder buffers without publishing into a retired owner.
            while self.loader.try_receive().is_some() {}
            return 0;
        }
        self.maintain_cache();
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
                if !self.active.get() {
                    continue;
                }
                // Missing thumbnails keep a small, byte-accounted negative entry.
                let bytes = pixels
                    .as_ref()
                    .map_or(128, crate::thumbnails::Pixels::byte_len);
                cache.insert(
                    id,
                    pixels
                        .map(crate::thumbnails::Pixels::image)
                        .unwrap_or_default(),
                    bytes,
                    self.clock.elapsed(),
                );
                let admission = self.next_admission.get().wrapping_add(1);
                self.next_admission.set(admission);
                self.admissions.borrow_mut().insert(id, admission);
                self.paints.borrow_mut().dirty(id);
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
            self.clear_thumbnail_cache();
        }
    }

    /// The thumbnail settings changed (their size, say): every thumbnail is
    /// decoded again.
    pub fn thumbnails_changed(&self) {
        self.clear_thumbnail_cache();
    }

    /// Explicit debug reset also invalidates in-flight decode results.
    pub fn clear_thumbnail_cache(&self) {
        if !self.active.get() {
            return;
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        self.cache.borrow_mut().clear();
        self.pending.borrow_mut().clear();
        self.admissions.borrow_mut().clear();
        self.paints.borrow_mut().clear();
        self.notify.reset();
    }
    /// Permanently stop this GUI incarnation's cache and loader result publication.
    pub fn retire(&self) {
        if !self.active.replace(false) {
            return;
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        self.cache.borrow_mut().clear();
        self.pending.borrow_mut().clear();
        self.admissions.borrow_mut().clear();
        self.paints.borrow_mut().clear();
        self.background.borrow_mut().clear();
        self.notify.reset();
    }
    /// Enforce saved preferences immediately, without changing decode generations.
    pub fn set_cache_policy(&self, policy: ThumbnailCacheSettings) {
        if self.active.get() {
            self.cache
                .borrow_mut()
                .set_policy(policy, self.clock.elapsed());
        }
    }
    /// Idle maintenance expires entries independently of scrolling or cache reads.
    pub fn maintain_cache(&self) {
        if self.active.get() {
            self.cache.borrow_mut().maintain(self.clock.elapsed());
            let retained: BTreeSet<_> = self.cache.borrow().keys().into_iter().collect();
            self.admissions
                .borrow_mut()
                .retain(|id, _| retained.contains(id));
        }
    }
    /// Byte estimate for owned cache buffers (renderer-held clones are separate).
    pub fn cached_bytes(&self) -> u64 {
        self.cache.borrow().bytes()
    }
    /// Current saved policy consumed by this grid.
    pub fn cache_policy(&self) -> ThumbnailCacheSettings {
        self.cache.borrow().policy()
    }
    /// How many thumbnails have been decoded (and are kept).
    pub fn cached(&self) -> usize {
        self.cache.borrow().len()
    }

    /// Show another page's files.
    pub fn set_page(&self, page: Rc<RefCell<SearchPage>>) {
        self.paints.borrow_mut().clear();
        *self.page.borrow_mut() = page;
        self.icon_facts.borrow_mut().clear();
        self.banners.borrow_mut().clear();
        self.notify.reset();
    }

    /// The page's files changed.
    pub fn reset(&self) {
        self.paints.borrow_mut().dirty_all();
        self.icon_facts.borrow_mut().clear();
        self.banners.borrow_mut().clear();
        self.notify.reset();
    }

    /// A thumbnail's border, and its width and height with it.
    pub fn set_cell(&self, border: i32, width: i32, height: i32) {
        if self.cell.get() != (border, width, height) {
            self.paints.borrow_mut().clear();
            self.cell.set((border, width, height));
            self.notify.reset();
        }
    }

    /// Some files were changed (archived or rated, say): their icons and
    /// ratings are read again.
    pub fn forget_files(&self) {
        self.paints.borrow_mut().dirty_all();
        self.icon_facts.borrow_mut().clear();
        for row in 0..self.row_count() {
            self.notify.row_changed(row);
        }
    }

    /// How ratings are drawn over thumbnails (the options').
    pub fn set_rating_settings(&self, settings: ThumbnailRatingSettings) {
        if self.rating_settings.get() != settings {
            self.paints.borrow_mut().dirty_all();
            self.rating_settings.set(settings);
            self.notify.reset();
        }
    }

    /// The tag summaries drawn over thumbnails (the options').
    pub fn set_summaries(&self, summaries: hydrus_core::tag_summary::TagSummaries) {
        if *self.summaries.borrow() != summaries {
            self.paints.borrow_mut().dirty_all();
            *self.summaries.borrow_mut() = summaries;
            self.banners.borrow_mut().clear();
            self.notify.reset();
        }
    }

    /// What the tag banners over `item` (a file or a collection) on `page`
    /// say.
    fn banners(&self, page: &SearchPage, item: HashId) -> (SharedString, SharedString) {
        if let Some(known) = self.banners.borrow().get(&item) {
            return known.clone();
        }
        let files = page
            .collection(item)
            .map_or_else(|| vec![item], <[HashId]>::to_vec);
        let (top, bottom) =
            thumbnail_icons::banners(page.store(), &files, &self.summaries.borrow());
        let made = (SharedString::from(top), SharedString::from(bottom));
        self.banners.borrow_mut().insert(item, made.clone());
        made
    }

    /// The icons and ratings over each of `items` (files or collections)
    /// on `page`, as the reference draws them.
    fn icons(&self, page: &SearchPage, items: &[HashId]) -> Vec<Overlay> {
        let members = |item: HashId| -> Vec<HashId> {
            page.collection(item)
                .map_or_else(|| vec![item], <[HashId]>::to_vec)
        };
        let missing: Vec<HashId> = {
            let known = self.icon_facts.borrow();
            items
                .iter()
                .flat_map(|&item| members(item))
                .filter(|id| !known.contains_key(id))
                .collect()
        };
        if !missing.is_empty() {
            let read = thumbnail_icons::facts_and_ratings(page.store(), &missing);
            self.icon_facts.borrow_mut().extend(read);
        }
        let known = self.icon_facts.borrow();
        let (border, width, height) = self.cell.get();
        let services = page.store().snapshot().services.clone();
        let settings = self.rating_settings.get();
        items
            .iter()
            .map(|&item| {
                let files = members(item);
                let collection = page.collection(item).is_some();
                let facts = if collection {
                    let of: Vec<IconFacts> = files
                        .iter()
                        .filter_map(|id| known.get(id).map(|(f, _)| f.clone()))
                        .collect();
                    IconFacts::of_collection(&of)
                } else {
                    known.get(&item).map(|(f, _)| f.clone()).unwrap_or_default()
                };
                // (a collection's ratings are its first file's, as the
                // reference's are)
                let rated = files
                    .first()
                    .and_then(|first| known.get(first))
                    .map(|(_, r)| r.clone())
                    .unwrap_or_default();
                let controls = crate::ratings::controls_of(&services, &rated);
                let layout = thumbnail_ratings::layout(
                    &services,
                    &controls,
                    &settings,
                    width,
                    border,
                    &text_width,
                );
                let icons = thumbnail_icons::placed(
                    &facts,
                    collection,
                    border,
                    width,
                    height,
                    layout.top_right_y,
                )
                .into_iter()
                .map(|p| ThumbIcon {
                    kind: p.icon.code(),
                    x: p.x as f32,
                    y: p.y as f32,
                })
                .collect();
                Overlay {
                    icons,
                    ratings: layout.drawn.iter().map(thumb_rating).collect(),
                    boxes: layout
                        .boxes
                        .iter()
                        .map(|b| ThumbBox {
                            x: b.x as f32,
                            y: b.y as f32,
                            width: b.width as f32,
                            height: b.height as f32,
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// The file at `index` changed (e.g. its selection).
    pub fn file_changed(&self, index: usize) {
        if let Some(&id) = self.page.borrow().borrow().results().get(index) {
            self.paints.borrow_mut().dirty(id);
        }
        self.notify.row_changed(index / self.columns.get());
    }

    /// The files selected changed from `before` to `after` (by index):
    /// the rows whose files' selection changed are drawn again.
    pub fn selection_changed(&self, before: &BTreeSet<usize>, after: &BTreeSet<usize>) {
        let page = self.page.borrow();
        let page = page.borrow();
        for index in before.symmetric_difference(after) {
            if let Some(&id) = page.results().get(*index) {
                self.paints.borrow_mut().dirty(id);
            }
        }
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
        if !self.active.get() {
            return slint::Image::default();
        }
        if let Some(image) = self.cache.borrow_mut().get(id, self.clock.elapsed()) {
            return image;
        }
        if self.pending.borrow_mut().insert(id, index).is_none() {
            self.loader
                .request(id, self.scale.get(), self.generation.get());
        }
        slint::Image::default()
    }

    /// Ownable clock for deterministic rendering/replays; swapping it drops old
    /// snapshots, so deadlines from a former clock cannot enter this owner.
    pub fn set_paint_clock(&self, clock: Rc<dyn Fn() -> Duration>) {
        *self.paint_clock.borrow_mut() = clock;
        self.paints.borrow_mut().clear();
        self.notify.reset();
    }

    /// Saved policy propagates only at Apply; renderer mode belongs to the page.
    pub fn set_appearance(&self, appearance: hydrus_store::thumbnail_appearance::Preferences) {
        if !self.active.get() {
            return;
        }
        let invalidate = self.appearance.borrow().blurhash != appearance.blurhash;
        *self.appearance.borrow_mut() = appearance;
        if invalidate {
            self.clear_thumbnail_cache();
        }
    }
    pub fn background(&self) -> slint::Image {
        if !self.active.get() {
            return slint::Image::default();
        }
        self.background
            .borrow_mut()
            .get(self.appearance.borrow().background.as_deref())
    }
    /// Palette changes invalidate copied cells so neither fade layer retains old colours.
    pub fn set_paint_palette(&self, palette: crate::thumbnail_paint::Palette) {
        if self.active.get() && *self.palette.borrow() != palette {
            *self.palette.borrow_mut() = palette;
            self.paints.borrow_mut().clear();
            self.notify.reset();
        }
    }
    /// Keep snapshots only for the visible grid slice, and release every old
    /// layer at completion. Hidden/retired windows own no fade snapshots.
    pub fn paint_tick(&self, visible: bool, first_row: usize, count: usize) {
        let changed_visibility =
            self.visible.replace(visible && self.active.get()) != self.visible.get();
        if !self.visible.get() {
            self.paints.borrow_mut().clear();
            if changed_visibility {
                self.notify.reset();
            }
            return;
        }
        if changed_visibility {
            self.notify.reset();
        }
        let page = self.page.borrow();
        let page = page.borrow();
        let start = first_row.saturating_mul(self.columns.get());
        let end = start
            .saturating_add(count.saturating_mul(self.columns.get()))
            .min(page.results().len());
        let keys = page
            .results()
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .copied()
            .collect();
        let mut paints = self.paints.borrow_mut();
        paints.retain(&keys);
        let changed = paints.tick(
            (self.paint_clock.borrow())(),
            page.new_thumbnail_renderer(),
            self.appearance.borrow().fade,
        );
        drop(paints);
        for row in changed
            .into_iter()
            .map(|index| index / self.columns.get())
            .collect::<BTreeSet<_>>()
        {
            self.notify.row_changed(row);
        }
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
        // Collections aggregate current locations; retained bytes do not imply local membership.
        let members: Vec<Vec<HashId>> = results[start..end]
            .iter()
            .map(|file| {
                page.collection(*file)
                    .map_or_else(|| vec![*file], |collection| collection.to_vec())
            })
            .collect();
        let files: Vec<HashId> = members.iter().flatten().copied().collect();
        let storage = page
            .store()
            .snapshot()
            .services
            .by_key(&hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE,
            ))
            .ok()
            .map(|service| service.id);
        let current = page
            .store()
            .read(move |conn| hydrus_store::media::current_domains(conn, &files))
            .unwrap_or_default();
        let local: Vec<bool> = members
            .iter()
            .map(|files| {
                storage.is_some_and(|storage| {
                    files.iter().any(|file| {
                        current
                            .get(file)
                            .is_some_and(|domains| domains.contains(&storage))
                    })
                })
            })
            .collect();
        let overlays = self.icons(&page, &results[start..end]);
        let thumbnails: Vec<Thumbnail> = (start..end)
            .zip(overlays)
            .map(|(i, overlay)| {
                let (top, bottom) = self.banners(&page, results[i]);
                (i, overlay, top, bottom)
            })
            .map(|(i, overlay, top, bottom)| {
                let mut thumbnail = Thumbnail {
                    icons: ModelRc::new(VecModel::from(overlay.icons)),
                    ratings: ModelRc::new(VecModel::from(overlay.ratings)),
                    rating_boxes: ModelRc::new(VecModel::from(overlay.boxes)),
                    top,
                    bottom,
                    image: self.image(results[i], i),
                    selected: page.is_selected(i),
                    local: local[i - start],
                    files: page
                        .collection(results[i])
                        .map_or_else(SharedString::new, |files| {
                            hydrus_core::numbers::human_int(files.len() as u64).into()
                        }),
                    ..Thumbnail::default()
                };
                let paint =
                    self.palette
                        .borrow()
                        .paint(&thumbnail, self.cell.get().0, thumbnail.local);
                let first = thumbnail.image.size().width > 0
                    && page.admit_thumbnail_fade(
                        results[i],
                        &self.paint_owner,
                        self.admissions
                            .borrow()
                            .get(&results[i])
                            .copied()
                            .unwrap_or(0),
                    );
                self.paints.borrow_mut().decorate(
                    results[i],
                    i,
                    paint,
                    &mut thumbnail,
                    (self.paint_clock.borrow())(),
                    self.visible.get() && self.appearance.borrow().fade,
                    !page.new_thumbnail_renderer() && first,
                );
                thumbnail
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

/// What is drawn over a thumbnail: its icons, and its ratings over their
/// boxes.
struct Overlay {
    icons: Vec<ThumbIcon>,
    ratings: Vec<ThumbRating>,
    boxes: Vec<ThumbBox>,
}

/// How wide a rating's "stars/of" is at a pixel size: the reference
/// measures it in its font; this guesses at the grid's (digits and "/"
/// about six tenths of the size wide).
fn text_width(text: &str, pixel_size: i32) -> i32 {
    let chars = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    (chars * pixel_size * 6 + 9) / 10
}

/// A rating as the grid draws it.
fn thumb_rating(drawn: &thumbnail_ratings::Drawn) -> ThumbRating {
    let colour =
        |rgb: hydrus_store::services::Rgb| slint::Color::from_rgb_u8(rgb.0[0], rgb.0[1], rgb.0[2]);
    let mut out = ThumbRating {
        x: drawn.x as f32,
        y: drawn.y as f32,
        ..ThumbRating::default()
    };
    let text = match &drawn.look {
        Look::Shapes {
            path,
            first,
            size,
            step,
            shapes,
            text,
        } => {
            out.kind = 0;
            out.path = (*path).into();
            out.first = *first as f32;
            out.size = *size as f32;
            out.step = *step as f32;
            out.outline = crate::ratings::outline_width(f64::from(*size)) as f32;
            let shapes: Vec<RatingShape> = shapes
                .iter()
                .map(|s| RatingShape {
                    pen: colour(s.pen),
                    brush: colour(s.brush),
                })
                .collect();
            out.shapes = ModelRc::new(VecModel::from(shapes));
            text.as_ref()
        }
        Look::Counter {
            width,
            height,
            colours,
            text,
        } => {
            out.kind = 1;
            out.width = *width as f32;
            out.height = *height as f32;
            out.pen = colour(colours.pen);
            out.brush = colour(colours.brush);
            out.text_height = (*height - 1) as f32;
            Some(text)
        }
    };
    if let Some(text) = text {
        out.text = text.text.as_str().into();
        out.text_x = text.x as f32;
        out.text_y = text.y as f32;
        out.text_width = text.width as f32;
        out.text_size = text.pixel_size as f32;
    }
    out
}

#[cfg(test)]
mod cache_regressions {
    use super::*;
    use slint::{Rgb8Pixel, SharedPixelBuffer};
    fn rows() -> (tempfile::TempDir, ThumbnailRows) {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let dir = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &dir.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        (
            dir,
            ThumbnailRows::new(Rc::new(RefCell::new(SearchPage::new(store)))),
        )
    }
    fn pixels() -> crate::thumbnails::Pixels {
        crate::thumbnails::Pixels::Rgb(SharedPixelBuffer::<Rgb8Pixel>::new(3, 4))
    }
    #[test]
    fn clear_scale_roundtrip_settings_and_retirement_reject_held_decodes() {
        let (_dir, rows) = rows();
        let file = HashId(1);
        let old = rows.generation.get();
        rows.pending.borrow_mut().insert(file, 0);
        rows.show(vec![(file, 1., old, Some(pixels()))]);
        assert_eq!(rows.cached_bytes(), 36);
        rows.clear_thumbnail_cache();
        assert_eq!(rows.cached(), 0);
        rows.pending.borrow_mut().insert(file, 0);
        rows.show(vec![(file, 1., old, Some(pixels()))]);
        assert!(rows.pending.borrow().contains_key(&file));
        assert_eq!(rows.cached(), 0);
        let before_roundtrip = rows.generation.get();
        rows.set_scale(2.);
        rows.set_scale(1.);
        rows.pending.borrow_mut().insert(file, 0);
        rows.show(vec![(file, 1., before_roundtrip, Some(pixels()))]);
        assert!(rows.pending.borrow().contains_key(&file));
        assert_eq!(rows.cached(), 0);
        let before_settings = rows.generation.get();
        rows.thumbnails_changed();
        rows.pending.borrow_mut().insert(file, 0);
        rows.show(vec![(file, 1., before_settings, Some(pixels()))]);
        assert!(rows.pending.borrow().contains_key(&file));
        rows.show(vec![(file, 1., rows.generation.get(), Some(pixels()))]);
        assert!(rows.pending.borrow().is_empty());
        assert_eq!(rows.cached_bytes(), 36);
        let retired_generation = rows.generation.get();
        rows.retire();
        rows.show(vec![(file, 1., retired_generation, Some(pixels()))]);
        assert_eq!(rows.cached_bytes(), 0);
        assert!(rows.pending.borrow().is_empty());
        assert_eq!(rows.image(file, 0).size().width, 0);
    }
    #[test]
    fn blurhash_apply_rejects_held_result_but_paint_only_changes_preserve_admission() {
        let (_dir, rows) = rows();
        let file = HashId(1);
        let generation = rows.generation.get();
        let mut appearance = rows.appearance.borrow().clone();
        appearance.blurhash = !appearance.blurhash;
        rows.set_appearance(appearance.clone());
        rows.pending.borrow_mut().insert(file, 0);
        rows.show(vec![(file, 1., generation, Some(pixels()))]);
        assert!(rows.pending.borrow().contains_key(&file));
        assert_eq!(rows.cached(), 0);
        let admitted = rows.generation.get();
        appearance.fade = !appearance.fade;
        appearance.background = Some("synthetic.png".into());
        rows.set_appearance(appearance);
        assert_eq!(rows.generation.get(), admitted);
        rows.show(vec![(file, 1., admitted, Some(pixels()))]);
        assert!(rows.pending.borrow().is_empty());
        assert_eq!(rows.cached_bytes(), 36);
    }
    #[test]
    fn receive_maintains_idle_cache_and_counts_missing_thumbnails() {
        let (_dir, rows) = rows();
        let file = HashId(1);
        rows.set_cache_policy(ThumbnailCacheSettings {
            bytes: 128,
            timeout: 0,
        });
        rows.cache
            .borrow_mut()
            .insert(file, slint::Image::default(), 128, Duration::ZERO);
        assert_eq!(rows.cached_bytes(), 128);
        rows.receive();
        assert_eq!(rows.cached_bytes(), 0);
        assert!(rows.pending.borrow().is_empty());
        rows.set_cache_policy(ThumbnailCacheSettings {
            bytes: 1,
            timeout: 300,
        });
        rows.show(vec![(file, 1., rows.generation.get(), None)]);
        assert_eq!(rows.cached_bytes(), 128);
        rows.maintain_cache();
        assert_eq!(rows.cached(), 0);
    }
}
