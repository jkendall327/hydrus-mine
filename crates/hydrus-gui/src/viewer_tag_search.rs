//! Canonical single-tag hover searches guarded by the file and live canvas owner.
use crate::{MediaViewer, MediaViewerWindow, list_text, viewing_tracking::CanvasTracker};
use hydrus_core::search::predicate::Predicate;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

pub(crate) type Launch = Rc<dyn Fn(hydrus_search::LocationContext, Vec<Vec<Predicate>>)>;

// Private identity within this viewer's Store, not a displayed content digest.
// The canvas/owner guards keep tokens from retired viewers out of successors.
fn current_file_token(model: &MediaViewer) -> String {
    model.current().0.to_string()
}

pub(crate) fn refresh(window: &MediaViewerWindow, model: &MediaViewer) {
    let entries = model.tag_entries();
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
}

pub(crate) fn bind(
    window: &MediaViewerWindow,
    model: Rc<RefCell<MediaViewer>>,
    tracker: CanvasTracker,
    owner: Rc<dyn Fn() -> bool>,
    deletion: crate::delete_files_window::Slot,
    launch: Launch,
) {
    let weak = window.as_weak();
    window.on_tag_search_requested(move |file, tag| {
        if !tracker.active() || !owner() || deletion.borrow().is_some() {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !window.get_question().is_empty()
            || !window.get_warning().is_empty()
            || window.get_period_asked()
        {
            return;
        }
        let model = model.borrow();
        if file.as_str() != current_file_token(&model)
            || file != window.get_tag_file()
            || !model
                .tag_entries()
                .iter()
                .any(|(canonical, _, _)| canonical == tag.as_str())
        {
            return;
        }
        let Some(tag) = hydrus_core::Tag::new(tag.as_str()) else {
            return;
        };
        let location = model.location().clone();
        drop(model);
        launch(
            location,
            vec![vec![Predicate::Tag {
                tag,
                inclusive: true,
            }]],
        );
    });
}
