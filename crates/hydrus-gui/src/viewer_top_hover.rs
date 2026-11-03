//! The media viewer's top hover frame (`viewer_top_hover.slint`): its
//! tooltips (the shortcuts' and the embedded metadata button's), whether
//! the volume control shows, and the buttons with no other way in: the
//! embedded metadata, the zoom options menu, moving the window, and
//! focusing the page and file the viewer came from.

use std::rc::Rc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::MediaViewerSettings;
use hydrus_core::media_viewer::ZoomType;
use hydrus_gui_model::viewer_top_hover as model;
use hydrus_store::Store;
use hydrus_store::media::FileFlags;
use hydrus_store::settings::{self, ViewerCanvasSettings, ViewerPlaybackSettings};
use slint::{ModelRc, VecModel};
use std::sync::Arc;

use crate::zoom::{ZoomPolicy, Zoomed};
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
/// `ZOOM_TYPES`), recenter on window resize, the three locks, setting the
/// default from the menu, and the duplicate filter's label.
const RECENTER: i32 = 0;
const ZOOM_TYPE: i32 = 10;
const RECENTER_ON_RESIZE: i32 = 20;
const LOCK_SIZE: i32 = 21;
const LOCK_ZOOM_TYPE: i32 = 22;
const LOCK_PAN: i32 = 23;
const DEFAULT_FROM_MENU: i32 = 24;
const LOCKED_LABEL: i32 = 30;
const LOCKED: &str = "media size and pan are locked";

/// The move button's resize menu (`_ShowWindowResizeOptionsMenu`): resize
/// to fit, then the zooms, then its note (a label, copied when chosen).
const RESIZES: [Option<f64>; 6] = [None, Some(0.5), Some(0.75), Some(1.0), Some(1.5), Some(2.0)];
const RESIZE_NOTE: &str = "Right click any of the above to skip centering the media in the viewer window before zooming and/or resizing.";

/// The smallest the reference lets a viewer's frame be (its minimum size,
/// set as it opens, `SetInitialTLWSizeAndPosition`).
const MINIMUM_SIZE: (i32, i32) = (240, 240);

/// The canvas whose top hover it is: the zoom options menu differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Canvas {
    /// The media viewer and the archive/delete filter: the whole menu.
    Viewer,
    /// The duplicate filter (`CanvasHoverFrameTopDuplicatesFilter`), whose
    /// size and pan stay as the pair's files change.
    Duplicates,
}

/// The zoom policy the viewer's options give, read each time it is asked.
pub(crate) fn zoom_policy(store: Arc<Store>) -> impl Fn() -> ZoomPolicy + 'static {
    move || {
        let canvas: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
        let viewer: MediaViewerSettings = store.read(settings::get).unwrap_or_default();
        ZoomPolicy {
            recenter_on_resize: canvas.recenter_on_resize,
            lock_size: canvas.lock_size,
            lock_zoom_type: canvas.lock_zoom_type,
            lock_pan: canvas.lock_pan,
            default_zoom_type: Some(viewer.default_zoom_type),
        }
    }
}

/// The zoom switch button's command (`zoom_switch_command`), read once when
/// the viewer is made, as the reference's top hover reads it.
pub(crate) fn zoom_switch_command(store: &Store) -> i32 {
    let playback: ViewerPlaybackSettings = store.read(settings::get).unwrap_or_default();
    ZOOM_SWITCH_COMMANDS[playback.zoom_switch.min(3)]
}

/// The tooltips for the file shown: the buttons' shortcuts, as the
/// shortcut sets now have them (the zoom switch's for the window's frozen
/// command, and a filter's back and skip), and what its embedded metadata
/// holds; as given and as Slint's Markdown takes them.
pub(crate) fn tip_sets(
    store: &Store,
    file: HashId,
    zoom_switch: i32,
    back_and_skip: Option<[(&str, i32); 2]>,
) -> (TopHoverTips, TopHoverTips) {
    let shortcuts: hydrus_core::shortcuts::Settings = store.read(settings::get).unwrap_or_default();
    let tips = model::tips(&shortcuts, zoom_switch);
    let [back, skip] = back_and_skip.map_or_else(Default::default, |pair| {
        pair.map(|(text, command)| model::with_shortcuts(text, command, &shortcuts))
    });
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
        back: shown(&back).into(),
        skip: shown(&skip).into(),
        zoom_in: shown(&tips.zoom_in).into(),
        zoom_out: shown(&tips.zoom_out).into(),
        zoom_switch: shown(&tips.zoom_switch).into(),
        open_externally: shown(&tips.open_externally).into(),
        metadata: shown(&metadata).into(),
    };
    (slint_tips(&|t| t.to_owned()), slint_tips(&model::markdown))
}

