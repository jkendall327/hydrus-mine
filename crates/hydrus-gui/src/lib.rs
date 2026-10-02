//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::HashId;
use hydrus_store::sessions;
use slint::{ModelRc, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

mod animation;
pub mod archive_delete;
mod archive_delete_window;
pub mod audio;
pub mod autocomplete;
pub mod collect;
pub mod daemon;
mod drops;
pub mod duplicate_filter;
pub mod favourites;
mod filter_window;
mod gallery;
mod grid;
pub mod headless;
mod import_window;
pub mod info_lines;
pub mod local_import;
pub mod main_menu;
pub mod manage_tags;
pub(crate) mod manage_tags_window;
pub mod media_actions;
mod menu_bar;
pub mod mpv;
pub mod options;
mod options_window;
mod page;
pub mod page_chooser;
mod pages;
mod playback;
mod popups;
pub mod ratings;
pub mod scanbar;
pub mod selection;
pub mod slideshow;
pub mod sort;
pub mod status;
pub mod still;
pub mod thumbnail_menu;
mod thumbnails;
mod unlock;
mod viewer;
pub mod viewer_menu;
mod watcher;
pub mod windows;
pub mod zoom;

/// A window's zoomed file ([`zoom::Zoomed`]), drawn in its `media-x`,
/// `media-y`, `media-width` and `media-height`, and a still's sharp
/// overlay in its `sharp` properties; its canvas is the window, or what
/// `$canvas` gives.
macro_rules! zoom_window {
    ($window:expr, $settings:expr) => {
        $crate::zoom_window!($window, $settings, |window| {
            let window = slint::ComponentHandle::window(window);
            let size = window.size().to_logical(window.scale_factor());
            (size.width as i32, size.height as i32)
        })
    };
    ($window:expr, $settings:expr, $canvas:expr) => {
        $crate::zoom::Zoomed::within(
            &$window,
            $settings,
            $canvas,
            |window, (x, y, width, height)| {
                window.set_media_x(x as f32);
                window.set_media_y(y as f32);
                window.set_media_width(width as f32);
                window.set_media_height(height as f32);
            },
            |window, overlay| match overlay {
                Some(overlay) => {
                    let (x, y, width, height) = overlay.rect;
                    window.set_sharp(overlay.image);
                    window.set_sharp_x(x);
                    window.set_sharp_y(y);
                    window.set_sharp_width(width);
                    window.set_sharp_height(height);
                    window.set_sharp_opaque(overlay.opaque);
                    window.set_sharp_shown(true);
                }
                None => {
                    window.set_sharp_shown(false);
                    window.set_sharp(slint::Image::default());
                }
            },
        )
    };
}
pub(crate) use zoom_window;

/// Zoom and pan a window's file from its `zoom`, `pan`, `drag` and
/// `canvas-resized` callbacks.
macro_rules! bind_zoom {
    ($window:expr, $zoomed:expr) => {{
        let zoomed = $zoomed.clone();
        $window.on_canvas_resized({
            let zoomed = zoomed.clone();
            move |_, _| zoomed.resized()
        });
        $window.on_zoom({
            let zoomed = zoomed.clone();
            move |direction, over, x, y| {
                zoomed.zoom(direction, over.then_some((x as i32, y as i32)))
            }
        });
        $window.on_pan({
            let zoomed = zoomed.clone();
            move |x, y| zoomed.pan(x, y)
        });
        $window.on_drag(move |x, y| zoomed.drag((x.round() as i32, y.round() as i32)));
    }};
}
pub(crate) use bind_zoom;

pub use grid::ThumbnailRows;
pub use page::SearchPage;
pub use pages::{Pages, Tabs};
pub use unlock::unlock_window;
pub use viewer::MediaViewer;

/// Pages bound to a window: the pages, the page shown, its grid's rows,
/// and the media viewer while one is open.
#[derive(Clone)]
pub struct Bound {
    pub pages: Rc<RefCell<Pages>>,
    pub current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
    pub rows: Rc<ThumbnailRows>,
    pub viewer: Rc<RefCell<Option<MediaViewerWindow>>>,
    /// The manage tags window while one is open.
    pub manage_tags: Rc<RefCell<Option<ManageTagsWindow>>>,
    /// The options window while it is open.
    pub options: Rc<RefCell<Option<OptionsWindow>>>,
    /// The archive/delete filter while one is open.
    pub archive_delete: Rc<RefCell<Option<ArchiveDeleteWindow>>>,
    /// The duplicate filter while one is open.
    pub filter: Rc<RefCell<Option<DuplicateFilterWindow>>>,
    /// Open a new page (as the page chooser does), and show it.
    pub open_page: Rc<dyn Fn(&page_chooser::NewPage)>,
    /// The "review files to import" window while it is open, and its list.
    pub review_imports: ReviewSlot,
    /// Files dropped on the main window: the "review files to import"
    /// window with them (they join its list if it is open).
    pub drop_files: Rc<dyn Fn(Vec<String>)>,
    /// Do what the Client API asked of the pages (`/manage_pages`), and keep
    /// the pages and the media viewer in the store as they are, for it to
    /// answer from: the client runs this every half second.
    pub sync: Rc<dyn Fn()>,
    /// Shows thumbnails as they are decoded (held to keep it running).
    _thumbnails: Rc<slint::Timer>,
    /// Shows the menu bar's titles and the status bar's network part as
    /// they change (held likewise).
    _menu_titles: Rc<slint::Timer>,
    /// Shows the popup messages (held likewise).
    _popups: Rc<slint::Timer>,
}

impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bound")
            .field("pages", &self.pages.borrow())
            .field("rows", &self.rows)
            .field("viewer", &self.viewer.borrow().is_some())
            .finish_non_exhaustive()
    }
}

/// The "review files to import" window while it is open, and its list.
pub type ReviewSlot = Rc<RefCell<Option<(ReviewImportsWindow, Rc<RefCell<local_import::Review>>)>>>;

/// Two presses this close together are a double click (where Slint's own
/// double click can't be had: list rows, and middle clicks).
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

/// A decoded image as Slint shows it.
pub fn image(raster: &hydrus_media::Raster) -> slint::Image {
    thumbnails::Pixels::new(raster).image()
}

/// A sort chosen on `page`: the default sort, if the options say a chosen
/// sort becomes it (the reference's `_UserChoseASort`).
fn sort_chosen(page: &SearchPage) {
    let sort = page.sort().clone();
    let saved = page.store().write(move |ctx| {
        let mut sorts: hydrus_core::pages::SortSettings = hydrus_store::settings::get(ctx.conn())?;
        if sorts.save_page_sort_on_change && sorts.default_sort != sort {
            sorts.default_sort = sort;
            hydrus_store::settings::set(ctx.conn(), &sorts)?;
        }
        Ok(())
    });
    if let Err(e) = saved {
        eprintln!("could not keep the default sort: {e}");
    }
}

/// The grid's cells as the reference's: the bounding box and its border,
/// with the margin round each.
fn lay_out_thumbnails(window: &MainWindow, store: &hydrus_store::Store) {
    let settings = store.snapshot().thumbnails;
    let layout = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::ThumbnailLayout>)
        .unwrap_or_default();
    let border = layout.border as f32;
    window.set_thumbnail_width(settings.bounding_width as f32 + 2.0 * border);
    window.set_thumbnail_height(settings.bounding_height as f32 + 2.0 * border);
    window.set_thumbnail_border(border);
    window.set_thumbnail_margin(layout.margin as f32);
}

