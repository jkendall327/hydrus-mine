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
}

/// The open chooser or editor.
#[derive(Default)]
struct Children {
    chooser: Option<ChoiceButtonsWindow>,
    editor: Option<MediaViewWindow>,
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
fn edit(row: &Row, title: &str, done: Rc<dyn Fn(Row)>) -> Option<MediaViewWindow> {
    let capability = model::capability(row.code)?;
    let window = MediaViewWindow::new().ok()?;
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
        move || {
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
        move || {
            if let Some(w) = weak.upgrade() {
                let view = chosen(&w);
                let _ = w.hide();
                done(Row { code, view });
            }
        }
    });
    window.on_cancel({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        }
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
        let (table, show, editor, active) =
            (table.clone(), show.clone(), editor.clone(), active.clone());
        move |row| {
            if !active.get() {
                return;
            }
            table.borrow_mut().put(row);
            editor.borrow_mut().set_media_views(table.borrow().values());
            show();
        }
    });
    window.on_media_view_action({
        let (table, show, editor, active) =
            (table.clone(), show.clone(), editor.clone(), active.clone());
        move |action| {
            if !active.get() || child_open() {
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
                    children.borrow_mut().editor =
                        edit(&row, "edit media view options information", accept.clone());
                }
                "add" => {
                    let addable = table.borrow().addable();
                    let codes: Vec<u8> = addable.iter().map(|(_, c)| *c).collect();
                    let (table, accept, children_after) =
                        (table.clone(), accept.clone(), children.clone());
                    let chooser = crate::choice_buttons::open(
                        &crate::choice_buttons::Ask {
                            title: "select the filetype to add",
                            message: "",
                            choices: addable.into_iter().map(|(p, _)| p).collect(),
                            no_label: "",
                        },
                        move |choice| {
                            let Some(code) = choice.and_then(|i| codes.get(i).copied()) else {
                                return;
                            };
                            let Some(row) = table.borrow().new_row(code) else {
                                return;
                            };
                            children_after.borrow_mut().editor =
                                edit(&row, "add media view options information", accept.clone());
                        },
                    );
                    if let Ok(chooser) = chooser {
                        children.borrow_mut().chooser = chooser;
                    }
                }
                _ => {}
            }
        }
    });
    Binding { show, cancel }
}