/// The media viewer's tooltips (see [`tip_sets`]).
pub(crate) fn refresh(window: &MediaViewerWindow, store: &Store, file: HashId, zoom_switch: i32) {
    let (tips, markdown) = tip_sets(store, file, zoom_switch, None);
    window.set_top_tips(tips);
    window.set_top_tips_markdown(markdown);
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

/// The zoom options menu (`_ShowZoomOptionsMenu`): recenter, the zoom
/// types (the current one checked), then the locks, setting the default
/// from the menu and recentering on resize; the duplicate filter's says its
/// size and pan are locked instead.
pub(crate) fn zoom_menu(
    canvas: Canvas,
    current: Option<ZoomType>,
    options: &ViewerCanvasSettings,
) -> MenuGroups {
    let types: Vec<(&str, i32, bool, bool)> = ZOOM_TYPES
        .iter()
        .zip(ZOOM_TYPE..)
        .map(|((zoom_type, label), id)| (*label, id, true, current == Some(*zoom_type)))
        .collect();
    let (g3, g4) = match canvas {
        Canvas::Viewer => (
            rows(&[
                (
                    "try to lock current size",
                    LOCK_SIZE,
                    true,
                    options.lock_size,
                ),
                (
                    "lock current zoom type",
                    LOCK_ZOOM_TYPE,
                    true,
                    options.lock_zoom_type,
                ),
                ("lock current pan", LOCK_PAN, true, options.lock_pan),
            ]),
            rows(&[
                (
                    "set default zoom type from this menu",
                    DEFAULT_FROM_MENU,
                    true,
                    options.default_zoom_type_from_menu,
                ),
                (
                    "recenter media on window resize",
                    RECENTER_ON_RESIZE,
                    true,
                    options.recenter_on_resize,
                ),
            ]),
        ),
        Canvas::Duplicates => (rows(&[(LOCKED, LOCKED_LABEL, false, false)]), rows(&[])),
    };
    MenuGroups {
        g1: rows(&[("recenter media", RECENTER, false, false)]),
        g2: rows(&types),
        g3,
        g4,
        ..MenuGroups::default()
    }
}

/// An entry of the zoom options menu chosen: recenter, a zoom type (made
/// the default too, if the menu says so), a lock or option flipped (the
/// size and zoom type locks exclusive, as `flip_background_boolean`
/// keeps them), or the label copied.
pub(crate) fn zoom_menu_chosen(store: &Store, zoomed: &Zoomed, id: i32) {
    let flip = |change: fn(&mut ViewerCanvasSettings)| {
        let flipped = store.write(move |ctx| {
            let mut canvas: ViewerCanvasSettings = settings::get(ctx.conn())?;
            change(&mut canvas);
            settings::set(ctx.conn(), &canvas)
        });
        if let Err(e) = flipped {
            eprintln!("could not save the zoom options: {e}");
        }
    };
    match id {
        RECENTER => zoomed.recenter(),
        RECENTER_ON_RESIZE => flip(|c| c.recenter_on_resize = !c.recenter_on_resize),
        LOCK_SIZE => flip(|c| {
            c.lock_size = !c.lock_size;
            if c.lock_size {
                c.lock_zoom_type = false;
            }
        }),
        LOCK_ZOOM_TYPE => flip(|c| {
            c.lock_zoom_type = !c.lock_zoom_type;
            if c.lock_zoom_type {
                c.lock_size = false;
            }
        }),
        LOCK_PAN => flip(|c| c.lock_pan = !c.lock_pan),
        DEFAULT_FROM_MENU => {
            flip(|c| c.default_zoom_type_from_menu = !c.default_zoom_type_from_menu);
        }
        LOCKED_LABEL => crate::copy_to_clipboard(LOCKED),
        _ => {
            let Some((zoom_type, _)) = usize::try_from(id - ZOOM_TYPE)
                .ok()
                .and_then(|i| ZOOM_TYPES.get(i))
            else {
                return;
            };
            zoomed.zoom_to_type(*zoom_type);
            let canvas: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
            if canvas.default_zoom_type_from_menu {
                let zoom_type = *zoom_type;
                let set = store.write(move |ctx| {
                    let mut viewer: MediaViewerSettings = settings::get(ctx.conn())?;
                    viewer.default_zoom_type = zoom_type;
                    settings::set(ctx.conn(), &viewer)
                });
                if let Err(e) = set {
                    eprintln!("could not save the default zoom type: {e}");
                }
            }
        }
    }
}

/// What a window does at the platform when the move button is used: a
/// system move (a left press), or moving it to a place (resizing it to
/// its file). Tests watch these; the headless platform has no place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMove {
    System,
    To(i32, i32),
}

type MoveWatch = Rc<dyn Fn(WindowMove)>;

thread_local! {
    static WINDOW_MOVES: std::cell::RefCell<Option<MoveWatch>> = std::cell::RefCell::new(None);
}

