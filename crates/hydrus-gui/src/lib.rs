//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::HashId;
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
pub mod autocomplete;
pub mod duplicate_filter;
pub mod favourites;
mod filter_window;
mod grid;
pub mod headless;
pub mod info_lines;
pub mod manage_tags;
pub(crate) mod manage_tags_window;
pub mod media_actions;
pub mod mpv;
mod page;
pub mod page_chooser;
mod pages;
mod playback;
pub mod ratings;
pub mod scanbar;
pub mod selection;
pub mod sort;
pub mod status;
pub mod still;
pub mod thumbnail_menu;
mod thumbnails;
mod unlock;
mod viewer;
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
    /// The archive/delete filter while one is open.
    pub archive_delete: Rc<RefCell<Option<ArchiveDeleteWindow>>>,
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
    window.on_unclose_page({
        let change_pages = change_pages.clone();
        move || {
            change_pages(&|pages| {
                pages.unclose();
                Ok(())
            });
        }
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
        move |yes| {
            let asked = pending.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            if let Some(asked) = asked.filter(|_| yes) {
                let store = page().borrow().store().clone();
                asked.act(&store, &*removed);
                shown(false);
            }
        }
    });
    // F12: the archive/delete filter, on the files selected, else them all
    let archive_delete: Rc<RefCell<Option<ArchiveDeleteWindow>>> = Rc::default();
    window.on_archive_delete_filter({
        let page = page.clone();
        let archive_delete = archive_delete.clone();
        let removed = removed.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let files = match page.selected_files() {
                selected if selected.is_empty() => page.results().to_vec(),
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
            let model = model.with_location(page.location().clone());
            let hooks = ViewerHooks {
                removed: removed.clone(),
                tags_changed: tags_changed.clone(),
                manage_tags: Rc::new(open_manage_tags.clone()),
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
    // the right-click menu: built for the file clicked (selecting it, as
    // a click would), and its entries done
    let menu_state: Rc<RefCell<(Vec<thumbnail_menu::Action>, Vec<thumbnail_menu::FileFacts>)>> =
        Rc::default();
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
            let files = thumbnail_menu::facts(page.store(), page.results());
            let selected: std::collections::HashSet<HashId> =
                page.selected_files().into_iter().collect();
            let snapshot = page.store().snapshot();
            let entries = thumbnail_menu::menu(&snapshot.services, &files, &selected);
            let slots = thumbnail_menu::Slots::new(&entries);
            let mut actions = Vec::new();
            let window_menu = thumbnail_menu_rows(&slots, &mut actions);
            *menu_state.borrow_mut() = (actions, files);
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
            let Some(action) = menu_state.borrow().0.get(id).copied() else {
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
                Action::OpenInNewPage => {
                    let (location, files) = {
                        let page = page.borrow();
                        (page.location().clone(), page.selected_files())
                    };
                    change_pages(&|pages| {
                        pages.open_files(location.clone(), files.clone());
                        Ok(())
                    });
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
        archive_delete,
        filter,
        _thumbnails: thumbnails,
    }
}

/// Called with files a viewer deleted out of the page's domains.
pub(crate) type Removed = Rc<dyn Fn(&[HashId])>;

/// The window's menu template filled from `slots`, each entry's action
/// put in `actions` at its id.
fn thumbnail_menu_rows(
    slots: &thumbnail_menu::Slots,
    actions: &mut Vec<thumbnail_menu::Action>,
) -> ThumbnailMenu {
    let mut rows = |items: &[thumbnail_menu::SlotItem]| -> ModelRc<MenuRow> {
        let rows: Vec<MenuRow> = items
            .iter()
            .map(|(label, action)| {
                actions.push(*action);
                MenuRow {
                    label: label.as_str().into(),
                    id: i32::try_from(actions.len() - 1).unwrap_or(-1),
                }
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    };
    let mut groups = |groups: &[Vec<thumbnail_menu::SlotItem>]| {
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
    let select = groups(&slots.select);
    let remove = groups(&slots.remove);
    let (delete_title, delete_menu) = match &slots.delete_menu {
        Some((title, items)) => (title.as_str().into(), rows(items)),
        None => (SharedString::new(), rows(&[])),
    };
    ThumbnailMenu {
        head: rows(&slots.head),
        has_select: !slots.select.is_empty(),
        select,
        has_remove: !slots.remove.is_empty(),
        remove,
        filter: rows(&slots.filter),
        delete: rows(&slots.delete),
        delete_title,
        delete_menu,
        trash: rows(&slots.trash),
        manage: rows(&slots.manage),
        open: rows(&slots.open),
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
    removed: Removed,
    tags_changed: Rc<dyn Fn()>,
    manage_tags: OpenManageTags,
}

/// Open a viewer window on `model`'s file; it forgets itself from `slot`
/// when closed.
fn open_viewer(
    model: MediaViewer,
    slot: &Rc<RefCell<Option<MediaViewerWindow>>>,
    hooks: ViewerHooks,
) -> Result<MediaViewerWindow, slint::PlatformError> {
    let ViewerHooks {
        removed,
        tags_changed,
        manage_tags,
    } = hooks;
    let window = MediaViewerWindow::new()?;
    let model = Rc::new(RefCell::new(model));
    let playback = playback::Playback::new(model.borrow().store().dir().join("mpv.conf"));
    let animator = animation::Animator::new();
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = zoom_window!(window, settings.clone());
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
    // the file's info line, in the top hover frame, and its notes
    let show_info = {
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let (line, notes) = viewer::info_line(model.store(), model.current());
            window.set_info_line(line.into());
            let notes: Vec<NoteRow> = notes
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
    let show = {
        let model = model.clone();
        let weak = window.as_weak();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        let scanbar = scanbar.clone();
        let show_scanbar = show_scanbar.clone();
        let show_ratings = show_ratings.clone();
        let show_info = show_info.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
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
            show_scanbar(0.0);
            drop(model);
            show_ratings();
            show_info();
        }
    };
    show();
    bind_zoom!(window, zoomed);
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
    window.on_next({
        let model = model.clone();
        let show = show.clone();
        move || {
            model.borrow_mut().next();
            show();
        }
    });
    window.on_previous({
        let model = model.clone();
        let show = show.clone();
        move || {
            model.borrow_mut().previous();
            show();
        }
    });
    // F3: manage the file's tags; once applied, the hover frame's and the
    // page's are shown again
    window.on_manage_tags({
        let model = model.clone();
        let show = show.clone();
        move || {
            let (store, file) = {
                let model = model.borrow();
                (model.store().clone(), model.current())
            };
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
    // the media shortcuts: F7 and shift+F7, delete and shift+delete
    let act = |action: fn(&hydrus_store::Store, &[HashId]) -> hydrus_store::Result<()>| {
        let model = model.clone();
        let show_info = show_info.clone();
        move || {
            let changed = {
                let model = model.borrow();
                action(model.store(), &[model.current()])
            };
            match changed {
                Ok(()) => show_info(),
                Err(e) => eprintln!("could not change the file: {e}"),
            }
        }
    };
    window.on_archive(act(media_actions::archive));
    window.on_inbox(act(media_actions::inbox));
    window.on_undelete(act(media_actions::undelete));
    let pending: Rc<RefCell<Option<media_actions::Deletion>>> = Rc::default();
    window.on_delete({
        let model = model.clone();
        let pending = pending.clone();
        let weak = window.as_weak();
        move || {
            let model = model.borrow();
            let deletion =
                media_actions::deletion(model.store(), model.location(), &[model.current()]);
            if let (Some(deletion), Some(window)) = (deletion, weak.upgrade()) {
                window.set_question(deletion.question(1).into());
                *pending.borrow_mut() = Some(deletion);
            }
        }
    });
    window.on_answer({
        let model = model.clone();
        let weak = window.as_weak();
        let show = show.clone();
        move |yes| {
            let deletion = pending.borrow_mut().take();
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_question(SharedString::new());
            let Some(deletion) = deletion.filter(|_| yes) else {
                return;
            };
            let (store, file, location) = {
                let model = model.borrow();
                (
                    model.store().clone(),
                    model.current(),
                    model.location().clone(),
                )
            };
            if let Err(e) = media_actions::delete(&store, &[file], &deletion) {
                eprintln!("could not delete the file: {e}");
                return;
            }
            // (out of the page's domains, it leaves the page and the viewer)
            if media_actions::still_in(&store, &location, &[file]).is_empty() {
                removed(&[file]);
                let any_left = model.borrow_mut().remove_current();
                if any_left {
                    show();
                } else {
                    window.invoke_close_requested();
                }
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
    window.on_close_requested({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            // (stops playing at once)
            scanning.stop();
            playback.close();
            animator.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
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

/// A tag or predicate list's row, in its namespace's colour.
pub(crate) fn list_text(text: &str, [r, g, b]: [u8; 3]) -> ListText {
    ListText {
        text: text.into(),
        colour: slint::Color::from_rgb_u8(r, g, b),
    }
}

/// Show the page's search: the box's text and suggestions, the predicates,
/// any error, and the status bar; or, for a page without a search, why.
fn refresh(window: &MainWindow, page: &SearchPage, favourites: &ModelRc<FavouriteRow>) {
    window.set_note(page.note().unwrap_or_default().into());
    // (only a page with a search can load one)
    window.set_favourites(if page.note().is_none() {
        favourites.clone()
    } else {
        ModelRc::default()
    });
    window.set_can_filter(page.duplicates().is_some());
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
