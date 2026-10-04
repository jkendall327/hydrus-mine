//! Live browser eye-menu rows and their owned native window/settings consumers.
use std::{cell::RefCell, rc::Rc, sync::Arc};

use hydrus_core::HashId;
use hydrus_store::{
    Store,
    settings::{
        self, ViewerBackgroundSettings, ViewerCanvasSettings, ViewerEyeMenuSettings,
        ViewerFocusSettings, ViewerHoverSettings,
    },
};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::{MediaViewerWindow, MenuGroups, MenuRow, ViewerEyeMenu};

fn rows(values: &[(&str, i32, bool)]) -> ModelRc<MenuRow> {
    ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|(label, id, checked)| MenuRow {
                label: (*label).into(),
                id: *id,
                checkable: true,
                checked: *checked,
            })
            .collect::<Vec<_>>(),
    ))
}

fn menu(window: &MediaViewerWindow, store: &Store) -> ViewerEyeMenu {
    let eye: ViewerEyeMenuSettings = store.read(settings::get).unwrap_or_default();
    let background: ViewerBackgroundSettings = store.read(settings::get).unwrap_or_default();
    let hovers: ViewerHoverSettings = store.read(settings::get).unwrap_or_default();
    let focus: ViewerFocusSettings = store.read(settings::get).unwrap_or_default();
    let canvas: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
    ViewerEyeMenu {
        collapse_window: eye.collapse_window,
        collapse_hovers: eye.collapse_hovers,
        collapse_rendering: eye.collapse_rendering,
        window: MenuGroups {
            g1: rows(&[
                ("always on top", 0, window.get_viewer_window_top()),
                (
                    "always on top (while playing)",
                    1,
                    window.get_viewer_top_while_playing(),
                ),
                (
                    "remove titlebar/frame",
                    2,
                    window.get_viewer_window_frameless(),
                ),
            ]),
            g2: rows(&[
                (
                    "always start new media viewers always on top",
                    3,
                    eye.start_on_top,
                ),
                (
                    "always start new media viewers on top while playing",
                    4,
                    eye.start_on_top_while_playing,
                ),
                (
                    "always start new media viewers without titlebar/frame",
                    5,
                    eye.start_frameless,
                ),
            ]),
            ..MenuGroups::default()
        },
        hovers: MenuGroups {
            g1: rows(&[
                ("draw tags (left) in the background", 6, background.tags),
                (
                    "draw file information (top) in the background",
                    7,
                    background.information,
                ),
                (
                    "draw ratings and locations (top-right) in the background",
                    8,
                    background.ratings,
                ),
                ("draw notes (right) in the background", 9, background.notes),
                (
                    "draw index text (bottom-right) in the background",
                    10,
                    hovers.index_background,
                ),
            ]),
            g2: rows(&[
                (
                    "hover window pop-in requires window focus",
                    11,
                    focus.hovers_require_focus,
                ),
                ("pop-in tags hover window on mouseover", 12, hovers.tags),
                (
                    "pop-in ratings and locations hover window on mouseover",
                    13,
                    hovers.ratings,
                ),
                ("pop-in notes hover window on mouseover", 14, hovers.notes),
            ]),
            ..MenuGroups::default()
        },
        rendering: MenuGroups {
            g1: rows(&[
                (
                    "draw transparency as checkerboard in media viewer",
                    15,
                    canvas.transparency_checkerboard,
                ),
                (
                    "instead of checkerboard, use a bright greenscreen",
                    16,
                    canvas.transparency_greenscreen,
                ),
            ]),
            ..MenuGroups::default()
        },
    }
}

fn change<T: settings::Setting>(
    store: &Store,
    edit: impl FnOnce(&mut T) + Send + 'static,
) -> Result<(), String> {
    store
        .write(move |ctx| {
            let mut value: T = settings::get(ctx.conn())?;
            edit(&mut value);
            settings::set(ctx.conn(), &value)
        })
        .map_err(|error| error.to_string())
}

pub(crate) fn bind(
    window: &MediaViewerWindow,
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<MediaViewerWindow>>>,
    current_file: Rc<dyn Fn() -> HashId>,
) {
    let initial: ViewerEyeMenuSettings = store.read(settings::get).unwrap_or_default();
    window.set_viewer_window_top(initial.start_on_top && !initial.start_on_top_while_playing);
    window.set_viewer_top_while_playing(initial.start_on_top_while_playing);
    window.set_viewer_window_frameless(initial.start_frameless);
    let owned = Rc::new({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            let window = weak.upgrade()?;
            slot.borrow()
                .as_ref()
                .filter(|current| std::ptr::eq(current.window(), window.window()))?;
            Some(window)
        }
    });
    window.on_eye_menu_requested({
        let store = store.clone();
        let owned = owned.clone();
        move || {
            if let Some(window) = owned() {
                window.set_eye_menu(menu(&window, &store));
            }
        }
    });
    window.on_eye_menu_chosen({
        let store = store.clone();
        move |id| {
            let Some(window) = owned() else { return };
            let result = match id {
                0 => {
                    window.set_viewer_window_top(!window.get_viewer_window_top());
                    Ok(())
                }
                1 => {
                    let tied = !window.get_viewer_top_while_playing();
                    window.set_viewer_top_while_playing(tied);
                    let top = if tied {
                        window.get_media_playing()
                    } else {
                        let settings: ViewerEyeMenuSettings =
                            store.read(settings::get).unwrap_or_default();
                        settings.start_on_top
                    };
                    window.set_viewer_window_top(top);
                    Ok(())
                }
                2 => {
                    window.set_viewer_window_frameless(!window.get_viewer_window_frameless());
                    Ok(())
                }
                3 => change::<ViewerEyeMenuSettings>(&store, |s| s.start_on_top = !s.start_on_top),
                4 => change::<ViewerEyeMenuSettings>(&store, |s| {
                    s.start_on_top_while_playing = !s.start_on_top_while_playing
                }),
                5 => change::<ViewerEyeMenuSettings>(&store, |s| {
                    s.start_frameless = !s.start_frameless
                }),
                6 => change::<ViewerBackgroundSettings>(&store, |s| s.tags = !s.tags),
                7 => change::<ViewerBackgroundSettings>(&store, |s| s.information = !s.information),
                8 => change::<ViewerBackgroundSettings>(&store, |s| s.ratings = !s.ratings),
                9 => change::<ViewerBackgroundSettings>(&store, |s| s.notes = !s.notes),
                10 => change::<ViewerHoverSettings>(&store, |s| {
                    s.index_background = !s.index_background
                }),
                11 => change::<ViewerFocusSettings>(&store, |s| {
                    s.hovers_require_focus = !s.hovers_require_focus
                }),
                12 => change::<ViewerHoverSettings>(&store, |s| s.tags = !s.tags),
                13 => change::<ViewerHoverSettings>(&store, |s| s.ratings = !s.ratings),
                14 => change::<ViewerHoverSettings>(&store, |s| s.notes = !s.notes),
                15 => change::<ViewerCanvasSettings>(&store, |s| {
                    s.transparency_checkerboard = !s.transparency_checkerboard
                }),
                16 => change::<ViewerCanvasSettings>(&store, |s| {
                    s.transparency_greenscreen = !s.transparency_greenscreen
                }),
                _ => return,
            };
            if let Err(error) = result {
                window.set_warning(error.into());
            }
            crate::viewer_presentation::refresh(&window, &store, current_file());
            window.set_eye_menu(menu(&window, &store));
        }
    });
}
