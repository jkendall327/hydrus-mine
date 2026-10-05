//! Owned thumbnail keyboard/wheel geometry callbacks.
use crate::MainWindow;
use hydrus_gui_model::thumbnail_navigation;
use hydrus_store::{
    Store,
    settings::{self, ThumbnailNavigation},
};
use slint::ComponentHandle as _;
use std::sync::Arc;

pub(crate) fn show(window: &MainWindow, store: &Store, span: u32) {
    let prefs: ThumbnailNavigation = store.read(settings::get).unwrap_or_default();
    window.set_thumbnail_wheel_step(thumbnail_navigation::single_step(
        f64::from(span),
        &prefs.scroll_rate,
        window.get_thumbnail_wheel_step(),
    ));
}

pub(crate) fn bind(window: &MainWindow, store: &Arc<Store>) {
    window.on_thumbnail_scroll_target({
        let store = store.clone();
        move |top, span, offset, viewport, content| {
            let prefs: ThumbnailNavigation = store.read(settings::get).unwrap_or_default();
            thumbnail_navigation::scroll_target(
                f64::from(top),
                f64::from(span),
                f64::from(offset),
                f64::from(viewport),
                f64::from(content),
                prefs.visibility_percent,
            ) as f32
        }
    });
    window.on_thumbnail_wheel_target({
        let weak = window.as_weak();
        move |delta, viewport, offset| {
            let Some(window) = weak.upgrade() else {
                return offset;
            };
            thumbnail_navigation::wheel_target(
                f64::from(delta),
                window.get_thumbnail_wheel_step(),
                f64::from(viewport),
                f64::from(offset),
            ) as f32
        }
    });
}