/// Show `pages` in `window`, and let the window change them.
pub fn bind(window: &MainWindow, pages: Pages) -> Bound {
    let pages = Rc::new(RefCell::new(pages));
    let first = pages.borrow_mut().current();
    let current = Rc::new(RefCell::new(first.clone()));
    let rows = Rc::new(ThumbnailRows::new(first));
    window.set_thumbnail_rows(ModelRc::from(rows.clone()));
    rows.set_columns(usize::try_from(window.get_grid_columns()).unwrap_or(1));
    rows.set_scale(window.window().scale_factor());
    lay_out_thumbnails(window, current.borrow().borrow().store());
    let thumbnails = Rc::new(slint::Timer::default());
    thumbnails.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(25),
        {
            let rows = rows.clone();
            let window = window.as_weak();
            move || {
                if let Some(window) = window.upgrade() {
                    rows.set_scale(window.window().scale_factor());
                }
                rows.receive();
            }
        },
    );
    show_tabs(window, &pages.borrow());
    // the favourite searches, read once (the reference reads its own
    // manager, loaded at boot)
    let favourites: Rc<Vec<hydrus_core::pages::FavouriteSearch>> = Rc::new(
        current
            .borrow()
            .borrow()
            .store()
            .read(hydrus_store::settings::get::<hydrus_store::settings::FavouriteSearches>)
            .map(|f| f.0)
            .unwrap_or_default(),
    );
    let favourite_rows: Vec<FavouriteRow> = favourites::favourite_rows(&favourites)
        .into_iter()
        .map(|row| FavouriteRow {
            label: row.label.into(),
            depth: i32::try_from(row.depth).unwrap_or(i32::MAX),
            search: row.search.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1),
        })
        .collect();
    let favourite_rows = ModelRc::new(VecModel::from(favourite_rows));
    refresh(window, &current.borrow().borrow(), &favourite_rows);

    // after a change to the page shown, show it; `true` if its files changed
    let shown = {
        let current = current.clone();
        let weak = window.as_weak();
        let rows = rows.clone();
        let favourite_rows = favourite_rows.clone();
        move |files: bool| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &current.borrow().borrow(), &favourite_rows);
                if files {
                    rows.reset();
                }
            }
        }
    };
    // the page shown, to change
    let page = {
        let current = current.clone();
        move || current.borrow().clone()
    };
    // change the pages, then show whichever page is now shown; a change
    // that can't be made says why
    // (the menu bar's titles, shown again after a change)
    let after_change: AfterChange = Rc::default();
    pages.borrow_mut().note_shown();
    // how far each page's thumbnails were scrolled when another page was
    // shown, as each of the reference's pages keeps its own (kept apart from
    // the pages: a page switch can come from a page's own action, while it
    // is in use)
    let scrolls: Rc<RefCell<std::collections::HashMap<hydrus_core::pages::PageKey, f32>>> =
        Rc::default();
    let change_pages = {
        let pages = pages.clone();
        let current = current.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        let shown = shown.clone();
        let after_change = after_change.clone();
        move |change: &dyn Fn(&mut Pages) -> Result<(), String>| {
            if let Some(window) = weak.upgrade() {
                let key = pages.borrow().shown().key;
                scrolls.borrow_mut().insert(key, window.get_grid_scroll());
            }
            let (result, opened) = {
                let mut pages = pages.borrow_mut();
                let result = change(&mut pages);
                pages.note_shown();
                (result, pages.current())
            };
            let after = after_change.borrow().clone();
            if let Some(after) = after {
                after();
            }
            *current.borrow_mut() = opened.clone();
            let key = pages.borrow().shown().key;
            let scroll = scrolls.borrow().get(&key).copied().unwrap_or(0.0);
            rows.set_page(opened);
            if let Some(window) = weak.upgrade() {
                show_tabs(&window, &pages.borrow());
                window.set_grid_scroll(scroll);
            }
            shown(false);
            if let (Err(e), Some(window)) = (result, weak.upgrade()) {
                window.set_error(e.into());
            }
        }
    };
    window.on_tab_chosen({
        let change_pages = change_pages.clone();
        move |level, index| {
            let (Ok(level), Ok(index)) = (usize::try_from(level), usize::try_from(index)) else {
                return;
            };
            change_pages(&|pages| {
                pages.select(level, index);
                Ok(())
            });
        }
    });
    // the page chooser, while open
    let chooser: Rc<RefCell<Option<page_chooser::PageChooser>>> = Rc::default();
    let show_chooser = {
        let chooser = chooser.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let labels: Vec<SharedString> = chooser
                .borrow()
                .as_ref()
                .map(|c| c.labels().iter().map(|l| l.as_str().into()).collect())
                .unwrap_or_default();
            window.set_chooser_labels(ModelRc::new(VecModel::from(labels)));
        }
    };
    let open_page: Rc<dyn Fn(&page_chooser::NewPage)> = Rc::new({
        let change_pages = change_pages.clone();
        move |choice: &page_chooser::NewPage| change_pages(&|pages| pages.new_page(choice))
    });
    // the "review files to import" window, with paths (from file > import
    // files, or dropped on the window); "import now" opens an import page
    let review_imports: ReviewSlot = Rc::default();
    let review_files: Rc<dyn Fn(Vec<String>)> = Rc::new({
        let slot = review_imports.clone();
        let open_page = open_page.clone();
        move |paths: Vec<String>| {
            if let Some((window, review)) = slot.borrow().as_ref() {
                review.borrow_mut().add_paths(paths);
                // (parsing again: its pause and stop buttons)
                window.set_working(review.borrow().working());
                let _ = window.show();
                return;
            }
            let import_now: import_window::ImportNow = Rc::new({
                let open_page = open_page.clone();
                move |paths, delete_after_success| {
                    open_page(&page_chooser::NewPage::LocalImport {
                        paths,
                        delete_after_success,
                    });
                }
            });
            if let Err(e) = import_window::open(&slot, paths, import_now) {
                eprintln!("could not open the import window: {e}");
            }
        }
    });
    drops::on_files_dropped(window.window(), {
        let review_files = review_files.clone();
        move |paths| review_files(paths)
    });
    // open the page chosen, if one was
    let chosen = {
        let chooser = chooser.clone();
        let show_chooser = show_chooser.clone();
        let change_pages = change_pages.clone();
        move |choice: Option<page_chooser::NewPage>| {
            if let Some(choice) = choice {
                chooser.borrow_mut().take();
                change_pages(&|pages| pages.new_page(&choice));
            }
            show_chooser();
        }
    };
    window.on_new_page({
        let chooser = chooser.clone();
        let pages = pages.clone();
        let show_chooser = show_chooser.clone();
        move || {
            let store = {
                let mut pages = pages.borrow_mut();
                pages.new_page_in(None);
                pages.store().clone()
            };
            *chooser.borrow_mut() = Some(page_chooser::PageChooser::new(&store));
            show_chooser();
        }
    });
    // a double click (left or middle) on a tab row's empty space: the
    // page chooser, for that row's notebook
    window.on_tab_space_pressed({
        let chooser = chooser.clone();
        let pages = pages.clone();
        let show_chooser = show_chooser.clone();
        let last: Rc<Cell<Option<(i32, bool, std::time::Instant)>>> = Rc::default();
        move |level, middle| {
            let now = std::time::Instant::now();
            let double = last.get().is_some_and(|(l, m, at)| {
                l == level && m == middle && now.duration_since(at) < DOUBLE_CLICK
            });
            if !double {
                last.set(Some((level, middle, now)));
                return;
            }
            last.set(None);
            let store = {
                let mut pages = pages.borrow_mut();
                pages.new_page_in(usize::try_from(level).ok());
                pages.store().clone()
            };
            *chooser.borrow_mut() = Some(page_chooser::PageChooser::new(&store));
            show_chooser();
        }
    });
    window.on_chooser_pressed({
        let chooser = chooser.clone();
        let chosen = chosen.clone();
        move |number| {
            let choice = chooser
                .borrow_mut()
                .as_mut()
                .and_then(|c| c.press(usize::try_from(number).unwrap_or(0)));
            chosen(choice);
        }
    });
    window.on_chooser_enter({
        let chooser = chooser.clone();
        let chosen = chosen.clone();
        move || {
            let choice = chooser
                .borrow_mut()
                .as_mut()
                .and_then(page_chooser::PageChooser::enter);
            chosen(choice);
        }
    });
    window.on_chooser_cancel({
        let chooser = chooser.clone();
        let pages = pages.clone();
        let show_chooser = show_chooser.clone();
        move || {
            chooser.borrow_mut().take();
            pages.borrow_mut().new_page_in(None);
            show_chooser();
        }
    });
    window.on_unclose_page({
        let change_pages = change_pages.clone();
        move || {
            change_pages(&|pages| {
                pages.unclose();
                Ok(())
            });
        }
    });
    // ctrl+page up and down: the page beside (`MoveSelection`)
    window.on_move_pages({
        let change_pages = change_pages.clone();
        move |delta| {
            change_pages(&|pages| {
                pages.move_selection(delta as isize, std::time::Instant::now());
                Ok(())
            });
        }
    });
    window.on_search_edited({
        let page = page.clone();
        let shown = shown.clone();
        move |text| {
            page().borrow_mut().type_text(&text);
            shown(false);
        }
    });
    window.on_search_accepted({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().enter();
            shown(true);
        }
    });
    window.on_favourite_chosen({
        let page = page.clone();
        let shown = shown.clone();
        let favourites = favourites.clone();
        move |index| {
            if let Some(favourite) = usize::try_from(index).ok().and_then(|i| favourites.get(i)) {
                page().borrow_mut().load_favourite(favourite);
                shown(true);
            }
        }
    });
    window.on_move_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move |by| {
            page().borrow_mut().move_highlight(by as isize);
            shown(false);
        }
    });
    window.on_suggestion_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            page()
                .borrow_mut()
                .choose(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
        }
    });
    window.on_tag_activated({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            page()
                .borrow_mut()
                .activate_tag(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
        }
    });
    window.on_remove_predicate({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            page()
                .borrow_mut()
                .remove_predicate(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
        }
    });
    // f5: search again (`RefreshQuery`)
    window.on_refresh_page({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().refresh();
            shown(true);
        }
    });
    // ctrl+i, or the button: searching as the search changes, or waiting
    window.on_flip_synchronised({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let synchronised = page.borrow().synchronised();
            page.borrow_mut().set_synchronised(!synchronised);
            shown(true);
        }
    });
    // (the sort types offered are the page's, as `refresh` lists them)
    window.on_sort_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            let page = page();
            let choices = {
                let page = page.borrow();
                sort::page_choices(page.store(), &page.sort().by)
            };
            if let Some(choice) = usize::try_from(index).ok().and_then(|i| choices.get(i)) {
                page.borrow_mut().set_sort_type(choice.by.clone());
                sort_chosen(&page.borrow());
                shown(true);
            }
        }
    });
    window.on_order_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            use hydrus_search::SortOrder;
            let order = if index == 0 {
                SortOrder::Ascending
            } else {
                SortOrder::Descending
            };
            let page = page();
            page.borrow_mut().set_sort_order(order);
            sort_chosen(&page.borrow());
            shown(true);
        }
    });
    // (the collect control's choices are as `refresh` lists them)
    window.on_collect_toggled({
        let page = page.clone();
        let shown = shown.clone();
        move |index, on| {
            let page = page();
            let collect = {
                let page = page.borrow();
                let choices = collect::choices(page.store());
                usize::try_from(index)
                    .ok()
                    .filter(|&i| i < choices.len())
                    .map(|i| collect::toggled(&choices, page.collect(), i, on))
            };
            if let Some(collect) = collect {
                page.borrow_mut().set_collect(collect);
                shown(true);
            }
        }
    });
    window.on_collect_unmatched_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |collect_unmatched| {
            let page = page();
            let collect = {
                let page = page.borrow();
                let choices = collect::choices(page.store());
                collect::with_unmatched(&choices, page.collect(), collect_unmatched)
            };
            page.borrow_mut().set_collect(collect);
            shown(true);
        }
    });
    window.on_columns_changed({
        let rows = rows.clone();
        move |columns| rows.set_columns(usize::try_from(columns).unwrap_or(1))
    });
    let filter: Rc<RefCell<Option<DuplicateFilterWindow>>> = Rc::default();
    window.on_launch_filter({
        let page = page.clone();
        let filter = filter.clone();
        let weak = window.as_weak();
        move || {
            let page = page();
            let page = page.borrow();
            let Some(duplicates) = page.duplicates() else {
                return;
            };
            let opened =
                duplicate_filter::DuplicateFilter::for_page(page.store().clone(), duplicates)
                    .and_then(|mut model| {
                        let step = model.load_batch();
                        filter_window::open_filter(model, step, &filter)
                            .map_err(|e| anyhow::anyhow!("{e}"))
                    });
            match opened {
                Ok(window) => *filter.borrow_mut() = Some(window),
                Err(e) => {
                    if let Some(window) = weak.upgrade() {
                        window
                            .set_error(format!("could not open the duplicate filter: {e}").into());
                    }
                }
            }
        }
    });
    let viewer: Rc<RefCell<Option<MediaViewerWindow>>> = Rc::default();
    // files deleted from the page's domain leave it
    let removed: Removed = Rc::new({
        let page = page.clone();
        let shown = shown.clone();
        move |files: &[HashId]| {
            page().borrow_mut().remove_files(files);
            shown(true);
        }
    });
    // F3: manage tags; once applied, the tags are counted again
    let manage_tags: Rc<RefCell<Option<ManageTagsWindow>>> = Rc::default();
    let tags_changed: Rc<dyn Fn()> = Rc::new({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().refresh_tags();
            shown(false);
        }
    });
    let open_manage_tags = {
        let manage_tags = manage_tags.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            let Some(model) = manage_tags::ManageTags::new(store, files) else {
                return;
            };
            match manage_tags_window::open(model, &manage_tags, applied) {
                Ok(window) => *manage_tags.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage tags: {e}"),
            }
        }
    };
    window.on_manage_tags_selected({
        let page = page.clone();
        let manage_tags = manage_tags.clone();
        let open_manage_tags = open_manage_tags.clone();
        let tags_changed = tags_changed.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let files = page.selected_files();
            if files.is_empty() {
                return;
            }
            let title = format!(
                "manage tags for {} files",
                hydrus_core::numbers::human_int(files.len() as u64)
            );
            open_manage_tags(page.store().clone(), files, tags_changed.clone());
            if let Some(window) = manage_tags.borrow().as_ref() {
                window.set_window_title(title.into());
            }
        }
    });
    // the selected files' shortcuts: F7, shift+F7, delete, shift+delete;
    // as the reference's defaults have it, archiving or inboxing several
    // asks first, and deleting always does
    let selected_files = {
        let page = page.clone();
        move || page().borrow().selected_files()
    };
    let pending: Rc<RefCell<Option<Asked>>> = Rc::default();
    let ask = {
        let pending = pending.clone();
        let weak = window.as_weak();
        move |asked: Asked| {
            if let Some(window) = weak.upgrade() {
                window.set_question(asked.question().into());
                *pending.borrow_mut() = Some(asked);
            }
        }
    };
    // (the status bar counts the selection's inbox)
    let archive_or_inbox = |archive: bool| {
        let page = page.clone();
        let selected_files = selected_files.clone();
        let ask = ask.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let store = page.borrow().store().clone();
            let (inbox, archived) = media_actions::by_inbox(&store, &selected_files());
            let files = if archive { inbox } else { archived };
            match files.len() {
                0 => {}
                1 => {
                    Asked::archive_or_inbox(archive, files).act(&store, &|_| {});
                    shown(false);
                }
                _ => ask(Asked::archive_or_inbox(archive, files)),
            }
        }
    };
    window.on_archive_selected(archive_or_inbox(true));
    window.on_inbox_selected(archive_or_inbox(false));
    window.on_undelete_selected({
        let page = page.clone();
        let selected_files = selected_files.clone();
        let shown = shown.clone();
        move || {
            let files = selected_files();
            if !files.is_empty()
                && let Err(e) = media_actions::undelete(page().borrow().store(), &files)
            {
                eprintln!("could not undelete the files: {e}");
            }
            shown(false);
        }
    });
    window.on_delete_selected({
        let page = page.clone();
        let ask = ask.clone();
        move || {
            let files = selected_files();
            if files.is_empty() {
                return;
            }
            let page = page();
            let page = page.borrow();
            if let Some(deletion) = media_actions::deletion(page.store(), page.location(), &files) {
                ask(Asked::Delete(files, deletion, page.location().clone()));
            }
        }
    });
    window.on_answer({
        let page = page.clone();
        let weak = window.as_weak();
        let removed = removed.clone();
        let shown = shown.clone();
        let change_pages = change_pages.clone();
        move |yes| {
            let asked = pending.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            if let Some(asked) = asked.filter(|_| yes) {
                if let Asked::LockSearch(_) = asked {
                    page().borrow_mut().lock_search();
                    shown(false);
                    return;
                }
                if let Asked::ClosePage(depth, index, _) = asked {
                    change_pages(&|pages| pages.close(depth, index));
                    return;
                }
                if let Asked::RemoveQuery(queue, _) = asked {
                    page().borrow_mut().remove_query(queue);
                    shown(true);
                    return;
                }
                let store = page().borrow().store().clone();
                asked.act(&store, &*removed);
                shown(false);
            }
        }
    });
    // ctrl+w, and a middle click on a tab: a URL downloader page still
    // importing, or holding imports, asks first (`AskIfAbleToClose`)
    let close = {
        let pages = pages.clone();
        let change_pages = change_pages.clone();
        let ask = ask.clone();
        move |depth: usize, index: usize| {
            let question = pages.borrow_mut().close_question(depth, index);
            match question {
                Some(question) => ask(Asked::ClosePage(depth, index, question)),
                None => change_pages(&|pages| pages.close(depth, index)),
            }
        }
    };
    window.on_close_page({
        let pages = pages.clone();
        let close = close.clone();
        move || {
            let shown = pages.borrow().shown_position();
            close(shown.0, shown.1);
        }
    });
    window.on_close_tab({
        let close = close.clone();
        move |level, index| {
            if let (Ok(level), Ok(index)) = (usize::try_from(level), usize::try_from(index)) {
                close(level, index);
            }
        }
    });
    // the menu bar, its titles shown again as what they say changes
    let options: Rc<RefCell<Option<OptionsWindow>>> = Rc::default();
    let menu_titles_shown = menu_bar::bind(
        window,
        menu_bar::Hooks {
            pages: pages.clone(),
            change_pages: Rc::new(change_pages.clone()),
            ask: {
                let ask = ask.clone();
                Rc::new(move |question, then| ask(Asked::Then(question, then)))
            },
            reshow: {
                let shown = shown.clone();
                Rc::new(move || shown(true))
            },
            // file > options; once applied, what the options change is
            // shown again
            options: {
                let pages = pages.clone();
                let slot = options.clone();
                let change_pages = change_pages.clone();
                let rows = rows.clone();
                let weak = window.as_weak();
                Rc::new(move || {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let store = pages.borrow().store().clone();
                    let thumbnails_before = store.snapshot().thumbnails;
                    let applied: Rc<dyn Fn()> = Rc::new({
                        let pages = pages.clone();
                        let change_pages = change_pages.clone();
                        let rows = rows.clone();
                        let weak = weak.clone();
                        let store = store.clone();
                        move || {
                            pages.borrow_mut().reload_settings();
                            // (the cells as the options now have them; and
                            // thumbnails of another size, every one again)
                            if let Some(window) = weak.upgrade() {
                                lay_out_thumbnails(&window, &store);
                            }
                            if store.snapshot().thumbnails != thumbnails_before {
                                rows.thumbnails_changed();
                            }
                            change_pages(&|_| Ok(()));
                        }
                    });
                    match options_window::open(&store, &slot, applied) {
                        Ok(window) => *slot.borrow_mut() = Some(window),
                        Err(e) => eprintln!("could not open the options: {e}"),
                    }
                })
            },
            import_files: {
                let review_files = review_files.clone();
                Rc::new(move || review_files(Vec::new()))
            },
        },
    );
    *after_change.borrow_mut() = Some(menu_titles_shown.clone());
    // (and the status bar's network part, from the daemon's word)
    let network_shown = {
        let pages = pages.clone();
        let weak = window.as_weak();
        let session = RefCell::new(status::SessionBytes::default());
        move || {
            let Some(window) = weak.upgrade() else { return };
            let store = pages.borrow().store().clone();
            let read = store.read(|conn| {
                let pauses: hydrus_store::settings::Pauses = hydrus_store::settings::get(conn)?;
                // (none said yet: never kept)
                let live: hydrus_store::live::DaemonLive = hydrus_store::settings::get(conn)?;
                Ok((pauses, (live.at != 0).then_some(live)))
            });
            let Ok((pauses, live)) = read else { return };
            let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
            let (bytes, per_second) = session.borrow_mut().read(live, now);
            window.set_status_network(status::bandwidth_status(bytes, per_second, &pauses).into());
        }
    };
    network_shown();
    let menu_titles = Rc::new(slint::Timer::default());
    let ticks = Cell::new(0u32);
    menu_titles.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(1),
        move || {
            network_shown();
            ticks.set(ticks.get() + 1);
            if ticks.get().is_multiple_of(2) {
                menu_titles_shown();
            }
        },
    );
    // popup messages, the daemon's and the Client API's
    let popup_timer = popups::bind(
        window,
        popups::Hooks {
            pages: pages.clone(),
            change_pages: Rc::new(change_pages.clone()),
        },
    );
    // a URL downloader page's importer: pausing, and URLs typed or pasted
    window.on_pause_play_files({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().pause_play_files();
            shown(false);
        }
    });
    window.on_cancel_download({
        let page = page.clone();
        move |gallery| {
            let kind = if gallery {
                hydrus_store::live::JobKind::Gallery
            } else {
                hydrus_store::live::JobKind::File
            };
            page().borrow().cancel_download(kind);
        }
    });
    window.on_url_entered({
        let page = page.clone();
        move |text| page().borrow().pend_urls(&text)
    });
    window.on_paste_urls({
        let page = page.clone();
        let weak = window.as_weak();
        move || match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => page().borrow().pend_urls(&text),
            Err(e) => {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("Problem pasting! {e}").into());
                }
            }
        }
    });
    // a gallery page's sidebar: a row pressed selects its search, twice
    // (a double click) shows it
    window.on_gallery_row_pressed({
        let page = page.clone();
        let shown = shown.clone();
        let last: Rc<Cell<Option<(i32, std::time::Instant)>>> = Rc::default();
        move |row| {
            let page = page();
            let queue = usize::try_from(row)
                .ok()
                .and_then(|row| page.borrow().gallery()?.queries.get(row).map(|q| q.queue));
            let Some(queue) = queue else {
                return;
            };
            let now = std::time::Instant::now();
            let double = last
                .get()
                .is_some_and(|(r, at)| r == row && now.duration_since(at) < DOUBLE_CLICK);
            page.borrow_mut().select_query(Some(queue));
            if double {
                last.set(None);
                page.borrow_mut().highlight_query(Some(queue));
                shown(true);
            } else {
                last.set(Some((row, now)));
                shown(false);
            }
        }
    });
    window.on_gallery_sort({
        let page = page.clone();
        let shown = shown.clone();
        move |column, ascending| {
            if let Some(column) = usize::try_from(column)
                .ok()
                .and_then(gallery::Column::from_index)
            {
                page().borrow_mut().sort_queries(column, ascending);
                shown(false);
            }
        }
    });
    window.on_gallery_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let selected = page.borrow().gallery().and_then(|g| g.selected);
            if selected.is_some() {
                page.borrow_mut().highlight_query(selected);
                shown(true);
            }
        }
    });
    window.on_gallery_clear_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().highlight_query(None);
            shown(true);
        }
    });
    window.on_gallery_pause_play({
        let page = page.clone();
        let shown = shown.clone();
        move |search, of_shown| {
            let page = page();
            let queue = page.borrow().gallery().and_then(|g| {
                if of_shown {
                    g.state.highlighted
                } else {
                    g.selected
                }
            });
            if let Some(queue) = queue {
                page.borrow_mut().pause_play_query(queue, search);
                shown(false);
            }
        }
    });
    window.on_gallery_retry({
        let page = page.clone();
        let shown = shown.clone();
        move |ignored| {
            let page = page();
            let selected = page.borrow().gallery().and_then(|g| g.selected);
            if let Some(queue) = selected {
                page.borrow_mut().retry_query(queue, ignored);
                shown(false);
            }
        }
    });
    window.on_gallery_remove({
        let page = page.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let Some(queue) = page.gallery().and_then(|g| g.selected) else {
                return;
            };
            if let Some(question) = page.remove_query_question(queue) {
                ask(Asked::RemoveQuery(queue, question));
            }
        }
    });
    let pend_queries = {
        let page = page.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        move |text: &str| {
            let result = page().borrow_mut().pend_queries(text);
            if let (Err(e), Some(window)) = (result, weak.upgrade()) {
                window.set_error(e.into());
            }
            shown(true);
        }
    };
    window.on_gallery_queries({
        let pend_queries = pend_queries.clone();
        move |text| pend_queries(&text)
    });
    window.on_gallery_paste({
        let weak = window.as_weak();
        move || match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => pend_queries(&text),
            Err(e) => {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("Problem pasting! {e}").into());
                }
            }
        }
    });
    window.on_gallery_gug({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            let page = page();
            let chosen = usize::try_from(index)
                .ok()
                .and_then(|i| page.borrow().gallery()?.gugs.get(i).cloned());
            if let Some((key, name, _)) = chosen {
                page.borrow_mut().set_gug(&key, &name);
                shown(false);
            }
        }
    });
    window.on_gallery_limit({
        let page = page.clone();
        let shown = shown.clone();
        move |none, value| {
            let limit = if none {
                None
            } else {
                u64::try_from(value).ok()
            };
            page().borrow_mut().set_file_limit(limit);
            shown(false);
        }
    });
    // a watcher page's sidebar, as a gallery page's: a row pressed selects
    // its watcher, twice (a double click) shows it
    window.on_watcher_row_pressed({
        let page = page.clone();
        let shown = shown.clone();
        let last: Rc<Cell<Option<(i32, std::time::Instant)>>> = Rc::default();
        move |row| {
            let page = page();
            let queue = usize::try_from(row)
                .ok()
                .and_then(|row| page.borrow().watchers()?.watchers.get(row).map(|w| w.queue));
            let Some(queue) = queue else {
                return;
            };
            let now = std::time::Instant::now();
            let double = last
                .get()
                .is_some_and(|(r, at)| r == row && now.duration_since(at) < DOUBLE_CLICK);
            page.borrow_mut().select_query(Some(queue));
            if double {
                last.set(None);
                page.borrow_mut().highlight_query(Some(queue));
                shown(true);
            } else {
                last.set(Some((row, now)));
                shown(false);
            }
        }
    });
    window.on_watcher_sort({
        let page = page.clone();
        let shown = shown.clone();
        move |column, ascending| {
            if let Some(column) = usize::try_from(column)
                .ok()
                .and_then(watcher::Column::from_index)
            {
                page().borrow_mut().sort_watchers(column, ascending);
                shown(false);
            }
        }
    });
    window.on_watcher_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let selected = page.borrow().watchers().and_then(|w| w.selected);
            if selected.is_some() {
                page.borrow_mut().highlight_query(selected);
                shown(true);
            }
        }
    });
    window.on_watcher_clear_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().highlight_query(None);
            shown(true);
        }
    });
    // (the list's selected watcher, or the one shown)
    let watcher_of = |page: &Rc<RefCell<SearchPage>>, of_shown: bool| {
        page.borrow().watchers().and_then(|w| {
            if of_shown {
                w.state.highlighted
            } else {
                w.selected
            }
        })
    };
    window.on_watcher_pause_play({
        let page = page.clone();
        let shown = shown.clone();
        move |checking, of_shown| {
            let page = page();
            if let Some(queue) = watcher_of(&page, of_shown) {
                page.borrow_mut().pause_play_watcher(queue, checking);
                shown(false);
            }
        }
    });
    window.on_watcher_check_now({
        let page = page.clone();
        let shown = shown.clone();
        move |of_shown| {
            let page = page();
            if let Some(queue) = watcher_of(&page, of_shown) {
                page.borrow_mut().check_watcher_now(queue);
                shown(false);
            }
        }
    });
    window.on_watcher_retry({
        let page = page.clone();
        let shown = shown.clone();
        move |ignored| {
            let page = page();
            if let Some(queue) = watcher_of(&page, false) {
                page.borrow_mut().retry_query(queue, ignored);
                shown(false);
            }
        }
    });
    window.on_watcher_remove({
        let page = page.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let Some(queue) = page.watchers().and_then(|w| w.selected) else {
                return;
            };
            if let Some(question) = page.remove_watcher_question(queue) {
                ask(Asked::RemoveQuery(queue, question));
            }
        }
    });
    let pend_watchers = {
        let page = page.clone();
        let shown = shown.clone();
        move |text: &str| {
            page().borrow_mut().pend_watchers(text);
            shown(true);
        }
    };
    window.on_watcher_urls({
        let pend_watchers = pend_watchers.clone();
        move |text| pend_watchers(&text)
    });
    window.on_watcher_paste({
        let weak = window.as_weak();
        move || match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => pend_watchers(&text),
            Err(e) => {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("Problem pasting! {e}").into());
                }
            }
        }
    });
    // the search's lock (the reference's lock button, and the lock box's
    // unlock button and cog)
    window.on_lock_search({
        let page = page.clone();
        let ask = ask.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let question = page.borrow().lock_question();
            if let Some(question) = question {
                ask(Asked::LockSearch(question));
            } else {
                page.borrow_mut().lock_search();
                shown(false);
            }
        }
    });
    window.on_unlock_search({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().unlock();
            shown(false);
        }
    });
    window.on_lock_syncs_changed({
        let page = page.clone();
        move |syncs_new, syncs_removes| {
            page().borrow_mut().set_lock_syncs(syncs_new, syncs_removes);
        }
    });
    // F12: the archive/delete filter, on the files selected, else them all
    let archive_delete: Rc<RefCell<Option<ArchiveDeleteWindow>>> = Rc::default();
    window.on_flip_global_mute({
        let pages = pages.clone();
        let viewer = viewer.clone();
        move || {
            audio::flip_global_mute(pages.borrow().store());
            if let Some(viewer) = viewer.borrow().as_ref() {
                viewer.invoke_audio_changed();
            }
        }
    });
    window.on_archive_delete_filter({
        let page = page.clone();
        let archive_delete = archive_delete.clone();
        let removed = removed.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let files = match page.selected_files() {
                selected if selected.is_empty() => page.files(),
                selected => selected,
            };
            let Some(model) = archive_delete::ArchiveDeleteFilter::new(
                page.store().clone(),
                files,
                page.location().clone(),
            ) else {
                return;
            };
            match archive_delete_window::open(
                model,
                page.location().clone(),
                &archive_delete,
                removed.clone(),
            ) {
                Ok(window) => *archive_delete.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the archive/delete filter: {e}"),
            }
        }
    });
    let viewing: Viewing = Rc::default();
    window.on_thumbnail_activated({
        let page = page.clone();
        let viewer = viewer.clone();
        let viewing = viewing.clone();
        let change_pages: ChangePages = Rc::new(change_pages.clone());
        move |index| {
            let page = page();
            let page = page.borrow();
            // (over all the page's files, from the item's first, as the
            // reference's `_LaunchMediaViewer` opens)
            let files = page.files();
            let start = usize::try_from(index)
                .ok()
                .and_then(|i| page.results().get(i))
                .and_then(|item| files.iter().position(|f| f == item))
                .unwrap_or(usize::MAX);
            let Some(model) = MediaViewer::new(page.store().clone(), files, start) else {
                return;
            };
            let model = model.with_location(page.location().clone());
            *viewing.borrow_mut() = Some((hydrus_core::pages::PageKey::random().0, None));
            let hooks = ViewerHooks {
                viewing: viewing.clone(),
                removed: removed.clone(),
                tags_changed: tags_changed.clone(),
                manage_tags: Rc::new(open_manage_tags.clone()),
                change_pages: change_pages.clone(),
            };
            match open_viewer(model, &viewer, hooks) {
                Ok(window) => *viewer.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the media viewer: {e}"),
            }
        }
    });
    // a change to the selection draws the rows it changed, and counts the
    // selection's tags
    let reselect = {
        let page = page.clone();
        let rows = rows.clone();
        let shown = shown.clone();
        move |change: &dyn Fn(&mut SearchPage) -> Option<usize>| {
            let page = page();
            let before = page.borrow().selected_indices();
            let focused = change(&mut page.borrow_mut());
            let after = page.borrow().selected_indices();
            rows.selection_changed(&before, &after);
            shown(false);
            focused
        }
    };
    window.on_thumbnail_clicked({
        let reselect = reselect.clone();
        move |index, ctrl, shift| {
            let index = usize::try_from(index).ok();
            reselect(&|page| {
                page.hit(index, ctrl, shift);
                None
            });
        }
    });
    window.on_select_all({
        let reselect = reselect.clone();
        move || {
            reselect(&|page| {
                page.select_all();
                None
            });
        }
    });
    window.on_select_none({
        let reselect = reselect.clone();
        move || {
            reselect(&|page| {
                page.select_none();
                None
            });
        }
    });
    window.on_move_focus({
        let reselect = reselect.clone();
        move |to, shift, columns, page_rows| {
            use selection::Move;
            let to = match to {
                0 => Move::Left,
                1 => Move::Right,
                2 => Move::Up,
                3 => Move::Down,
                4 => Move::PageUp,
                5 => Move::PageDown,
                6 => Move::Home,
                _ => Move::End,
            };
            let size = |n: i32| usize::try_from(n).unwrap_or(1);
            reselect(&|page| page.move_focus(to, shift, size(columns), size(page_rows)))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        }
    });
    // enter: the media viewer, on the focused file (else the first)
    window.on_launch_viewer({
        let page = page.clone();
        let weak = window.as_weak();
        move || {
            let focused = page().borrow().focused().unwrap_or(0);
            if let Some(window) = weak.upgrade() {
                window.invoke_thumbnail_activated(i32::try_from(focused).unwrap_or(0));
            }
        }
    });
    // ctrl+r: the selected files leave the page (`_Remove`, the selection's
    // filter), a selected collection with all its files
    window.on_remove_selected({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let selected = page.borrow().selected_files();
            if !selected.is_empty() {
                page.borrow_mut().remove_files(&selected);
                shown(true);
            }
        }
    });
    // alt and home, end, left or right (`SIMPLE_REARRANGE_THUMBNAILS`)
    window.on_rearrange({
        let page = page.clone();
        let shown = shown.clone();
        move |to| {
            use thumbnail_menu::Rearrange;
            let to = match to {
                0 => Rearrange::Start,
                1 => Rearrange::Back,
                3 => Rearrange::Forward,
                4 => Rearrange::End,
                _ => return,
            };
            page().borrow_mut().rearrange(to);
            shown(true);
        }
    });
    // ctrl+c: the selected files, as files (`SIMPLE_COPY_FILES`)
    window.on_copy_files({
        let page = page.clone();
        move || {
            let page = page();
            let page = page.borrow();
            copy_files(page.store(), &page.selected_files());
        }
    });
    // ctrl+e: the focused file as the OS opens it, if one file is focused
    // (`_HasFocusSingleton`)
    window.on_open_externally({
        let page = page.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let focused = page
                .focused()
                .map(|i| page.results()[i])
                .filter(|&item| page.collection(item).is_none());
            if let Some(path) =
                focused.and_then(|f| thumbnail_menu::paths(page.store(), &[f]).pop())
            {
                launch(&path);
            }
        }
    });
    // the right-click menu: built for the file clicked (selecting it, as
    // a click would), and its entries done
    let menu_state: MenuState = Rc::default();
    window.on_thumbnail_menu_requested({
        let page = page.clone();
        let reselect = reselect.clone();
        let menu_state = menu_state.clone();
        let weak = window.as_weak();
        move |index| {
            if let Ok(index) = usize::try_from(index) {
                reselect(&|page| {
                    page.hit(Some(index), false, false);
                    None
                });
            }
            let page = page();
            let page = page.borrow();
            let files = thumbnail_menu::facts(page.store(), &page.files());
            let selected: std::collections::HashSet<HashId> =
                page.selected_files().into_iter().collect();
            let snapshot = page.store().snapshot();
            let settings: hydrus_core::media_viewer::InfoLineSettings = page
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let info = thumbnail_menu::info_menu(
                page.store(),
                page.focused().map(|i| page.results()[i]),
                (&page.selected_files(), page.selected_counts()),
                &settings,
                hydrus_core::TimestampMs::now().0,
            );
            let share = (!selected.is_empty()).then(|| {
                thumbnail_menu::share_menu(
                    page.store(),
                    &files,
                    page.focused().map(|i| page.results()[i]),
                    &page.selected_files(),
                )
            });
            let open = thumbnail_menu::open_menu(
                page.store(),
                page.focused().map(|i| page.results()[i]),
                selected.len(),
            );
            let url_facts = thumbnail_menu::url_facts(
                page.store(),
                page.focused().map(|i| page.results()[i]),
                &page.selected_files(),
            );
            let urls = (!selected.is_empty())
                .then(|| thumbnail_menu::urls_menu(&url_facts))
                .flatten();
            let rearrange = thumbnail_menu::rearrange_menu(
                page.results(),
                &page.selected_items().into_iter().collect(),
                page.focused().map(|i| page.results()[i]),
            );
            let entries = thumbnail_menu::menu(
                &snapshot.services,
                &files,
                &selected,
                info,
                urls,
                open,
                share,
                rearrange,
            );
            let slots = thumbnail_menu::Slots::new(&entries);
            let mut actions = Vec::new();
            let window_menu = thumbnail_menu_rows(&slots, &mut actions);
            *menu_state.borrow_mut() = (actions, files, url_facts);
            if let Some(window) = weak.upgrade() {
                window.set_thumbnail_menu(window_menu);
            }
        }
    });
    window.on_menu_chosen({
        let page = page.clone();
        let weak = window.as_weak();
        let change_pages = change_pages.clone();
        let shown = shown.clone();
        move |id| {
            use thumbnail_menu::Action;
            let (Some(window), Ok(id)) = (weak.upgrade(), usize::try_from(id)) else {
                return;
            };
            let Some((action, label)) = menu_state.borrow().0.get(id).cloned() else {
                return;
            };
            let page = page();
            let snapshot = page.borrow().store().snapshot();
            let Ok(roles) = hydrus_store::content::DomainRoles::new(&snapshot.services) else {
                return;
            };
            let files_of = |filter: thumbnail_menu::Filter| {
                let state = menu_state.borrow();
                let selected: std::collections::HashSet<HashId> =
                    page.borrow().selected_files().into_iter().collect();
                thumbnail_menu::matching(filter, &state.1, &selected, &roles)
            };
            let selected_in = |domain: hydrus_core::ServiceId| {
                let state = menu_state.borrow();
                let selected: std::collections::HashSet<HashId> =
                    page.borrow().selected_files().into_iter().collect();
                state
                    .1
                    .iter()
                    .filter(|f| selected.contains(&f.file) && f.current.contains(&domain))
                    .map(|f| f.file)
                    .collect::<Vec<_>>()
            };
            let delete = |files: Vec<HashId>, deletion: media_actions::Deletion| {
                if !files.is_empty() {
                    let location = page.borrow().location().clone();
                    ask(Asked::Delete(files, deletion, location));
                }
            };
            match action {
                Action::Refresh => {
                    page.borrow_mut().refresh();
                    shown(true);
                }
                Action::Rearrange(to) => {
                    page.borrow_mut().rearrange(to);
                    shown(true);
                }
                // (the viewer's menu's own, which the thumbnails' hasn't)
                Action::Viewer(_) => {}
                Action::Select(filter) => {
                    let files = files_of(filter);
                    reselect(&|page| {
                        page.select_files(&files);
                        None
                    });
                }
                Action::Remove(filter) => {
                    let files = files_of(filter);
                    page.borrow_mut().remove_files(&files);
                    shown(true);
                }
                Action::ArchiveDeleteFilter => window.invoke_archive_delete_filter(),
                Action::Archive => window.invoke_archive_selected(),
                Action::Inbox => window.invoke_inbox_selected(),
                Action::DeleteFrom(domain) => {
                    let name = snapshot
                        .services
                        .get(domain)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    delete(
                        selected_in(domain),
                        media_actions::Deletion::FromDomain { domain, name },
                    );
                }
                Action::DeleteTrashPhysically => {
                    delete(
                        selected_in(roles.trash),
                        media_actions::Deletion::Physically,
                    );
                }
                Action::DeletePhysically => delete(
                    page.borrow().selected_files(),
                    media_actions::Deletion::Physically,
                ),
                Action::Undelete => window.invoke_undelete_selected(),
                Action::ManageTags => window.invoke_manage_tags_selected(),
                _ => {
                    let page = page.borrow();
                    let menu = MenuTarget {
                        store: page.store(),
                        location: page.location(),
                        selected: page.selected_files(),
                        focused: page.focused().map(|i| page.results()[i]),
                        sort: Some(page.sort().clone()),
                        collect: Some(page.collect().clone()),
                    };
                    let state = menu_state.borrow();
                    shared_menu_action(action, &label, &menu, &state.2, &change_pages, &|urls| {
                        ask(Asked::OpenUrls(urls));
                    });
                }
            }
        }
    });
    // what the Client API asked of the pages, done, and the pages and media
    // viewer as they are, kept in the store for it
    let sync: Rc<dyn Fn()> = Rc::new({
        let pages = pages.clone();
        let current = current.clone();
        let shown = shown.clone();
        let viewing = viewing.clone();
        let viewers_kept: Rc<RefCell<Option<Vec<sessions::MediaViewer>>>> = Rc::default();
        let labels_shown: Rc<RefCell<Vec<Vec<String>>>> = Rc::default();
        let weak = window.as_weak();
        move || {
            let store = pages.borrow().store().clone();
            let asked = store
                .write(|ctx| sessions::take_commands(ctx.conn()))
                .unwrap_or_else(|e| {
                    eprintln!("could not read what the Client API asked: {e}");
                    Vec::new()
                });
            for (key, command) in asked {
                let changed = |page: &Rc<RefCell<SearchPage>>| {
                    if Rc::ptr_eq(page, &current.borrow()) {
                        shown(true);
                    }
                };
                match command {
                    sessions::PageCommand::Focus => change_pages(&|pages| {
                        pages.show(&key);
                        Ok(())
                    }),
                    sessions::PageCommand::AddFiles(files) => {
                        let page = pages.borrow_mut().page(&key);
                        if let Some(page) = page
                            && page.borrow_mut().add_files(&files)
                        {
                            changed(&page);
                        }
                    }
                    // (a notebook's pages, all of them for the top one)
                    sessions::PageCommand::Refresh => {
                        let keys = pages.borrow().pages_under(&key);
                        for key in keys {
                            let page = pages.borrow_mut().page(&key);
                            if let Some(page) = page {
                                page.borrow_mut().refresh();
                                changed(&page);
                            }
                        }
                    }
                }
            }
            // downloader pages' importers, as the daemon works them: the
            // page shown again when files came, else just its importer
            let open = pages.borrow().open_pages();
            for page in open {
                {
                    let page = page.borrow();
                    if page.importer().is_none()
                        && page.gallery().is_none()
                        && page.watchers().is_none()
                    {
                        continue;
                    }
                }
                let refreshed = page.borrow_mut().refresh_import();
                if !Rc::ptr_eq(&page, &current.borrow()) {
                    continue;
                }
                match refreshed {
                    page::ImportRefresh::Files => shown(true),
                    page::ImportRefresh::Status => {
                        if let Some(window) = weak.upgrade() {
                            let page = page.borrow();
                            if let Some(importer) = page.importer() {
                                show_importer(&window, importer);
                            }
                            show_gallery(&window, &page);
                            show_watchers(&window, &page);
                        }
                    }
                    page::ImportRefresh::Nothing => {}
                }
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
            if let Err(e) = pages.borrow_mut().sync(now) {
                eprintln!("could not keep the pages: {e}");
            }
            // the tabs' names, as their files and progress change
            let labels = pages.borrow().tab_labels();
            if *labels_shown.borrow() != labels {
                if let Some(window) = weak.upgrade() {
                    show_tabs(&window, &pages.borrow());
                }
                *labels_shown.borrow_mut() = labels;
            }
            let viewers: Vec<sessions::MediaViewer> = viewing
                .borrow()
                .iter()
                .map(|&(canvas_key, file)| sessions::MediaViewer {
                    canvas_key,
                    // (CANVAS_MEDIA_VIEWER)
                    canvas_type: 0,
                    file,
                })
                .collect();
            if viewers_kept.borrow().as_ref() != Some(&viewers) {
                let written = viewers.clone();
                match store.write(move |ctx| sessions::set_media_viewers(ctx.conn(), &written)) {
                    Ok(()) => *viewers_kept.borrow_mut() = Some(viewers),
                    Err(e) => eprintln!("could not keep the media viewers: {e}"),
                }
            }
        }
    });
    Bound {
        pages,
        current,
        rows,
        viewer,
        manage_tags,
        options,
        archive_delete,
        filter,
        open_page,
        review_imports,
        drop_files: review_files,
        sync,
        _thumbnails: thumbnails,
        _menu_titles: menu_titles,
        _popups: popup_timer,
    }
}

