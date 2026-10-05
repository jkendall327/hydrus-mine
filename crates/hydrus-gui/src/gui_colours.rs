//! Each GUI owner follows saved legacy colour roles without altering its palette.
use crate::Theme;
use hydrus_store::{Store, gui_colours::Settings};
use slint::{ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

/// The global owns only a timer and weak global/Store edges. Retiring or rebinding
/// stops the old observer; a retained closed component cannot resume it.
pub(crate) fn bind(theme: Theme<'_>, store: &Arc<Store>, active: Rc<Cell<bool>>) {
    bind_guard(theme, store, Rc::new(move || active.get()));
}
pub(crate) fn bind_guard(theme: Theme<'_>, store: &Arc<Store>, active: Rc<dyn Fn() -> bool>) {
    theme.invoke_retire_colours();
    let weak = <Theme<'_> as slint::Global<'_, crate::MainWindow>>::as_weak(&theme);
    let store = Arc::downgrade(store);
    let saved = Rc::new(RefCell::new(None::<Settings>));
    let timer = Rc::new(slint::Timer::default());
    let refresh: Rc<dyn Fn()> = Rc::new({
        let timer = Rc::downgrade(&timer);
        move || {
            if !active() {
                if let Some(timer) = timer.upgrade() {
                    timer.stop();
                }
                return;
            }
            let (Some(theme), Some(store)) = (weak.upgrade(), store.upgrade()) else {
                if let Some(timer) = timer.upgrade() {
                    timer.stop();
                }
                return;
            };
            let Ok(settings) = store.read(hydrus_store::gui_colours::load) else {
                return;
            };
            if saved.borrow().as_ref() == Some(&settings) {
                return;
            }
            theme.set_colours(ModelRc::new(VecModel::from(
                settings
                    .active()
                    .iter()
                    .map(|rgb| slint::Color::from_rgb_u8(rgb.0[0], rgb.0[1], rgb.0[2]))
                    .collect::<Vec<_>>(),
            )));
            theme.set_colours_override(settings.override_stylesheet);
            *saved.borrow_mut() = Some(settings);
            theme.invoke_colours_changed();
        }
    });
    refresh();
    theme.on_refresh_colours({
        let refresh = refresh.clone();
        move || refresh()
    });
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || refresh(),
    );
    theme.on_retire_colours(move || timer.stop());
}
