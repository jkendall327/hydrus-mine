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
use slint::{Model, ModelRc, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

pub mod about_window;
mod animation;
mod archive_delete_window;
pub mod archive_repair_window;
mod auto_resolution_preview_window;
mod auto_resolution_review_window;
mod auto_resolution_rules_window;
mod autocomplete_tabs;
mod checker_options_window;
mod client_exit;
pub mod clipboard_monitor;
pub mod command_palette_window;
pub mod daemon;
pub mod delete_files_window;
pub mod domain_mask_entry;
pub mod downloader_definitions_window;
pub mod downloader_display_window;
pub mod downloader_interchange_window;
mod drops;
mod duplicates_sidebar;
mod edit_subscription_window;
mod embedded_metadata_window;
pub mod export_files_window;
pub mod external_call_window;
pub mod favourites_window;
pub mod file_history_window;
mod file_log_window;
mod filename_regex_menu;
mod filename_tagging_window;
mod filter_window;
mod folders_lifecycle;
mod folders_window;
mod force_filetype_window;
pub mod formula_window;
mod gallery;
pub mod gallery_source_window;
mod grid;
pub mod headless;
pub mod import_options_favourites_window;
pub mod import_options_overwrite_window;
pub mod import_options_panel_window;
mod import_options_window;
mod import_window;
mod importer_list_menu;
pub mod incremental_tagging_window;
pub mod locations_window;
pub mod login_cookies_window;
pub mod login_credential_window;
pub mod login_domain_entry;
pub mod login_domains_window;
pub mod login_example_window;
pub mod login_step_window;
pub mod login_test_window;
pub mod login_workflows_window;
mod manage_notes_window;
mod manage_ratings_window;
pub(crate) mod manage_tags_window;
mod manage_times_window;
mod manage_urls_window;
mod menu_bar;
pub mod merge_options_window;
pub mod mpv;
pub mod network_header_approval;
pub mod options_deletion;
mod options_external_calls;
pub mod options_frames;
pub mod options_namespace_colours;
mod options_palette;
mod options_window;
mod page;
mod pages;
pub mod parser_editors_window;
mod parser_test_fetch;
mod playback;
pub mod png_export_window;
mod popup_menu;
mod popups;
pub mod predicate_editor_window;
pub mod regex_favourites_window;
mod related_tags_worker;
pub mod related_weights_window;
pub mod search_log_import_window;
mod search_log_window;
pub mod search_or_window;
pub mod services_editor_window;
pub mod services_review_window;
pub mod session_autosave;
mod session_dialog;
pub mod session_startup;
mod sidebar_context_cog;
pub mod sidecars_window;
pub mod simple_formulae_window;
pub mod slideshow;
pub mod still;
pub mod string_processor_window;
mod subscription_quality_control;
mod subscriptions_window;
mod tab_context_window;
mod tab_drag;
mod tab_presentation;
pub mod tag_banner_window;
pub(crate) mod tag_display_window;
pub mod tag_filter_window;
pub mod tag_migration_window;
pub(crate) mod tag_relationships_window;
pub mod tag_suggestions_window;
pub mod thumbnail_menu;
mod thumbnail_navigation;
mod thumbnails;
mod unlock;
mod viewer;
pub mod viewer_closing;
pub mod viewer_cursor;
mod viewer_drag;
mod viewer_eye_menu;
pub mod viewer_focus;
pub mod viewer_menu;
mod viewer_presentation;
mod viewer_tag_wheel;
mod viewing_tracking;
mod watcher;
pub mod windows;
mod write_tag_history;
pub mod write_tag_menu;
pub mod write_tag_window;
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

// (the workings without the windows, in their own crate, under their
// names here)
pub use grid::ThumbnailRows;
pub use hydrus_gui_model::downloader_definitions;
pub use hydrus_gui_model::{
    archive_delete, audio, auto_resolution_preview, auto_resolution_review, auto_resolution_rules,
    autocomplete, checker_options, collect, datetime_editor, domains, duplicate_filter,
    duplicates_page, edit_subscription, embedded_metadata, export_files, favourites, file_log,
    filename_tagging, filetype_tree, folders, force_filetype, formula_editors,
    import_options_editor, importer_menu, info_lines, list_selection, local_import, main_menu,
    manage_tags, media_actions, merge_options_editor, notes_editor, options, page_chooser,
    predicate_editors, ratings, ratings_editor, scanbar, search_log, selection, session_saving,
    sidecar_editors, sidecars, simple_downloader, sort, status, string_editors,
    subscriptions_dedupe, subscriptions_dialog, subscriptions_list, tab_context, tag_filter_editor,
    tag_relationships, thumbnail_icons, thumbnail_ratings, times_editor, urls_editor,
};
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
    /// The current viewer's explicitly owned advanced deletion child.
    pub viewer_deletion: delete_files_window::Slot,
    /// The manage tags window while one is open.
    pub manage_tags: Rc<RefCell<Option<ManageTagsWindow>>>,
    pub incremental_tags: incremental_tagging_window::Slot,
    /// Siblings or parents while the corresponding editor is open.
    pub tag_relationships: Rc<RefCell<Option<TagRelationshipsWindow>>>,
    /// Display/search or relationship application configuration.
    pub tag_display: Rc<RefCell<Option<TagDisplayWindow>>>,
    /// The global tag migration window while one is open.
    pub tag_migration: tag_migration_window::Slot,
    /// The manage notes dialog while one is open.
    pub manage_notes: Rc<RefCell<Option<ManageNotesWindow>>>,
    /// The manage ratings dialog while one is open.
    pub manage_ratings: Rc<RefCell<Option<ManageRatingsWindow>>>,
    /// The manage times dialog while one is open, and the date-time
    /// editor it opens.
    pub manage_times: Rc<RefCell<Option<ManageTimesWindow>>>,
    pub datetime_editor: Rc<RefCell<Option<DateTimeEditorWindow>>>,
    /// The force filetypes dialog while one is open.
    pub force_filetype: Rc<RefCell<Option<ForceFiletypeWindow>>>,
    /// Manual file export dialog and sidecar editor.
    pub export_files: export_files_window::Slots,
    /// The focused file's detailed metadata window while open.
    pub embedded_metadata: Rc<RefCell<Option<EmbeddedMetadataWindow>>>,
    /// The manage urls dialog while one is open.
    pub manage_urls: Rc<RefCell<Option<ManageUrlsWindow>>>,
    /// The options window while it is open.
    pub options: Rc<RefCell<Option<OptionsWindow>>>,
    /// Options-owned custom reason Enter Text/question child.
    pub options_reason_child: options_deletion::Slot,
    /// Options-owned namespace Add/Delete question.
    pub options_colour_child: options_namespace_colours::Slot,
    /// Options-owned detached frame geometry editor.
    pub options_frame_child: options_frames::Slot,
    /// The Options-owned detached banner editor, while one is open.
    pub options_banner_child: tag_banner_window::Slot,
    /// Options-owned registered-call and command child family.
    pub options_external_calls: external_call_window::Slots,
    pub options_suggested_tags_slot: tag_suggestions_window::Slots,
    /// The Ctrl+P command palette while open.
    pub command_palette: command_palette_window::Slot,
    /// The about window while it is open.
    pub about: Rc<RefCell<Option<AboutWindow>>>,
    /// Live network reviews and their detached rules editor.
    pub network_data: network_data_window::Slots,
    pub network_controls: network_job_control::Binding,
    /// The review services window while it is open.
    pub services_review: Rc<RefCell<Option<ServicesReviewWindow>>>,
    /// Staged local service editors while open.
    pub services_editor: services_editor_window::Slots,
    /// Owned global archive-time maintenance window.
    pub archive_repair: archive_repair_window::Slot,
    /// Independent global file-history frame.
    pub file_history: file_history_window::Slot,
    /// The checker options editor while one is open (from the options
    /// window).
    pub checker_options: Rc<RefCell<Option<CheckerOptionsWindow>>>,
    /// The session saving dialog while it is open (pages > sessions >
    /// save).
    pub session_dialog: Rc<RefCell<Option<SessionDialog>>>,
    /// The clicked page or newly grouped notebook's text-entry window.
    pub tab_name_dialog: Rc<RefCell<Option<SessionDialog>>>,
    /// The manage subscriptions dialog while it is open.
    pub subscriptions: Rc<RefCell<Option<SubscriptionsWindow>>>,
    /// Serialized full subscription list exchange child.
    pub subscription_exchange: downloader_interchange_window::Slots,
    /// The subscriptions gallery chooser, before adding or overwriting.
    pub subscription_gallery: Rc<RefCell<Option<SubscriptionGalleryWindow>>>,
    /// URL class and gallery URL generator definition editors.
    pub downloader_definitions: downloader_definitions_window::Slots,
    /// Login script lists and their staged descendants.
    pub login_workflows: login_workflows_window::Slots,
    /// Native parser and URL-class link windows.
    pub parser_editors: parser_editors_window::Slots,
    pub network_sessions: network_sessions_window::Slots,
    /// The edit subscription dialog while it is open (from the manage
    /// subscriptions dialog).
    pub edit_subscription: Rc<RefCell<Option<EditSubscriptionWindow>>>,
    /// The import and export folders dialogs while they are open.
    pub folders: folders_window::Slots,
    /// Simple downloader formula list and reusable editors.
    pub simple_formulae: simple_formulae_window::Slots,
    /// The duplicates auto-resolution rules editor's windows.
    pub auto_resolution: auto_resolution_rules_window::Slots,
    /// The rules' "review actions" windows, by a number each.
    pub auto_resolution_reviews: Rc<RefCell<Vec<(u64, AutoResolutionReviewWindow)>>>,
    /// The duplicate filter opened from a review's pending pairs.
    pub auto_resolution_review_filter: Rc<RefCell<Option<DuplicateFilterWindow>>>,
    /// An importer's file log while one is open.
    pub file_log: Rc<RefCell<Option<FileLogWindow>>>,
    /// The archive/delete filter while one is open.
    pub archive_delete: Rc<RefCell<Option<ArchiveDeleteWindow>>>,
    /// The duplicate filter while one is open.
    pub filter: Rc<RefCell<Option<DuplicateFilterWindow>>>,
    /// Open a new page (as the page chooser does), and show it.
    pub open_page: Rc<dyn Fn(&page_chooser::NewPage)>,
    /// The advanced local deletion draft owned by the thumbnail panel.
    pub delete_files: delete_files_window::Slot,
    /// The "review files to import" window while it is open, and its list.
    pub review_imports: ReviewSlot,
    /// Its "filename tagging" dialog.
    pub filename_tagging: Rc<RefCell<Option<FilenameTaggingWindow>>>,
    /// That dialog's sidecar editors, from its "sidecars" tab.
    pub filename_tagging_sidecars: sidecars_window::Slots,
    /// The "multiple/deleted locations" list while it is open.
    pub locations: Rc<RefCell<Option<LocationsWindow>>>,
    /// The favourite searches' dialogs while they are open.
    pub favourites: favourites_window::Slots,
    /// A system predicate's editor while one is open.
    pub predicate_editor: Rc<RefCell<Option<PredicateEditorWindow>>>,
    pub search_or: search_or_window::Slot,
    _autocomplete_tabs: Rc<slint::Timer>,
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
    /// Automatic recognised URL imports while this desktop window is bound.
    pub clipboard_monitor: clipboard_monitor::Monitor,
    /// Historical autosaves, with real input activity and a bounded timer.
    pub session_autosave: session_autosave::Monitor,
    _header_approval: network_header_approval::Monitor,
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
fn lay_out_thumbnails(window: &MainWindow, store: &hydrus_store::Store, rows: &ThumbnailRows) {
    let settings = store.snapshot().thumbnails;
    let layout = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::ThumbnailLayout>)
        .unwrap_or_default();
    thumbnail_navigation::show(
        window,
        store,
        settings.bounding_height + 2 * layout.border + 2 * layout.margin,
    );
    let border = layout.border as f32;
    let (width, height) = (
        settings.bounding_width + 2 * layout.border,
        settings.bounding_height + 2 * layout.border,
    );
    let pixels = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
    rows.set_cell(pixels(layout.border), pixels(width), pixels(height));
    rows.set_rating_settings(store.read(hydrus_store::settings::get).unwrap_or_default());
    // the tag banners, and their colours
    let summaries: hydrus_core::tag_summary::TagSummaries =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let colour = |[r, g, b, a]: [u8; 4]| slint::Color::from_argb_u8(a, r, g, b);
    window.set_banner_top_background(colour(summaries.thumbnail_top.background));
    window.set_banner_top_text(colour(summaries.thumbnail_top.text));
    window.set_banner_bottom_background(colour(summaries.thumbnail_bottom_right.background));
    window.set_banner_bottom_text(colour(summaries.thumbnail_bottom_right.text));
    rows.set_summaries(summaries);
    window.set_thumbnail_width(width as f32);
    window.set_thumbnail_height(height as f32);
    window.set_thumbnail_border(border);
    window.set_thumbnail_margin(layout.margin as f32);
}