/// The files a menu acts on: the selection (in order) and the focused
/// file, where they are, and (for a page) how it sorts and collects.
struct MenuTarget<'a> {
    store: &'a hydrus_store::Store,
    location: &'a hydrus_search::LocationContext,
    selected: Vec<HashId>,
    focused: Option<HashId>,
    sort: Option<hydrus_core::pages::PageSort>,
    collect: Option<hydrus_core::pages::PageCollect>,
}

/// Makes changes to the pages, saying why one can't be made.
type ChangePages = Rc<dyn Fn(&dyn Fn(&mut Pages) -> Result<(), String>)>;

/// What runs after the pages change, once there is something to.
type AfterChange = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// The viewer's menu shown: each entry's action and label by id, the
/// file's URLs it was built from, and the file (which its entries act on,
/// though a slideshow has moved on since).
type ViewerMenuState = Rc<
    RefCell<(
        Vec<(thumbnail_menu::Action, String)>,
        thumbnail_menu::UrlFacts,
        Option<HashId>,
    )>,
>;

/// What the thumbnails' and the viewer's menus both do: copying,
/// opening externally, the urls submenu's, and opening new pages
/// (`ask_urls` asks before opening several URLs).
#[allow(clippy::type_complexity)]
fn shared_menu_action(
    action: thumbnail_menu::Action,
    label: &str,
    target: &MenuTarget<'_>,
    url_facts: &thumbnail_menu::UrlFacts,
    change_pages: &dyn Fn(&dyn Fn(&mut Pages) -> Result<(), String>),
    ask_urls: &dyn Fn(Vec<String>),
) {
    use thumbnail_menu::Action;
    let store = target.store;
    let focused: Vec<HashId> = target.focused.into_iter().collect();
    match action {
        Action::Copy => copy_to_clipboard(label),
        Action::OpenExternally | Action::OpenInWebBrowser | Action::OpenInFileBrowser => {
            if let Some(path) = thumbnail_menu::paths(store, &focused).pop() {
                match action {
                    Action::OpenExternally => launch(&path),
                    Action::OpenInWebBrowser => launch(&file_url(&path)),
                    _ => show_in_file_browser(&path),
                }
            }
        }
        Action::CopyFiles => copy_files(store, &target.selected),
        Action::CopyFile => copy_files(store, &focused),
        Action::CopyPaths
        | Action::CopyHashes(_)
        | Action::CopyFileIds
        | Action::CopyPath
        | Action::CopyHash(_)
        | Action::CopyFileId => {
            let files = match action {
                Action::CopyPath | Action::CopyHash(_) | Action::CopyFileId => &focused,
                _ => &target.selected,
            };
            let lines = match action {
                Action::CopyPaths | Action::CopyPath => thumbnail_menu::paths(store, files),
                Action::CopyHashes(kind) | Action::CopyHash(kind) => {
                    thumbnail_menu::hashes(store, files, kind)
                }
                _ => files.iter().map(|f| f.get().to_string()).collect(),
            };
            if !lines.is_empty() {
                copy_to_clipboard(&lines.join("\n"));
            }
        }
        Action::OpenInNewPage => {
            let (location, files) = (target.location.clone(), target.selected.clone());
            let (sort, collect) = (target.sort.clone(), target.collect.clone());
            change_pages(&|pages| {
                pages.open_files(
                    location.clone(),
                    files.clone(),
                    sort.as_ref(),
                    collect.as_ref(),
                );
                Ok(())
            });
        }
        Action::OpenUrls(which) | Action::CopyUrls(which) => {
            let mut urls = thumbnail_menu::urls_for(store, url_facts, which, &target.selected);
            if matches!(action, Action::CopyUrls(_)) {
                if !urls.is_empty() {
                    copy_to_clipboard(&urls.join("\n"));
                }
            } else {
                // (sorted, asking first for more than one, as `OpenURLs`
                // does)
                urls.sort();
                match urls.len() {
                    0 => {}
                    1 => launch(&urls[0]),
                    _ => ask_urls(urls),
                }
            }
        }
        Action::UrlPage(which) => {
            let search = thumbnail_menu::url_search(url_facts, which);
            let all_my_files =
                hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
                ));
            if !search.is_empty() {
                change_pages(&|pages| {
                    pages.open_search(all_my_files.clone(), search.clone(), "url search");
                    Ok(())
                });
            }
        }
        Action::OpenInDuplicateFilterPage => {
            let settings: hydrus_store::settings::PageSettings =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            let location = if settings.duplicate_filter_uses_all_my_files {
                hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
                ))
            } else {
                target.location.clone()
            };
            let files = target.selected.clone();
            change_pages(&|pages| {
                pages.open_duplicates(location.clone(), &files);
                Ok(())
            });
        }
        Action::OpenSimilar(distance) => {
            let location = target.location.clone();
            if let Some(search) = thumbnail_menu::similar_search(store, &target.selected, distance)
            {
                change_pages(&|pages| {
                    pages.open_search(location.clone(), vec![search.clone()], "files");
                    Ok(())
                });
            }
        }
        // (the rest are each menu's own)
        _ => {}
    }
}

