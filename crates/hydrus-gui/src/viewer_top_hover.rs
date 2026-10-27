//! The media viewer's top hover frame (`viewer_top_hover.slint`): its
//! tooltips (the shortcuts' and the embedded metadata button's), whether
//! the volume control shows, and the buttons with no other way in: the
//! embedded metadata, the zoom options menu, moving the window, and
//! focusing the page and file the viewer came from.

use std::rc::Rc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::ZoomType;
use hydrus_gui_model::viewer_top_hover as model;
use hydrus_store::Store;
use hydrus_store::media::FileFlags;
use hydrus_store::settings::{self, ViewerCanvasSettings, ViewerPlaybackSettings};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::zoom::Zoomed;
use crate::{MediaViewerWindow, MenuGroups, MenuRow, TopHoverTips};

/// The zoom switch button's command for the `zoom_switch_command` choice.
const ZOOM_SWITCH_COMMANDS: [i32; 4] = [91, 106, 172, 173];

/// The zoom types as the zoom options menu lists them
/// (`MEDIA_VIEWER_ZOOM_TYPES`), with their labels.
const ZOOM_TYPES: [(ZoomType, &str); 6] = [
    (ZoomType::DefaultForFiletype, "default for filetype"),
    (ZoomType::Full, "100% zoom"),
    (ZoomType::Canvas, "canvas fit"),
    (ZoomType::FillX, "fill horizontally"),
    (ZoomType::FillY, "fill vertically"),
    (ZoomType::FillAuto, "canvas fill"),
];

/// The zoom options menu's ids: recenter, a zoom type (+ its place in
/// `ZOOM_TYPES`), and recenter on window resize.
const RECENTER: i32 = 0;
const ZOOM_TYPE: i32 = 10;
const RECENTER_ON_RESIZE: i32 = 20;

/// The tooltips for the file shown: the buttons' shortcuts, as the
/// shortcut sets now have them, and what its embedded metadata holds.
pub(crate) fn refresh(window: &MediaViewerWindow, store: &Store, file: HashId) {
    let shortcuts: hydrus_core::shortcuts::Settings = store.read(settings::get).unwrap_or_default();
    let playback: ViewerPlaybackSettings = store.read(settings::get).unwrap_or_default();
    let command = ZOOM_SWITCH_COMMANDS[playback.zoom_switch.min(3)];
    let tips = model::tips(&shortcuts, command);
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[file]))
        .ok()
        .and_then(|results| results.into_iter().next()?.info);
    let metadata = model::embedded_metadata(info.map_or_else(Default::default, |info| {
        let has = |flag| info.flags.has(flag);
        model::Embedded {
            jpeg: info.mime == hydrus_core::Mime::ImageJpeg,
            exif: has(FileFlags::EXIF),
            xmp: has(FileFlags::XMP),
            iptc: has(FileFlags::IPTC),
            human_readable: has(FileFlags::HUMAN_READABLE_METADATA),
            software: has(FileFlags::SOFTWARE_SOURCE),
        }
    }));
    let slint_tips = |shown: &dyn Fn(&str) -> String| TopHoverTips {
        first: shown(&tips.first).into(),
        previous: shown(&tips.previous).into(),
        next: shown(&tips.next).into(),
        last: shown(&tips.last).into(),
        zoom_in: shown(&tips.zoom_in).into(),
        zoom_out: shown(&tips.zoom_out).into(),
        zoom_switch: shown(&tips.zoom_switch).into(),
        open_externally: shown(&tips.open_externally).into(),
        metadata: shown(&metadata).into(),
    };
    window.set_top_tips(slint_tips(&|t| t.to_owned()));
    window.set_top_tips_markdown(slint_tips(&model::markdown));
}

/// What the top hover's own buttons reach.
pub(crate) struct Hooks {
    pub store: std::sync::Arc<Store>,
    pub current: Rc<dyn Fn() -> HashId>,
    pub zoomed: Zoomed,
    pub embedded_metadata: Rc<dyn Fn(HashId)>,
    pub focus_media: Rc<dyn Fn(HashId)>,
}

fn rows(values: &[(&str, i32, bool, bool)]) -> ModelRc<MenuRow> {
    ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|(label, id, checkable, checked)| MenuRow {
                label: (*label).into(),
                id: *id,
                checkable: *checkable,
                checked: *checked,
            })
            .collect::<Vec<_>>(),
    ))
}

/// The zoom options menu (`_ShowZoomOptionsMenu`), as far as hydrus-rs has
/// its entries: recenter, the zoom types, and recentering on resize.
fn zoom_menu(current: Option<ZoomType>, recenter_on_resize: bool) -> MenuGroups {
    let types: Vec<(&str, i32, bool, bool)> = ZOOM_TYPES
        .iter()
        .zip(ZOOM_TYPE..)
        .map(|((zoom_type, label), id)| (*label, id, true, current == Some(*zoom_type)))
        .collect();
    MenuGroups {
        g1: rows(&[("recenter media", RECENTER, false, false)]),
        g2: rows(&types),
        g3: rows(&[(
            "recenter media on window resize",
            RECENTER_ON_RESIZE,
            true,
            recenter_on_resize,
        )]),
        ..MenuGroups::default()
    }
}

pub(crate) fn bind(window: &MediaViewerWindow, hooks: Hooks) {
    let Hooks {
        store,
        current,
        zoomed,
        embedded_metadata,
        focus_media,
    } = hooks;
    window.set_volume_available(crate::mpv::available());
    window.on_show_metadata({
        let current = current.clone();
        move || embedded_metadata(current())
    });
    window.on_focus_media({
        let current = current.clone();
        move || focus_media(current())
    });
    let show_zoom_menu = {
        let weak = window.as_weak();
        let zoomed = zoomed.clone();
        let store = store.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let canvas: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
            window.set_top_zoom_menu(zoom_menu(zoomed.zoom_type(), canvas.recenter_on_resize));
        }
    };
    show_zoom_menu();
    window.on_top_zoom_menu_requested(show_zoom_menu.clone());
    window.on_top_zoom_menu_chosen({
        let store = store.clone();
        move |id| {
            match id {
                RECENTER => zoomed.recenter(),
                RECENTER_ON_RESIZE => {
                    let flipped = store.write(|ctx| {
                        let mut canvas: ViewerCanvasSettings = settings::get(ctx.conn())?;
                        canvas.recenter_on_resize = !canvas.recenter_on_resize;
                        settings::set(ctx.conn(), &canvas)
                    });
                    if let Err(e) = flipped {
                        eprintln!("could not save the recenter on resize option: {e}");
                    }
                }
                _ => {
                    if let Some((zoom_type, _)) = usize::try_from(id - ZOOM_TYPE)
                        .ok()
                        .and_then(|i| ZOOM_TYPES.get(i))
                    {
                        zoomed.zoom_to_type(*zoom_type);
                    }
                }
            }
            show_zoom_menu();
        }
    });
    // (a left press on the move button moves the window, as the reference's
    // `startSystemMove`)
    window.on_move_window({
        let weak = window.as_weak();
        move || {
            use slint::winit_030::WinitWindowAccessor as _;
            if let Some(window) = weak.upgrade() {
                let _ = window
                    .window()
                    .with_winit_window(slint::winit_030::winit::window::Window::drag_window);
            }
        }
    });
    window.on_top_resize_menu_requested(|| {});
    window.on_top_resize_chosen(|_| {});
}
