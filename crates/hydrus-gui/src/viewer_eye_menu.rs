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

/// The menu, for a window always on top (`top`), tied to its playing
/// (`while_playing`) and without its frame (`frameless`), as it is now.
pub(crate) fn menu(
    store: &Store,
    top: bool,
    while_playing: bool,
    frameless: bool,
) -> ViewerEyeMenu {
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
                ("always on top", 0, top),
                ("always on top (while playing)", 1, while_playing),
                ("remove titlebar/frame", 2, frameless),
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

/// An entry of the menu chosen: the store's options changed (the window's
/// own rows are the window's, and not here: `None`).
pub(crate) fn chosen(store: &Store, id: i32) -> Option<Result<(), String>> {
    Some(match id {
        3 => change::<ViewerEyeMenuSettings>(store, |s| s.start_on_top = !s.start_on_top),
        4 => change::<ViewerEyeMenuSettings>(store, |s| {
            s.start_on_top_while_playing = !s.start_on_top_while_playing;
        }),
        5 => change::<ViewerEyeMenuSettings>(store, |s| {
            s.start_frameless = !s.start_frameless;
        }),
        6 => change::<ViewerBackgroundSettings>(store, |s| s.tags = !s.tags),
        7 => change::<ViewerBackgroundSettings>(store, |s| s.information = !s.information),
        8 => change::<ViewerBackgroundSettings>(store, |s| s.ratings = !s.ratings),
        9 => change::<ViewerBackgroundSettings>(store, |s| s.notes = !s.notes),
        10 => change::<ViewerHoverSettings>(store, |s| {
            s.index_background = !s.index_background;
        }),
        11 => change::<ViewerFocusSettings>(store, |s| {
            s.hovers_require_focus = !s.hovers_require_focus;
        }),
        12 => change::<ViewerHoverSettings>(store, |s| s.tags = !s.tags),
        13 => change::<ViewerHoverSettings>(store, |s| s.ratings = !s.ratings),
        14 => change::<ViewerHoverSettings>(store, |s| s.notes = !s.notes),
        15 => change::<ViewerCanvasSettings>(store, |s| {
            s.transparency_checkerboard = !s.transparency_checkerboard;
        }),
        16 => change::<ViewerCanvasSettings>(store, |s| {
            s.transparency_greenscreen = !s.transparency_greenscreen;
        }),
        _ => return None,
    })
}

/// Wire a window's eye menu (`eye-menu`, `eye-menu-requested`,
/// `eye-menu-chosen`) and its window rows (`viewer-window-top`,
/// `viewer-top-while-playing`, `viewer-window-frameless`, read with
/// `media-playing`) to `store`, while `$owned()` gives the window; after a
/// change, `$changed(&window, result)`.
macro_rules! bind_eye_menu {
    ($window:expr, $store:expr, $owned:expr, $changed:expr) => {{
        let window = &$window;
        let changed = $changed;
        let store: std::sync::Arc<hydrus_store::Store> = $store.clone();
        let initial: hydrus_store::settings::ViewerEyeMenuSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        window.set_viewer_window_top(initial.start_on_top && !initial.start_on_top_while_playing);
        window.set_viewer_top_while_playing(initial.start_on_top_while_playing);
        window.set_viewer_window_frameless(initial.start_frameless);
        let owned = std::rc::Rc::new($owned);
        window.on_eye_menu_requested({
            let store = store.clone();
            let owned = owned.clone();
            move || {
                if let Some(window) = owned() {
                    window.set_eye_menu($crate::viewer_eye_menu::menu(
                        &store,
                        window.get_viewer_window_top(),
                        window.get_viewer_top_while_playing(),
                        window.get_viewer_window_frameless(),
                    ));
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
                            let settings: hydrus_store::settings::ViewerEyeMenuSettings =
                                store.read(hydrus_store::settings::get).unwrap_or_default();
                            settings.start_on_top
                        };
                        window.set_viewer_window_top(top);
                        Ok(())
                    }
                    2 => {
                        window.set_viewer_window_frameless(!window.get_viewer_window_frameless());
                        Ok(())
                    }
                    id => match $crate::viewer_eye_menu::chosen(&store, id) {
                        Some(result) => result,
                        None => return,
                    },
                };
                changed(&window, result);
                window.set_eye_menu($crate::viewer_eye_menu::menu(
                    &store,
                    window.get_viewer_window_top(),
                    window.get_viewer_top_while_playing(),
                    window.get_viewer_window_frameless(),
                ));
            }
        });
    }};
}
pub(crate) use bind_eye_menu;

pub(crate) fn bind(
    window: &MediaViewerWindow,
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<MediaViewerWindow>>>,
    current_file: Rc<dyn Fn() -> HashId>,
) {
    let owned = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            let window = weak.upgrade()?;
            slot.borrow()
                .as_ref()
                .filter(|current| std::ptr::eq(current.window(), window.window()))?;
            Some(window)
        }
    };
    let refresh_store = store.clone();
    bind_eye_menu!(
        window,
        store,
        owned,
        move |window: &MediaViewerWindow, result: Result<(), String>| {
            if let Err(error) = result {
                window.set_warning(error.into());
            }
            crate::viewer_presentation::refresh(window, &refresh_store, current_file());
        }
    );
}
