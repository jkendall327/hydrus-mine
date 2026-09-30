//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, SharedPixelBuffer, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

pub mod autocomplete;
mod grid;
pub mod headless;
mod page;
mod pages;
pub mod sort;
mod viewer;

pub use grid::ThumbnailRows;
pub use page::SearchPage;
pub use pages::{Pages, Tabs};
pub use viewer::MediaViewer;

/// Pages bound to a window: the pages, the page shown, its grid's rows,
/// and the media viewer while one is open.
#[derive(Clone)]
pub struct Bound {
    pub pages: Rc<RefCell<Pages>>,
    pub current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
    pub rows: Rc<ThumbnailRows>,
    pub viewer: Rc<RefCell<Option<MediaViewerWindow>>>,
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

/// A decoded image as Slint shows it.
pub fn image(raster: &hydrus_media::Raster) -> slint::Image {
    let (width, height) = (raster.width(), raster.height());
    match raster.channels() {
        4 => slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(
            raster.data(),
            width,
            height,
        )),
        3 => slint::Image::from_rgb8(SharedPixelBuffer::clone_from_slice(
            raster.data(),
            width,
            height,
        )),
        _ => {
            let rgb: Vec<u8> = raster.data().iter().flat_map(|&v| [v, v, v]).collect();
            slint::Image::from_rgb8(SharedPixelBuffer::clone_from_slice(&rgb, width, height))
        }
    }
}

/// Show `pages` in `window`, and let the window change them.
pub fn bind(window: &MainWindow, pages: Pages) -> Bound {
    let pages = Rc::new(RefCell::new(pages));
    let first = pages.borrow_mut().current();
    let current = Rc::new(RefCell::new(first.clone()));
    let rows = Rc::new(ThumbnailRows::new(first));
    window.set_thumbnail_rows(ModelRc::from(rows.clone()));
    rows.set_columns(usize::try_from(window.get_grid_columns()).unwrap_or(1));
    show_tabs(window, &pages.borrow());
    refresh(window, &current.borrow().borrow());

    // after a change to the page shown, show it; `true` if its files changed
    let shown = {
        let current = current.clone();
        let weak = window.as_weak();
        let rows = rows.clone();
        move |files: bool| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &current.borrow().borrow());
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
    window.on_tab_chosen({
        let pages = pages.clone();
        let current = current.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        let shown = shown.clone();
        move |level, index| {
            let (Ok(level), Ok(index)) = (usize::try_from(level), usize::try_from(index)) else {
                return;
            };
            let opened = {
                let mut pages = pages.borrow_mut();
                pages.select(level, index);
                pages.current()
            };
            *current.borrow_mut() = opened.clone();
            rows.set_page(opened);
            if let Some(window) = weak.upgrade() {
                show_tabs(&window, &pages.borrow());
            }
            shown(false);
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
    let choices = sort::choices();
    let names: Vec<SharedString> = choices.iter().map(|c| c.name.as_str().into()).collect();
    window.set_sort_names(ModelRc::new(VecModel::from(names)));
    window.on_sort_chosen({
        let page = page.clone();
        let shown = shown.clone();
        let choices = choices.clone();
        move |index| {
            if let Some(choice) = usize::try_from(index).ok().and_then(|i| choices.get(i)) {
                page().borrow_mut().set_sort_by(choice.by);
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
            page().borrow_mut().set_sort_order(order);
            shown(true);
        }
    });
    window.on_columns_changed({
        let rows = rows.clone();
        move |columns| rows.set_columns(usize::try_from(columns).unwrap_or(1))
    });
    let viewer: Rc<RefCell<Option<MediaViewerWindow>>> = Rc::default();
    window.on_thumbnail_activated({
        let page = page.clone();
        let viewer = viewer.clone();
        move |index| {
            let page = page();
            let page = page.borrow();
            let Some(model) = MediaViewer::new(
                page.store().clone(),
                page.results().to_vec(),
                usize::try_from(index).unwrap_or(usize::MAX),
            ) else {
                return;
            };
            match open_viewer(model, &viewer) {
                Ok(window) => *viewer.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the media viewer: {e}"),
            }
        }
    });
    window.on_thumbnail_clicked({
        let rows = rows.clone();
        move |index| {
            let page = page();
            let before = page.borrow().selected();
            page.borrow_mut()
                .select(usize::try_from(index).unwrap_or(usize::MAX));
            let after = page.borrow().selected();
            for changed in [before, after].into_iter().flatten() {
                rows.file_changed(changed);
            }
        }
    });
    Bound {
        pages,
        current,
        rows,
        viewer,
    }
}

/// Open a viewer window on `model`'s file; it forgets itself from `slot`
/// when closed.
fn open_viewer(
    model: MediaViewer,
    slot: &Rc<RefCell<Option<MediaViewerWindow>>>,
) -> Result<MediaViewerWindow, slint::PlatformError> {
    let window = MediaViewerWindow::new()?;
    let model = Rc::new(RefCell::new(model));
    let show = {
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                let model = model.borrow();
                window.set_caption(model.caption().into());
                window.set_media(model.media().as_ref().map(image).unwrap_or_default());
            }
        }
    };
    show();
    window.on_next({
        let model = model.clone();
        let show = show.clone();
        move || {
            model.borrow_mut().next();
            show();
        }
    });
    window.on_previous(move || {
        model.borrow_mut().previous();
        show();
    });
    window.on_close_requested({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.show()?;
    Ok(window)
}

/// Show the tabs of each notebook on the way to the page shown.
fn show_tabs(window: &MainWindow, pages: &Pages) {
    let rows: Vec<TabRow> = pages
        .tabs()
        .into_iter()
        .map(|tabs| {
            let names: Vec<SharedString> = tabs.names.iter().map(|n| n.as_str().into()).collect();
            TabRow {
                names: ModelRc::new(VecModel::from(names)),
                selected: i32::try_from(tabs.selected).unwrap_or(0),
            }
        })
        .collect();
    window.set_tab_rows(ModelRc::new(VecModel::from(rows)));
}

/// Show the page's search: the box's text and suggestions, the predicates,
/// any error, and the status bar; or, for a page without a search, why.
fn refresh(window: &MainWindow, page: &SearchPage) {
    window.set_note(page.note().unwrap_or_default().into());
    let autocomplete = page.autocomplete();
    window.set_search_text(autocomplete.text().into());
    let suggestions: Vec<SharedString> = autocomplete
        .suggestions()
        .iter()
        .map(|s| s.label.as_str().into())
        .collect();
    window.set_suggestions(ModelRc::new(VecModel::from(suggestions)));
    window.set_highlighted(
        autocomplete
            .highlighted()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
    );
    let predicates: Vec<SharedString> = page
        .predicates()
        .into_iter()
        .map(SharedString::from)
        .collect();
    window.set_predicates(ModelRc::new(VecModel::from(predicates)));
    window.set_error(page.error().unwrap_or_default().into());
    window.set_status(page.status().into());
    let sort = page.sort();
    let choices = sort::choices();
    if let Some(i) = choices.iter().position(|c| c.by == sort.by) {
        window.set_sort_index(i32::try_from(i).unwrap_or(0));
        let orders: Vec<SharedString> = choices[i].orders.iter().map(|&o| o.into()).collect();
        window.set_order_names(ModelRc::new(VecModel::from(orders)));
        window.set_order_index(i32::from(
            sort.order == hydrus_search::SortOrder::Descending,
        ));
    }
}