/// Show `pages` in `window`, and let the window change them.
pub fn bind(window: &MainWindow, pages: Pages) -> Bound {
    about_window::note_boot();
    let pages = Rc::new(RefCell::new(pages));
    let session_autosave = session_autosave::bind(window, &pages);
    let first = pages.borrow_mut().current();
    let current = Rc::new(RefCell::new(first.clone()));
    let rows = Rc::new(ThumbnailRows::new(first));
    window.set_thumbnail_rows(ModelRc::from(rows.clone()));
    rows.set_columns(usize::try_from(window.get_grid_columns()).unwrap_or(1));
    rows.set_scale(window.window().scale_factor());
    lay_out_thumbnails(window, current.borrow().borrow().store(), &rows);
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
    tab_presentation::bind_names(window);
    show_tabs(window, &pages.borrow());
    refresh(window, &current.borrow().borrow());
    // a duplicates page's sidebar tabs
    let duplicates = Rc::new(duplicates_sidebar::Sidebar::default());
    duplicates.show(window, &current.borrow().borrow());

    // after a change to the page shown, show it; `true` if its files changed
    let shown = {
        let current = current.clone();
        let weak = window.as_weak();
        let rows = rows.clone();
        let duplicates = duplicates.clone();
        move |files: bool| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &current.borrow().borrow());
                duplicates.show(&window, &current.borrow().borrow());
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
    duplicates_sidebar::bind(window, &duplicates, page.clone());
    sidebar_context_cog::bind(window, page.clone(), shown.clone());
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
    let viewer_exit_scrolls: Rc<
        RefCell<std::collections::HashMap<hydrus_core::pages::PageKey, HashId>>,
    > = Rc::default();
    let reveal_viewer_exit: Rc<dyn Fn(hydrus_core::pages::PageKey, HashId)> = Rc::new({
        let pending = viewer_exit_scrolls.clone();
        move |key, file| {
            pending.borrow_mut().insert(key, file);
        }
    });
    let change_pages = {
        let viewer_exit_scrolls = viewer_exit_scrolls.clone();
        let scrolls = scrolls.clone();
        let pages = pages.clone();
        let current = current.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        let shown = shown.clone();
        let after_change = after_change.clone();
        move |change: &dyn Fn(&mut Pages) -> Result<(), String>| {
            let previous = pages.borrow().shown().key;
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
            if !Rc::ptr_eq(&opened, &current.borrow()) {
                // Background pages pick up current tag presentation when activated.
                opened.borrow_mut().refresh_tags();
            }
            *current.borrow_mut() = opened.clone();
            let key = pages.borrow().shown().key;
            let focus_on_change = previous != key && {
                let settings: hydrus_store::settings::PageNavigationSettings = pages
                    .borrow()
                    .store()
                    .read(hydrus_store::settings::get)
                    .unwrap_or_default();
                settings.focus_search_on_change
            };
            let scroll = scrolls.borrow().get(&key).copied().unwrap_or(0.0);
            rows.set_page(opened.clone());
            if let Some(window) = weak.upgrade() {
                // A conditional sidebar may initialize after the property-change
                // notification. Its initial input focus uses this same eligibility.
                window.set_page_focus_on_show(focus_on_change);
                show_tabs(&window, &pages.borrow());
                window.set_grid_scroll(scroll);
            }
            shown(false);
            if focus_on_change && let Some(window) = weak.upgrade() {
                if window.get_note().is_empty() && !window.get_search_locked() {
                    window.set_search_focus_requests(
                        window.get_search_focus_requests().wrapping_add(1),
                    );
                } else if !window.get_local_import()
                    && (window.get_importing()
                        || window.get_gallery_page()
                        || window.get_watcher_page())
                {
                    window
                        .set_page_focus_requests(window.get_page_focus_requests().wrapping_add(1));
                }
            }
            if let Some(file) = viewer_exit_scrolls.borrow_mut().remove(&key) {
                let page = opened.borrow();
                let index = page
                    .results()
                    .iter()
                    .position(|&item| page.files_of(item).contains(&file));
                if let (Some(index), Some(window)) = (index, weak.upgrade()) {
                    window.set_viewer_reveal_index(i32::try_from(index).unwrap_or(i32::MAX));
                    window.set_viewer_reveal_request(
                        window.get_viewer_reveal_request().wrapping_add(1),
                    );
                }
            }
            if let (Err(e), Some(window)) = (result, weak.upgrade()) {
                window.set_error(e.into());
            }
        }
    };
    // Tag-list menus publish through weak main-window/page handles.
    write_tag_menu::install_search_launcher(Rc::new({
        let pages = Rc::downgrade(&pages);
        let current = Rc::downgrade(&current);
        let rows = Rc::downgrade(&rows);
        let duplicates = Rc::downgrade(&duplicates);
        let after_change = Rc::downgrade(&after_change);
        let scrolls = Rc::downgrade(&scrolls);
        let weak = window.as_weak();
        move |location, tags, predicates, duplicate| {
            let (
                Some(window),
                Some(pages),
                Some(current),
                Some(rows),
                Some(duplicates),
                Some(after_change),
                Some(scrolls),
            ) = (
                weak.upgrade(),
                pages.upgrade(),
                current.upgrade(),
                rows.upgrade(),
                duplicates.upgrade(),
                after_change.upgrade(),
                scrolls.upgrade(),
            )
            else {
                return;
            };
            // OR groups keep their structure in the search. Visit their tags
            // only to name the new page, just as for top-level AND predicates.
            let mut pending = predicates.iter().collect::<Vec<_>>();
            let mut names = Vec::new();
            while let Some(predicate) = pending.pop() {
                match predicate {
                    hydrus_core::search::predicate::Predicate::Tag { tag, .. } => {
                        names.push(tag.as_str().to_owned());
                    }
                    hydrus_core::search::predicate::Predicate::Or(children) => {
                        pending.extend(children);
                    }
                    _ => (),
                }
            }
            if names.is_empty() {
                return;
            }
            names.sort();
            let name = names.join(", ");
            let previous = pages.borrow().shown().key;
            scrolls
                .borrow_mut()
                .insert(previous, window.get_grid_scroll());
            let opened = {
                let mut pages = pages.borrow_mut();
                if duplicate {
                    pages.open_duplicates_with_context(
                        location,
                        tags,
                        predicates,
                        &format!("duplicates: {name}"),
                    );
                } else {
                    pages.open_search_with_context(location, Some(tags), predicates, &name);
                }
                pages.note_shown();
                pages.current()
            };
            let after = after_change.borrow().clone();
            if let Some(after) = after {
                after();
            }
            *current.borrow_mut() = opened.clone();
            rows.set_page(opened);
            show_tabs(&window, &pages.borrow());
            window.set_grid_scroll(0.0);
            refresh(&window, &current.borrow().borrow());
            duplicates.show(&window, &current.borrow().borrow());
        }
    }));
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
    window.on_page_tree_chosen({
        let change_pages = change_pages.clone();
        move |text| {
            let Some(key) = hydrus_core::pages::PageKey::from_hex(&text) else {
                return;
            };
            change_pages(&|pages| {
                pages.show(&key);
                Ok(())
            });
        }
    });
    window.on_tab_wheel({
        let change_pages = change_pages.clone();
        move |level, step| {
            if let Ok(level) = usize::try_from(level) {
                change_pages(&|pages| {
                    pages.wheel_tab(level, step);
                    Ok(())
                });
            }
        }
    });
    tab_drag::bind(window, &pages, Rc::new(change_pages.clone()));
    tab_presentation::bind_tree(window, &pages);
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
    let filename_tagging: Rc<RefCell<Option<FilenameTaggingWindow>>> = Rc::default();
    let filename_tagging_sidecars = sidecars_window::Slots::default();
    // the "multiple/deleted locations" list, from the file domain button
    let locations: Rc<RefCell<Option<LocationsWindow>>> = Rc::default();
    let favourite_dialogs = favourites_window::Slots::default();
    // the favourites the star button's menu was made from, to load from
    let menu_favourites: Rc<RefCell<Vec<hydrus_core::pages::FavouriteSearch>>> = Rc::default();
    // a system predicate's editor, from the search box
    let predicate_editor: Rc<RefCell<Option<PredicateEditorWindow>>> = Rc::default();
    let search_or = search_or_window::Slot::default();
    let review_files: Rc<dyn Fn(Vec<String>)> = Rc::new({
        let slot = review_imports.clone();
        let tagging = filename_tagging.clone();
        let tagging_sidecars = filename_tagging_sidecars.clone();
        let open_page = open_page.clone();
        let pages = pages.clone();
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
                move |paths, tags, routers, delete_after_success| {
                    open_page(&page_chooser::NewPage::LocalImport {
                        paths,
                        tags,
                        routers,
                        delete_after_success,
                    });
                }
            });
            let tag_services: Vec<(String, String)> = pages
                .borrow()
                .store()
                .snapshot()
                .services
                .all()
                .filter(|s| s.service_type().is_real_tag_service())
                .map(|s| (s.key.to_hex(), s.name.clone()))
                .collect();
            let sidecars = filename_tagging_window::Sidecars {
                store: pages.borrow().store().clone(),
                slots: tagging_sidecars.clone(),
            };
            if let Err(e) =
                import_window::open(&slot, paths, tag_services, &tagging, &sidecars, &import_now)
            {
                eprintln!("could not open the import window: {e}");
            }
        }
    });
    windows::watch_named_events(
        window.window(),
        pages.borrow().store(),
        "main_gui",
        drops::file_handler({
            let review_files = review_files.clone();
            move |paths| review_files(paths)
        }),
    );
    // open the page chosen, if one was
    let chosen = {
        let chooser = chooser.clone();
        let show_chooser = show_chooser.clone();
        let change_pages = change_pages.clone();
        let pages = pages.clone();
        let weak = window.as_weak();
        move |choice: Option<page_chooser::NewPage>| {
            if let Some(choice) = choice {
                chooser.borrow_mut().take();
                let created = Cell::new(None);
                change_pages(&|pages| {
                    created.set(pages.new_page_from_chooser(&choice)?);
                    Ok(())
                });
                let settings: hydrus_store::settings::NotebookCreationSettings = pages
                    .borrow()
                    .store()
                    .read(hydrus_store::settings::get)
                    .unwrap_or_default();
                if settings.rename_new_notebooks
                    && let (Some(key), Some(window)) = (created.get(), weak.upgrade())
                {
                    window.invoke_notebook_rename_requested(key.to_hex().into());
                }
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
    window.on_tab_new_page_requested({
        let chooser = chooser.clone();
        let pages = pages.clone();
        let show_chooser = show_chooser.clone();
        move |parent, before| {
            let parse = |text: slint::SharedString| {
                if text.is_empty() {
                    Some(None)
                } else {
                    hydrus_core::pages::PageKey::from_hex(text.as_str()).map(Some)
                }
            };
            let (Some(parent), Some(before)) = (parse(parent), parse(before)) else {
                return;
            };
            let store = {
                let mut pages = pages.borrow_mut();
                if pages.new_page_at(parent, before).is_err() {
                    return;
                }
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
    let autocomplete_tabs = autocomplete_tabs::bind(window, page.clone(), shown.clone());
    window.on_autocomplete_tab_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |tab| {
            page().borrow_mut().set_autocomplete_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(tab).unwrap_or(0),
                ),
            );
            shown(false);
        }
    });
    window.on_search_fetch({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().fetch_autocomplete();
            shown(false);
        }
    });
    window.on_search_edited({
        let slot = search_or.clone();
        let page = page.clone();
        let shown = shown.clone();
        move |text| {
            if slot.borrow().is_some() {
                return;
            }
            page().borrow_mut().type_text(&text);
            shown(false);
        }
    });
    // a system predicate chosen that needs more: its editor, whose
    // predicates join the page's search
    let open_editor = {
        let slot = predicate_editor.clone();
        let shown = shown.clone();
        let current_page = page.clone();
        let main = window.as_weak();
        move |page: Rc<RefCell<SearchPage>>| {
            let Some((blank, shift)) = page.borrow_mut().take_system_editor_wanted() else {
                return;
            };
            let store = page.borrow().store().clone();
            let snapshot = store.snapshot();
            let url_classes = snapshot
                .url_classes
                .settings()
                .url_classes
                .iter()
                .filter(|c| c.should_be_associated_with_files)
                .map(|c| c.name.clone())
                .collect();
            let context = predicate_editors::Context::new(
                &snapshot.services,
                url_classes,
                hydrus_search::Clock::system().today(),
            );
            let editor = predicate_editors::Editor::new(blank, &context);
            let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
            let text = hydrus_search::TextContext::from_store(&snapshot.services, &viewing);
            let owner: Rc<dyn Fn() -> bool> = Rc::new({
                let original = Rc::downgrade(&page);
                let current_page = current_page.clone();
                let main = main.clone();
                move || {
                    original.upgrade().is_some_and(|original| {
                        Rc::ptr_eq(&original, &current_page()) && original.borrow().lock().is_none()
                    }) && main
                        .upgrade()
                        .is_some_and(|window| window.window().is_visible())
                }
            });
            let chosen: Rc<dyn Fn(Vec<hydrus_search::Predicate>)> = Rc::new({
                let shown = shown.clone();
                let owner = owner.clone();
                move |predicates| {
                    if !owner() {
                        return;
                    }
                    page.borrow_mut().apply_system_editor(predicates, shift);
                    shown(true);
                }
            });
            if let Err(e) = predicate_editor_window::open(
                &slot,
                &store,
                editor,
                context,
                text,
                chosen,
                Some(owner),
            ) {
                eprintln!("could not open the predicate editor: {e}");
            }
        }
    };
    window.on_search_or_action({
        let slot = search_or.clone();
        let weak = window.as_weak();
        let page = page.clone();
        let shown = shown.clone();
        let open_editor = open_editor.clone();
        move |action| {
            let current = page();
            if slot.borrow().is_some() {
                return;
            }
            if action == 3 || action == 4 {
                if current.borrow().lock().is_some()
                    || current.borrow().favourite_to_save().is_none()
                {
                    return;
                }
                let store = current.borrow().store().clone();
                let mode: hydrus_store::settings::AdvancedMode =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                if action == 4 && !mode.0 {
                    return;
                }
                let context = hydrus_search::FileSearchContext {
                    location: current.borrow().location().clone(),
                    tags: current.borrow().tag_context().clone(),
                    predicates: Vec::new(),
                };
                let owner: search_or_window::ValidOwner = Rc::new({
                    let original = Rc::downgrade(&current);
                    let page = page.clone();
                    let weak = weak.clone();
                    move || {
                        original.upgrade().is_some_and(|original| {
                            Rc::ptr_eq(&original, &page()) && original.borrow().lock().is_none()
                        }) && weak
                            .upgrade()
                            .is_some_and(|window| window.window().is_visible())
                    }
                });
                let applied: search_or_window::Applied = Rc::new({
                    let current = Rc::downgrade(&current);
                    let owner = owner.clone();
                    let shown = shown.clone();
                    move |predicates| {
                        if owner()
                            && let Some(current) = current.upgrade()
                        {
                            current.borrow_mut().apply_or_editor(predicates);
                            shown(true);
                        }
                    }
                });
                match search_or_window::open(store, context, action == 4, &slot, owner, applied) {
                    Ok(child) => {
                        if let Some(window) = weak.upgrade() {
                            window.set_search_or_open(true);
                        }
                        let weak = weak.clone();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_search_or_open(false);
                                window.set_search_focus_requests(
                                    window.get_search_focus_requests() + 1,
                                );
                            }
                        });
                    }
                    Err(error) => eprintln!("could not open the OR editor: {error}"),
                }
                return;
            }
            match action {
                0 => current.borrow_mut().enter_or(true),
                1 => current.borrow_mut().change_or_draft(true),
                2 => current.borrow_mut().change_or_draft(false),
                _ => return,
            }
            shown(true);
            if action == 0 {
                open_editor(current);
            }
        }
    });
    window.on_search_or_escape({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let handled = page().borrow_mut().escape_or();
            if handled {
                shown(false);
            }
            handled
        }
    });
    window.on_search_accepted({
        let slot = search_or.clone();
        let page = page.clone();
        let shown = shown.clone();
        let open_editor = open_editor.clone();
        move || {
            if slot.borrow().is_some() {
                return;
            }
            page().borrow_mut().enter();
            shown(true);
            open_editor(page());
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
        let slot = search_or.clone();
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            if slot.borrow().is_some() {
                return;
            }
            page()
                .borrow_mut()
                .choose(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
            open_editor(page());
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
    window.on_include_flipped({
        let page = page.clone();
        let shown = shown.clone();
        move |which| {
            page().borrow_mut().flip_include(which == 1);
            shown(true);
        }
    });
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
    // a duplicates rule's pairs' files, shown in a new page
    *duplicates.open_files.borrow_mut() = Some(Rc::new({
        let change_pages = change_pages.clone();
        move |location: hydrus_search::LocationContext, files: Vec<HashId>| {
            change_pages(&|pages| {
                pages.open_files(location.clone(), files.clone(), None, None);
                Ok(())
            });
        }
    }));
    // logs' files, shown in new pages
    let open_files = file_log_window::OpenFiles(
        {
            let change_pages = change_pages.clone();
            Rc::new(move |files| {
                let location =
                    hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                        hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
                    ));
                change_pages(&|pages| {
                    pages.open_files(location.clone(), files.clone(), None, None);
                    Ok(())
                });
            })
        },
        {
            let change_pages = change_pages.clone();
            Rc::new(move |urls| {
                let location =
                    hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                        hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
                    ));
                let predicates = crate::file_log::url_search(&urls);
                change_pages(&|pages| {
                    pages.open_search(location.clone(), predicates.clone(), "url search");
                    Ok(())
                });
            })
        },
        png_export_window::Slots::default(),
        search_log_import_window::Slots::default(),
    );
    // the page's importer's file log
    let file_log_slot: Rc<RefCell<Option<FileLogWindow>>> = Rc::default();
    let file_log = file_log_slot.clone();
    window.on_open_file_log({
        let page = page.clone();
        let open_files = open_files.clone();
        let file_log = file_log.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let Some(importer) = page.importer() else {
                return;
            };
            let old = file_log.borrow_mut().take();
            if let Some(old) = old {
                old.invoke_close_window();
            }
            match file_log_window::open(page.store(), importer.queue, &file_log, &open_files) {
                Ok(window) => *file_log.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the file log: {e}"),
            }
        }
    });
    // and its search log (a watcher's check log)
    window.on_open_search_log({
        let page = page.clone();
        let file_log = file_log.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let Some(importer) = page.importer() else {
                return;
            };
            let old = file_log.borrow_mut().take();
            if let Some(old) = old {
                old.invoke_close_window();
            }
            match search_log_window::open(page.store(), importer.queue, &file_log) {
                Ok(window) => *file_log.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the search log: {e}"),
            }
        }
    });
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
                        filter_window::open_filter(model, step, &filter, None)
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
    let viewer_deletion: delete_files_window::Slot = Rc::default();
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
    let incremental_tags = incremental_tagging_window::Slot::default();
    let tag_relationships: Rc<RefCell<Option<TagRelationshipsWindow>>> = Rc::default();
    let tag_display: Rc<RefCell<Option<TagDisplayWindow>>> = Rc::default();
    let tag_migration = tag_migration_window::Slot::default();
    let tags_changed: Rc<dyn Fn()> = Rc::new({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().refresh_tags();
            shown(false);
        }
    });
    // a file archived or rated in the viewer, say: its thumbnail's icons
    // and ratings, and the status bar, are shown again
    let files_changed: Rc<dyn Fn()> = Rc::new({
        let rows = rows.clone();
        let shown = shown.clone();
        move || {
            rows.forget_files();
            shown(false);
        }
    });
    let open_manage_tags = {
        let manage_tags = manage_tags.clone();
        let incremental_tags = incremental_tags.clone();
        let page = page.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            if let Some(window) = manage_tags.borrow().as_ref() {
                let _ = window.show();
                return;
            }
            let Some(mut model) = manage_tags::ManageTags::new(store, files) else {
                return;
            };
            model.set_location(page().borrow().location().clone());
            match manage_tags_window::open(model, &manage_tags, &incremental_tags, applied) {
                Ok(window) => *manage_tags.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage tags: {e}"),
            }
        }
    };
    // a thumbnail's or the viewer's "manage > notes"
    let manage_notes: Rc<RefCell<Option<ManageNotesWindow>>> = Rc::default();
    let open_manage_notes: OpenManageNotes = Rc::new({
        let manage_notes = manage_notes.clone();
        move |store: Arc<hydrus_store::Store>, file: HashId, applied: Rc<dyn Fn()>| {
            if let Some(existing) = manage_notes.borrow().as_ref() {
                existing.invoke_focus_note();
                return;
            }
            match manage_notes_window::open(&store, file, &manage_notes, applied) {
                Ok(window) => *manage_notes.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage notes: {e}"),
            }
        }
    });
    // a thumbnail's or the viewer's "manage > ratings"
    let manage_ratings: Rc<RefCell<Option<ManageRatingsWindow>>> = Rc::default();
    let open_manage_ratings: OpenOnFiles = Rc::new({
        let manage_ratings = manage_ratings.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            match manage_ratings_window::open(&store, files, &manage_ratings, applied) {
                Ok(window) => *manage_ratings.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage ratings: {e}"),
            }
        }
    });
    // a thumbnail's or the viewer's "manage > times"
    let manage_times: Rc<RefCell<Option<ManageTimesWindow>>> = Rc::default();
    let datetime_editor: Rc<RefCell<Option<DateTimeEditorWindow>>> = Rc::default();
    let open_manage_times: OpenOnFiles = Rc::new({
        let manage_times = manage_times.clone();
        let datetime_editor = datetime_editor.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            match manage_times_window::open(
                &store,
                &files,
                &manage_times,
                &datetime_editor,
                applied,
            ) {
                Ok(window) => *manage_times.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage times: {e}"),
            }
        }
    });
    // The single-file info menu's detailed metadata window.
    let embedded_metadata: Rc<RefCell<Option<EmbeddedMetadataWindow>>> = Rc::default();
    let open_embedded_metadata: OpenOnFiles = Rc::new({
        let slot = embedded_metadata.clone();
        move |store, files, _| {
            if let Some(&file) = files.first() {
                match embedded_metadata_window::open(&store, file, &slot) {
                    Ok(window) => *slot.borrow_mut() = Some(window),
                    Err(e) => eprintln!("could not open detailed file metadata: {e}"),
                }
            }
        }
    });
    // a thumbnail's or the viewer's "manage > force filetype"
    let force_filetype: Rc<RefCell<Option<ForceFiletypeWindow>>> = Rc::default();
    let open_force_filetype: OpenOnFiles = Rc::new({
        let force_filetype = force_filetype.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            match force_filetype_window::open(&store, &files, &force_filetype, applied) {
                Ok(window) => *force_filetype.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open force filetypes: {e}"),
            }
        }
    });
    let export_files = export_files_window::Slots::default();
    let open_export_files: OpenOnFiles = Rc::new({
        let slots = export_files.clone();
        move |store, files, changed| {
            let facts = thumbnail_menu::facts(&store, &files);
            let storage = hydrus_store::content::DomainRoles::new(&store.snapshot().services)
                .ok()
                .map(|r| r.local_file_storage);
            let files = files
                .into_iter()
                .filter(|f| {
                    facts
                        .iter()
                        .any(|m| m.file == *f && storage.is_some_and(|s| m.current.contains(&s)))
                })
                .collect();
            if let Err(e) = export_files_window::open(&store, files, &slots, changed) {
                eprintln!("could not open export files: {e}");
            }
        }
    });
    // a thumbnail's or the viewer's "urls > manage"
    let manage_urls: Rc<RefCell<Option<ManageUrlsWindow>>> = Rc::default();
    let open_manage_urls: OpenOnFiles = Rc::new({
        let manage_urls = manage_urls.clone();
        move |store: Arc<hydrus_store::Store>, files: Vec<HashId>, applied: Rc<dyn Fn()>| {
            match manage_urls_window::open(&store, files, &manage_urls, applied) {
                Ok(window) => *manage_urls.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open manage urls: {e}"),
            }
        }
    });
    window.on_manage_tags_selected({
        let page = page.clone();
        let manage_tags = manage_tags.clone();
        let open_manage_tags = open_manage_tags.clone();
        let tags_changed = tags_changed.clone();
        move || {
            if let Some(window) = manage_tags.borrow().as_ref() {
                let _ = window.show();
                return;
            }
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
    let delete_files: delete_files_window::Slot = Rc::default();
    let exit_confirmation = Rc::new(slint::Timer::default());
    let pending: Rc<RefCell<Option<Asked>>> = Rc::default();
    let ask = {
        let pending = pending.clone();
        let weak = window.as_weak();
        let page = page.clone();
        let removed = removed.clone();
        let shown = shown.clone();
        let delete_files = delete_files.clone();
        move |asked: Asked| {
            let store = page().borrow().store().clone();
            if let Asked::Delete(files, deletion, location) = &asked
                && store
                    .read(
                        hydrus_store::settings::get::<hydrus_store::settings::DeletionPreferences>,
                    )
                    .unwrap_or_default()
                    .advanced
            {
                let owner = page();
                let guard: delete_files_window::Guard = Rc::new({
                    let owner = owner.clone();
                    let page = page.clone();
                    let weak = weak.clone();
                    move || weak.upgrade().is_some() && Rc::ptr_eq(&owner, &page())
                });
                let applied = Rc::new({
                    let store = store.clone();
                    let files = files.clone();
                    let location = location.clone();
                    let owner = owner.clone();
                    let shown = shown.clone();
                    move || {
                        let remaining = media_actions::still_in(&store, &location, &files);
                        let gone = files
                            .iter()
                            .copied()
                            .filter(|f| !remaining.contains(f))
                            .collect::<Vec<_>>();
                        owner.borrow_mut().remove_files(&gone);
                        shown(false);
                    }
                });
                let suggested = media_actions::suggested_action(&store, deletion);
                if let Err(error) = delete_files_window::open(
                    &delete_files,
                    &store,
                    files,
                    suggested.as_ref(),
                    media_actions::DELETE_REASON,
                    guard,
                    applied,
                ) {
                    eprintln!("could not open deletion: {error}");
                }
                return;
            }
            if let Asked::Delete(files, deletion, _) = &asked
                && !media_actions::confirm_deletion(&store, files, deletion)
            {
                asked.act(&store, &*removed);
                shown(false);
                return;
            }
            if let Some(window) = weak.upgrade() {
                window.set_question(asked.question().into());
                *pending.borrow_mut() = Some(asked);
            }
        }
    };
    client_exit::bind(
        window,
        page().borrow().store().clone(),
        &exit_confirmation,
        Rc::new({
            let ask = ask.clone();
            move |question, then| ask(Asked::Then(question, then))
        }),
    );
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
            if !files.is_empty() {
                if media_actions::confirm_archive(&store, files.len()) {
                    ask(Asked::archive_or_inbox(archive, files));
                } else {
                    Asked::archive_or_inbox(archive, files).act(&store, &|_| {});
                    shown(false);
                }
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
            let (deletion, location) = {
                let page = page.borrow();
                (
                    media_actions::deletion(page.store(), page.location(), &files),
                    page.location().clone(),
                )
            };
            if let Some(deletion) = deletion {
                ask(Asked::Delete(files, deletion, location));
            }
        }
    });
    window.on_answer({
        let exit_confirmation = exit_confirmation.clone();
        let page = page.clone();
        let weak = window.as_weak();
        let removed = removed.clone();
        let shown = shown.clone();
        let change_pages = change_pages.clone();
        move |yes| {
            exit_confirmation.stop();
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
                if let Asked::RemoveQueries(queues, _) = asked {
                    let page = page();
                    for queue in queues {
                        page.borrow_mut().remove_query(queue);
                    }
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
    let tab_name_dialog = tab_context_window::bind(window, &pages, Rc::new(change_pages.clone()));
    // the menu bar, its titles shown again as what they say changes
    let options: Rc<RefCell<Option<OptionsWindow>>> = Rc::default();
    let options_reason_child: options_deletion::Slot = Rc::default();
    let options_colour_child: options_namespace_colours::Slot = Rc::default();
    let options_frame_child: options_frames::Slot = Rc::default();
    let options_banner_child: tag_banner_window::Slot = Rc::default();
    let options_external_calls = external_call_window::Slots::default();
    let options_suggested_tags_slot = tag_suggestions_window::Slots::default();
    let about: Rc<RefCell<Option<AboutWindow>>> = Rc::default();
    let services_review: Rc<RefCell<Option<ServicesReviewWindow>>> = Rc::default();
    let services_editor = services_editor_window::Slots::default();
    let archive_repair = archive_repair_window::Slot::default();
    let file_history = file_history_window::Slot::default();
    let network_data = network_data_window::Slots::default();
    let checker_options: Rc<RefCell<Option<CheckerOptionsWindow>>> = Rc::default();
    let session_dialog: Rc<RefCell<Option<SessionDialog>>> = Rc::default();
    let subscription_exchange = downloader_interchange_window::Slots::default();
    let subscriptions: Rc<RefCell<Option<SubscriptionsWindow>>> = Rc::default();
    let subscription_gallery: Rc<RefCell<Option<SubscriptionGalleryWindow>>> = Rc::default();
    let downloader_definitions = downloader_definitions_window::Slots::default();
    let login_workflows = login_workflows_window::Slots::default();
    let parser_editors = parser_editors_window::Slots::default();
    let network_sessions = network_sessions_window::Slots::default();
    let edit_subscription: Rc<RefCell<Option<EditSubscriptionWindow>>> = Rc::default();
    // a downloader list's menu's actions, as last opened
    let importer_actions: Rc<RefCell<Vec<importer_menu::Action>>> = Rc::default();
    let folders = folders_window::Slots {
        open_files: open_files.clone(),
        ..folders_window::Slots::default()
    };
    let clipboard_monitor = clipboard_monitor::Monitor::bind(
        window,
        pages.clone(),
        Rc::new({
            let change_pages = change_pages.clone();
            move || change_pages(&|_| Ok(()))
        }),
    );
    let header_approval =
        network_header_approval::Monitor::bind(window, pages.borrow().store().clone());
    let command_palette: command_palette_window::Slot = Rc::default();
    let palette_dispatcher: command_palette_window::MainDispatcher = Rc::default();
    let menu_titles_shown = menu_bar::bind(
        window,
        menu_bar::Hooks {
            watch_clipboard: Rc::new({
                let monitor = clipboard_monitor.clone();
                move |watchers| monitor.toggle(watchers)
            }),
            tag_display: {
                let slot = tag_display.clone();
                let pages = pages.clone();
                let applied: Rc<dyn Fn()> = Rc::new({
                    let pages = pages.clone();
                    let shown = shown.clone();
                    let viewer = viewer.clone();
                    let rows = rows.clone();
                    let manage_tags = manage_tags.clone();
                    move || {
                        for page in pages.borrow().open_pages() {
                            page.borrow_mut().refresh_tags();
                        }
                        rows.forget_files();
                        shown(false);
                        if let Some(w) = manage_tags.borrow().as_ref() {
                            w.invoke_refresh_autocomplete();
                        }
                        if let Some(w) = viewer.borrow().as_ref() {
                            w.invoke_refresh_tags();
                        }
                    }
                });
                Rc::new(move |application| {
                    if slot.borrow().is_some() {
                        return;
                    }
                    match hydrus_gui_model::tag_display::TagDisplayEditor::new(
                        pages.borrow().store().clone(),
                    ) {
                        Ok(model) => match tag_display_window::open(
                            model,
                            application,
                            &slot,
                            applied.clone(),
                        ) {
                            Ok(w) => *slot.borrow_mut() = Some(w),
                            Err(e) => eprintln!("could not open tag display: {e}"),
                        },
                        Err(e) => eprintln!("could not load tag display: {e}"),
                    }
                })
            },
            tag_migrate: Rc::new({
                let slot = tag_migration.clone();
                let pages = pages.clone();
                let changed: Rc<dyn Fn()> = Rc::new({
                    let pages = pages.clone();
                    let shown = shown.clone();
                    let rows = rows.clone();
                    let viewer = viewer.clone();
                    let manage_tags = manage_tags.clone();
                    move || {
                        for page in pages.borrow().open_pages() {
                            page.borrow_mut().refresh_tags();
                        }
                        rows.forget_files();
                        shown(false);
                        if let Some(window) = viewer.borrow().as_ref() {
                            window.invoke_refresh_tags();
                        }
                        if let Some(window) = manage_tags.borrow().as_ref() {
                            window.invoke_refresh_autocomplete();
                        }
                    }
                });
                move || {
                    let store = pages.borrow().store().clone();
                    let preferences = store
                        .read(
                            hydrus_store::settings::get::<
                                hydrus_store::tag_editing::TagEditingSettings,
                            >,
                        )
                        .unwrap_or_default();
                    let key = preferences.default_service;
                    if let Err(error) =
                        tag_migration_window::open(&store, &key, Vec::new(), &slot, changed.clone())
                    {
                        eprintln!("could not open tag migration: {error}");
                    }
                }
            }),
            tag_relationships: {
                let slot = tag_relationships.clone();
                let pages = pages.clone();
                let applied: Rc<dyn Fn()> = Rc::new({
                    let tags_changed = tags_changed.clone();
                    let viewer = viewer.clone();
                    move || {
                        tags_changed();
                        if let Some(window) = viewer.borrow().as_ref() {
                            window.invoke_refresh_tags();
                        }
                    }
                });
                Rc::new(move |kind| {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let store = pages.borrow().store().clone();
                    match tag_relationships::Relationships::new(store, kind) {
                        Ok(model) => {
                            match tag_relationships_window::open(model, &slot, applied.clone()) {
                                Ok(window) => *slot.borrow_mut() = Some(window),
                                Err(e) => eprintln!("could not open tag relationships: {e}"),
                            }
                        }
                        Err(e) => eprintln!("could not load tag relationships: {e}"),
                    }
                })
            },
            pages: pages.clone(),
            network_data: network_data.clone(),
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
                let reason_slot = options_reason_child.clone();
                let colour_slot = options_colour_child.clone();
                let frame_slot = options_frame_child.clone();
                let banner_slot = options_banner_child.clone();
                let external_slots = options_external_calls.clone();
                let suggested_slot = options_suggested_tags_slot.clone();
                let checker_slot = checker_options.clone();
                let viewer = viewer.clone();
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
                        let viewer = viewer.clone();
                        let pages = pages.clone();
                        let change_pages = change_pages.clone();
                        let rows = rows.clone();
                        let weak = weak.clone();
                        let store = store.clone();
                        move || {
                            pages.borrow_mut().reload_settings();
                            if let Some(window) = viewer.borrow().as_ref() {
                                window.invoke_presentation_settings_changed();
                            }
                            // (the cells as the options now have them; and
                            // thumbnails of another size, every one again)
                            if let Some(window) = weak.upgrade() {
                                lay_out_thumbnails(&window, &store, &rows);
                            }
                            if store.snapshot().thumbnails != thumbnails_before {
                                rows.thumbnails_changed();
                            }
                            change_pages(&|pages| {
                                pages.current().borrow_mut().refresh_tags();
                                Ok(())
                            });
                        }
                    });
                    match options_window::open(
                        &store,
                        &slot,
                        &checker_slot,
                        &reason_slot,
                        &colour_slot,
                        &frame_slot,
                        &banner_slot,
                        &suggested_slot,
                        &external_slots,
                        applied,
                    ) {
                        Ok(window) => *slot.borrow_mut() = Some(window),
                        Err(e) => eprintln!("could not open the options: {e}"),
                    }
                })
            },
            manage_network_sessions: {
                let pages = pages.clone();
                let slots = network_sessions.clone();
                Rc::new(move |headers| {
                    let store = pages.borrow().store().clone();
                    if let Err(e) = network_sessions_window::open(&store, &slots, headers) {
                        eprintln!("could not open network data: {e}");
                    }
                })
            },
            manage_parsers: {
                let pages = pages.clone();
                let slots = parser_editors.clone();
                Rc::new(move |links| {
                    let store = pages.borrow().store().clone();
                    if let Err(e) = parser_editors_window::open(&store, &slots, links) {
                        eprintln!("could not open parser definitions: {e}");
                    }
                })
            },
            exchange_downloaders: {
                let pages = pages.clone();
                let slots = downloader_interchange_window::Slots::default();
                Rc::new(move |importing| {
                    let store = pages.borrow().store().clone();
                    if let Err(e) =
                        downloader_interchange_window::package(&store, &slots, importing)
                    {
                        eprintln!("could not open downloader interchange: {e}");
                    }
                })
            },
            manage_downloader_display: {
                let pages = pages.clone();
                let slots = downloader_display_window::Slots::default();
                Rc::new(move || {
                    let store = pages.borrow().store().clone();
                    if let Err(e) = downloader_display_window::open(&store, &slots) {
                        eprintln!("could not open downloader display: {e}");
                    }
                })
            },
            manage_logins: {
                let pages = pages.clone();
                let slots = login_workflows.domains.clone();
                Rc::new(move || {
                    let store = pages.borrow().store().clone();
                    if let Err(error) = login_domains_window::open(&store, &slots) {
                        eprintln!("could not open domain logins: {error}");
                    }
                })
            },
            manage_login_scripts: {
                let pages = pages.clone();
                let slots = login_workflows.clone();
                Rc::new(move || {
                    let store = pages.borrow().store().clone();
                    if let Err(error) = login_workflows_window::open_scripts(&store, &slots) {
                        eprintln!("could not open login scripts: {error}");
                    }
                })
            },
            manage_downloader_definitions: {
                let pages = pages.clone();
                let slots = downloader_definitions.clone();
                Rc::new(move |classes| {
                    let store = pages.borrow().store().clone();
                    if let Err(e) = downloader_definitions_window::open(&store, &slots, classes) {
                        eprintln!("could not open downloader definitions: {e}");
                    }
                })
            },
            // network > subscriptions…
            manage_subscriptions: {
                let pages = pages.clone();
                let slot = subscriptions.clone();
                let edit_slot = edit_subscription.clone();
                let exchange = subscription_exchange.clone();
                let gallery_slot = subscription_gallery.clone();
                let checker_slot = checker_options.clone();
                let log_slot = folders.log.clone();
                let import_options_slot = folders.import_options.clone();
                let open_files = open_files.clone();
                Rc::new(move || {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let store = pages.borrow().store().clone();
                    let slots = edit_subscription_window::Slots {
                        exchange: exchange.clone(),
                        edit: edit_slot.clone(),
                        checker: checker_slot.clone(),
                        log: log_slot.clone(),
                        open_files: open_files.clone(),
                        import_options: import_options_slot.clone(),
                    };
                    match subscriptions_window::open(&store, &slot, &gallery_slot, slots) {
                        Ok(window) => *slot.borrow_mut() = Some(window),
                        Err(e) => eprintln!("could not open the subscriptions: {e}"),
                    }
                })
            },
            // file > import/export folders > manage import/export folders…
            manage_folders: {
                let pages = pages.clone();
                let slots = folders.clone();
                Rc::new(move |import| {
                    let store = pages.borrow().store().clone();
                    let opened = if import {
                        folders_window::open_import_folders(&store, &slots)
                    } else {
                        folders_window::open_export_folders(&store, &slots)
                    };
                    if let Err(e) = opened {
                        eprintln!("could not open the folders: {e}");
                    }
                })
            },
            import_files: {
                let review_files = review_files.clone();
                Rc::new(move || review_files(Vec::new()))
            },
            save_session: {
                let pages = pages.clone();
                let slot = session_dialog.clone();
                Rc::new(move |name, scope| {
                    if slot.borrow().is_some() {
                        return;
                    }
                    match session_dialog::open(&pages, name.as_deref(), scope, &slot) {
                        Ok(window) => *slot.borrow_mut() = Some(window),
                        Err(e) => eprintln!("could not save the session: {e}"),
                    }
                })
            },
            file_history: {
                let pages = pages.clone();
                let slot = file_history.clone();
                let weak = window.as_weak();
                Rc::new(move || {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let owner = Rc::new({
                        let weak = weak.clone();
                        move || weak.upgrade().is_some_and(|w| w.window().is_visible())
                    });
                    if let Err(error) =
                        file_history_window::open(pages.borrow().store(), &slot, owner)
                    {
                        eprintln!("could not open file history: {error}");
                    }
                })
            },
            repair_archive_times: {
                let pages = pages.clone();
                let slot = archive_repair.clone();
                let weak = window.as_weak();
                Rc::new(move || {
                    if slot.borrow().is_some() {
                        return;
                    }
                    let valid = Rc::new({
                        let weak = weak.clone();
                        move || weak.upgrade().is_some_and(|w| w.window().is_visible())
                    });
                    let changed = Rc::new({
                        let weak = weak.clone();
                        move || {
                            if let Some(w) = weak.upgrade() {
                                w.invoke_refresh_page();
                            }
                        }
                    });
                    if let Err(error) =
                        archive_repair_window::open(pages.borrow().store(), &slot, changed, valid)
                    {
                        eprintln!("could not repair archive times: {error}");
                    }
                })
            },
            manage_services: {
                let pages = pages.clone();
                let slots = services_editor.clone();
                let change_pages = change_pages.clone();
                let rows = rows.clone();
                let tags_changed = tags_changed.clone();
                let viewer = viewer.clone();
                let weak = window.as_weak();
                Rc::new(move || {
                    if slots.manage.borrow().is_some() {
                        return;
                    }
                    let changed: Rc<dyn Fn()> = Rc::new({
                        let pages = pages.clone();
                        let rows = rows.clone();
                        let change_pages = change_pages.clone();
                        let tags_changed = tags_changed.clone();
                        let viewer = viewer.clone();
                        let weak = weak.clone();
                        move || {
                            pages.borrow_mut().reload_settings();
                            rows.thumbnails_changed();
                            change_pages(&|_| Ok(()));
                            if let Some(window) = weak.upgrade() {
                                window.invoke_refresh_page();
                            }
                            tags_changed();
                            if let Some(window) = viewer.borrow().as_ref() {
                                window.invoke_refresh_tags();
                            }
                        }
                    });
                    match services_editor_window::open(pages.borrow().store(), &slots, changed) {
                        Ok(window) => *slots.manage.borrow_mut() = Some(window),
                        Err(e) => eprintln!("could not manage services: {e}"),
                    }
                })
            },
            review_services: {
                let pages = pages.clone();
                let slot = services_review.clone();
                let tags_changed = tags_changed.clone();
                let viewer = viewer.clone();
                let changed: Rc<dyn Fn()> = Rc::new(move || {
                    tags_changed();
                    if let Some(window) = viewer.borrow().as_ref() {
                        window.invoke_refresh_tags();
                    }
                });
                Rc::new(move || {
                    match services_review_window::open_with_changed(
                        pages.borrow().store().clone(),
                        changed.clone(),
                    ) {
                        Ok(window) => {
                            let previous = slot.borrow_mut().take();
                            if let Some(previous) = previous {
                                previous.invoke_close_clicked();
                            }
                            *slot.borrow_mut() = Some(window);
                        }
                        Err(e) => eprintln!("could not review services: {e}"),
                    }
                })
            },
            // help > about
            about: {
                let pages = pages.clone();
                let slot = about.clone();
                Rc::new(move || {
                    let store = pages.borrow().store().clone();
                    match about_window::open(&store) {
                        Ok(w) => *slot.borrow_mut() = Some(w),
                        Err(e) => eprintln!("could not open the about window: {e}"),
                    }
                })
            },
            importer_menu: {
                let page = page.clone();
                let actions = importer_actions.clone();
                Rc::new(move |row| {
                    let Ok(row) = usize::try_from(row) else {
                        return Vec::new();
                    };
                    let page = page();
                    let (entries, chosen) = importer_list_menu::open(&mut page.borrow_mut(), row);
                    *actions.borrow_mut() = chosen;
                    entries
                })
            },
            domain_menu: {
                let page = page.clone();
                Rc::new(move |which| {
                    let page = page();
                    let page = page.borrow();
                    if page.note().is_some() || page.lock().is_some() {
                        return Vec::new();
                    }
                    let store = page.store();
                    let snapshot = store.snapshot();
                    let rows = if which == 0 {
                        let hydrus_store::settings::AdvancedMode(advanced) =
                            store.read(hydrus_store::settings::get).unwrap_or_default();
                        domains::location_menu(&snapshot.services, advanced, page.location())
                    } else {
                        domains::tag_menu(&snapshot.services, page.tag_context())
                    };
                    domains::entries(rows)
                })
            },
            search_domain: {
                let page = page.clone();
                let shown = shown.clone();
                let locations = locations.clone();
                Rc::new(move |choice| {
                    let page = page();
                    match choice {
                        domains::Choice::Location(location) => {
                            page.borrow_mut().choose_location(location);
                        }
                        domains::Choice::Tags(service) => {
                            page.borrow_mut().choose_tag_service(service);
                        }
                        domains::Choice::Multiple => {
                            let (store, current) = {
                                let page = page.borrow();
                                (page.store().clone(), page.location().clone())
                            };
                            let chosen: Rc<dyn Fn(hydrus_search::LocationContext)> = Rc::new({
                                let page = page.clone();
                                let shown = shown.clone();
                                move |location| {
                                    page.borrow_mut().choose_location(location);
                                    shown(true);
                                }
                            });
                            if let Err(e) =
                                locations_window::open(&locations, store, &current, chosen)
                            {
                                eprintln!("could not open the locations list: {e}");
                            }
                            return;
                        }
                    }
                    shown(true);
                })
            },
            favourites_menu: {
                let page = page.clone();
                let menu_favourites = menu_favourites.clone();
                Rc::new(move || {
                    let page = page();
                    let page = page.borrow();
                    if page.note().is_some() {
                        return Vec::new();
                    }
                    // (read as the menu opens, as the reference's manager
                    // has them now)
                    let read: Vec<hydrus_core::pages::FavouriteSearch> =
                        page.store()
                            .read(
                                hydrus_store::settings::get::<
                                    hydrus_store::settings::FavouriteSearches,
                                >,
                            )
                            .map(|f| f.0)
                            .unwrap_or_default();
                    let entries = favourites::menu(&read);
                    *menu_favourites.borrow_mut() = read;
                    entries
                })
            },
            favourite: {
                let page = page.clone();
                let shown = shown.clone();
                let menu_favourites = menu_favourites.clone();
                let dialogs = favourite_dialogs.clone();
                Rc::new(move |action| {
                    let page = page();
                    let store = page.borrow().store().clone();
                    let opened = match action {
                        favourites::Action::Load(i) => {
                            if let Some(favourite) = menu_favourites.borrow().get(i) {
                                page.borrow_mut().load_favourite(favourite);
                            }
                            shown(true);
                            return;
                        }
                        favourites::Action::Manage => {
                            favourites_window::open(&dialogs, &store, None)
                        }
                        favourites::Action::Save => {
                            let to_save = page.borrow().favourite_to_save();
                            favourites_window::open(&dialogs, &store, to_save)
                        }
                    };
                    if let Err(e) = opened {
                        eprintln!("could not open the favourite searches: {e}");
                    }
                })
            },
        },
        &palette_dispatcher,
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
            window.set_status_network(
                status::bandwidth_status_with_format(
                    bytes,
                    per_second,
                    &pauses,
                    &hydrus_gui_model::gui_format::preferences(&store),
                )
                .into(),
            );
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
    let network_controls = network_job_control::bind_owned(
        window,
        page().borrow().store().clone(),
        Rc::new({
            let page = page.clone();
            move |gallery| {
                page().borrow().importer().filter(|i| !i.local).map(|i| {
                    hydrus_gui_model::network_job_control::Target {
                        queue: i.queue,
                        gallery,
                    }
                })
            }
        }),
        Rc::new({
            let pages = pages.clone();
            move || hex::encode(pages.borrow().shown().key.0)
        }),
        network_data.clone(),
    );
    network_controls.set_owner_alive(Rc::new({
        let pages = pages.clone();
        move |key| {
            pages
                .borrow()
                .session()
                .all_pages()
                .iter()
                .any(|page| hex::encode(page.key.0) == key)
        }
    }));
    let simple_formulae = simple_formulae_window::Slots::default();
    window.on_simple_edit_formulae({
        let page = page.clone();
        let shown = shown.clone();
        let slots = simple_formulae.clone();
        move || {
            if slots.list.borrow().is_some() {
                return;
            }
            let store = page().borrow().store().clone();
            let applied = Rc::new({
                let shown = shown.clone();
                move || shown(false)
            });
            match simple_formulae_window::open(&store, &slots, applied) {
                Ok(w) => *slots.list.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open simple formulae: {e}"),
            }
        }
    });
    // a simple downloader page's parsing box
    window.on_simple_pause_play_queue({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().pause_play_queue();
            shown(false);
        }
    });
    window.on_simple_job_clicked({
        let page = page.clone();
        let shown = shown.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                page().borrow_mut().click_simple_job(row, ctrl, shift);
                shown(false);
            }
        }
    });
    window.on_simple_move_jobs({
        let page = page.clone();
        let shown = shown.clone();
        move |distance| {
            page().borrow_mut().move_simple_jobs(distance as isize);
            shown(false);
        }
    });
    window.on_simple_delete_jobs({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page().borrow_mut().delete_simple_jobs();
            shown(false);
        }
    });
    window.on_simple_formula_chosen({
        let page = page.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        move |index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let names = window.get_simple_formulae();
            if let Some(name) = usize::try_from(index)
                .ok()
                .and_then(|i| slint::Model::row_data(&names, i))
            {
                page().borrow_mut().choose_simple_formula(name.to_string());
                shown(false);
            }
        }
    });
    window.on_url_entered({
        let page = page.clone();
        let shown = shown.clone();
        move |text| {
            page().borrow_mut().pend_urls(&text);
            shown(false);
        }
    });
    window.on_paste_urls({
        let page = page.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        move || match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => {
                page().borrow_mut().pend_urls(&text);
                shown(false);
            }
            Err(e) => {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("Problem pasting! {e}").into());
                }
            }
        }
    });
    // a gallery page's sidebar: a row clicked selects its search (with
    // ctrl or shift, as a list selects); a double click highlights it, as
    // the highlight button does
    window.on_gallery_row_clicked({
        let page = page.clone();
        let shown = shown.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                page().borrow_mut().click_query(row, ctrl, shift);
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
            // (the one shown again: shown no longer, as the reference's
            // `_HighlightGalleryImport`, as `highlight_query` has it)
            let selected = page.borrow().gallery().and_then(|g| g.selection.one());
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
    // (the list's selected searches, or the one shown)
    let queries_of = |page: &Rc<RefCell<SearchPage>>, of_shown: bool| -> Vec<i64> {
        page.borrow().gallery().map_or_else(Vec::new, |g| {
            if of_shown {
                g.state.highlighted.into_iter().collect()
            } else {
                g.selected()
            }
        })
    };
    window.on_gallery_pause_play({
        let page = page.clone();
        let shown = shown.clone();
        move |search, of_shown| {
            let page = page();
            for queue in queries_of(&page, of_shown) {
                page.borrow_mut().pause_play_query(queue, search);
            }
            shown(false);
        }
    });
    window.on_gallery_retry({
        let page = page.clone();
        let shown = shown.clone();
        move |ignored| {
            let page = page();
            for queue in queries_of(&page, false) {
                page.borrow_mut().retry_query(queue, ignored);
            }
            shown(false);
        }
    });
    // "update selected with current options", asking first
    window.on_gallery_set_options({
        let page = page.clone();
        let shown = shown.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let Some(question) = page.borrow().set_options_question() else {
                return;
            };
            let shown = shown.clone();
            ask(Asked::Then(
                question.to_owned(),
                Rc::new(move || {
                    page.borrow_mut().set_options_to_selected();
                    shown(false);
                }),
            ));
        }
    });
    window.on_gallery_remove({
        let page = page.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let queues = page.gallery().map(gallery::GalleryView::selected);
            let queues = queues.unwrap_or_default();
            if let Some(question) = page.remove_queries_question(&queues) {
                ask(Asked::RemoveQueries(queues, question));
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
            let chosen = usize::try_from(index).ok().and_then(|i| {
                page.borrow()
                    .gallery()?
                    .selector_gugs()
                    .get(i)
                    .map(|g| (**g).clone())
            });
            if let Some((key, name, _)) = chosen {
                page.borrow_mut().set_gug(&key, &name);
                shown(false);
            }
        }
    });
    window.on_gallery_other_gugs({
        let page = page.clone();
        let shown = shown.clone();
        move |show| {
            page().borrow_mut().set_show_other_gugs(show);
            shown(false);
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
    // the gallery's or watcher's list's menu, chosen from
    window.on_importer_list_action({
        let page = page.clone();
        let shown = shown.clone();
        let ask = ask.clone();
        let weak = window.as_weak();
        let log = file_log_slot.clone();
        let open_files = open_files.clone();
        move |i| {
            let action = usize::try_from(i)
                .ok()
                .and_then(|i| importer_actions.borrow().get(i).cloned());
            let (Some(action), Some(window)) = (action, weak.upgrade()) else {
                return;
            };
            let ask = |question: String, then: Rc<dyn Fn()>| ask(Asked::Then(question, then));
            importer_list_menu::act(
                &importer_list_menu::Context {
                    window: &window,
                    page: &page(),
                    log: &log,
                    open_files: &open_files,
                    ask: &ask,
                    shown: Rc::new(shown.clone()),
                },
                &action,
            );
        }
    });
    // a gallery or watcher page's import options for its new searches or
    // watchers, in the editor
    window.on_page_import_options({
        let page = page.clone();
        let shown = shown.clone();
        let slot = folders.import_options.clone();
        move || {
            if slot.borrow().is_some() {
                return;
            }
            let page = page();
            let (caller, own) = {
                let page = page.borrow();
                if let Some(g) = page.gallery() {
                    (
                        hydrus_core::import_options::CallerType::PostUrls,
                        g.state.options.clone(),
                    )
                } else if let Some(w) = page.watchers() {
                    (
                        hydrus_core::import_options::CallerType::WatcherUrls,
                        w.state.options.clone(),
                    )
                } else {
                    return;
                }
            };
            let store = page.borrow().store().clone();
            let done: Rc<dyn Fn(hydrus_core::import_options::ImportOptionsSlice)> = {
                let page = page.clone();
                let shown = shown.clone();
                Rc::new(move |options| {
                    page.borrow_mut().set_page_import_options(options);
                    shown(false);
                })
            };
            match import_options_window::open(&store, caller, &own, &slot, done) {
                Ok(editor) => *slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the import options: {e}"),
            }
        }
    });
    // a URL downloader's or local import's own import options, in the
    // editor
    window.on_importer_import_options({
        let page = page.clone();
        let shown = shown.clone();
        let slot = folders.import_options.clone();
        move || {
            if slot.borrow().is_some() {
                return;
            }
            let page = page();
            let Some((queue, local, own)) = page
                .borrow()
                .importer()
                .map(|i| (i.queue, i.local, i.options.clone()))
            else {
                return;
            };
            let caller = if local {
                hydrus_core::import_options::CallerType::LocalImport
            } else {
                hydrus_core::import_options::CallerType::PostUrls
            };
            let store = page.borrow().store().clone();
            let done: Rc<dyn Fn(hydrus_core::import_options::ImportOptionsSlice)> = {
                let page = page.clone();
                let shown = shown.clone();
                Rc::new(move |options| {
                    page.borrow_mut().set_query_import_options(queue, &options);
                    shown(false);
                })
            };
            match import_options_window::open(&store, caller, &own, &slot, done) {
                Ok(editor) => *slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the import options: {e}"),
            }
        }
    });
    // the shown search's or watcher's own import options, in the editor
    window.on_shown_import_options({
        let page = page.clone();
        let shown = shown.clone();
        let slot = folders.import_options.clone();
        move || {
            if slot.borrow().is_some() {
                return;
            }
            let page = page();
            let found = {
                let page = page.borrow();
                if let Some(g) = page.gallery() {
                    g.state.highlighted.and_then(|q| g.query(q)).map(|q| {
                        (
                            q.queue,
                            hydrus_core::import_options::CallerType::PostUrls,
                            q.options.clone(),
                        )
                    })
                } else {
                    page.watchers().and_then(|w| {
                        w.state.highlighted.and_then(|q| w.watcher(q)).map(|r| {
                            (
                                r.queue,
                                hydrus_core::import_options::CallerType::WatcherUrls,
                                r.options.clone(),
                            )
                        })
                    })
                }
            };
            let Some((queue, caller, own)) = found else {
                return;
            };
            let store = page.borrow().store().clone();
            let done: Rc<dyn Fn(hydrus_core::import_options::ImportOptionsSlice)> = {
                let page = page.clone();
                let shown = shown.clone();
                Rc::new(move |options| {
                    page.borrow_mut().set_query_import_options(queue, &options);
                    shown(false);
                })
            };
            match import_options_window::open(&store, caller, &own, &slot, done) {
                Ok(editor) => *slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the import options: {e}"),
            }
        }
    });
    // and the shown search's file limit
    window.on_gallery_shown_limit({
        let page = page.clone();
        let shown = shown.clone();
        move |none, value| {
            let page = page();
            let queue = page.borrow().gallery().and_then(|g| g.state.highlighted);
            if let Some(queue) = queue {
                let limit = if none {
                    None
                } else {
                    u64::try_from(value).ok()
                };
                page.borrow_mut().set_query_file_limit(queue, limit);
                shown(false);
            }
        }
    });
    // a watcher page's sidebar, as a gallery page's
    window.on_watcher_row_clicked({
        let page = page.clone();
        let shown = shown.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                page().borrow_mut().click_query(row, ctrl, shift);
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
            let selected = page.borrow().watchers().and_then(|w| w.selection.one());
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
    // (the list's selected watchers, or the one shown)
    let watchers_of = |page: &Rc<RefCell<SearchPage>>, of_shown: bool| -> Vec<i64> {
        page.borrow().watchers().map_or_else(Vec::new, |w| {
            if of_shown {
                w.state.highlighted.into_iter().collect()
            } else {
                w.selected()
            }
        })
    };
    window.on_watcher_pause_play({
        let page = page.clone();
        let shown = shown.clone();
        move |checking, of_shown| {
            let page = page();
            for queue in watchers_of(&page, of_shown) {
                page.borrow_mut().pause_play_watcher(queue, checking);
            }
            shown(false);
        }
    });
    window.on_watcher_check_now({
        let page = page.clone();
        let shown = shown.clone();
        move |of_shown| {
            let page = page();
            for queue in watchers_of(&page, of_shown) {
                page.borrow_mut().check_watcher_now(queue);
            }
            shown(false);
        }
    });
    window.on_watcher_retry({
        let page = page.clone();
        let shown = shown.clone();
        move |ignored| {
            let page = page();
            for queue in watchers_of(&page, false) {
                page.borrow_mut().retry_query(queue, ignored);
            }
            shown(false);
        }
    });
    // "update selected with current options", asking first
    window.on_watcher_set_options({
        let page = page.clone();
        let shown = shown.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let Some(question) = page.borrow().set_options_question() else {
                return;
            };
            let shown = shown.clone();
            ask(Asked::Then(
                question.to_owned(),
                Rc::new(move || {
                    page.borrow_mut().set_options_to_selected();
                    shown(false);
                }),
            ));
        }
    });
    window.on_watcher_remove({
        let page = page.clone();
        let ask = ask.clone();
        move || {
            let page = page();
            let page = page.borrow();
            let queues = page.watchers().map(watcher::WatcherView::selected);
            let queues = queues.unwrap_or_default();
            if let Some(question) = page.remove_watchers_question(&queues) {
                ask(Asked::RemoveQueries(queues, question));
            }
        }
    });
    // the checker options editor, for the page's new watchers or the
    // shown watcher (advanced mode's tiny least times if the options have
    // it on)
    let edit_checker = {
        let slot = checker_options.clone();
        move |store: &hydrus_store::Store,
              current: hydrus_core::subscriptions::CheckerOptions,
              done: &Rc<dyn Fn(hydrus_core::subscriptions::CheckerOptions)>| {
            if slot.borrow().is_some() {
                return;
            }
            let advanced = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .is_ok_and(|a| a.0);
            match checker_options_window::open(&current, advanced, &slot, done) {
                Ok(window) => *slot.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the checker options: {e}"),
            }
        }
    };
    window.on_watcher_page_checker({
        let page = page.clone();
        let shown = shown.clone();
        let edit_checker = edit_checker.clone();
        move || {
            let page = page();
            let Some(current) = page.borrow().watcher_page_checker() else {
                return;
            };
            let store = page.borrow().store().clone();
            // (shown again: the selected may differ from the page now)
            let shown = shown.clone();
            edit_checker(
                &store,
                current,
                &(Rc::new(move |checker| {
                    page.borrow_mut().set_watcher_page_checker(checker);
                    shown(false);
                }) as Rc<dyn Fn(_)>),
            );
        }
    });
    window.on_watcher_checker({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            let page = page();
            let shown_queue = page.borrow().watchers().and_then(|w| w.state.highlighted);
            let Some(queue) = shown_queue else {
                return;
            };
            let Some(current) = page.borrow().watcher_checker(queue) else {
                return;
            };
            let shown = shown.clone();
            let store = page.borrow().store().clone();
            edit_checker(
                &store,
                current,
                &(Rc::new(move |checker| {
                    page.borrow_mut().set_watcher_checker(queue, checker);
                    shown(false);
                }) as Rc<dyn Fn(_)>),
            );
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
    // the media viewer on files not a page's (a duplicates rule's actioned
    // pair, say)
    *duplicates.open_viewer.borrow_mut() = Some(Rc::new({
        let weak_main = window.as_weak();
        let reveal_viewer_exit = reveal_viewer_exit.clone();
        let page = page.clone();
        let viewer = viewer.clone();
        let viewer_deletion = viewer_deletion.clone();
        let viewing = viewing.clone();
        let change_pages: ChangePages = Rc::new(change_pages.clone());
        let open_manage_notes = open_manage_notes.clone();
        let open_manage_urls = open_manage_urls.clone();
        let open_manage_ratings = open_manage_ratings.clone();
        let open_manage_times = open_manage_times.clone();
        let open_force_filetype = open_force_filetype.clone();
        let open_export_files = open_export_files.clone();
        let open_embedded_metadata = open_embedded_metadata.clone();
        let files_changed = files_changed.clone();
        let removed = removed.clone();
        let tags_changed = tags_changed.clone();
        let open_manage_tags = open_manage_tags.clone();
        move |files: Vec<HashId>, start: usize| {
            let store = page().borrow().store().clone();
            let Some(model) = MediaViewer::new(store, files, start) else {
                return;
            };
            *viewing.borrow_mut() = Some((hydrus_core::pages::PageKey::random().0, None));
            let hooks = ViewerHooks {
                deletion: viewer_deletion.clone(),
                closing_owner: viewer_closing::Owner::new(
                    None,
                    weak_main.clone(),
                    change_pages.clone(),
                    reveal_viewer_exit.clone(),
                ),
                viewing: viewing.clone(),
                removed: removed.clone(),
                tags_changed: tags_changed.clone(),
                files_changed: files_changed.clone(),
                manage_tags: Rc::new(open_manage_tags.clone()),
                manage_notes: open_manage_notes.clone(),
                manage_urls: open_manage_urls.clone(),
                manage_ratings: open_manage_ratings.clone(),
                manage_times: open_manage_times.clone(),
                force_filetype: open_force_filetype.clone(),
                export_files: open_export_files.clone(),
                embedded_metadata: open_embedded_metadata.clone(),
                change_pages: change_pages.clone(),
            };
            match open_viewer(model, &viewer, hooks) {
                Ok(window) => *viewer.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the media viewer: {e}"),
            }
        }
    }));
    window.on_thumbnail_activated({
        let weak_main = window.as_weak();
        let origin_pages = pages.clone();
        let reveal_viewer_exit = reveal_viewer_exit.clone();
        let page = page.clone();
        let viewer = viewer.clone();
        let viewer_deletion = viewer_deletion.clone();
        let viewing = viewing.clone();
        let change_pages: ChangePages = Rc::new(change_pages.clone());
        let open_manage_notes = open_manage_notes.clone();
        let open_manage_urls = open_manage_urls.clone();
        let open_manage_ratings = open_manage_ratings.clone();
        let open_manage_times = open_manage_times.clone();
        let open_force_filetype = open_force_filetype.clone();
        let open_export_files = open_export_files.clone();
        let open_embedded_metadata = open_embedded_metadata.clone();
        let files_changed = files_changed.clone();
        move |index| {
            let source_page = page();
            let original_key = origin_pages.borrow().shown().key;
            let closing_owner = viewer_closing::Owner::new(
                Some((original_key, Rc::downgrade(&source_page))),
                weak_main.clone(),
                change_pages.clone(),
                reveal_viewer_exit.clone(),
            );
            let page = source_page.borrow();
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
                deletion: viewer_deletion.clone(),
                closing_owner,
                viewing: viewing.clone(),
                removed: removed.clone(),
                tags_changed: tags_changed.clone(),
                files_changed: files_changed.clone(),
                manage_tags: Rc::new(open_manage_tags.clone()),
                manage_notes: open_manage_notes.clone(),
                manage_urls: open_manage_urls.clone(),
                manage_ratings: open_manage_ratings.clone(),
                manage_times: open_manage_times.clone(),
                force_filetype: open_force_filetype.clone(),
                export_files: open_export_files.clone(),
                embedded_metadata: open_embedded_metadata.clone(),
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
    thumbnail_navigation::bind(window, current.borrow().borrow().store());
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
    let palette_media_items: Rc<RefCell<Vec<hydrus_gui_model::command_palette::MenuItem>>> =
        Rc::default();
    window.on_thumbnail_menu_requested({
        let page = page.clone();
        let reselect = reselect.clone();
        let menu_state = menu_state.clone();
        let palette_media_items = palette_media_items.clone();
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
            let urls = (!selected.is_empty()).then(|| thumbnail_menu::urls_menu(&url_facts));
            let rearrange = thumbnail_menu::rearrange_menu(
                page.results(),
                &page.selected_items().into_iter().collect(),
                page.focused().map(|i| page.results()[i]),
            );
            // (the focused file's notes, counted in "manage")
            let notes = page
                .focused()
                .map(|i| page.results()[i])
                .and_then(|f| page.store().read(|c| hydrus_store::media::notes(c, f)).ok())
                .map(|n| n.len());
            let entries = thumbnail_menu::menu(
                &snapshot.services,
                &files,
                &selected,
                info,
                urls,
                open,
                share,
                rearrange,
                notes,
            );
            let slots = thumbnail_menu::Slots::new(&entries);
            let mut actions = Vec::new();
            let window_menu = thumbnail_menu_rows(&slots, &mut actions);
            *palette_media_items.borrow_mut() =
                command_palette_window::media_menu_items(&entries, &actions);
            *menu_state.borrow_mut() = (actions, files, url_facts);
            if let Some(window) = weak.upgrade() {
                window.set_thumbnail_menu(window_menu);
            }
        }
    });
    window.on_menu_chosen({
        let page = page.clone();
        let menu_state = menu_state.clone();
        let weak = window.as_weak();
        let change_pages = change_pages.clone();
        let shown = shown.clone();
        let open_manage_notes = open_manage_notes.clone();
        let open_manage_urls = open_manage_urls.clone();
        let open_manage_ratings = open_manage_ratings.clone();
        let open_manage_times = open_manage_times.clone();
        let open_force_filetype = open_force_filetype.clone();
        let open_export_files = open_export_files.clone();
        let open_embedded_metadata = open_embedded_metadata.clone();
        let files_changed = files_changed.clone();
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
                Action::DeletePhysically => {
                    let files = page.borrow().selected_files();
                    delete(files, media_actions::Deletion::Physically);
                }
                Action::Undelete => window.invoke_undelete_selected(),
                Action::ManageTags => window.invoke_manage_tags_selected(),
                Action::ManageNotes => {
                    let page = page.borrow();
                    if let Some(i) = page.focused() {
                        let file = page.results()[i];
                        open_manage_notes(page.store().clone(), file, files_changed.clone());
                    }
                }
                Action::ManageUrls => {
                    let page = page.borrow();
                    let files = page.selected_files();
                    if !files.is_empty() {
                        open_manage_urls(page.store().clone(), files, files_changed.clone());
                    }
                }
                Action::ManageRatings => {
                    let page = page.borrow();
                    let files = page.selected_files();
                    if !files.is_empty() {
                        open_manage_ratings(page.store().clone(), files, files_changed.clone());
                    }
                }
                Action::ManageTimes => {
                    let page = page.borrow();
                    let files = page.selected_files();
                    if !files.is_empty() {
                        open_manage_times(page.store().clone(), files, files_changed.clone());
                    }
                }
                Action::ExportFiles => {
                    let page = page.borrow();
                    open_export_files(
                        page.store().clone(),
                        page.selected_files(),
                        files_changed.clone(),
                    );
                }
                Action::EmbeddedMetadata => {
                    let page = page.borrow();
                    if let Some(index) = page.focused() {
                        open_embedded_metadata(
                            page.store().clone(),
                            vec![page.results()[index]],
                            Rc::new(|| {}),
                        );
                    }
                }
                Action::ForceFiletype => {
                    let page = page.borrow();
                    let files = page.selected_files();
                    if !files.is_empty() {
                        open_force_filetype(page.store().clone(), files, files_changed.clone());
                    }
                }
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
    let palette_change_pages: ChangePages = Rc::new(change_pages.clone());
    command_palette_window::bind(
        window,
        &command_palette,
        &pages,
        &current,
        &palette_change_pages,
        &palette_dispatcher,
        &menu_state,
        &palette_media_items,
    );
    // what the Client API asked of the pages, done, and the pages and media
    // viewer as they are, kept in the store for it
    let sync: Rc<dyn Fn()> = Rc::new({
        let session_autosave = session_autosave.clone();
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
                session_autosave.api_at(hydrus_core::TimestampMs::now().0);
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
                            show_importer(&window, &page);
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
        session_autosave,
        pages,
        current,
        rows,
        viewer,
        viewer_deletion,
        manage_tags,
        incremental_tags,
        tag_relationships,
        tag_display,
        tag_migration,
        manage_notes,
        manage_ratings,
        manage_times,
        datetime_editor,
        force_filetype,
        export_files,
        embedded_metadata,
        manage_urls,
        options,
        options_reason_child,
        options_colour_child,
        options_frame_child,
        options_banner_child,
        options_suggested_tags_slot,
        options_external_calls,
        command_palette,
        about,
        services_review,
        network_data,
        network_controls,
        services_editor,
        archive_repair,
        file_history,
        checker_options,
        session_dialog,
        tab_name_dialog,
        subscriptions,
        subscription_gallery,
        subscription_exchange,
        downloader_definitions,
        login_workflows,
        parser_editors,
        network_sessions,
        edit_subscription,
        folders,
        simple_formulae,
        file_log: file_log_slot,
        archive_delete,
        delete_files,
        filter,
        open_page,
        review_imports,
        filename_tagging,
        filename_tagging_sidecars,
        auto_resolution: duplicates.rules_editor.clone(),
        auto_resolution_reviews: duplicates.reviews.clone(),
        auto_resolution_review_filter: duplicates.review_filter.clone(),
        locations,
        favourites: favourite_dialogs,
        _autocomplete_tabs: autocomplete_tabs,
        predicate_editor,
        search_or,
        drop_files: review_files,
        sync,
        _thumbnails: thumbnails,
        _menu_titles: menu_titles,
        _popups: popup_timer,
        clipboard_monitor,
        _header_approval: header_approval,
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
        share_export: rows(&share.export.iter().cloned().collect::<Vec<_>>()),
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
        info_metadata: if info.metadata {
            id(Action::EmbeddedMetadata, thumbnail_menu::EMBEDDED_METADATA)
        } else {
            -1
        },
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
        urls_manage: id(Action::ManageUrls, "manage"),
        has_url_lists: urls.lists,
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
pub(crate) fn copy_to_clipboard(text: &str) {
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
    /// What gives text in place of the clipboard, if anything.
    static PASTER: RefCell<Option<Rc<dyn Fn() -> String>>> = RefCell::new(None);
}

/// What the system's picker is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Files that exist, several.
    Files,
    /// One folder.
    Folder,
}

/// Something that picks paths in place of the system's picker.
type Picker = Rc<dyn Fn(Pick, &str) -> Vec<std::path::PathBuf>>;

thread_local! {
    /// What picks paths in place of the system's picker, if anything.
    static PICKER: RefCell<Option<Picker>> = RefCell::new(None);
}

/// Pick paths with `picker` rather than the system's picker (for tests),
/// on this thread.
pub fn set_picker(picker: impl Fn(Pick, &str) -> Vec<std::path::PathBuf> + 'static) {
    PICKER.with(|p| *p.borrow_mut() = Some(Rc::new(picker)));
}

/// Ask the system's file or folder picker, titled `title`, for paths;
/// none if cancelled (`FileDialog`, `PickDirectory`).
pub(crate) fn pick(kind: Pick, title: &str) -> Vec<std::path::PathBuf> {
    if let Some(picker) = PICKER.with(|p| p.borrow().clone()) {
        return picker(kind, title);
    }
    let dialog = rfd::FileDialog::new().set_title(title);
    match kind {
        Pick::Files => dialog.pick_files().unwrap_or_default(),
        Pick::Folder => dialog.pick_folder().into_iter().collect(),
    }
}

/// Pick several reference exchange files with the original format filter.
pub(crate) fn pick_exchange_files(title: &str, extension: &str) -> Vec<std::path::PathBuf> {
    if let Some(picker) = PICKER.with(|p| p.borrow().clone()) {
        return picker(Pick::Files, title);
    }
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(extension, &[extension])
        .pick_files()
        .unwrap_or_default()
}

/// Pick a reference JSON export path, with the existing injected picker boundary.
pub(crate) fn pick_exchange_export() -> Option<std::path::PathBuf> {
    const TITLE: &str = "select where to save the json file";
    if let Some(picker) = PICKER.with(|p| p.borrow().clone()) {
        return picker(Pick::Files, TITLE).into_iter().next();
    }
    rfd::FileDialog::new()
        .set_title(TITLE)
        .add_filter("JSON", &["json"])
        .set_file_name("export.json")
        .save_file()
}

/// Read pasted text from `paster` rather than the clipboard (for tests),
/// on this thread.
pub fn set_paster(paster: impl Fn() -> String + 'static) {
    PASTER.with(|p| *p.borrow_mut() = Some(Rc::new(paster)));
}

/// The clipboard's text (or the paster's).
pub(crate) fn from_clipboard() -> Result<String, String> {
    clipboard_text()?.ok_or_else(|| arboard::Error::ContentNotAvailable.to_string())
}

/// Text reads distinguish an empty/non-text clipboard from an access failure.
pub(crate) fn clipboard_text() -> Result<Option<String>, String> {
    if let Some(reader) = CLIPBOARD_READER.with(|reader| reader.borrow().clone()) {
        return reader();
    }
    if let Some(paster) = PASTER.with(|p| p.borrow().clone()) {
        return Ok(Some(paster()));
    }
    match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

type ClipboardReader = Rc<dyn Fn() -> Result<Option<String>, String>>;
thread_local! {
    static CLIPBOARD_READER: RefCell<Option<ClipboardReader>> = RefCell::new(None);
}

/// Substitute clipboard reads, including unavailable text and access failures,
/// on this thread for deterministic monitoring tests.
pub fn set_clipboard_reader(reader: impl Fn() -> Result<Option<String>, String> + 'static) {
    CLIPBOARD_READER.with(|slot| *slot.borrow_mut() = Some(Rc::new(reader)));
}

/// Restore normal clipboard reads after an injected transport failure.
pub fn clear_clipboard_reader() {
    CLIPBOARD_READER.with(|slot| {
        let _ = slot.borrow_mut().take();
    });
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
    /// Removing a downloader page's searches or watchers, asking this.
    RemoveQueries(Vec<i64>, String),
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
            | Self::RemoveQueries(_, question)
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
            Self::LockSearch(_) | Self::ClosePage(..) | Self::RemoveQueries(..) => Ok(()),
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

/// Opens manage notes on a file, calling the hook given once applied.
type OpenManageNotes = Rc<dyn Fn(Arc<hydrus_store::Store>, HashId, Rc<dyn Fn()>)>;

/// Opens a manage dialog (urls, ratings) on some files, calling the hook
/// given once applied.
type OpenOnFiles = Rc<dyn Fn(Arc<hydrus_store::Store>, Vec<HashId>, Rc<dyn Fn()>)>;

/// What a viewer tells its page of, and how it opens manage tags and
/// notes.
struct ViewerHooks {
    deletion: delete_files_window::Slot,
    closing_owner: Rc<viewer_closing::Owner>,
    /// The viewer and the file it shows, for the Client API.
    viewing: Viewing,
    removed: Removed,
    tags_changed: Rc<dyn Fn()>,
    /// A file was archived or rated, say: the thumbnails are drawn again.
    files_changed: Rc<dyn Fn()>,
    manage_tags: OpenManageTags,
    manage_notes: OpenManageNotes,
    manage_urls: OpenOnFiles,
    manage_ratings: OpenOnFiles,
    manage_times: OpenOnFiles,
    force_filetype: OpenOnFiles,
    export_files: OpenOnFiles,
    embedded_metadata: OpenOnFiles,
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
        deletion: viewer_delete,
        closing_owner,
        viewing,
        removed,
        tags_changed,
        files_changed,
        manage_tags,
        manage_notes,
        manage_urls,
        manage_ratings,
        manage_times,
        force_filetype,
        export_files,
        embedded_metadata,
        change_pages,
    } = hooks;
    delete_files_window::cancel(&viewer_delete);
    let window = MediaViewerWindow::new()?;
    let viewing_stats = viewing_tracking::CanvasTracker::new(
        model.store().clone(),
        hydrus_core::CanvasType::MediaViewer,
    );
    let model = Rc::new(RefCell::new(model));
    viewer_eye_menu::bind(
        &window,
        model.borrow().store(),
        slot,
        Rc::new({
            let model = model.clone();
            move || model.borrow().current()
        }),
    );
    let playback = playback::Playback::for_store(model.borrow().store().clone());
    let animator = animation::Animator::for_store(model.borrow().store().clone());
    // (where it opens, and how big: fullscreen, by hydrus's default)
    let settings_frame = windows::settings(model.borrow().store()).media_viewer;
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = zoom_window!(window, settings.clone());
    zoomed.set_resize_policy({
        let store = model.borrow().store().clone();
        move || {
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::ViewerCanvasSettings>)
                .unwrap_or_default()
                .recenter_on_resize
        }
    });
    let native_cursor = viewer_cursor::NativeCursor::new(&window, model.borrow().store().clone());
    window.on_cursor_menu_starting({
        let native_cursor = native_cursor.clone();
        move || native_cursor.menu_starting()
    });
    window.on_cursor_menu_returned({
        let native_cursor = native_cursor.clone();
        move || native_cursor.menu_returned()
    });
    let native_focus = viewer_focus::NativeFocus::new(&window);
    window.on_presentation_settings_changed({
        let native_cursor = native_cursor.clone();
        let native_focus = native_focus.clone();
        let weak = window.as_weak();
        let model = model.clone();
        move || {
            native_focus.watch_native();
            native_cursor.refresh();
            if let Some(window) = weak.upgrade() {
                let model = model.borrow();
                viewer_presentation::refresh(&window, model.store(), model.current());
            }
        }
    });
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
    window.on_refresh_tags({
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                let tags = model
                    .borrow()
                    .tag_rows()
                    .into_iter()
                    .map(|(row, rgb)| list_text(&row, rgb))
                    .collect::<Vec<_>>();
                window.set_tags(ModelRc::new(VecModel::from(tags)));
            }
        }
    });
    // Display choices reach an already-open viewer as well as navigation.
    let show_url_links: Rc<dyn Fn()> = Rc::new({
        let model = model.clone();
        let weak = window.as_weak();
        let previous = RefCell::new(Vec::<(String, String)>::new());
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            match downloader_display_window::file_links(model.store(), model.current()) {
                Ok(links) if *previous.borrow() != links => {
                    window.set_url_links(ModelRc::new(VecModel::from(
                        links
                            .iter()
                            .map(|(label, url)| ViewerUrlRow {
                                label: label.as_str().into(),
                                url: url.as_str().into(),
                            })
                            .collect::<Vec<_>>(),
                    )));
                    *previous.borrow_mut() = links;
                }
                Ok(_) => {}
                Err(e) => eprintln!("could not read media viewer URLs: {e}"),
            }
        }
    });
    window.on_url_clicked({
        let weak = window.as_weak();
        move |index| {
            if let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index))
                && let Some(link) = window.get_url_links().row_data(index)
                && hydrus_core::url::functions::check_full_url(link.url.as_str()).is_ok()
            {
                launch(link.url.as_str());
            }
        }
    });
    window.on_note_copy_requested({
        let model = model.clone();
        let weak = window.as_weak();
        move |index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.window().is_visible() {
                return;
            }
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let Some(note) = window.get_notes().row_data(index) else {
                return;
            };
            let model = model.borrow();
            match model
                .store()
                .read(hydrus_store::settings::get::<hydrus_store::settings::NotePreferences>)
            {
                Ok(preferences) => copy_to_clipboard(&notes_editor::hover_copy(
                    note.name.as_str(),
                    note.text.as_str(),
                    preferences.hover_text_only,
                )),
                Err(error) => eprintln!("could not read note copy preference: {error}"),
            }
        }
    });
    // the file's info line and buttons, in the top hover frame, and its
    // notes
    let show_info = {
        let model = model.clone();
        let weak = window.as_weak();
        let show_url_links = show_url_links.clone();
        move || {
            show_url_links();
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            viewer_presentation::refresh(&window, model.store(), model.current());
            let shown = viewer::shown(model.store(), model.current());
            window.set_info_line(shown.line.into());
            window.set_location_strings(ModelRc::new(VecModel::from(
                shown
                    .locations
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            )));
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
    viewer_tag_wheel::bind(&window, slot);
    let last_tag_file = Rc::new(std::cell::Cell::new(None));
    let show = {
        let last_tag_file = last_tag_file.clone();
        let viewing_stats = viewing_stats.clone();
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
            if !viewing_stats.active() {
                return;
            }
            let model = model.borrow();
            viewing_stats.show(Some(model.current()));
            if let Some((_, file)) = viewing.borrow_mut().as_mut() {
                *file = Some(model.current());
            }
            window.set_caption(model.caption().into());
            if last_tag_file.replace(Some(model.current())) != Some(model.current()) {
                window.invoke_tag_media_changed();
            }
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
    viewer_drag::bind(&window, slot);
    window.on_zoom_switch_requested({
        let zoomed = zoomed.clone();
        // The reference captures this command when the top hover is built.
        let preferences = model
            .borrow()
            .store()
            .read(hydrus_store::settings::get::<hydrus_store::settings::ViewerPlaybackSettings>)
            .unwrap_or_default();
        move |x, y| zoomed.switch_with_policy(preferences.zoom_switch, Some((x as i32, y as i32)))
    });
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
        let files_changed = files_changed.clone();
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
            files_changed();
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
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_media_playing(match scanbar.get() {
                    Some((_, false)) => !playback.paused(),
                    Some((_, true)) => animator.status().is_some_and(|status| !status.paused),
                    None => false,
                });
            }
            match scanbar.get() {
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
                let nub = weak
                    .upgrade()
                    .map_or(scanbar::NUB_WIDTH, |w| w.get_seek_nub_width());
                let to = bar.seek_to_with_nub(x, width, nub);
                playback.seek_ms(to);
                show_scanbar(to);
            }
            Some((bar, true)) => {
                // (the frame's text follows once it is shown)
                let nub = weak
                    .upgrade()
                    .map_or(scanbar::NUB_WIDTH, |w| w.get_seek_nub_width());
                let index = bar.frame_at_with_nub(x, width, nub);
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
        let show_url_links = show_url_links.clone();
        let url_ticks = Cell::new(0_u8);
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
            url_ticks.set((url_ticks.get() + 1) % 10);
            if url_ticks.get() == 0 {
                show_url_links();
            }
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
        let model = model.clone();
        let viewer_delete = viewer_delete.clone();
        let viewer_slot = slot.clone();
        let remove_file = remove_file.clone();
        let show_info = show_info.clone();
        move |asked: ViewerAsked| {
            if let Some(window) = weak.upgrade() {
                if let ViewerAsked::Delete(deletion, file) = &asked {
                    let store = model.borrow().store().clone();
                    if store
                        .read(
                            hydrus_store::settings::get::<
                                hydrus_store::settings::DeletionPreferences,
                            >,
                        )
                        .unwrap_or_default()
                        .advanced
                    {
                        let guard: delete_files_window::Guard = Rc::new({
                            let weak = weak.clone();
                            let viewer_slot = Rc::downgrade(&viewer_slot);
                            move || {
                                weak.upgrade().is_some_and(|window| {
                                    viewer_slot.upgrade().is_some_and(|slot| {
                                        slot.borrow().as_ref().is_some_and(|current| {
                                            std::ptr::eq(current.window(), window.window())
                                        })
                                    })
                                })
                            }
                        });
                        let applied = Rc::new({
                            let file = *file;
                            let store = store.clone();
                            let model = model.clone();
                            let remove_file = remove_file.clone();
                            let show_info = show_info.clone();
                            move || {
                                let location = model.borrow().location().clone();
                                if media_actions::still_in(&store, &location, &[file]).is_empty() {
                                    remove_file(file);
                                } else {
                                    show_info();
                                }
                            }
                        });
                        let suggested = media_actions::suggested_action(&store, deletion);
                        if let Err(error) = delete_files_window::open(
                            &viewer_delete,
                            &store,
                            &[*file],
                            suggested.as_ref(),
                            media_actions::DELETE_REASON,
                            guard,
                            applied,
                        ) {
                            eprintln!("could not open deletion: {error}");
                        }
                        return;
                    }
                }
                let question = match &asked {
                    ViewerAsked::Delete(deletion, _) => deletion.question(1),
                    ViewerAsked::OpenUrls(urls) => Asked::OpenUrls(urls.clone()).question(),
                };
                let auto_accept = if let ViewerAsked::Delete(deletion, file) = &asked {
                    !media_actions::confirm_deletion(model.borrow().store(), &[*file], deletion)
                } else {
                    false
                };
                window.set_question(question.into());
                *pending.borrow_mut() = Some(asked);
                if auto_accept {
                    window.invoke_answer(true);
                }
            }
        }
    };
    // a change to a file (archiving, say), its info line shown again
    let change_file: Rc<dyn Fn(FileChange, HashId)> = Rc::new({
        let model = model.clone();
        let show_info = show_info.clone();
        let files_changed = files_changed.clone();
        move |change, file| {
            let changed = change(model.borrow().store(), &[file]);
            match changed {
                Ok(()) => {
                    show_info();
                    files_changed();
                }
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
        let manage_notes = manage_notes.clone();
        let manage_urls = manage_urls.clone();
        let manage_ratings = manage_ratings.clone();
        let manage_times = manage_times.clone();
        let force_filetype = force_filetype.clone();
        let export_files = export_files.clone();
        let embedded_metadata = embedded_metadata.clone();
        let show = show.clone();
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
                Action::ManageNotes => {
                    let store = model.borrow().store().clone();
                    manage_notes(store, file, Rc::new(show.clone()));
                }
                Action::ManageUrls => {
                    let store = model.borrow().store().clone();
                    manage_urls(store, vec![file], Rc::new(show.clone()));
                }
                Action::ManageRatings => {
                    let store = model.borrow().store().clone();
                    manage_ratings(store, vec![file], Rc::new(show.clone()));
                }
                Action::ManageTimes => {
                    let store = model.borrow().store().clone();
                    manage_times(store, vec![file], Rc::new(show.clone()));
                }
                Action::ExportFiles => {
                    let store = model.borrow().store().clone();
                    export_files(store, vec![file], Rc::new(show.clone()));
                }
                Action::EmbeddedMetadata => {
                    embedded_metadata(model.borrow().store().clone(), vec![file], Rc::new(|| {}));
                }
                Action::ForceFiletype => {
                    let store = model.borrow().store().clone();
                    force_filetype(store, vec![file], Rc::new(show.clone()));
                }
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
            let (deletion, file) = {
                let model = model.borrow();
                (
                    media_actions::deletion(model.store(), model.location(), &[model.current()]),
                    model.current(),
                )
            };
            if let Some(deletion) = deletion {
                ask(ViewerAsked::Delete(deletion, file));
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
        let viewing_stats = viewing_stats.clone();
        let native_cursor = native_cursor.clone();
        let zoomed = zoomed.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let viewing = viewing.clone();
        let model = model.clone();
        let viewer_delete = viewer_delete.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let current = slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
            viewing_stats.close();
            // Own resources belong to this viewer, even after another viewer
            // occupies the shared slot. Its close must not leave a shown Slint
            // component retaining its renderer or touch the successor's slot.
            native_cursor.close();
            scanning.stop();
            moving.stop();
            playback.close();
            animator.stop();
            zoomed.close();
            if current {
                // Save the native geometry/state before hiding this window.
                windows::save_named(window.window(), &store, "media_viewer");
            }
            let _ = window.hide();
            if !current {
                return;
            }
            delete_files_window::cancel(&viewer_delete);
            let exit = model.borrow().exit_media();
            closing_owner.closed(&store, exit);
            viewing.borrow_mut().take();
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
    native_focus.watch_native();
    native_cursor.watch_native();
    Ok(window)
}

/// A rating control as the viewer draws it.
pub(crate) fn rating_row(control: &ratings::Control) -> RatingRow {
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
    tab_presentation::show(window, pages);
    let rows: Vec<TabRow> = pages
        .tabs()
        .into_iter()
        .zip(pages.tab_labels())
        .map(|(tabs, labels)| {
            let names: Vec<SharedString> = labels.iter().map(|n| n.as_str().into()).collect();
            TabRow {
                keys: ModelRc::new(VecModel::from(
                    tabs.keys
                        .iter()
                        .map(|key| SharedString::from(key.to_hex()))
                        .collect::<Vec<_>>(),
                )),
                parent: tabs
                    .parent
                    .map(|key| key.to_hex())
                    .unwrap_or_default()
                    .into(),
                names: ModelRc::new(VecModel::from(names)),
                full_names: ModelRc::new(VecModel::from(
                    tabs.names
                        .iter()
                        .map(|name| {
                            name.lines()
                                .flat_map(str::chars)
                                .take(256)
                                .collect::<String>()
                                .into()
                        })
                        .collect::<Vec<_>>(),
                )),
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
fn show_importer(window: &MainWindow, page: &SearchPage) {
    let Some(importer) = page.importer() else {
        window.set_simple_downloader(false);
        return;
    };
    show_simple_downloader(window, page, importer);
    window.set_local_import(importer.local);
    window.set_import_action(importer.live.files_status.as_str().into());
    window.set_import_status(importer.files_status().into());
    window.set_import_options_label(
        edit_subscription::import_options_label(&importer.options).into(),
    );
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

/// A simple downloader page's parsing box: its status line and pause, its
/// jobs waiting, and its formulae.
fn show_simple_downloader(window: &MainWindow, page: &SearchPage, importer: &page::Importer) {
    let Some(simple) = &importer.simple else {
        window.set_simple_downloader(false);
        return;
    };
    window.set_simple_downloader(true);
    window.set_simple_parser_status(
        gallery::live_line(
            &importer.live.gallery_status,
            importer.gallery_paused,
            false,
        )
        .into(),
    );
    window.set_simple_queue_paused(importer.gallery_paused);
    let selected = page.simple_selected();
    let rows: Vec<TableRow> = simple
        .pending
        .iter()
        .enumerate()
        .map(|(i, job)| table_row(&[simple_downloader::job_label(job)], selected.contains(&i)))
        .collect();
    window.set_simple_jobs(ModelRc::new(VecModel::from(rows)));
    window.set_simple_job_selected(!selected.is_empty());
    let formulae: hydrus_store::settings::SimpleDownloaderFormulae = page
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let (names, index) =
        simple_downloader::formula_choices(&formulae.formulae, &simple.formula_name);
    let names: Vec<SharedString> = names.into_iter().map(Into::into).collect();
    window.set_simple_formulae(ModelRc::new(VecModel::from(names)));
    window.set_simple_formula(index.and_then(|i| i32::try_from(i).ok()).unwrap_or(0));
}

/// A list's row, its cells and whether it is selected.
fn table_row(cells: &[String], selected: bool) -> TableRow {
    let cells: Vec<SharedString> = cells.iter().map(|c| c.as_str().into()).collect();
    TableRow {
        cells: ModelRc::new(VecModel::from(cells)),
        selected,
    }
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
    let rows: Vec<TableRow> = gallery
        .queries
        .iter()
        .map(|q| {
            let cells = q.row(
                highlighted == Some(q.queue),
                &gallery.settings,
                gallery.short_summary,
                now,
            );
            table_row(&cells, gallery.selection.is_selected(q.queue))
        })
        .collect();
    window.set_gallery_rows(ModelRc::new(VecModel::from(rows)));
    window.set_gallery_sort_column(i32::try_from(gallery.sort.0.index()).unwrap_or(-1));
    window.set_gallery_ascending(gallery.sort.1);
    let (top_status, bottom_status) = gallery::totals(&gallery.queries);
    let selected: Vec<&gallery::GalleryQuery> = gallery
        .selected()
        .into_iter()
        .filter_map(|s| gallery.query(s))
        .collect();
    let has = |status| {
        selected
            .iter()
            .any(|q| q.files.get(&status).is_some_and(|&n| n > 0))
    };
    let one = gallery.selection.one();
    let own = gallery.gallery();
    // (the downloader among those offered; one not found, or none, after
    // them, as the reference's selector labels it)
    let offered = gallery.selector_gugs();
    let found = offered
        .iter()
        .position(|g| g.0 == own.gug_key)
        .or_else(|| offered.iter().position(|g| g.1 == own.gug_name))
        .filter(|_| !own.gug_name.is_empty());
    let mut gug_names: Vec<SharedString> = offered.iter().map(|g| g.1.as_str().into()).collect();
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
        has_selection: !selected.is_empty(),
        can_highlight: one.is_some() && one != highlighted,
        can_clear_highlight: highlighted.is_some(),
        can_retry_ignored: has(SeedStatus::Vetoed),
        can_retry_failed: has(SeedStatus::Error),
        can_set_options: page.selected_options_differ(),
        import_options: edit_subscription::import_options_label(&gallery.state.options).into(),
        gug_names: ModelRc::new(VecModel::from(gug_names)),
        gug_index: i32::try_from(gug_index).unwrap_or(0),
        show_other_gugs: gallery.show_other_gugs,
        has_other_gugs: gallery
            .gugs
            .iter()
            .any(|g| !gallery.gug_keys_to_display.contains(&g.0)),
        initial_search_text: found
            .map(|i| offered[i].2.as_str())
            .unwrap_or_default()
            .into(),
        no_limit: own.file_limit.is_none(),
        file_limit: own
            .file_limit
            .map_or(2000, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        highlighted: shown.is_some(),
        shown_no_limit: shown.is_none_or(|q| q.file_limit.is_none()),
        shown_file_limit: shown
            .and_then(|q| q.file_limit)
            .map_or(2000, |n| i32::try_from(n).unwrap_or(i32::MAX)),
        shown_import_options: shown
            .map(|q| edit_subscription::import_options_label(&q.options))
            .unwrap_or_default()
            .into(),
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
    let rows: Vec<TableRow> = view
        .rows(now)
        .iter()
        .zip(&view.watchers)
        .map(|(cells, w)| table_row(cells, view.selection.is_selected(w.queue)))
        .collect();
    window.set_watcher_rows(ModelRc::new(VecModel::from(rows)));
    window.set_watcher_sort_column(i32::try_from(view.sort.0.index()).unwrap_or(-1));
    window.set_watcher_ascending(view.sort.1);
    let (top_status, bottom_status) = watcher::totals(&view.watchers);
    let highlighted = view.state.highlighted;
    let selected: Vec<&watcher::WatcherRow> = view
        .selected()
        .into_iter()
        .filter_map(|s| view.watcher(s))
        .collect();
    let has = |status| {
        selected
            .iter()
            .any(|w| w.files.get(&status).is_some_and(|&n| n > 0))
    };
    let one = view.selection.one();
    let shown = highlighted.and_then(|h| view.watcher(h));
    window.set_watcher_data(WatcherData {
        top_status: top_status.into(),
        bottom_status: bottom_status.into(),
        has_selection: !selected.is_empty(),
        can_highlight: one.is_some() && one != highlighted,
        can_clear_highlight: highlighted.is_some(),
        can_retry_ignored: has(SeedStatus::Vetoed),
        can_retry_failed: has(SeedStatus::Error),
        can_set_options: page.selected_options_differ(),
        import_options: edit_subscription::import_options_label(&view.state.options).into(),
        highlighted: shown.is_some(),
        shown_import_options: shown
            .map(|w| edit_subscription::import_options_label(&w.options))
            .unwrap_or_default()
            .into(),
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

fn refresh(window: &MainWindow, page: &SearchPage) {
    let gui = page
        .store()
        .read(hydrus_store::settings::get::<hydrus_store::settings::GuiSettings>)
        .unwrap_or_default();
    window.set_window_title(
        format!(
            "{} {}",
            gui.application_display_name,
            env!("CARGO_PKG_VERSION")
        )
        .into(),
    );
    window.set_note(page.note().unwrap_or_default().into());
    let importer = page.importer();
    window.set_importing(importer.is_some());
    show_importer(window, page);
    window.set_gallery_page(page.gallery().is_some());
    show_gallery(window, page);
    window.set_watcher_page(page.watchers().is_some());
    show_watchers(window, page);
    // (only a page with a search can save or load one)
    window.set_can_favourite(page.note().is_none());
    window.set_can_filter(page.duplicates().is_some());
    window.set_can_lock_search(page.note().is_none());
    window.set_synchronised(page.synchronised());
    let presentation = page.autocomplete().presentation_settings();
    window.set_active_predicate_rows(presentation.active_predicate_rows as i32);
    window.set_autocomplete_rows(presentation.autocomplete_rows as i32);
    window.set_float_autocomplete(presentation.float_autocomplete);
    let snapshot = page.store().snapshot();
    window.set_location_label(domains::location_label(&snapshot.services, page.location()).into());
    let tags = page.tag_context();
    window.set_tags_label(domains::tag_label(&snapshot.services, tags).into());
    window.set_include_current(tags.include_current);
    window.set_include_pending(tags.include_pending);
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
    window.set_autocomplete_tab(i32::try_from(autocomplete.tab().index()).unwrap_or(0));
    window.set_or_active(page.or_terms().is_some());
    window.set_advanced_or_visible(
        page.store()
            .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
            .unwrap_or_default()
            .0,
    );
    window.set_or_rewind_visible(page.or_terms().is_some_and(|terms| terms.len() > 1));
    let suggestions: Vec<ListText> = autocomplete
        .suggestions()
        .iter()
        .map(|s| list_text(&s.label, colours.predicate_text(&s.predicate)))
        .collect();
    window.set_suggestion_selected(ModelRc::new(VecModel::from(autocomplete.selected())));
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
    window.set_selection_tags_title(page.tag_list_title().into());
    window.set_error(page.error().unwrap_or_default().into());
    window.set_status(page.status().into());
    let sort = page.sort();
    window.set_sort_cog_visible(!hydrus_gui_model::sort_cog::groups(sort).is_empty());
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

pub mod client_api_admin_window;

pub mod network_sessions_window;

pub mod network_data_window;

pub mod network_job_control;

pub mod namespace_sorts_window;