/// Have `watch` told of each window move the top hover asks for.
pub fn watch_window_moves(watch: impl Fn(WindowMove) + 'static) {
    WINDOW_MOVES.with(|w| *w.borrow_mut() = Some(Rc::new(watch)));
}

fn moved(what: WindowMove) {
    let watch = WINDOW_MOVES.with(|w| w.borrow().clone());
    if let Some(watch) = watch {
        watch(what);
    }
}

/// A left press on the move button: the platform's own window move, as the
/// reference's `startSystemMove`.
pub(crate) fn system_move(window: &slint::Window) {
    use slint::winit_030::WinitWindowAccessor as _;
    moved(WindowMove::System);
    if window.has_winit_window() {
        let _ = window.with_winit_window(slint::winit_030::winit::window::Window::drag_window);
    }
}

/// An entry of the move button's resize menu (its place in the menu):
/// zoomed (about the viewer's centre with the left button; with the right,
/// as the zoom centre option says), then the window sized to the file and
/// moved to where the file was (`SizeSelfToMedia`), no smaller than its
/// minimum, and the file recentred; the note copied.
pub(crate) fn resize_chosen(
    window: &slint::Window,
    store: &Store,
    zoomed: &Zoomed,
    choice: i32,
    right: bool,
) {
    let Some(resize) = usize::try_from(choice).ok().and_then(|i| RESIZES.get(i)) else {
        if choice == RESIZES.len() as i32 {
            crate::copy_to_clipboard(RESIZE_NOTE);
        }
        return;
    };
    if let Some(zoom) = resize {
        zoomed.zoom_to(*zoom, !right);
    }
    let Some((x, y, width, height)) = zoomed.rect() else {
        return;
    };
    window.set_fullscreen(false);
    window.set_maximized(false);
    let scale = window.scale_factor();
    let origin = window.position().to_logical(scale);
    let wanted = (
        i64::from(origin.x as i32 + x),
        i64::from(origin.y as i32 + y),
    );
    let rescue: hydrus_store::settings::WindowRescueSettings =
        store.read(settings::get).unwrap_or_default();
    let size = (width.max(MINIMUM_SIZE.0), height.max(MINIMUM_SIZE.1));
    let screens = crate::windows::native_screens(window).unwrap_or_default();
    let place = hydrus_gui_model::window_rescue::safe_position(
        wanted,
        Some((i64::from(size.0), i64::from(size.1))),
        &rescue,
        &screens,
    )
    .unwrap_or(wanted);
    let place = (
        i32::try_from(place.0).unwrap_or(0),
        i32::try_from(place.1).unwrap_or(0),
    );
    window.set_size(slint::LogicalSize::new(size.0 as f32, size.1 as f32));
    window.set_position(slint::LogicalPosition::new(place.0 as f32, place.1 as f32));
    moved(WindowMove::To(place.0, place.1));
    zoomed.resized();
    zoomed.recenter();
}

/// Wire a window's top hover zoom options menu (its `top-zoom-menu`
/// property and callbacks) and move button (`move-window`,
/// `top-resize-chosen`) to its zoomed file, for `canvas`.
macro_rules! bind_zoom_menus {
    ($window:expr, $store:expr, $zoomed:expr, $canvas:expr) => {{
        use slint::ComponentHandle as _;
        let window = &$window;
        let store: std::sync::Arc<hydrus_store::Store> = $store.clone();
        let zoomed: $crate::zoom::Zoomed = $zoomed.clone();
        let show_zoom_menu = {
            let weak = window.as_weak();
            let zoomed = zoomed.clone();
            let store = store.clone();
            move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let options: hydrus_store::settings::ViewerCanvasSettings =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                window.set_top_zoom_menu($crate::viewer_top_hover::zoom_menu(
                    $canvas,
                    zoomed.zoom_type(),
                    &options,
                ));
            }
        };
        show_zoom_menu();
        window.on_top_zoom_menu_requested(show_zoom_menu.clone());
        window.on_top_zoom_menu_chosen({
            let store = store.clone();
            let zoomed = zoomed.clone();
            move |id| {
                $crate::viewer_top_hover::zoom_menu_chosen(&store, &zoomed, id);
                show_zoom_menu();
            }
        });
        window.on_move_window({
            let weak = window.as_weak();
            move || {
                if let Some(window) = weak.upgrade() {
                    $crate::viewer_top_hover::system_move(window.window());
                }
            }
        });
        window.on_top_resize_chosen({
            let weak = window.as_weak();
            move |choice, right| {
                if let Some(window) = weak.upgrade() {
                    $crate::viewer_top_hover::resize_chosen(
                        window.window(),
                        &store,
                        &zoomed,
                        choice,
                        right,
                    );
                }
            }
        });
    }};
}
pub(crate) use bind_zoom_menus;

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
    bind_zoom_menus!(window, store, zoomed, Canvas::Viewer);
}
