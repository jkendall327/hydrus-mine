//! Viewer-close focus actions, in the reference's notification order.
use hydrus_store::settings::ViewerClosingSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReselectPage,
    SelectExitMedia,
    ActivateFocusing,
    ActivateDebug,
}

/// Normal exit signals reach a surviving original media panel only when
/// there is exit media. The debug exit signal always reaches the main GUI.
pub fn actions(settings: &ViewerClosingSettings, owned_exit: bool) -> Vec<Action> {
    let mut actions = Vec::new();
    if owned_exit {
        if settings.reselect_page {
            actions.push(Action::ReselectPage);
        }
        if settings.select_exit_media {
            actions.push(Action::SelectExitMedia);
        }
        if settings.activate_focusing && (settings.reselect_page || settings.select_exit_media) {
            actions.push(Action::ActivateFocusing);
        }
    }
    if settings.activate_always {
        actions.push(Action::ActivateDebug);
    }
    actions
}
