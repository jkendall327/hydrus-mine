//! PopupMessageManager::_OKToAlterUI; focus is not a freeze condition.
/// An unavailable platform minimized signal does not invent a frozen state.
pub fn can_alter(hidden: bool, minimized: Option<bool>, freeze_minimized: bool) -> bool {
    !(hidden || freeze_minimized && minimized == Some(true))
}
