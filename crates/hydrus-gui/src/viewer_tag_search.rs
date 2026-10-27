//! The media viewer's tags hover list (the reference's
//! `ListBoxTagsMediaHoverFrame`): its selection, presses, double-clicks and
//! keys, its middle-click searches and its right-click tag menu, each
//! guarded by the file shown and the live canvas owner.
use crate::{MediaViewer, MediaViewerWindow, list_text, viewing_tracking::CanvasTracker};
use hydrus_core::search::predicate::Predicate;
use hydrus_gui_model::selection_tags::{self, Command, Place};
use hydrus_gui_model::tag_list_box::{Key, Outcome};
use slint::{ComponentHandle as _, Model as _, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

pub(crate) type Launch = Rc<dyn Fn(hydrus_search::LocationContext, Vec<Vec<Predicate>>)>;
/// Opens a search page for predicates from the viewer's location.
type Search = Rc<dyn Fn(&MediaViewer, Vec<Predicate>)>;

// Private identity within this viewer's Store, not a displayed content digest.
// The canvas/owner guards keep tokens from retired viewers out of successors.
fn current_file_token(model: &MediaViewer) -> String {
    model.current().0.to_string()
}

/// Show the current file's tags in the hover, keeping the selected ones that
/// are still there selected (`_UpdateTerms`).
pub(crate) fn refresh(window: &MediaViewerWindow, model: &MediaViewer) {
    let entries = model.tag_entries();
    let rows: Vec<(String, String)> = entries
        .iter()
        .map(|(tag, text, _)| (tag.clone(), text.clone()))
        .collect();
    model.tag_selection().borrow_mut().set_rows(&rows);
    window.set_tags(ModelRc::new(VecModel::from(
        entries
            .iter()
            .map(|(_, text, rgb)| list_text(text, *rgb))
            .collect::<Vec<_>>(),
    )));
    window.set_tag_identities(ModelRc::new(VecModel::from(
        entries
            .into_iter()
            .map(|(tag, _, _)| tag.into())
            .collect::<Vec<slint::SharedString>>(),
    )));
    window.set_tag_file(current_file_token(model).into());
    show_selection(window, model);
}

fn show_selection(window: &MediaViewerWindow, model: &MediaViewer) {
    window.set_tag_selected(ModelRc::new(VecModel::from(
        model.tag_selection().borrow().mask(),
    )));
}

#[allow(clippy::too_many_lines)] // (one list's callbacks)
pub(crate) fn bind(
    window: &MediaViewerWindow,
    model: &Rc<RefCell<MediaViewer>>,
    tracker: &CanvasTracker,
    owner: &Rc<dyn Fn() -> bool>,
    deletion: crate::delete_files_window::Slot,
    launch: Launch,
) {
    // whether the hover may act: the viewer live and nothing it owns asking
    let valid: Rc<dyn Fn(&MediaViewerWindow) -> bool> = Rc::new({
        let tracker = tracker.clone();
        let owner = owner.clone();
        move |window| {
            tracker.active()
                && owner()
                && deletion.borrow().is_none()
                && window.get_question().is_empty()
                && window.get_warning().is_empty()
                && !window.get_period_asked()
        }
    });
    // a press on the file the hover shows (not one gone since)
    let current = |window: &MediaViewerWindow, model: &MediaViewer, file: &str| {
        file == current_file_token(model) && file == window.get_tag_file().as_str()
    };
    let search: Search = Rc::new(move |model, predicates| {
        if !predicates.is_empty() {
            launch(model.location().clone(), vec![predicates]);
        }
    });
    // the hover's own tag menu, as the main window's
    let menu = crate::write_tag_menu::TagMenu::new(
        model.borrow().store().clone(),
        Rc::new({
            let tracker = tracker.clone();
            let owner = owner.clone();
            move || tracker.active() && owner()
        }),
        Rc::new(|_| {}),
        Rc::new({
            let weak = window.as_weak();
            move || {
                if let Some(window) = weak.upgrade() {
                    window.invoke_refresh_tags();
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |question: &str| {
                if let Some(window) = weak.upgrade() {
                    window.set_tag_menu_question(question.into());
                }
            }
        }),
        Rc::new(|error: &str| eprintln!("{error}")),
    );
    crate::write_tag_menu::bind!(window, menu);
    // what the list's own menu entries do, for the tags selected as it opened
    let command: Rc<dyn Fn(Command, Vec<String>)> = Rc::new({
        let model = model.clone();
        let weak = window.as_weak();
        move |command, tags| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            match command {
                Command::HideTags | Command::HideNamespaces => {
                    let (_, slices) =
                        selection_tags::hide_question(&tags, command == Command::HideNamespaces);
                    let view = match model.borrow().tag_display_type() {
                        hydrus_core::tag_presentation::TagDisplayType::SelectionList => {
                            hydrus_store::tag_display::TagView::SelectionList
                        }
                        _ => hydrus_store::tag_display::TagView::SingleMedia,
                    };
                    let store = model.borrow().store().clone();
                    if let Err(error) = selection_tags::hide(&store, view, &slices) {
                        eprintln!("could not hide tags: {error}");
                    }
                    window.invoke_refresh_tags();
                }
                Command::Display(display) => {
                    model.borrow_mut().set_tag_display_type(display);
                    window.invoke_refresh_tags();
                }
                // (the hover has no search and no page to select in)
                _ => {}
            }
        }
    });
    window.on_tag_pressed({
        let weak = window.as_weak();
        let model = model.clone();
        let valid = valid.clone();
        let search = search.clone();
        move |file, row, button, ctrl, shift, x, y| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !valid(&window) {
                return;
            }
            let model = model.borrow();
            if !current(&window, &model, &file) {
                return;
            }
            model
                .tag_selection()
                .borrow_mut()
                .press(usize::try_from(row).ok(), ctrl, shift);
            show_selection(&window, &model);
            match button {
                // middle: a new search page for the selection
                2 => {
                    let predicates = model.tag_selection().borrow().middle_click(shift);
                    search(&model, predicates);
                }
                // right: the menu
                1 => {
                    let store = model.store().clone();
                    let advanced = store
                        .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                        .unwrap_or_default()
                        .0;
                    let defaults: hydrus_store::settings::SearchDefaults =
                        store.read(hydrus_store::settings::get).unwrap_or_default();
                    let place = Place {
                        counts: false,
                        search: None,
                        select: false,
                        display: model.tag_display_type(),
                        advanced,
                        location: model.location().clone(),
                        tags: hydrus_search::TagContext::new(defaults.tag_service, true, true),
                    };
                    let viewing: hydrus_store::settings::FileViewingStatistics =
                        store.read(hydrus_store::settings::get).unwrap_or_default();
                    let text = hydrus_search::TextContext::from_store(
                        &store.snapshot().services,
                        &viewing,
                    );
                    let entries = model.tag_selection().borrow().menu(&store, &place, &text);
                    let captured = model.tag_selection().borrow().selected_tags();
                    let command = command.clone();
                    menu.open_owned(
                        &entries,
                        x,
                        y,
                        Rc::new(move |id| {
                            if let Some(chosen) = Command::from_id(id) {
                                command(chosen, captured.clone());
                            }
                        }),
                    );
                }
                _ => {}
            }
        }
    });
    // a double-click (or enter) manages the file's tags
    // (`ListBoxTagsMediaHoverFrame._Activate`)
    window.on_tag_double_clicked({
        let weak = window.as_weak();
        let model = model.clone();
        let valid = valid.clone();
        move |file, row, ctrl, _| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !valid(&window) || !current(&window, &model.borrow(), &file) {
                return;
            }
            if ctrl && let Ok(row) = usize::try_from(row) {
                model
                    .borrow()
                    .tag_selection()
                    .borrow_mut()
                    .reselect_for_double_click(row);
                show_selection(&window, &model.borrow());
            }
            window.invoke_manage_tags();
        }
    });
    window.on_tag_key({
        let weak = window.as_weak();
        let model = model.clone();
        let valid = valid.clone();
        move |text, ctrl, shift| {
            let Some(window) = weak.upgrade() else {
                return false;
            };
            if !valid(&window) {
                return false;
            }
            let Some(key) = Key::from_text(&text) else {
                return false;
            };
            // (a page holds what the hover shows)
            let height = window
                .window()
                .size()
                .to_logical(window.window().scale_factor())
                .height;
            let rows = (height / window.get_tag_row_height()).floor();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let page = rows.max(1.0) as usize;
            let outcome = model
                .borrow()
                .tag_selection()
                .borrow_mut()
                .key(key, ctrl, shift, page);
            match outcome {
                Outcome::Ignored => return false,
                Outcome::Taken => show_selection(&window, &model.borrow()),
                Outcome::Copy(text) => {
                    if !text.is_empty() {
                        crate::copy_to_clipboard(&text);
                    }
                }
                Outcome::Activate => window.invoke_manage_tags(),
            }
            true
        }
    });
    // a middle press on the row showing `tag` in `file` (as the hover sends
    // its presses, for callers that name the tag)
    let weak = window.as_weak();
    window.on_tag_search_requested(move |file, tag| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let row = window
            .get_tag_identities()
            .iter()
            .position(|t| t == tag)
            .and_then(|i| i32::try_from(i).ok());
        if let Some(row) = row {
            window.invoke_tag_pressed(file, row, 2, false, false, 0.0, 0.0);
        }
    });
}