/// Called with files a viewer deleted out of the page's domains.
pub(crate) type Removed = Rc<dyn Fn(&[HashId])>;

/// The window's menu template filled from `slots`, each entry's action
/// put in `actions` at its id.
fn thumbnail_menu_rows(
    slots: &thumbnail_menu::Slots,
    actions: &mut Vec<(thumbnail_menu::Action, String)>,
) -> ThumbnailMenu {
    use thumbnail_menu::Action;
    let actions = std::cell::RefCell::new(actions);
    let id = |action: Action, label: &str| {
        // (the volume, shown, does nothing: shown disabled)
        if action == Action::Viewer(viewer_menu::ViewerAction::Volume) {
            return -1;
        }
        let mut actions = actions.borrow_mut();
        actions.push((action, label.to_owned()));
        i32::try_from(actions.len() - 1).unwrap_or(-1)
    };
    let rows = |items: &[thumbnail_menu::SlotItem]| -> ModelRc<MenuRow> {
        let rows: Vec<MenuRow> = items
            .iter()
            .map(|(label, action)| MenuRow {
                label: label.as_str().into(),
                id: id(*action, label),
                ..MenuRow::default()
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    };
    // (labels copy themselves)
    let labels = |lines: &[String]| -> ModelRc<MenuRow> {
        let rows: Vec<MenuRow> = lines
            .iter()
            .map(|label| MenuRow {
                label: label.as_str().into(),
                id: id(Action::Copy, label),
                ..MenuRow::default()
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    };
    let info = slots.info.clone().unwrap_or_default();
    let (info_sub_title, info_sub) = info.sub.clone().unwrap_or_default();
    let (views_sub_title, views_sub) = info.views_sub.clone().unwrap_or_default();
    let groups = |groups: &[Vec<thumbnail_menu::SlotItem>]| {
        let group = |i: usize| groups.get(i).map_or(&[][..], Vec::as_slice);
        MenuGroups {
            g1: rows(group(0)),
            g2: rows(group(1)),
            g3: rows(group(2)),
            g4: rows(group(3)),
            g5: rows(group(4)),
            g6: rows(group(5)),
        }
    };
    // (with their checks)
    let check_groups = |groups: &[Vec<thumbnail_menu::CheckSlot>]| {
        let group = |i: usize| -> ModelRc<MenuRow> {
            let rows: Vec<MenuRow> = groups
                .get(i)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(|(label, action, checked)| MenuRow {
                    label: label.as_str().into(),
                    id: id(*action, label),
                    checkable: checked.is_some(),
                    checked: checked.unwrap_or(false),
                })
                .collect();
            ModelRc::new(VecModel::from(rows))
        };
        MenuGroups {
            g1: group(0),
            g2: group(1),
            g3: group(2),
            g4: group(3),
            g5: group(4),
            g6: group(5),
        }
    };
    let (slideshow_title, slideshow_groups) = slots.slideshow.clone().unwrap_or_default();
    let select = groups(&slots.select);
    let remove = groups(&slots.remove);
    let (delete_title, delete_menu) = match &slots.delete_menu {
        Some((title, items)) => (title.as_str().into(), rows(items)),
        None => (SharedString::new(), rows(&[])),
    };
    let urls = slots.urls.clone().unwrap_or_default();
    let open = slots.open.clone().unwrap_or_default();
    let (open_similar_title, open_similar) = open.similar.clone().unwrap_or_default();
    let share = slots.share.clone().unwrap_or_default();
    let (share_hashes_title, share_hashes) = share.hashes.clone().unwrap_or_default();
    let (share_hash_title, share_hash) = share.hash.clone().unwrap_or_default();
    ThumbnailMenu {
        has_share: slots.share.is_some(),
        share_a: rows(&share.a),
        share_hashes_title: share_hashes_title.into(),
        share_hashes: rows(&share_hashes),
        share_b: rows(&share.b),
        share_c: rows(&share.c),
        share_hash_title: share_hash_title.into(),
        share_hash: rows(&share_hash),
        share_d: rows(&share.d),
        info_title_id: if info.is_menu || info.title.is_empty() {
            -1
        } else {
            id(Action::Copy, &info.title)
        },
        info_title: info.title.as_str().into(),
        info_is_menu: info.is_menu,
        info_before: labels(&info.before),
        info_sub_title: info_sub_title.into(),
        info_sub: labels(&info_sub),
        info_after: labels(&info.after),
        views: labels(&info.views),
        views_sub_title: views_sub_title.into(),
        views_sub: labels(&views_sub),
        head: rows(&slots.head),
        has_select: !slots.select.is_empty(),
        select,
        has_remove: !slots.remove.is_empty(),
        remove,
        has_rearrange: !slots.rearrange.is_empty(),
        rearrange: rows(&slots.rearrange),
        filter: rows(&slots.filter),
        delete: rows(&slots.delete),
        delete_title,
        delete_menu,
        trash: rows(&slots.trash),
        manage: rows(&slots.manage),
        has_urls: slots.urls.is_some(),
        urls_visit: groups(&urls.visit),
        has_url_pages: urls.pages.is_some(),
        urls_pages: groups(urls.pages.as_deref().unwrap_or_default()),
        urls_copy: groups(&urls.copy),
        has_open: slots.open.is_some(),
        open_a: rows(&open.a),
        open_similar_title: open_similar_title.into(),
        open_similar: rows(&open_similar),
        open_b: rows(&open.b),
        zoom_title: slots
            .zoom
            .as_ref()
            .map_or_else(SharedString::new, |(title, _)| title.as_str().into()),
        zoom: rows(slots.zoom.as_ref().map_or(&[][..], |(_, items)| items)),
        slideshow_title: slideshow_title.into(),
        slideshow: check_groups(&slideshow_groups),
        has_volume: !slots.volume.is_empty(),
        volume: groups(&slots.volume),
        dismiss: rows(&slots.dismiss),
        player_title: slots
            .player
            .as_ref()
            .map_or_else(SharedString::new, |(title, _)| title.as_str().into()),
        player: labels(slots.player.as_ref().map_or(&[][..], |(_, lines)| lines)),
    }
}

/// The right-click menu shown: each entry's action and label by id, and
/// the page's files' facts it was built from.
type MenuState = Rc<
    RefCell<(
        Vec<(thumbnail_menu::Action, String)>,
        Vec<thumbnail_menu::FileFacts>,
        thumbnail_menu::UrlFacts,
    )>,
>;

/// Something that opens a file or URL.
type Launcher = Rc<dyn Fn(&str)>;

thread_local! {
    /// What opens files and URLs in place of the OS, if anything.
    static LAUNCHER: RefCell<Option<Launcher>> = RefCell::new(None);
}

/// Open files and URLs with `launcher` rather than as the OS opens them
/// (for tests, which shouldn't open anything), on this thread.
pub fn set_launcher(launcher: impl Fn(&str) + 'static) {
    LAUNCHER.with(|l| *l.borrow_mut() = Some(Rc::new(launcher)));
}

/// Open `target` (a path or URL) as the OS opens it.
fn launch(target: &str) {
    use std::process::Command;
    if let Some(launcher) = LAUNCHER.with(|l| l.borrow().clone()) {
        launcher(target);
        return;
    }
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(target);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };
    if let Err(e) = command.spawn() {
        eprintln!("could not open {target}: {e}");
    }
}

thread_local! {
    /// What shows files in a file browser in place of the OS, if anything.
    static FILE_BROWSER: RefCell<Option<Launcher>> = RefCell::new(None);
}

/// Show files with `browser` rather than in the OS's file browser (for
/// tests, which shouldn't open anything), on this thread.
pub fn set_file_browser(browser: impl Fn(&str) + 'static) {
    FILE_BROWSER.with(|b| *b.borrow_mut() = Some(Rc::new(browser)));
}

/// Show the file at `path`, selected, in the OS's file browser, as the
/// reference's `OpenFileLocation` does: Explorer's `/select,` and
/// Finder's `open -R`; elsewhere the file manager's own
/// org.freedesktop.FileManager1 ShowItems, as the show-in-file-manager
/// package the reference uses there does, else its folder opened.
fn show_in_file_browser(path: &str) {
    use std::process::Command;
    if let Some(browser) = FILE_BROWSER.with(|b| b.borrow().clone()) {
        browser(path);
        return;
    }
    #[cfg(windows)]
    let shown = Command::new("explorer").arg("/select,").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let shown = Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let shown = {
        let (url, path) = (file_url(path), path.to_owned());
        // (dbus-send waits for the reply, so off this thread)
        std::thread::Builder::new()
            .name("file browser".into())
            .spawn(move || {
                let shown = Command::new("dbus-send")
                    .args([
                        "--session",
                        "--print-reply",
                        "--dest=org.freedesktop.FileManager1",
                        "--type=method_call",
                        "/org/freedesktop/FileManager1",
                        "org.freedesktop.FileManager1.ShowItems",
                    ])
                    .arg(format!("array:string:{url}"))
                    .arg("string:")
                    .output()
                    .is_ok_and(|output| output.status.success());
                if !shown && let Some(folder) = std::path::Path::new(&path).parent() {
                    launch(&folder.display().to_string());
                }
            })
            .map(|_| ())
    };
    if let Err(e) = shown {
        eprintln!("could not show {path} in the file browser: {e}");
    }
}

/// A `file://` URL for `path`.
fn file_url(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut url = String::from(if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            url.push(char::from(b));
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

/// Put `text` on the clipboard (as the reference's menu labels do when
/// chosen).
fn copy_to_clipboard(text: &str) {
    to_clipboard(&Clip::Text(text.to_owned()));
}

/// What goes on the clipboard: text, or files (as the reference's
/// `ToClipboard` puts them, as file URLs a file manager pastes as files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clip {
    Text(String),
    Files(Vec<std::path::PathBuf>),
}

/// Something that takes what is copied.
type Clipper = Rc<dyn Fn(&Clip)>;

thread_local! {
    /// What takes copies in place of the clipboard, if anything.
    static CLIPPER: RefCell<Option<Clipper>> = RefCell::new(None);
}

/// Give what is copied to `clipper` rather than the clipboard (for tests,
/// which shouldn't touch it), on this thread.
pub fn set_clipper(clipper: impl Fn(&Clip) + 'static) {
    CLIPPER.with(|c| *c.borrow_mut() = Some(Rc::new(clipper)));
}

fn to_clipboard(clip: &Clip) {
    if let Some(clipper) = CLIPPER.with(|c| c.borrow().clone()) {
        clipper(clip);
        return;
    }
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(e) => {
            eprintln!("could not open the clipboard: {e}");
            return;
        }
    };
    let done = match clip {
        Clip::Text(text) => clipboard.set_text(text.as_str()),
        Clip::Files(paths) => clipboard.set().file_list(paths),
    };
    if let Err(e) = done {
        eprintln!("could not copy to the clipboard: {e}");
    }
}

/// Copy `files`, those that are local, as files (`CopyFilesToClipboard`).
fn copy_files(store: &hydrus_store::Store, files: &[HashId]) {
    let paths: Vec<std::path::PathBuf> = thumbnail_menu::paths(store, files)
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect();
    if !paths.is_empty() {
        to_clipboard(&Clip::Files(paths));
    }
}

/// What the main window asks before doing it to the selected files.
enum Asked {
    Archive(Vec<HashId>),
    Inbox(Vec<HashId>),
    /// Deleting, as the page's file domains have it.
    Delete(
        Vec<HashId>,
        media_actions::Deletion,
        hydrus_search::LocationContext,
    ),
    /// Locking the page's search to its files, asking this.
    LockSearch(&'static str),
    /// Opening these URLs in the web browser.
    OpenUrls(Vec<String>),
    /// Closing the page at this depth and index, asking this.
    ClosePage(usize, usize, String),
    /// Removing a gallery page's search, asking this.
    RemoveQuery(i64, String),
    /// Asking this, then doing that (the menu bar's questions).
    Then(String, Rc<dyn Fn()>),
}

impl Asked {
    fn archive_or_inbox(archive: bool, files: Vec<HashId>) -> Self {
        if archive {
            Self::Archive(files)
        } else {
            Self::Inbox(files)
        }
    }

    /// The question, as the reference asks it.
    fn question(&self) -> String {
        let count = |files: &[HashId]| hydrus_core::numbers::human_int(files.len() as u64);
        match self {
            Self::Archive(files) => format!("Archive {} files?", count(files)),
            Self::Inbox(files) => format!("Send {} files to inbox?", count(files)),
            Self::Delete(files, deletion, _) => deletion.question(files.len()),
            Self::LockSearch(question) => (*question).to_owned(),
            Self::ClosePage(_, _, question)
            | Self::RemoveQuery(_, question)
            | Self::Then(question, _) => question.clone(),
            Self::OpenUrls(urls) => {
                let mut question = format!("Open the {} URLs in your web browser?", urls.len());
                if urls.len() > 10 {
                    question.push_str(" This will take some time.");
                }
                question
            }
        }
    }

    /// Do it; files deleted from the page's domains leave it.
    fn act(&self, store: &hydrus_store::Store, removed: &dyn Fn(&[HashId])) {
        let done = match self {
            Self::Archive(files) => media_actions::archive(store, files),
            Self::Inbox(files) => media_actions::inbox(store, files),
            Self::Delete(files, deletion, location) => {
                media_actions::delete(store, files, deletion).map(|()| {
                    let still = media_actions::still_in(store, location, files);
                    let gone: Vec<HashId> = files
                        .iter()
                        .copied()
                        .filter(|f| !still.contains(f))
                        .collect();
                    if !gone.is_empty() {
                        removed(&gone);
                    }
                })
            }
            // (the page locks itself; the pages close it)
            Self::LockSearch(_) | Self::ClosePage(..) | Self::RemoveQuery(..) => Ok(()),
            Self::Then(_, then) => {
                then();
                Ok(())
            }
            Self::OpenUrls(urls) => {
                for url in urls {
                    launch(url);
                }
                Ok(())
            }
        };
        if let Err(e) = done {
            eprintln!("could not change the files: {e}");
        }
    }
}

/// Opens manage tags on files, calling the hook given once applied.
type OpenManageTags = Rc<dyn Fn(Arc<hydrus_store::Store>, Vec<HashId>, Rc<dyn Fn()>)>;

/// What a viewer tells its page of, and how it opens manage tags.
struct ViewerHooks {
    /// The viewer and the file it shows, for the Client API.
    viewing: Viewing,
    removed: Removed,
    tags_changed: Rc<dyn Fn()>,
    manage_tags: OpenManageTags,
    /// Opens pages (from the menu's open and urls entries).
    change_pages: ChangePages,
}

/// The media viewer while one is open, as the Client API's
/// `/manage_pages/get_media_viewers` lists it: its canvas key (random) and
/// the file it shows.
type Viewing = Rc<RefCell<Option<([u8; 32], Option<HashId>)>>>;

/// A change to files: archiving them, say.
type FileChange = fn(&hydrus_store::Store, &[HashId]) -> hydrus_store::Result<()>;

/// A change to a viewer's slideshow, made at a time (in seconds) with a
/// file shown.
type ChangeSlideshow<'a> = dyn Fn(&mut slideshow::Slideshow, f64, slideshow::Shown) + 'a;

/// What the viewer asks before doing it.
enum ViewerAsked {
    /// Deleting this file.
    Delete(media_actions::Deletion, HashId),
    /// Opening these URLs in the web browser.
    OpenUrls(Vec<String>),
}

/// Open a viewer window on `model`'s file; it forgets itself from `slot`
/// when closed.
fn open_viewer(
    model: MediaViewer,
    slot: &Rc<RefCell<Option<MediaViewerWindow>>>,
    hooks: ViewerHooks,
) -> Result<MediaViewerWindow, slint::PlatformError> {
    let ViewerHooks {
        viewing,
        removed,
        tags_changed,
        manage_tags,
        change_pages,
    } = hooks;
    let window = MediaViewerWindow::new()?;
    let model = Rc::new(RefCell::new(model));
    let playback = playback::Playback::new(model.borrow().store().dir().join("mpv.conf"));
    let animator = animation::Animator::new();
    // (where it opens, and how big: fullscreen, by hydrus's default)
    let settings_frame = windows::settings(model.borrow().store()).media_viewer;
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = zoom_window!(window, settings.clone());
    // the zoom, in the top hover frame
    zoomed.watch({
        let weak = window.as_weak();
        move |zoom| {
            if let Some(window) = weak.upgrade() {
                let text = zoom.map(viewer_menu::zoom_percentage).unwrap_or_default();
                window.set_zoom_text(text.into());
            }
        }
    });
    // the volume and mutes, as kept, on the window and the player; and
    // this viewer's own mute, if forced (`SetPerPlayerMuteState`, for as
    // long as the viewer is open)
    let forced_mute: Rc<std::cell::Cell<Option<bool>>> = Rc::default();
    let show_audio = {
        let weak = window.as_weak();
        let playback = playback.clone();
        let store = model.borrow().store().clone();
        let forced_mute = forced_mute.clone();
        move || {
            let audio = audio::settings(&store);
            let muted = forced_mute.get().unwrap_or_else(|| audio.viewer_muted());
            playback.set_audio(audio.current_viewer_volume(), muted);
            if let Some(window) = weak.upgrade() {
                window.set_global_muted(audio.global_mute);
                window.set_viewer_muted(audio.viewer_mute);
                window.set_volume(i32::from(audio.current_viewer_volume()));
            }
        }
    };
    show_audio();
    // the scanbar of the file playing, if it has one, and which player
    // plays it: mpv, or the client's own (`true`)
    let scanbar: Rc<std::cell::Cell<Option<(scanbar::Scanbar, bool)>>> = Rc::default();
    let show_scanbar = {
        let weak = window.as_weak();
        let scanbar = scanbar.clone();
        move |position_ms: f64| {
            if let (Some(window), Some((bar, _))) = (weak.upgrade(), scanbar.get()) {
                let (progress, text) = bar.at(position_ms);
                window.set_scanbar_progress(progress);
                window.set_scanbar_text(text.into());
            }
        }
    };
    let show_frame = {
        let weak = window.as_weak();
        let scanbar = scanbar.clone();
        move |index: usize, at_ms: u64| {
            if let (Some(window), Some((bar, _))) = (weak.upgrade(), scanbar.get()) {
                let (progress, text) = bar.at_frame(index, at_ms);
                window.set_scanbar_progress(progress);
                window.set_scanbar_text(text.into());
            }
        }
    };
    // the file's info line and buttons, in the top hover frame, and its
    // notes
    let show_info = {
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let shown = viewer::shown(model.store(), model.current());
            window.set_info_line(shown.line.into());
            window.set_file_inbox(shown.inbox);
            window.set_file_trashed(shown.trashed);
            window.set_file_local(shown.local);
            window.set_file_undeletable(shown.undeletable);
            let notes: Vec<NoteRow> = shown
                .notes
                .into_iter()
                .map(|(name, text)| NoteRow {
                    name: name.into(),
                    text: text.into(),
                })
                .collect();
            window.set_notes(ModelRc::new(VecModel::from(notes)));
        }
    };
    // the file's ratings, in the top-right hover frame
    window.set_rating_size(settings.rating_icon_size as f32);
    window.set_incdec_height(settings.rating_incdec_height as f32);
    window.set_rating_outline(ratings::outline_width(settings.rating_icon_size) as f32);
    let rating_controls: Rc<RefCell<Vec<ratings::Control>>> = Rc::default();
    let show_ratings = {
        let model = model.clone();
        let weak = window.as_weak();
        let rating_controls = rating_controls.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let controls = ratings::controls(model.store(), model.current());
            window.set_ratings(ModelRc::new(VecModel::from(
                controls.iter().map(rating_row).collect::<Vec<_>>(),
            )));
            *rating_controls.borrow_mut() = controls;
        }
    };
    // what the file shown is, for the slideshow's timing: one the viewer
    // plays (`CurrentlyPresentingMediaWithDuration`), with its duration
    let presenting = Rc::new(std::cell::Cell::new(slideshow::Shown::Still));
    let show = {
        let model = model.clone();
        let weak = window.as_weak();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        let presenting = presenting.clone();
        let viewing = viewing.clone();
        let scanbar = scanbar.clone();
        let show_scanbar = show_scanbar.clone();
        let show_ratings = show_ratings.clone();
        let show_info = show_info.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            if let Some((_, file)) = viewing.borrow_mut().as_mut() {
                *file = Some(model.current());
            }
            window.set_caption(model.caption().into());
            let tags: Vec<ListText> = model
                .tag_rows()
                .iter()
                .map(|(row, rgb)| list_text(row, *rgb))
                .collect();
            window.set_tags(ModelRc::new(VecModel::from(tags)));
            // (for a file that plays, its thumbnail until the first frame)
            let (shape, media) = (model.shape(), model.media().map(Arc::new));
            let (playable, animation) = (model.playable(), model.animation());
            window.set_media(media.as_deref().map(image).unwrap_or_default());
            let still = playable.is_none() && animation.is_none();
            zoomed.set_still(viewer::still_of(media, shape, still));
            zoomed.show(shape);
            let (size, frame) = (weak.clone(), weak.clone());
            let zoomed = zoomed.clone();
            playback.play(
                playable.as_deref(),
                move || {
                    // (rendered at the size shown)
                    zoomed.render_size().or_else(|| {
                        let size = size.upgrade()?.window().size();
                        Some((size.width, size.height))
                    })
                },
                move |image| {
                    if let Some(window) = frame.upgrade() {
                        window.set_media(image);
                    }
                },
            );
            let own = animation.as_ref().map(|f| (f.len(), f.total_ms()));
            let frame = weak.clone();
            animator.play(animation, move |image| {
                if let Some(window) = frame.upgrade() {
                    window.set_media(image);
                }
            });
            let (duration_ms, num_frames) = viewer::timing(model.store(), model.current());
            presenting.set(
                if (playable.is_some() && mpv::available()) || own.is_some() {
                    #[allow(clippy::cast_precision_loss)] // (milliseconds)
                    slideshow::Shown::Playing(duration_ms.map(|ms| ms as f64 / 1000.0))
                } else {
                    slideshow::Shown::Still
                },
            );
            let bar = match (playable.is_some(), own) {
                (true, _) => scanbar::Scanbar::new(duration_ms, num_frames).map(|b| (b, false)),
                (false, Some((frames, total_ms))) => scanbar::Scanbar::new(
                    duration_ms.or(Some(total_ms)),
                    num_frames.or(Some(frames as u64)),
                )
                .map(|b| (b, true)),
                (false, None) => None,
            };
            scanbar.set(bar);
            window.set_scanbar_shown(bar.is_some());
            window.set_volume_shown(playable.is_some() && model.has_audio());
            // (a control made afresh starts closed)
            window.set_volume_open(false);
            show_scanbar(0.0);
            drop(model);
            show_ratings();
            show_info();
        }
    };
    show();
    bind_zoom!(window, zoomed);
    window.on_audio_changed(show_audio.clone());
    window.on_flip_global_mute({
        let store = model.borrow().store().clone();
        let show_audio = show_audio.clone();
        move || {
            audio::flip_global_mute(&store);
            show_audio();
        }
    });
    window.on_flip_viewer_mute({
        let store = model.borrow().store().clone();
        let show_audio = show_audio.clone();
        move || {
            audio::change(&store, |a| a.viewer_mute = !a.viewer_mute);
            show_audio();
        }
    });
    window.on_volume_changed({
        let store = model.borrow().store().clone();
        let show_audio = show_audio.clone();
        move |volume| {
            let volume = u8::try_from(volume.clamp(0, 100)).unwrap_or(0);
            if audio::settings(&store).current_viewer_volume() != volume {
                audio::change(&store, |a| a.set_viewer_volume(volume));
                show_audio();
            }
        }
    });
    window.on_rating_clicked({
        let model = model.clone();
        move |row, left, proportion| {
            let control = usize::try_from(row)
                .ok()
                .and_then(|row| rating_controls.borrow().get(row).cloned());
            let Some(control) = control else { return };
            let (store, file) = {
                let model = model.borrow();
                (model.store().clone(), model.current())
            };
            let done = if left {
                ratings::left_click(&store, file, &control, f64::from(proportion))
            } else {
                ratings::right_click(&store, file, &control)
            };
            if let Err(e) = done {
                eprintln!("could not set the rating: {e}");
            }
            show_ratings();
        }
    });
    // the scanbar follows playing, and seeks
    let scanning = Rc::new(slint::Timer::default());
    scanning.start(slint::TimerMode::Repeated, Duration::from_millis(50), {
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        let show_scanbar = show_scanbar.clone();
        let show_frame = show_frame.clone();
        move || match scanbar.get() {
            Some((_, false)) => {
                if let Some(position) = playback.position_ms() {
                    show_scanbar(position);
                }
            }
            Some((_, true)) => {
                if let Some(status) = animator.status() {
                    show_frame(status.index, status.at_ms);
                }
            }
            None => {}
        }
    });
    window.on_scan({
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        let show_scanbar = show_scanbar.clone();
        let weak = window.as_weak();
        move |x, width| match scanbar.get() {
            Some((bar, false)) => {
                let to = bar.seek_to(x, width);
                playback.seek_ms(to);
                show_scanbar(to);
            }
            Some((bar, true)) => {
                // (the frame's text follows once it is shown)
                let index = bar.frame_at(x, width);
                animator.goto(index);
                if let Some(window) = weak.upgrade() {
                    window.set_scanbar_progress(bar.at_frame(index, 0).0);
                }
            }
            None => {}
        }
    });
    // as the reference: playing pauses while the scanbar is dragged
    let playing_before_scan = Rc::new(std::cell::Cell::new(false));
    window.on_scan_started({
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        let playing_before_scan = playing_before_scan.clone();
        move || {
            let playing = match scanbar.get() {
                Some((_, false)) => !playback.paused(),
                Some((_, true)) => animator.status().is_some_and(|s| !s.paused),
                None => false,
            };
            playing_before_scan.set(playing);
            if playing {
                playback.set_paused(true);
                animator.set_paused(true);
            }
        }
    });
    window.on_scan_ended({
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        move || {
            if playing_before_scan.replace(false) {
                match scanbar.get() {
                    Some((_, false)) => playback.set_paused(false),
                    Some((_, true)) => animator.set_paused(false),
                    None => {}
                }
            }
        }
    });
    window.on_seek_delta({
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        move |direction, step| {
            let step = u64::try_from(step).unwrap_or(0);
            match scanbar.get() {
                Some((bar, false)) => {
                    if let Some(position) = playback.position_ms() {
                        let to = bar.seek_delta(position, direction, step);
                        playback.seek_ms(to);
                        show_scanbar(to);
                    }
                }
                // (the frame's text follows once it is shown)
                Some((_, true)) => animator.seek_delta(direction, step),
                None => {}
            }
        }
    });
    // the slideshow (`CanvasMediaListBrowser`'s), timed in seconds from
    // the viewer opening: this viewer's shuffling and playing through are
    // the options' to begin with
    let opened = std::time::Instant::now();
    let now = move || opened.elapsed().as_secs_f64();
    let slideshow_settings = {
        let store = model.borrow().store().clone();
        move || slideshow::settings(&store)
    };
    let slideshow = Rc::new(std::cell::Cell::new(slideshow::Slideshow::new(
        &slideshow_settings(),
    )));
    // change it, telling the player whether to stop at the file's end
    let with_slideshow: Rc<dyn Fn(&ChangeSlideshow<'_>)> = Rc::new({
        let slideshow = slideshow.clone();
        let presenting = presenting.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        move |change| {
            let mut changed = slideshow.get();
            change(&mut changed, now(), presenting.get());
            slideshow.set(changed);
            playback.set_stop_at_end(changed.stops_player());
            animator.set_stop_at_end(changed.stops_player());
        }
    });
    // a file shown by the user starts the period again (`userChangedMedia`)
    let user_moved = {
        let slideshow = slideshow.clone();
        let with_slideshow = with_slideshow.clone();
        let slideshow_settings = slideshow_settings.clone();
        move || {
            if slideshow.get().running() {
                let settings = slideshow_settings();
                with_slideshow(&|s, now, shown| s.shown(now, shown, &settings));
            }
        }
    };
    let moving = slint::Timer::default();
    moving.start(slint::TimerMode::Repeated, Duration::from_millis(100), {
        let model = model.clone();
        let show = show.clone();
        let slideshow = slideshow.clone();
        let presenting = presenting.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        let with_slideshow = with_slideshow.clone();
        let slideshow_settings = slideshow_settings.clone();
        let weak = window.as_weak();
        move || {
            // (not while the viewer asks something, as the reference's
            // waits while a menu is open)
            let asking = weak.upgrade().is_some_and(|w| {
                !w.get_question().is_empty() || w.get_period_asked() || !w.get_warning().is_empty()
            });
            let current = slideshow.get();
            if asking && current.running() {
                return;
            }
            let played = playback.played_through() || animator.played_through();
            if !current.due(now(), presenting.get(), played) {
                return;
            }
            let moved = {
                let mut model = model.borrow_mut();
                let before = model.index();
                if current.shuffling() {
                    model.random();
                } else {
                    model.next();
                }
                model.index() != before
            };
            if moved {
                show();
            }
            let settings = slideshow_settings();
            with_slideshow(&|s, now, shown| s.shown(now, shown, &settings));
        }
    });
    let navigate = |go: fn(&mut viewer::MediaViewer)| {
        let model = model.clone();
        let show = show.clone();
        let user_moved = user_moved.clone();
        move || {
            go(&mut model.borrow_mut());
            show();
            user_moved();
        }
    };
    window.on_next(navigate(viewer::MediaViewer::next));
    window.on_previous(navigate(viewer::MediaViewer::previous));
    window.on_first(navigate(viewer::MediaViewer::first));
    window.on_last(navigate(viewer::MediaViewer::last));
    window.on_period_answered({
        let weak = window.as_weak();
        let with_slideshow = with_slideshow.clone();
        let slideshow_settings = slideshow_settings.clone();
        move |accepted, text| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_period_asked(false);
            window.invoke_refocus();
            if !accepted {
                return;
            }
            match slideshow::parse_period(&text) {
                Some(period) => {
                    let settings = slideshow_settings();
                    with_slideshow(&|s, now, shown| s.start(period, now, shown, &settings));
                }
                None => window.set_warning("Could not parse that slideshow period!".into()),
            }
        }
    });
    // a file leaves the viewer and its page: the next is shown if it was
    // shown; with none left, the viewer closes
    let remove_file: Rc<dyn Fn(HashId)> = Rc::new({
        let model = model.clone();
        let show = show.clone();
        let removed = removed.clone();
        let weak = window.as_weak();
        move |file| {
            removed(&[file]);
            let shown = model.borrow().current() == file;
            let any_left = model.borrow_mut().remove(file);
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !any_left {
                window.invoke_close_requested();
            } else if shown {
                show();
            } else {
                // (the one shown stays, its place in the list changed)
                window.set_caption(model.borrow().caption().into());
            }
        }
    });
    // ctrl+r: the file shown (`_Remove`)
    window.on_remove_from_view({
        let model = model.clone();
        let remove_file = remove_file.clone();
        move || {
            let file = model.borrow().current();
            remove_file(file);
        }
    });
    // ctrl+c: the file, as a file
    window.on_copy_file({
        let model = model.clone();
        move || {
            let model = model.borrow();
            copy_files(model.store(), &[model.current()]);
        }
    });
    // ctrl+e: the file as the OS opens it, pausing one that plays
    // (`_MediaFocusWentToExternalProgram`)
    window.on_open_externally({
        let model = model.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        move || {
            let (store, file) = {
                let model = model.borrow();
                (model.store().clone(), model.current())
            };
            let Some(path) = thumbnail_menu::paths(&store, &[file]).pop() else {
                return;
            };
            launch(&path);
            if viewer::timing(&store, file).0.is_some_and(|ms| ms > 0) {
                playback.set_paused(true);
                animator.set_paused(true);
            }
        }
    });
    // the top hover frame's stand-in for dragging the file out: it shown,
    // selected, in the OS's file browser
    window.on_show_in_file_browser({
        let model = model.clone();
        move || {
            let model = model.borrow();
            if let Some(path) = thumbnail_menu::paths(model.store(), &[model.current()]).pop() {
                show_in_file_browser(&path);
            }
        }
    });
    let pending: Rc<RefCell<Option<ViewerAsked>>> = Rc::default();
    let ask = {
        let pending = pending.clone();
        let weak = window.as_weak();
        move |asked: ViewerAsked| {
            if let Some(window) = weak.upgrade() {
                let question = match &asked {
                    ViewerAsked::Delete(deletion, _) => deletion.question(1),
                    ViewerAsked::OpenUrls(urls) => Asked::OpenUrls(urls.clone()).question(),
                };
                window.set_question(question.into());
                *pending.borrow_mut() = Some(asked);
            }
        }
    };
    // a change to a file (archiving, say), its info line shown again
    let change_file: Rc<dyn Fn(FileChange, HashId)> = Rc::new({
        let model = model.clone();
        let show_info = show_info.clone();
        move |change, file| {
            let changed = change(model.borrow().store(), &[file]);
            match changed {
                Ok(()) => show_info(),
                Err(e) => eprintln!("could not change the file: {e}"),
            }
        }
    });
    // manage a file's tags; once applied, the hover frame's and the page's
    // are shown again
    let manage_tags_of: Rc<dyn Fn(HashId)> = Rc::new({
        let model = model.clone();
        let show = show.clone();
        move |file| {
            let store = model.borrow().store().clone();
            let show = show.clone();
            let tags_changed = tags_changed.clone();
            manage_tags(
                store,
                vec![file],
                Rc::new(move || {
                    show();
                    tags_changed();
                }),
            );
        }
    });
    // the right-click menu (`ShowMenuFromSignal`): built for the file shown,
    // as the viewer is now, and its entries done
    let menu_state: ViewerMenuState = Rc::default();
    window.on_context_menu_requested({
        let model = model.clone();
        let zoomed = zoomed.clone();
        let forced_mute = forced_mute.clone();
        let slideshow = slideshow.clone();
        let menu_state = menu_state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let (store, file) = (model.store(), model.current());
            let state = viewer_menu::ViewerState {
                zoom: zoomed.state(),
                fullscreen: window.window().is_fullscreen(),
                audio: audio::settings(store),
                forced_mute: forced_mute.get(),
                player: viewer_menu::player(store, file),
                slideshow: slideshow.get(),
                slideshow_settings: slideshow::settings(store),
            };
            let info_settings: hydrus_core::media_viewer::InfoLineSettings =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            let entries = viewer_menu::viewer_menu(
                store,
                file,
                &state,
                &info_settings,
                hydrus_core::TimestampMs::now().0,
            );
            let slots = thumbnail_menu::Slots::new(&entries);
            let mut actions = Vec::new();
            let rows = thumbnail_menu_rows(&slots, &mut actions);
            *menu_state.borrow_mut() = (
                actions,
                thumbnail_menu::url_facts(store, Some(file), &[]),
                Some(file),
            );
            window.set_context_menu(rows);
        }
    });
    window.on_menu_chosen({
        let model = model.clone();
        let zoomed = zoomed.clone();
        let forced_mute = forced_mute.clone();
        let change_file = change_file.clone();
        let manage_tags_of = manage_tags_of.clone();
        let remove_file = remove_file.clone();
        let with_slideshow = with_slideshow.clone();
        let slideshow_settings = slideshow_settings.clone();
        let show_audio = show_audio.clone();
        let ask = ask.clone();
        let weak = window.as_weak();
        move |id| {
            use thumbnail_menu::Action;
            use viewer_menu::ViewerAction;
            let Some(window) = weak.upgrade() else {
                return;
            };
            let chosen = usize::try_from(id)
                .ok()
                .and_then(|i| menu_state.borrow().0.get(i).cloned());
            let Some((action, label)) = chosen else {
                return;
            };
            // (the file the menu was for, though a slideshow has moved on)
            let file = menu_state
                .borrow()
                .2
                .unwrap_or_else(|| model.borrow().current());
            match action {
                Action::Viewer(ViewerAction::ZoomIn) => zoomed.zoom(1, None),
                Action::Viewer(ViewerAction::ZoomOut) => zoomed.zoom(-1, None),
                Action::Viewer(ViewerAction::ZoomSwitch) => zoomed.zoom(0, None),
                Action::Viewer(ViewerAction::ZoomMax) => zoomed.zoom_max(),
                Action::Viewer(ViewerAction::Fullscreen) => window.invoke_toggle_fullscreen(),
                Action::Viewer(ViewerAction::MuteGlobal) => window.invoke_flip_global_mute(),
                Action::Viewer(ViewerAction::MuteViewer) => window.invoke_flip_viewer_mute(),
                Action::Viewer(ViewerAction::ForceMute(muted)) => {
                    forced_mute.set(muted);
                    show_audio();
                }
                Action::Viewer(ViewerAction::Volume) => {}
                Action::Viewer(ViewerAction::RemoveFromView) => remove_file(file),
                Action::Viewer(ViewerAction::PausePlaySlideshow) => {
                    let settings = slideshow_settings();
                    with_slideshow(&|s, now, shown| s.pause_play(now, shown, &settings));
                }
                Action::Viewer(ViewerAction::StartSlideshow(Some(viewer_menu::Seconds(
                    period,
                )))) => {
                    let settings = slideshow_settings();
                    with_slideshow(&|s, now, shown| s.start(period, now, shown, &settings));
                }
                Action::Viewer(ViewerAction::StartSlideshow(None)) => window.set_period_asked(true),
                Action::Viewer(ViewerAction::FlipShuffle) => {
                    with_slideshow(&|s, _, _| s.set_shuffling(!s.shuffling()));
                }
                Action::Viewer(ViewerAction::FlipOnceThrough) => {
                    with_slideshow(&|s, _, _| s.set_once_through(!s.once_through()));
                }
                Action::Viewer(ViewerAction::FlipGlobalShuffle) => {
                    let store = model.borrow().store().clone();
                    let shuffle = slideshow::change(&store, |o| o.shuffle = !o.shuffle).shuffle;
                    with_slideshow(&|s, _, _| s.set_shuffling(shuffle));
                }
                Action::Viewer(ViewerAction::FlipGlobalOnceThrough) => {
                    let store = model.borrow().store().clone();
                    let once = slideshow::change(&store, |o| o.once_through = !o.once_through)
                        .once_through;
                    with_slideshow(&|s, _, _| s.set_once_through(once));
                }
                Action::Archive => change_file(media_actions::archive, file),
                Action::Inbox => change_file(media_actions::inbox, file),
                Action::Undelete => change_file(media_actions::undelete, file),
                Action::ManageTags => manage_tags_of(file),
                Action::DeleteFrom(domain) => {
                    let name = model
                        .borrow()
                        .store()
                        .snapshot()
                        .services
                        .get(domain)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    ask(ViewerAsked::Delete(
                        media_actions::Deletion::FromDomain { domain, name },
                        file,
                    ));
                }
                Action::DeletePhysically => {
                    ask(ViewerAsked::Delete(
                        media_actions::Deletion::Physically,
                        file,
                    ));
                }
                _ => {
                    let model = model.borrow();
                    let target = MenuTarget {
                        store: model.store(),
                        location: model.location(),
                        selected: vec![file],
                        focused: Some(file),
                        sort: None,
                        collect: None,
                    };
                    let state = menu_state.borrow();
                    shared_menu_action(
                        action,
                        &label,
                        &target,
                        &state.1,
                        &*change_pages,
                        &|urls| {
                            ask(ViewerAsked::OpenUrls(urls));
                        },
                    );
                }
            }
        }
    });
    // ctrl+b and ctrl+n, for a file with a scanbar (`GotoPreviousOrNextFrame`)
    window.on_frame_step({
        let playback = playback.clone();
        let animator = animator.clone();
        let scanbar = scanbar.clone();
        move |direction| match scanbar.get() {
            Some((_, false)) => playback.frame_step(direction),
            Some((_, true)) => animator.step(direction),
            None => {}
        }
    });
    // F3: manage the file's tags; once applied, the hover frame's and the
    // page's are shown again
    window.on_manage_tags({
        let model = model.clone();
        move || {
            let file = model.borrow().current();
            manage_tags_of(file);
        }
    });
    // the media shortcuts: F7 and shift+F7, delete and shift+delete
    let act = |change: FileChange| {
        let model = model.clone();
        let change_file = change_file.clone();
        move || {
            let file = model.borrow().current();
            change_file(change, file);
        }
    };
    window.on_archive(act(media_actions::archive));
    window.on_inbox(act(media_actions::inbox));
    window.on_undelete(act(media_actions::undelete));
    window.on_delete({
        let model = model.clone();
        let ask = ask.clone();
        move || {
            let model = model.borrow();
            let deletion =
                media_actions::deletion(model.store(), model.location(), &[model.current()]);
            if let Some(deletion) = deletion {
                ask(ViewerAsked::Delete(deletion, model.current()));
            }
        }
    });
    window.on_answer({
        let model = model.clone();
        let show_info = show_info.clone();
        let weak = window.as_weak();
        move |yes| {
            let asked = pending.borrow_mut().take();
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_question(SharedString::new());
            let (deletion, file) = match asked.filter(|_| yes) {
                Some(ViewerAsked::Delete(deletion, file)) => (deletion, file),
                Some(ViewerAsked::OpenUrls(urls)) => {
                    let store = model.borrow().store().clone();
                    Asked::OpenUrls(urls).act(&store, &|_| {});
                    return;
                }
                None => return,
            };
            let (store, location) = {
                let model = model.borrow();
                (model.store().clone(), model.location().clone())
            };
            if let Err(e) = media_actions::delete(&store, &[file], &deletion) {
                eprintln!("could not delete the file: {e}");
                return;
            }
            // (out of the page's domains, it leaves the page and the viewer;
            // else, trashed say, its hover frame says so)
            if media_actions::still_in(&store, &location, &[file]).is_empty() {
                remove_file(file);
            } else {
                show_info();
            }
        }
    });
    window.on_toggle_pause({
        let playback = playback.clone();
        let animator = animator.clone();
        move || {
            playback.toggle_pause();
            animator.toggle_pause();
        }
    });
    // F, as the reference's default shortcut: between fullscreen and the
    // window it was (maximised, to begin with)
    let maximised_before = Rc::new(std::cell::Cell::new(true));
    window.on_toggle_fullscreen({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                windows::switch_fullscreen(window.window(), &maximised_before);
            }
        }
    });
    let store = model.borrow().store().clone();
    window.on_close_requested({
        let weak = window.as_weak();
        let slot = slot.clone();
        let viewing = viewing.clone();
        move || {
            viewing.borrow_mut().take();
            // (stops playing at once)
            scanning.stop();
            moving.stop();
            playback.close();
            animator.stop();
            if let Some(window) = weak.upgrade() {
                // its size and place, if hydrus's option says to keep them
                let mut frames = windows::settings(&store);
                if frames.save_media_viewer_on_close {
                    frames.media_viewer =
                        frames.media_viewer.saved(windows::state(window.window()));
                    windows::keep(&store, frames);
                }
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    // (closed by its frame's button, it closes as by escape)
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_close_requested();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    windows::place(window.window(), &settings_frame);
    window.show()?;
    Ok(window)
}

/// A rating control as the viewer draws it.
fn rating_row(control: &ratings::Control) -> RatingRow {
    let colour =
        |rgb: hydrus_store::services::Rgb| slint::Color::from_rgb_u8(rgb.0[0], rgb.0[1], rgb.0[2]);
    let shapes: Vec<RatingShape> = control
        .shapes()
        .into_iter()
        .map(|s| RatingShape {
            pen: colour(s.pen),
            brush: colour(s.brush),
        })
        .collect();
    let (kind, shape, pad, text) = match &control.kind {
        ratings::Kind::Like { shape, .. } => (0, *shape, 0.0, String::new()),
        ratings::Kind::Numerical { shape, config, .. } => {
            (1, *shape, config.custom_pad.max(0) as f32, String::new())
        }
        ratings::Kind::IncDec { value } => (
            2,
            "",
            0.0,
            hydrus_core::numbers::human_int(value.unsigned_abs()),
        ),
    };
    RatingRow {
        kind,
        shape: shape.into(),
        shapes: ModelRc::new(VecModel::from(shapes)),
        pad,
        text: text.into(),
        pen: colour(control.colours.like.pen),
        brush: colour(control.colours.like.brush),
    }
}

/// Show the tabs of each notebook on the way to the page shown.
fn show_tabs(window: &MainWindow, pages: &Pages) {
    let rows: Vec<TabRow> = pages
        .tabs()
        .into_iter()
        .zip(pages.tab_labels())
        .map(|(tabs, labels)| {
            let names: Vec<SharedString> = labels.iter().map(|n| n.as_str().into()).collect();
            TabRow {
                names: ModelRc::new(VecModel::from(names)),
                selected: i32::try_from(tabs.selected).unwrap_or(0),
            }
        })
        .collect();
    window.set_tab_rows(ModelRc::new(VecModel::from(rows)));
}

/// A tag or predicate list's row, in its namespace's colour.
pub(crate) fn list_text(text: &str, [r, g, b]: [u8; 3]) -> ListText {
    ListText {
        text: text.into(),
        colour: slint::Color::from_rgb_u8(r, g, b),
    }
}

/// Show the page's search: the box's text and suggestions, the predicates,
/// any error, and the status bar; or, for a page without a search, why.
/// A downloader page's importer: its logs' statuses and progress, its
/// pause, and its downloads.
fn show_importer(window: &MainWindow, importer: &page::Importer) {
    window.set_local_import(importer.local);
    window.set_import_action(importer.live.files_status.as_str().into());
    window.set_import_status(importer.files_status().into());
    window.set_import_progress(importer.progress_text().into());
    #[allow(clippy::cast_precision_loss)] // (a progress bar)
    let fraction = match importer.progress() {
        (_, 0) => 0.0,
        (done, total) => done as f32 / total as f32,
    };
    window.set_import_fraction(fraction);
    window.set_import_paused(importer.paused);
    window.set_search_status(importer.search_status().into());
    window.set_file_download(download_line(&importer.file_job_line()));
    window.set_search_download(download_line(&importer.gallery_job_line()));
}

/// A gallery page's sidebar: its searches' list, its totals, what its
/// buttons can do, its downloader and file limit, and the search it shows.
fn show_gallery(window: &MainWindow, page: &SearchPage) {
    use hydrus_store::queues::SeedStatus;
    let Some(gallery) = page.gallery() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let highlighted = gallery.state.highlighted;
    let rows: Vec<ModelRc<slint::StandardListViewItem>> = gallery
        .queries
        .iter()
        .map(|q| {
            let cells = q.row(
                highlighted == Some(q.queue),
                &gallery.settings,
                gallery.short_summary,
                now,
            );
            let cells: Vec<slint::StandardListViewItem> = cells
                .iter()
                .map(|c| slint::StandardListViewItem::from(c.as_str()))
                .collect();
            ModelRc::new(VecModel::from(cells))
        })
        .collect();
    window.set_gallery_rows(ModelRc::new(VecModel::from(rows)));
    let current = gallery
        .selected
        .and_then(|s| gallery.queries.iter().position(|q| q.queue == s));
    window.set_gallery_current_row(current.map_or(-1, |i| i32::try_from(i).unwrap_or(-1)));
    let (top_status, bottom_status) = gallery::totals(&gallery.queries);
    let selected = gallery.selected.and_then(|s| gallery.query(s));
    let has = |q: Option<&gallery::GalleryQuery>, status| {
        q.is_some_and(|q| q.files.get(&status).is_some_and(|&n| n > 0))
    };
    let own = gallery.gallery();
    // (the downloader among those offered; one not found, or none, after
    // them, as the reference's selector labels it)
    let found = gallery
        .gugs
        .iter()
        .position(|g| g.0 == own.gug_key)
        .or_else(|| gallery.gugs.iter().position(|g| g.1 == own.gug_name))
        .filter(|_| !own.gug_name.is_empty());
    let mut gug_names: Vec<SharedString> =
        gallery.gugs.iter().map(|g| g.1.as_str().into()).collect();
    let gug_index = found.unwrap_or_else(|| {
        gug_names.push(if own.gug_name.is_empty() {
            "no downloader set".into()
        } else {
            format!("not found: {}", own.gug_name).into()
        });
        gug_names.len() - 1
    });
    let shown = highlighted.and_then(|h| gallery.query(h));
    window.set_gallery_data(GalleryData {
        top_status: top_status.into(),
        bottom_status: bottom_status.into(),
        has_selection: selected.is_some(),
        can_highlight: selected.is_some_and(|q| Some(q.queue) != highlighted),
        can_clear_highlight: highlighted.is_some(),
        can_retry_ignored: has(selected, SeedStatus::Vetoed),
        can_retry_failed: has(selected, SeedStatus::Error),
        gug_names: ModelRc::new(VecModel::from(gug_names)),
        gug_index: i32::try_from(gug_index).unwrap_or(0),
        initial_search_text: found
            .map(|i| gallery.gugs[i].2.as_str())
            .unwrap_or_default()
            .into(),
        no_limit: own.file_limit.is_none(),
        file_limit: own
            .file_limit
            .map_or(2000, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        highlighted: shown.is_some(),
        highlighted_query: shown.map(|q| q.query.as_str()).unwrap_or_default().into(),
        files_line: shown
            .map(|q| gallery::live_line(&q.live.files_status, q.files_paused, q.working()))
            .unwrap_or_default()
            .into(),
        search_line: shown
            .map(|q| gallery::live_line(&q.live.gallery_status, q.gallery_paused, q.working()))
            .unwrap_or_default()
            .into(),
        search_paused: shown.is_some_and(|q| q.gallery_paused),
    });
}

fn show_watchers(window: &MainWindow, page: &SearchPage) {
    use hydrus_store::queues::SeedStatus;
    let Some(view) = page.watchers() else {
        return;
    };
    let now = page::now();
    let rows: Vec<ModelRc<slint::StandardListViewItem>> = view
        .rows(now)
        .iter()
        .map(|cells| {
            let cells: Vec<slint::StandardListViewItem> = cells
                .iter()
                .map(|c| slint::StandardListViewItem::from(c.as_str()))
                .collect();
            ModelRc::new(VecModel::from(cells))
        })
        .collect();
    window.set_watcher_rows(ModelRc::new(VecModel::from(rows)));
    let current = view
        .selected
        .and_then(|s| view.watchers.iter().position(|w| w.queue == s));
    window.set_watcher_current_row(current.map_or(-1, |i| i32::try_from(i).unwrap_or(-1)));
    let (top_status, bottom_status) = watcher::totals(&view.watchers);
    let highlighted = view.state.highlighted;
    let selected = view.selected.and_then(|s| view.watcher(s));
    let has = |w: Option<&watcher::WatcherRow>, status| {
        w.is_some_and(|w| w.files.get(&status).is_some_and(|&n| n > 0))
    };
    let shown = highlighted.and_then(|h| view.watcher(h));
    window.set_watcher_data(WatcherData {
        top_status: top_status.into(),
        bottom_status: bottom_status.into(),
        has_selection: selected.is_some(),
        can_highlight: selected.is_some_and(|w| Some(w.queue) != highlighted),
        can_clear_highlight: highlighted.is_some(),
        can_retry_ignored: has(selected, SeedStatus::Vetoed),
        can_retry_failed: has(selected, SeedStatus::Error),
        highlighted: shown.is_some(),
        // (`WatcherReviewPanel`: "no subject" for one with none yet)
        subject: shown
            .map(|w| match w.subject() {
                "" | "unknown subject" => "no subject",
                subject => subject,
            })
            .unwrap_or_default()
            .into(),
        url: shown
            .map(|w| w.state.url.as_str())
            .unwrap_or_default()
            .into(),
        files_line: shown.map(|w| w.files_line(now)).unwrap_or_default().into(),
        files_paused: shown.is_some_and(|w| w.files_paused),
        velocity_line: page.watcher_velocity().into(),
        checker_line: shown
            .map(|w| w.checker_line(now))
            .unwrap_or_default()
            .into(),
        checking_paused: shown.is_some_and(|w| w.state.checking_paused),
        can_pause_checking: shown.is_some_and(|w| !w.dead()),
        can_check_now: shown.is_some_and(|w| !w.state.check_now),
    });
}

/// A download's line for the window.
pub(crate) fn download_line(line: &hydrus_store::live::JobLine) -> DownloadLine {
    DownloadLine {
        left: line.left.as_str().into(),
        right: line.right.as_str().into(),
        fraction: line.fraction(),
        can_cancel: line.can_cancel,
    }
}

fn refresh(window: &MainWindow, page: &SearchPage, favourites: &ModelRc<FavouriteRow>) {
    window.set_note(page.note().unwrap_or_default().into());
    let importer = page.importer();
    window.set_importing(importer.is_some());
    if let Some(importer) = importer {
        show_importer(window, importer);
    }
    window.set_gallery_page(page.gallery().is_some());
    show_gallery(window, page);
    window.set_watcher_page(page.watchers().is_some());
    show_watchers(window, page);
    // (only a page with a search can load one)
    window.set_favourites(if page.note().is_none() {
        favourites.clone()
    } else {
        ModelRc::default()
    });
    window.set_can_filter(page.duplicates().is_some());
    window.set_can_lock_search(page.note().is_none());
    window.set_synchronised(page.synchronised());
    let lock = page.lock();
    window.set_search_locked(lock.is_some());
    if let Some(lock) = lock {
        window.set_lock_label(
            format!(
                "Locked at {} files.",
                hydrus_core::numbers::human_int(page.locked_count() as u64)
            )
            .into(),
        );
        window.set_lock_syncs_new(lock.syncs_new);
        window.set_lock_syncs_removes(lock.syncs_removes);
    }
    let colours: hydrus_core::tag_presentation::NamespaceColours = page
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let autocomplete = page.autocomplete();
    window.set_search_text(autocomplete.text().into());
    let suggestions: Vec<ListText> = autocomplete
        .suggestions()
        .iter()
        .map(|s| list_text(&s.label, colours.predicate_text(&s.predicate)))
        .collect();
    window.set_suggestions(ModelRc::new(VecModel::from(suggestions)));
    window.set_highlighted(
        autocomplete
            .highlighted()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
    );
    let predicates: Vec<ListText> = page
        .predicates()
        .iter()
        .zip(page.predicate_colours(&colours))
        .map(|(text, rgb)| list_text(text, rgb))
        .collect();
    window.set_predicates(ModelRc::new(VecModel::from(predicates)));
    let tags: Vec<ListText> = page
        .tag_rows()
        .into_iter()
        .zip(page.tag_colours(&colours))
        .map(|(text, rgb)| list_text(text, rgb))
        .collect();
    window.set_tags(ModelRc::new(VecModel::from(tags)));
    window.set_error(page.error().unwrap_or_default().into());
    window.set_status(page.status().into());
    let sort = page.sort();
    let choices = sort::page_choices(page.store(), &sort.by);
    let names: Vec<SharedString> = choices.iter().map(|c| c.name.as_str().into()).collect();
    window.set_sort_names(ModelRc::new(VecModel::from(names)));
    if let Some(i) = choices.iter().position(|c| c.by == sort.by) {
        window.set_sort_index(i32::try_from(i).unwrap_or(0));
        let orders: Vec<SharedString> = choices[i].orders.iter().map(|&o| o.into()).collect();
        window.set_order_names(ModelRc::new(VecModel::from(orders)));
        window.set_order_index(i32::from(!sort.ascending));
    }
    let collect = page.collect();
    let choices = collect::choices(page.store());
    window.set_collect_label(collect::label(&choices, collect).into());
    let rows: Vec<CollectRow> = choices
        .iter()
        .map(|c| CollectRow {
            name: c.name.as_str().into(),
            checked: c.checked(collect),
        })
        .collect();
    window.set_collect_choices(ModelRc::new(VecModel::from(rows)));
    window.set_collect_unmatched(collect.collect_unmatched);
    window.set_collect_separate(!collect.collect_unmatched);
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_urls_are_escaped() {
        assert_eq!(
            super::file_url("/home/a b/f\u{e9}.jpg"),
            "file:///home/a%20b/f%C3%A9.jpg"
        );
        assert_eq!(
            super::file_url("C:\\files\\f0\\x.png"),
            "file:///C:/files/f0/x.png"
        );
    }
}
