//! The filters' top hover frames (`viewer_top_hover.slint`'s
//! `TopHoverFrame`): the archive/delete filter's
//! (`CanvasHoverFrameTopArchiveDeleteFilter`) and the duplicate filter's
//! (`CanvasHoverFrameTopDuplicatesFilter`), whose buttons reach what the
//! media viewer's do: the zoom and its options, the volume, the shortcuts
//! and view options menus, moving and resizing the window, fullscreen,
//! opening the file externally, showing it in the file browser, and the
//! file's embedded metadata.

use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_store::Store;

/// The openers a filter's top hover reaches that live with the main
/// window: the embedded metadata window and the shortcuts editor.
pub(crate) struct FilterHooks {
    pub embedded_metadata: Rc<dyn Fn(Arc<Store>, HashId)>,
    pub edit_shortcuts: Rc<dyn Fn()>,
}

/// The hooks, once the main window has made them (the filters are opened
/// from closures made before them).
pub(crate) type FilterHooksCell = Rc<std::cell::RefCell<Option<Rc<FilterHooks>>>>;

/// The back and skip buttons' texts and commands, for their tooltips.
pub(crate) fn back_and_skip(canvas: crate::viewer_top_hover::Canvas) -> [(&'static str, i32); 2] {
    use hydrus_gui_model::viewer_top_hover as model;
    match canvas {
        crate::viewer_top_hover::Canvas::Duplicates => [
            ("go back a pair", model::DUPLICATE_BACK),
            ("show a different pair", model::DUPLICATE_SKIP),
        ],
        crate::viewer_top_hover::Canvas::Viewer => [
            ("back", model::ARCHIVE_DELETE_BACK),
            ("skip", model::ARCHIVE_DELETE_SKIP),
        ],
    }
}

/// Show the file's state, info line and tooltips in a filter window's top
/// hover frame (its `file-*`, `info-line` and `top-tips` properties).
macro_rules! refresh_filter_top_hover {
    ($window:expr, $store:expr, $file:expr, $zoom_switch:expr, $canvas:expr) => {{
        let window = &$window;
        let store: &hydrus_store::Store = &$store;
        let file: hydrus_core::HashId = $file;
        let shown = $crate::viewer::shown(store, file);
        window.set_info_line(shown.line.into());
        window.set_file_inbox(shown.inbox);
        window.set_file_trashed(shown.trashed);
        window.set_file_local(shown.local);
        window.set_file_undeletable(shown.undeletable);
        let (tips, markdown) = $crate::viewer_top_hover::tip_sets(
            store,
            file,
            $zoom_switch,
            Some($crate::filter_top_hover::back_and_skip($canvas)),
        );
        window.set_top_tips(tips);
        window.set_top_tips_markdown(markdown);
    }};
}
pub(crate) use refresh_filter_top_hover;

/// Wire a filter window's top hover frame: `$current()` is the file shown,
/// `$owned()` the window while it is the filter's, `$hooks` the main
/// window's openers, and `$focus` shows the file in the page it came from
/// (none, for the duplicate filter, which has no page). Returns the zoom
/// switch button's command, read once, as the reference's top hover does.
macro_rules! bind_filter_top_hover {
    ($window:expr, $store:expr, $zoomed:expr, $canvas:expr, $current:expr, $owned:expr, $hooks:expr, $focus:expr) => {{
        use slint::ComponentHandle as _;
        let window = &$window;
        let store: std::sync::Arc<hydrus_store::Store> = $store.clone();
        let zoomed: $crate::zoom::Zoomed = $zoomed.clone();
        let current: std::rc::Rc<dyn Fn() -> Option<hydrus_core::HashId>> = $current;
        let hooks: $crate::filter_top_hover::FilterHooksCell = $hooks;
        let focus: Option<std::rc::Rc<dyn Fn(hydrus_core::HashId)>> = $focus;
        let owned = std::rc::Rc::new($owned);
        window.set_volume_available($crate::mpv::available());
        $crate::viewer_top_hover::bind_zoom_menus!(window, store, zoomed, $canvas);
        zoomed.watch({
            let weak = window.as_weak();
            move |zoom| {
                if let Some(window) = weak.upgrade() {
                    let text = zoom
                        .map($crate::viewer_menu::zoom_percentage)
                        .unwrap_or_default();
                    window.set_zoom_text(text.into());
                }
            }
        });
        let zoom_switch = $crate::viewer_top_hover::zoom_switch_command(&store);
        let playback: hydrus_store::settings::ViewerPlaybackSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        window.on_zoom_switch_requested({
            let zoomed = zoomed.clone();
            move |x, y| zoomed.switch_with_policy(playback.zoom_switch, Some((x as i32, y as i32)))
        });
        window.on_show_metadata({
            let (hooks, current, store) = (hooks.clone(), current.clone(), store.clone());
            move || {
                let hooks = hooks.borrow().clone();
                if let (Some(hooks), Some(file)) = (hooks, current()) {
                    (hooks.embedded_metadata)(store.clone(), file);
                }
            }
        });
        let launcher = $crate::open_externally_launch::Launcher::new(std::rc::Rc::new({
            let owned = owned.clone();
            move || owned().is_some()
        }));
        window.on_open_externally({
            let (current, store) = (current.clone(), store.clone());
            move || {
                if let Some(file) = current() {
                    launcher.file(&store, file);
                }
            }
        });
        window.on_show_in_file_browser({
            let (current, store) = (current.clone(), store.clone());
            move || {
                if let Some(path) =
                    current().and_then(|file| $crate::thumbnail_menu::paths(&store, &[file]).pop())
                {
                    $crate::show_in_file_browser(&path);
                }
            }
        });
        window.on_toggle_fullscreen({
            let weak = window.as_weak();
            let maximised_before = std::rc::Rc::new(std::cell::Cell::new(false));
            move || {
                if let Some(window) = weak.upgrade() {
                    $crate::windows::switch_fullscreen(window.window(), &maximised_before);
                }
            }
        });
        window.on_focus_media({
            let current = current.clone();
            move || {
                if let (Some(focus), Some(file)) = (&focus, current()) {
                    focus(file);
                }
            }
        });
        // the keyboard button: custom shortcut sets, for this window and by
        // default
        let active_custom: std::rc::Rc<std::cell::RefCell<Vec<String>>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                store
                    .read(
                        hydrus_store::settings::get::<
                            hydrus_store::reference_options::ReferenceOptions,
                        >,
                    )
                    .unwrap_or_default()
                    .string_list(hydrus_gui_model::viewer_shortcut_menu::DEFAULTS_OPTION),
            ));
        let show_shortcut_menu = {
            let weak = window.as_weak();
            let store = store.clone();
            let active_custom = active_custom.clone();
            move || {
                use hydrus_gui_model::viewer_shortcut_menu as menu;
                let Some(window) = weak.upgrade() else { return };
                let settings: hydrus_core::shortcuts::Settings =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                let names = menu::custom_names(&settings);
                let defaults = store
                    .read(
                        hydrus_store::settings::get::<
                            hydrus_store::reference_options::ReferenceOptions,
                        >,
                    )
                    .unwrap_or_default()
                    .string_list(menu::DEFAULTS_OPTION);
                window.set_shortcut_current(slint::ModelRc::new(slint::VecModel::from(
                    menu::checks(&names, &active_custom.borrow()),
                )));
                window.set_shortcut_defaults(slint::ModelRc::new(slint::VecModel::from(
                    menu::checks(&names, &defaults),
                )));
                window.set_shortcut_names(slint::ModelRc::new(slint::VecModel::from(
                    names
                        .into_iter()
                        .map(slint::SharedString::from)
                        .collect::<Vec<_>>(),
                )));
            }
        };
        show_shortcut_menu();
        window.on_shortcut_menu_requested(show_shortcut_menu.clone());
        window.on_shortcut_menu_chosen({
            let store = store.clone();
            let hooks = hooks.clone();
            move |kind, i| {
                use hydrus_gui_model::viewer_shortcut_menu as menu;
                let settings: hydrus_core::shortcuts::Settings =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                let name = usize::try_from(i)
                    .ok()
                    .and_then(|i| menu::custom_names(&settings).get(i).cloned());
                match (kind, name) {
                    (0, _) => {
                        let hooks = hooks.borrow().clone();
                        if let Some(hooks) = hooks {
                            (hooks.edit_shortcuts)();
                        }
                    }
                    (1, Some(name)) => menu::flip(&mut active_custom.borrow_mut(), &name),
                    (2, Some(name)) => {
                        let done = store.write(move |ctx| {
                            let mut options: hydrus_store::reference_options::ReferenceOptions =
                                hydrus_store::settings::get(ctx.conn())?;
                            let mut defaults = options.string_list(menu::DEFAULTS_OPTION);
                            menu::flip(&mut defaults, &name);
                            options.set_string_list(menu::DEFAULTS_OPTION, defaults);
                            hydrus_store::settings::set(ctx.conn(), &options)
                        });
                        if let Err(e) = done {
                            eprintln!("could not save the default shortcuts: {e}");
                        }
                    }
                    _ => {}
                }
                show_shortcut_menu();
            }
        });
        $crate::viewer_eye_menu::bind_eye_menu!(
            window,
            store,
            {
                let owned = owned.clone();
                move || owned()
            },
            |_: &_, _: i32, result: Result<(), String>| {
                if let Err(e) = result {
                    eprintln!("could not save the view options: {e}");
                }
            }
        );
        zoom_switch
    }};
}
pub(crate) use bind_filter_top_hover;
