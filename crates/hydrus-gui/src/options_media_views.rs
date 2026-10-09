//! Options > media playback > per-filetype handling (hydrus-gui-model's
//! `media_view_options`): the list, "add" (choosing a filetype, then the
//! editor on a copy of its class's options), "edit" and "delete". Child
//! Apply only changes the Options draft; Options Apply saves it.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hydrus_core::media_viewer::{MediaView, ScaleAction};
use hydrus_gui_model::media_view_options::{
    self as model, Row, SCALE_DOWN_QUALITIES, SCALE_UP_QUALITIES, SCALES, Table,
};
use hydrus_gui_model::options::Editor;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::{ChoiceButtonsWindow, MediaViewWindow, OptionsWindow, TableRow};

pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
    #[cfg(test)]
    children: Rc<RefCell<Children>>,
}

/// The open chooser or editor.
#[derive(Default)]
struct Children {
    chooser: Option<ChoiceButtonsWindow>,
    editor: Option<MediaViewWindow>,
    live: Rc<Cell<bool>>,
}

impl Children {
    fn start(&mut self) -> Rc<Cell<bool>> {
        self.live.set(false);
        self.live = Rc::new(Cell::new(true));
        self.live.clone()
    }
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

fn index_of<T: PartialEq + Copy>(items: &[(T, &str)], value: T) -> i32 {
    items
        .iter()
        .position(|(v, _)| *v == value)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

fn pick<T: Copy>(items: &[(T, &str)], index: i32, fallback: T) -> T {
    usize::try_from(index)
        .ok()
        .and_then(|i| items.get(i))
        .map_or(fallback, |(v, _)| *v)
}

/// Open the editor on `row`; `done` gets the edited row on apply.
fn edit(
    row: &Row,
    title: &str,
    live: Rc<Cell<bool>>,
    changed: Rc<dyn Fn()>,
    done: Rc<dyn Fn(Row)>,
) -> Option<MediaViewWindow> {
    let capability = model::capability(row.code)?;
    let window = crate::app_title::new::<crate::MediaViewWindow>().ok()?;
    window.set_window_title(title.into());
    window.set_intro(capability.intro.into());
    let label = |a| model::action_text(a).to_owned();
    window.set_media_actions(strings(capability.media.iter().map(|a| label(*a))));
    window.set_preview_actions(strings(capability.preview.iter().map(|a| label(*a))));
    window.set_scales(strings(SCALES.iter().map(|(_, s)| (*s).to_owned())));
    window.set_up_qualities(strings(
        SCALE_UP_QUALITIES.iter().map(|(_, s)| (*s).to_owned()),
    ));
    window.set_down_qualities(strings(
        SCALE_DOWN_QUALITIES.iter().map(|(_, s)| (*s).to_owned()),
    ));
    window.set_zoom_rows(capability.zoom_rows);
    let v = row.view;
    let position = |actions: &[hydrus_core::media_viewer::ShowAction], a| {
        actions
            .iter()
            .position(|x| *x == a)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(0)
    };
    window.set_media_action(position(capability.media, v.media_show_action));
    window.set_preview_action(position(capability.preview, v.preview_show_action));
    window.set_media_paused(v.media_start_paused);
    window.set_media_embed(v.media_start_with_embed);
    window.set_preview_paused(v.preview_start_paused);
    window.set_preview_embed(v.preview_start_with_embed);
    let scale = |a: ScaleAction| index_of(&SCALES, a);
    window.set_media_up(scale(v.zoom.media_scale_up));
    window.set_media_down(scale(v.zoom.media_scale_down));
    window.set_preview_up(scale(v.zoom.preview_scale_up));
    window.set_preview_down(scale(v.zoom.preview_scale_down));
    window.set_exact_zooms(v.zoom.exact_zooms_only);
    window.set_up_quality(index_of(&SCALE_UP_QUALITIES, v.zoom.scale_up_quality));
    window.set_down_quality(index_of(&SCALE_DOWN_QUALITIES, v.zoom.scale_down_quality));
    let chosen = move |w: &MediaViewWindow| -> MediaView {
        let action = |actions: &[hydrus_core::media_viewer::ShowAction], i: i32, old| {
            usize::try_from(i)
                .ok()
                .and_then(|i| actions.get(i))
                .copied()
                .unwrap_or(old)
        };
        let mut view = v;
        view.media_show_action =
            action(capability.media, w.get_media_action(), v.media_show_action);
        view.preview_show_action = action(
            capability.preview,
            w.get_preview_action(),
            v.preview_show_action,
        );
        view.media_start_paused = w.get_media_paused();
        view.media_start_with_embed = w.get_media_embed();
        view.preview_start_paused = w.get_preview_paused();
        view.preview_start_with_embed = w.get_preview_embed();
        view.zoom.media_scale_up = pick(&SCALES, w.get_media_up(), v.zoom.media_scale_up);
        view.zoom.media_scale_down = pick(&SCALES, w.get_media_down(), v.zoom.media_scale_down);
        view.zoom.preview_scale_up = pick(&SCALES, w.get_preview_up(), v.zoom.preview_scale_up);
        view.zoom.preview_scale_down =
            pick(&SCALES, w.get_preview_down(), v.zoom.preview_scale_down);
        view.zoom.exact_zooms_only = w.get_exact_zooms();
        view.zoom.scale_up_quality = pick(
            &SCALE_UP_QUALITIES,
            w.get_up_quality(),
            v.zoom.scale_up_quality,
        );
        view.zoom.scale_down_quality = pick(
            &SCALE_DOWN_QUALITIES,
            w.get_down_quality(),
            v.zoom.scale_down_quality,
        );
        view
    };
    let update = Rc::new({
        let weak = window.as_weak();
        let code = row.code;
        let live = live.clone();
        move || {
            if !live.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let view = chosen(&w);
            let e = model::enabled(code, view.media_show_action, view.preview_show_action);
            w.set_media_paused_enabled(e.media_paused);
            w.set_media_embed_enabled(e.media_embed);
            w.set_preview_paused_enabled(e.preview_paused);
            w.set_preview_embed_enabled(e.preview_embed);
            w.set_media_scales_enabled(e.media_scales);
            w.set_preview_scales_enabled(e.preview_scales);
            w.set_exact_zooms_enabled(e.exact_zooms);
            w.set_qualities_enabled(e.qualities);
        }
    });
    update();
    window.on_actions_changed({
        let update = update.clone();
        move || update()
    });
    window.on_apply({
        let weak = window.as_weak();
        let code = row.code;
        let live = live.clone();
        move || {
            if !live.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade().filter(|w| w.window().is_visible()) {
                let view = chosen(&w);
                let _ = w.hide();
                done(Row { code, view });
            }
        }
    });
    let cancel = Rc::new({
        let weak = window.as_weak();
        move || {
            let was_live = live.replace(false);
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if was_live {
                changed();
            }
        }
    });
    window.on_cancel({
        let cancel = cancel.clone();
        move || cancel()
    });
    window.window().on_close_requested(move || {
        cancel();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().ok()?;
    Some(window)
}

pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
) -> Binding {
    let table = Rc::new(RefCell::new(Table::new(
        &editor.borrow().edited_media_views(),
    )));
    let children: Rc<RefCell<Children>> = Rc::default();
    let child_open = {
        let children = children.clone();
        move || {
            let c = children.borrow();
            c.chooser.as_ref().is_some_and(|w| w.window().is_visible())
                || c.editor.as_ref().is_some_and(|w| w.window().is_visible())
        }
    };
    let has_open: Rc<dyn Fn() -> bool> = Rc::new(child_open.clone());
    let show: Rc<dyn Fn()> = Rc::new({
        let table = table.clone();
        let weak = window.as_weak();
        let child_open = child_open.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let table = table.borrow();
            window.set_media_view_rows(ModelRc::new(VecModel::from(
                table
                    .rows
                    .iter()
                    .map(|row| TableRow {
                        cells: strings(row.cells()),
                        selected: table.selection.is_selected(row.code),
                    })
                    .collect::<Vec<_>>(),
            )));
            window.set_media_view_single(table.selected().is_some());
            window.set_media_view_can_add(!table.addable().is_empty());
            window.set_media_view_can_delete(table.can_delete());
            window.set_media_view_sort_column(i32::try_from(table.sort_column).unwrap_or(0));
            window.set_media_view_ascending(table.ascending);
            window.set_media_view_child_open(child_open());
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new({
        let children = children.clone();
        move || {
            let mut c = children.borrow_mut();
            c.live.set(false);
            if let Some(w) = c.chooser.take() {
                let _ = w.hide();
            }
            if let Some(w) = c.editor.take() {
                let _ = w.hide();
            }
        }
    });
    window.on_media_view_clicked({
        let (table, show, active) = (table.clone(), show.clone(), active.clone());
        let child_open = child_open.clone();
        move |i, c, s| {
            if active.get()
                && !child_open()
                && let Ok(i) = usize::try_from(i)
            {
                table.borrow_mut().click(i, c, s);
                show();
            }
        }
    });
    window.on_media_view_sort({
        let (table, show, active) = (table.clone(), show.clone(), active.clone());
        move |i, a| {
            if active.get()
                && let Ok(i) = usize::try_from(i)
            {
                table.borrow_mut().sort(i, a);
                show();
            }
        }
    });
    window.on_media_view_activated({
        let weak = window.as_weak();
        move |i| {
            if let Some(window) = weak.upgrade() {
                window.invoke_media_view_clicked(i, false, false);
                window.invoke_media_view_action("edit".into());
            }
        }
    });
    // (an edited row goes into the draft)
    let accept: Rc<dyn Fn(Row)> = Rc::new({
        let parent = window.as_weak();
        let (table, show, editor, active) =
            (table.clone(), show.clone(), editor.clone(), active.clone());
        move |row| {
            if !active.get()
                || !parent
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
            {
                show();
                return;
            }
            table.borrow_mut().put(row);
            editor.borrow_mut().set_media_views(table.borrow().values());
            show();
        }
    });
    window.on_media_view_action({
        let parent = window.as_weak();
        let children = children.clone();
        let (table, show, editor, active) =
            (table.clone(), show.clone(), editor.clone(), active.clone());
        move |action| {
            if !active.get()
                || !parent
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
                || child_open()
            {
                return;
            }
            match action.as_str() {
                "delete" => {
                    table.borrow_mut().delete_selected();
                    editor.borrow_mut().set_media_views(table.borrow().values());
                    show();
                }
                "edit" => {
                    let Some(row) = table.borrow().selected().cloned() else {
                        return;
                    };
                    let live = children.borrow_mut().start();
                    children.borrow_mut().editor = edit(
                        &row,
                        "edit media view options information",
                        live,
                        show.clone(),
                        accept.clone(),
                    );
                    show();
                }
                "add" => {
                    let addable = table.borrow().addable();
                    let codes: Vec<u8> = addable.iter().map(|(_, c)| *c).collect();
                    let live = children.borrow_mut().start();
                    let (table, accept, children_after, active, show_after) = (
                        table.clone(),
                        accept.clone(),
                        children.clone(),
                        active.clone(),
                        show.clone(),
                    );
                    let parent = parent.clone();
                    let chooser = crate::choice_buttons::open(
                        &crate::choice_buttons::Ask {
                            title: "select the filetype to add",
                            message: "",
                            choices: addable.into_iter().map(|(p, _)| p).collect(),
                            no_label: "",
                        },
                        move |choice| {
                            if !active.get()
                                || !live.get()
                                || !parent
                                    .upgrade()
                                    .is_some_and(|window| window.window().is_visible())
                            {
                                live.set(false);
                                show_after();
                                return;
                            }
                            let row = choice
                                .and_then(|i| codes.get(i).copied())
                                .and_then(|code| table.borrow().new_row(code));
                            if let Some(row) = row {
                                children_after.borrow_mut().editor = edit(
                                    &row,
                                    "add media view options information",
                                    live,
                                    show_after.clone(),
                                    accept,
                                );
                            } else {
                                live.set(false);
                            }
                            show_after();
                        },
                    );
                    if let Ok(chooser) = chooser {
                        children.borrow_mut().chooser = chooser;
                    }
                    show();
                }
                _ => {}
            }
        }
    });
    Binding {
        show,
        cancel,
        has_open,
        #[cfg(test)]
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::{media_viewer::MediaViewerSettings, mime::Mime};
    use hydrus_gui_model::options::Settings;
    use hydrus_store::{Store, settings};
    use slint::{ComponentHandle, platform::WindowAdapter as _};

    #[test]
    fn media_children_retire_stale_callbacks_and_stage_only_current_apply() {
        let windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let initial = store.read(Settings::load).unwrap();
        let original = initial.media_viewer.clone();
        let editor = Rc::new(RefCell::new(Editor::new(initial)));
        let active = Rc::new(Cell::new(true));
        let window = crate::app_title::new::<crate::OptionsWindow>().unwrap();
        window.show().unwrap();
        let binding = bind(&window, &editor, &active);
        (binding.show)();
        let code = Mime::GeneralVideo.code();
        let table = Table::new(&editor.borrow().edited_media_views());
        let index = table.rows.iter().position(|row| row.code == code).unwrap();
        let before = table.rows[index].view;
        window.invoke_media_view_clicked(i32::try_from(index).unwrap(), false, false);
        window.invoke_media_view_action("edit".into());
        let cancelled = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        assert!((binding.has_open)() && window.get_media_view_child_open());
        cancelled.set_media_paused(!before.media_start_paused);
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        assert_eq!(editor.borrow().edited_media_views()[&code], before);
        assert!(!(binding.has_open)() && !window.get_media_view_child_open());

        window.invoke_media_view_action("edit".into());
        let closed = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        closed.set_media_paused(!before.media_start_paused);
        let _ = closed
            .window()
            .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
        closed.invoke_apply();
        assert_eq!(editor.borrow().edited_media_views()[&code], before);

        window.invoke_media_view_action("edit".into());
        let hidden_parent_child = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        hidden_parent_child.set_media_paused(!before.media_start_paused);
        window.hide().unwrap();
        hidden_parent_child.invoke_apply();
        assert!(!window.get_media_view_child_open());
        assert_eq!(editor.borrow().edited_media_views()[&code], before);
        assert_eq!(
            store.read(settings::get::<MediaViewerSettings>).unwrap(),
            original
        );
        let count = windows.count();
        window.invoke_media_view_action("add".into());
        assert_eq!(
            windows.count(),
            count,
            "a hidden Options owner cannot open children"
        );
        window.show().unwrap();
        hidden_parent_child.show().unwrap();
        hidden_parent_child.invoke_apply();
        assert_eq!(editor.borrow().edited_media_views()[&code], before);
        hidden_parent_child.hide().unwrap();

        window.invoke_media_view_action("add".into());
        let hidden_parent_chooser = binding
            .children
            .borrow()
            .chooser
            .as_ref()
            .unwrap()
            .clone_strong();
        window.hide().unwrap();
        let count = windows.count();
        hidden_parent_chooser.invoke_chosen(0);
        assert!(!window.get_media_view_child_open());
        assert_eq!(
            windows.count(),
            count,
            "a pending Add answer needs a visible parent"
        );
        assert_eq!(editor.borrow().edited_media_views()[&code], before);
        window.show().unwrap();
        hidden_parent_chooser.show().unwrap();
        hidden_parent_chooser.invoke_chosen(0);
        assert_eq!(windows.count(), count);
        hidden_parent_chooser.hide().unwrap();

        window.invoke_media_view_action("edit".into());
        let current = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        current.set_media_paused(!before.media_start_paused);
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        closed.invoke_apply();
        assert!(current.window().is_visible());
        assert_eq!(editor.borrow().edited_media_views()[&code], before);
        current.invoke_apply();
        let mut staged = before;
        staged.media_start_paused = !before.media_start_paused;
        assert_eq!(editor.borrow().edited_media_views()[&code], staged);
        current.set_media_paused(before.media_start_paused);
        current.invoke_apply();
        assert_eq!(editor.borrow().edited_media_views()[&code], staged);
        assert_eq!(
            store.read(settings::get::<MediaViewerSettings>).unwrap(),
            original
        );
        let (after, before_settings, problems) = {
            let editor = editor.borrow();
            let (after, before, problems) = editor.applied();
            (after, before.clone(), problems)
        };
        assert!(problems.is_empty());
        store
            .write(move |ctx| after.save(ctx.conn(), &before_settings))
            .unwrap();
        let saved = store.read(settings::get::<MediaViewerSettings>).unwrap();
        assert_eq!(saved.view(Mime::VideoMp4), staged);

        window.invoke_media_view_action("add".into());
        let retired_chooser = binding
            .children
            .borrow()
            .chooser
            .as_ref()
            .unwrap()
            .clone_strong();
        active.set(false);
        (binding.cancel)();
        current.invoke_apply();
        assert_eq!(editor.borrow().edited_media_views()[&code], staged);
        assert_eq!(
            store.read(settings::get::<MediaViewerSettings>).unwrap(),
            saved
        );

        // A new Options owner still permits its current Add path, while old callbacks stay inert.
        let successor = crate::app_title::new::<crate::OptionsWindow>().unwrap();
        successor.show().unwrap();
        let successor_editor = Rc::new(RefCell::new(Editor::new(
            store.read(Settings::load).unwrap(),
        )));
        let successor_active = Rc::new(Cell::new(true));
        let successor_binding = bind(&successor, &successor_editor, &successor_active);
        (successor_binding.show)();
        successor.invoke_media_view_action("add".into());
        let chooser = successor_binding
            .children
            .borrow()
            .chooser
            .as_ref()
            .unwrap()
            .clone_strong();
        let addable = Table::new(&successor_editor.borrow().edited_media_views()).addable();
        let index = addable
            .iter()
            .position(|(_, code)| *code == Mime::ImageJpeg.code())
            .unwrap();
        let count = windows.count();
        retired_chooser.invoke_chosen(0);
        assert_eq!(
            windows.count(),
            count,
            "a retired chooser cannot spawn an editor"
        );
        assert!(chooser.window().is_visible());
        chooser.invoke_chosen(i32::try_from(index).unwrap());
        let added = successor_binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        let inherited = saved.view(Mime::ImageJpeg);
        added.set_media_up(index_of(&SCALES, ScaleAction::Full));
        added.invoke_apply();
        let mut override_view = inherited;
        override_view.zoom.media_scale_up = ScaleAction::Full;
        assert_eq!(
            successor_editor.borrow().edited_media_views()[&Mime::ImageJpeg.code()],
            override_view
        );
        assert!(!saved.media_view.contains_key(&Mime::ImageJpeg.code()));
        successor_active.set(false);
        (successor_binding.cancel)();
        added.invoke_apply();
        assert_eq!(
            store.read(settings::get::<MediaViewerSettings>).unwrap(),
            saved,
            "parent cancellation discards staged additions"
        );
    }

    // leaf: audit-options-media-playback-per-filetype-handling-add
    // leaf: audit-options-media-playback-per-filetype-handling-edit
    // leaf: audit-options-media-playback-per-filetype-handling-delete
    #[test]
    fn added_edited_and_deleted_filetype_handling_reaches_the_viewer_s_zoom() {
        let _windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let jpeg = Mime::ImageJpeg.code();
        let saved = || store.read(settings::get::<MediaViewerSettings>).unwrap();
        // File > options..., opened afresh on what is saved.
        let open = || {
            let editor = Rc::new(RefCell::new(Editor::new(
                store.read(Settings::load).unwrap(),
            )));
            let window = crate::app_title::new::<crate::OptionsWindow>().unwrap();
            window.show().unwrap();
            let binding = bind(&window, &editor, &Rc::new(Cell::new(true)));
            (binding.show)();
            (editor, window, binding)
        };
        // Apply: what the viewer opens with afterwards.
        let apply = |editor: &Rc<RefCell<Editor>>| {
            let (after, before, problems) = {
                let editor = editor.borrow();
                let (after, before, problems) = editor.applied();
                (after, before.clone(), problems)
            };
            assert!(problems.is_empty());
            store
                .write(move |ctx| after.save(ctx.conn(), &before))
                .unwrap();
        };
        // a small picture in a bigger window, as the viewer's zoom has it
        let rect = || {
            crate::zoom::Zoom::new(saved(), Mime::ImageJpeg, Some((300, 200)), (1000, 750), 1.0)
                .rect()
        };
        let rows = |editor: &Rc<RefCell<Editor>>| editor.borrow().edited_media_views();
        let click = |window: &OptionsWindow, editor: &Rc<RefCell<Editor>>| {
            let table = Table::new(&rows(editor));
            let index = table.rows.iter().position(|row| row.code == jpeg).unwrap();
            window.invoke_media_view_clicked(i32::try_from(index).unwrap(), false, false);
        };
        let fitted = rect();
        assert_eq!(fitted, (0, 41, 1000, 667), "scaled up to the window");

        // add: the filetype chosen from those without a row, then its editor
        let (editor, window, binding) = open();
        assert!(
            !rows(&editor).contains_key(&jpeg),
            "jpeg has no row of its own"
        );
        window.invoke_media_view_action("add".into());
        let chooser = binding
            .children
            .borrow()
            .chooser
            .as_ref()
            .unwrap()
            .clone_strong();
        let addable = Table::new(&rows(&editor)).addable();
        let at = addable.iter().position(|(_, code)| *code == jpeg).unwrap();
        chooser.invoke_chosen(i32::try_from(at).unwrap());
        let added = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(
            added.get_window_title(),
            "add media view options information"
        );
        added.set_media_up(index_of(&SCALES, ScaleAction::Full));
        added.invoke_apply();
        assert_eq!(rows(&editor)[&jpeg].zoom.media_scale_up, ScaleAction::Full);
        assert_eq!(rect(), fitted, "nothing until the options are applied");
        apply(&editor);
        assert_eq!(
            saved().media_view[&jpeg].zoom.media_scale_up,
            ScaleAction::Full
        );
        assert_eq!(rect(), (350, 275, 300, 200), "shown as it is");

        // edit: the row chosen, its editor on its options
        let (editor, window, binding) = open();
        click(&window, &editor);
        window.invoke_media_view_action("edit".into());
        let edit = binding
            .children
            .borrow()
            .editor
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(
            edit.get_window_title(),
            "edit media view options information"
        );
        assert_eq!(edit.get_media_up(), index_of(&SCALES, ScaleAction::Full));
        edit.set_media_up(index_of(&SCALES, ScaleAction::ToCanvas));
        edit.invoke_apply();
        apply(&editor);
        assert_eq!(
            saved().media_view[&jpeg].zoom.media_scale_up,
            ScaleAction::ToCanvas
        );
        assert_eq!(rect(), fitted, "scaled up again");

        // delete: the selected row goes, and the filetype takes its class's
        let (editor, window, _binding) = open();
        click(&window, &editor);
        window.invoke_media_view_action("delete".into());
        assert!(!rows(&editor).contains_key(&jpeg));
        assert!(saved().media_view.contains_key(&jpeg), "not before apply");
        apply(&editor);
        assert!(!saved().media_view.contains_key(&jpeg));
        assert_eq!(rect(), fitted);
    }

    #[test]
    fn options_apply_waits_for_current_media_child_to_close() {
        let windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let slot = Rc::default();
        let window = crate::options_window::open(
            &store,
            &slot,
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &crate::tag_suggestions_window::Slots::default(),
            &crate::external_call_window::Slots::default(),
            &crate::options_open_externally::Slots::default(),
            Rc::new(|| {}),
        )
        .unwrap();
        let table = Table::new(
            &store
                .read(settings::get::<MediaViewerSettings>)
                .unwrap()
                .media_view,
        );
        let index = table
            .rows
            .iter()
            .position(|row| row.code == Mime::GeneralVideo.code())
            .unwrap();
        window.invoke_media_view_clicked(i32::try_from(index).unwrap(), false, false);
        window.invoke_media_view_action("edit".into());
        assert!(window.get_media_view_child_open());
        window.invoke_apply();
        assert!(
            window.window().is_visible(),
            "Options cannot apply while its modal child is open"
        );
        let child = windows.get(windows.count() - 1).unwrap();
        let _ = child
            .window()
            .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
        assert!(!window.get_media_view_child_open());
        window.invoke_apply();
        assert!(
            !window.window().is_visible(),
            "Options apply succeeds after child cancellation"
        );
    }
}
