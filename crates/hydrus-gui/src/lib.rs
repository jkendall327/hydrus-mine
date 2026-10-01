//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

pub mod autocomplete;
pub mod duplicate_filter;
mod filter_window;
mod grid;
pub mod headless;
pub mod mpv;
mod page;
pub mod page_chooser;
mod pages;
mod playback;
pub mod sort;
mod thumbnails;
mod unlock;
mod viewer;

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
    /// The duplicate filter while one is open.
    pub filter: Rc<RefCell<Option<DuplicateFilterWindow>>>,
    /// Shows thumbnails as they are decoded (held to keep it running).
    _thumbnails: Rc<slint::Timer>,
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
    thumbnails::Pixels::new(raster).image()
}

/// Show `pages` in `window`, and let the window change them.
pub fn bind(window: &MainWindow, pages: Pages) -> Bound {
    let pages = Rc::new(RefCell::new(pages));
    let first = pages.borrow_mut().current();
    let current = Rc::new(RefCell::new(first.clone()));
    let rows = Rc::new(ThumbnailRows::new(first));
    window.set_thumbnail_rows(ModelRc::from(rows.clone()));
    rows.set_columns(usize::try_from(window.get_grid_columns()).unwrap_or(1));
    let thumbnails = Rc::new(slint::Timer::default());
    thumbnails.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(25),
        {
            let rows = rows.clone();
            move || {
                rows.receive();
            }
        },
    );
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
    // change the pages, then show whichever page is now shown; a change
    // that can't be made says why
    let change_pages = {
        let pages = pages.clone();
        let current = current.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        let shown = shown.clone();
        move |change: &dyn Fn(&mut Pages) -> Result<(), String>| {
            let (result, opened) = {
                let mut pages = pages.borrow_mut();
                let result = change(&mut pages);
                (result, pages.current())
            };
            *current.borrow_mut() = opened.clone();
            rows.set_page(opened);
            if let Some(window) = weak.upgrade() {
                show_tabs(&window, &pages.borrow());
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
            let store = pages.borrow().store().clone();
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
        let show_chooser = show_chooser.clone();
        move || {
            chooser.borrow_mut().take();
            show_chooser();
        }
    });
    window.on_close_page({
        let change_pages = change_pages.clone();
        move || change_pages(&Pages::close_shown)
    });
    window.on_close_tab({
        let change_pages = change_pages.clone();
        move |level, index| {
            let (Ok(level), Ok(index)) = (usize::try_from(level), usize::try_from(index)) else {
                return;
            };
            change_pages(&|pages| pages.close(level, index));
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
            shown(false);
        }
    });
    Bound {
        pages,
        current,
        rows,
        viewer,
        filter,
        _thumbnails: thumbnails,
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
    let playback = playback::Playback::new(model.borrow().store().dir().join("mpv.conf"));
    let show = {
        let model = model.clone();
        let weak = window.as_weak();
        let playback = playback.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            window.set_caption(model.caption().into());
            // (for a file that plays, its thumbnail until the first frame)
            window.set_media(model.media().as_ref().map(image).unwrap_or_default());
            let (size, frame) = (weak.clone(), weak.clone());
            playback.play(
                model.playable().as_deref(),
                move || {
                    let size = size.upgrade()?.window().size();
                    Some((size.width, size.height))
                },
                move |image| {
                    if let Some(window) = frame.upgrade() {
                        window.set_media(image);
                    }
                },
            );
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
    window.on_toggle_pause({
        let playback = playback.clone();
        move || playback.toggle_pause()
    });
    window.on_close_requested({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            // (stops playing at once)
            playback.close();
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
    window.set_can_filter(page.duplicates().is_some());
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
    let tags: Vec<SharedString> = page
        .tag_rows()
        .into_iter()
        .map(SharedString::from)
        .collect();
    window.set_tags(ModelRc::new(VecModel::from(tags)));
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
