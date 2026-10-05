//! Options owns the set draft; the set owns its detached command capture.
use crate::{OptionsWindow, ShortcutCommandWindow, ShortcutSetWindow, TableRow};
use hydrus_core::shortcuts::{Binding as Command, Gesture, Settings};
use hydrus_gui_model::{
    list_selection::ListSelection,
    options::Editor,
    shortcut_capture::{Capture, Wheel, commands},
    shortcut_sets as sets,
};
use slint::winit_030::{
    EventResult, WinitWindowAccessor as _,
    winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
thread_local! {
    static LAST_SET:RefCell<Option<slint::Weak<ShortcutSetWindow>>>=const {RefCell::new(None)};
    static LAST_COMMAND:RefCell<Option<slint::Weak<ShortcutCommandWindow>>>=const {RefCell::new(None)};
    static WHEEL:RefCell<Wheel>=RefCell::new(Wheel::default());
}
pub fn last_set() -> Option<ShortcutSetWindow> {
    LAST_SET.with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
}
pub fn last_command() -> Option<ShortcutCommandWindow> {
    LAST_COMMAND.with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
}
struct Owned<T> {
    window: T,
    live: Rc<Cell<bool>>,
}
#[derive(Default)]
struct Slots {
    set: RefCell<Option<Owned<ShortcutSetWindow>>>,
    command: RefCell<Option<Owned<ShortcutCommandWindow>>>,
    /// A question or message the set lists asked.
    chooser: RefCell<Option<crate::ChoiceButtonsWindow>>,
    /// Whether that question waits for its answer.
    asking: Cell<bool>,
    reserved: RefCell<ListSelection<usize>>,
    custom: RefCell<ListSelection<usize>>,
}
impl Slots {
    fn cancel_command(&self) {
        if let Some(child) = self.command.borrow_mut().take() {
            child.live.set(false);
            let _ = child.window.hide();
        }
    }
    fn cancel(&self) {
        self.cancel_command();
        self.asking.set(false);
        if let Some(chooser) = self.chooser.borrow_mut().take() {
            let _ = chooser.hide();
        }
        if let Some(child) = self.set.borrow_mut().take() {
            child.live.set(false);
            let _ = child.window.hide();
        }
    }
}
pub(crate) struct Owner {
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
pub(crate) fn bind(
    parent: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
    other_open: Rc<dyn Fn() -> bool>,
) -> Owner {
    let slots = Rc::new(Slots::default());
    let settings = editor.borrow().edited_shortcuts();
    parent.set_shortcuts_merge_numpad(settings.merge_numpad);
    parent.set_shortcuts_primary_labels(settings.primary_labels);
    parent.on_shortcuts_policy({
        let editor = editor.clone();
        let active = active.clone();
        let slots = slots.clone();
        let other_open = other_open.clone();
        let weak = parent.as_weak();
        move |merge, primary| {
            if !active.get()
                || slots.set.borrow().is_some()
                || other_open()
                || !weak
                    .upgrade()
                    .is_some_and(|parent| parent.window().is_visible())
            {
                return;
            }
            let mut settings = editor.borrow().edited_shortcuts();
            settings.merge_numpad = merge;
            settings.primary_labels = primary;
            editor.borrow_mut().set_shortcuts(settings);
        }
    });
    let paint: Rc<dyn Fn()> = Rc::new({
        let editor = editor.clone();
        let slots = slots.clone();
        let weak = parent.as_weak();
        move || {
            if let Some(parent) = weak.upgrade() {
                paint_lists(&parent, &editor.borrow().edited_shortcuts(), &slots);
            }
        }
    });
    paint();
    let usable: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let slots = slots.clone();
        let weak = parent.as_weak();
        move || {
            active.get()
                && slots.set.borrow().is_none()
                && !slots.asking.get()
                && !other_open()
                && weak
                    .upgrade()
                    .is_some_and(|parent| parent.window().is_visible())
        }
    });
    parent.on_shortcut_set_clicked({
        let editor = editor.clone();
        let slots = slots.clone();
        let usable = usable.clone();
        let paint = paint.clone();
        move |custom, row, ctrl, shift| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            if !usable() {
                return;
            }
            let settings = editor.borrow().edited_shortcuts();
            let count = if custom {
                sets::custom_rows(&settings).len()
            } else {
                sets::reserved_rows(&settings).len()
            };
            if row >= count {
                return;
            }
            let selection = if custom {
                &slots.custom
            } else {
                &slots.reserved
            };
            let order: Vec<usize> = (0..count).collect();
            selection.borrow_mut().click(&order, row, ctrl, shift);
            paint();
        }
    });
    let edit: Rc<dyn Fn(Target)> = Rc::new({
        let editor = editor.clone();
        let active = active.clone();
        let slots = slots.clone();
        let weak = parent.as_weak();
        let paint = paint.clone();
        move |target| {
            if let Ok(window) = open_set(
                editor.clone(),
                &active,
                &slots,
                &weak,
                target,
                paint.clone(),
            ) {
                *slots.set.borrow_mut() = Some(window);
                if let Some(parent) = weak.upgrade() {
                    parent.set_shortcuts_child_open(true);
                }
            }
        }
    });
    let name_at = {
        let editor = editor.clone();
        move |custom: bool, row: usize| -> Option<String> {
            let settings = editor.borrow().edited_shortcuts();
            let rows = if custom {
                sets::custom_rows(&settings)
            } else {
                sets::reserved_rows(&settings)
            };
            rows.get(row).map(|r| r.name.clone())
        }
    };
    parent.on_shortcut_set_activated({
        let usable = usable.clone();
        let edit = edit.clone();
        let name_at = name_at.clone();
        move |custom, row| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            if !usable() {
                return;
            }
            if let Some(name) = name_at(custom, row) {
                edit(if custom {
                    Target::Custom(Some(name))
                } else {
                    Target::Reserved(name)
                });
            }
        }
    });
    parent.on_shortcut_set_action({
        let editor = editor.clone();
        let slots = slots.clone();
        let usable = usable.clone();
        let paint = paint.clone();
        move |action| {
            if !usable() {
                return;
            }
            let top = |custom: bool| {
                let selection = if custom {
                    &slots.custom
                } else {
                    &slots.reserved
                };
                let first = selection.borrow().selected_order().iter().min().copied();
                first.and_then(|row| name_at(custom, row))
            };
            match action.as_str() {
                "help" => crate::debug_actions::message("Information", sets::HELP),
                "edit-reserved" => {
                    if let Some(name) = top(false) {
                        edit(Target::Reserved(name));
                    }
                }
                "edit-custom" => {
                    if let Some(name) = top(true) {
                        edit(Target::Custom(Some(name)));
                    }
                }
                "add" => edit(Target::Custom(None)),
                "delete" => {
                    let names: Vec<String> = slots
                        .custom
                        .borrow()
                        .selected_order()
                        .iter()
                        .filter_map(|&row| name_at(true, row))
                        .collect();
                    if names.is_empty() {
                        return;
                    }
                    let editor = editor.clone();
                    let paint = paint.clone();
                    let after = Rc::downgrade(&slots);
                    ask(
                        &slots,
                        sets::DELETE_QUESTION,
                        vec!["yes".into()],
                        "no",
                        move |yes| {
                            if yes == Some(0) {
                                let mut settings = editor.borrow().edited_shortcuts();
                                sets::delete_custom(&mut settings, &names);
                                editor.borrow_mut().set_shortcuts(settings);
                                if let Some(slots) = after.upgrade() {
                                    *slots.custom.borrow_mut() = ListSelection::default();
                                }
                                paint();
                            }
                        },
                    );
                }
                "restore" => {
                    let names = sets::default_names();
                    let editor = editor.clone();
                    let paint = paint.clone();
                    let after = Rc::downgrade(&slots);
                    ask(
                        &slots,
                        sets::RESTORE_TITLE,
                        names.clone(),
                        "cancel",
                        move |chosen| {
                            let (Some(name), Some(slots)) =
                                (chosen.and_then(|i| names.get(i).cloned()), after.upgrade())
                            else {
                                return;
                            };
                            let settings = editor.borrow().edited_shortcuts();
                            let restore = {
                                let editor = editor.clone();
                                let paint = paint.clone();
                                let name = name.clone();
                                move || {
                                    let mut settings = editor.borrow().edited_shortcuts();
                                    sets::restore(&mut settings, &name);
                                    editor.borrow_mut().set_shortcuts(settings);
                                    paint();
                                }
                            };
                            match sets::restore_question(&settings, &name) {
                                sets::Restore::Missing(text) => {
                                    ask(&slots, &text, Vec::new(), "ok", move |_| restore());
                                }
                                sets::Restore::Replace(question) => {
                                    ask(&slots, &question, vec!["yes".into()], "no", move |yes| {
                                        if yes == Some(0) {
                                            restore();
                                        }
                                    });
                                }
                            }
                        },
                    );
                }
                _ => {}
            }
        }
    });
    Owner {
        cancel: Rc::new({
            let slots = slots.clone();
            let weak = parent.as_weak();
            move || {
                slots.cancel();
                if let Some(parent) = weak.upgrade() {
                    parent.set_shortcuts_child_open(false);
                }
            }
        }),
        has_open: Rc::new(move || slots.set.borrow().is_some()),
    }
}
fn paint_lists(parent: &OptionsWindow, settings: &Settings, slots: &Slots) {
    let rows = |rows: Vec<sets::SetRow>, selection: &ListSelection<usize>| {
        ModelRc::new(VecModel::from(
            rows.iter()
                .enumerate()
                .map(|(i, row)| TableRow {
                    cells: ModelRc::new(VecModel::from(
                        row.cells()
                            .into_iter()
                            .map(Into::into)
                            .collect::<Vec<slint::SharedString>>(),
                    )),
                    selected: selection.selected_order().contains(&i),
                })
                .collect::<Vec<_>>(),
        ))
    };
    let reserved = slots.reserved.borrow();
    let custom = slots.custom.borrow();
    parent.set_shortcut_reserved_rows(rows(sets::reserved_rows(settings), &reserved));
    parent.set_shortcut_custom_rows(rows(sets::custom_rows(settings), &custom));
    parent.set_shortcut_reserved_selected(!reserved.selected_order().is_empty());
    parent.set_shortcut_custom_selected(!custom.selected_order().is_empty());
}
/// Ask a question (or say something, with no choices) in a chooser the
/// set lists own until it is answered.
fn ask(
    slots: &Rc<Slots>,
    message: &str,
    choices: Vec<String>,
    no_label: &str,
    answer: impl FnOnce(Option<usize>) + 'static,
) {
    let after = Rc::downgrade(slots);
    let asked = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: "shortcuts",
            message,
            choices,
            no_label,
        },
        move |chosen| {
            let Some(slots) = after.upgrade() else { return };
            if !slots.asking.replace(false) {
                return;
            }
            answer(chosen);
        },
    );
    match asked {
        Ok(window) => {
            slots.asking.set(window.is_some());
            // (the answered chooser is kept until the next replaces it)
            if window.is_some() {
                *slots.chooser.borrow_mut() = window;
            }
        }
        Err(e) => eprintln!("could not ask: {e}"),
    }
}
/// Which set the editor edits.
#[derive(Clone)]
enum Target {
    Reserved(String),
    /// A custom set: its current name, or none for a new one.
    Custom(Option<String>),
}
impl Target {
    fn name(&self) -> String {
        match self {
            Self::Reserved(name) | Self::Custom(Some(name)) => name.clone(),
            Self::Custom(None) => sets::NEW_NAME.to_owned(),
        }
    }
}
fn show_set(window: &ShortcutSetWindow, bindings: &[Command], primary: bool) {
    let selected = window.get_selected();
    let rows = bindings
        .iter()
        .enumerate()
        .map(|(i, b)| TableRow {
            cells: ModelRc::new(VecModel::from(vec![
                b.gesture.text(primary).into(),
                sets::command_text(b).into(),
            ])),
            selected: i32::try_from(i).ok() == Some(selected),
        })
        .collect::<Vec<_>>();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
}
#[allow(clippy::too_many_lines)]
fn open_set(
    editor: Rc<RefCell<Editor>>,
    parent_live: &Rc<Cell<bool>>,
    slots: &Rc<Slots>,
    parent: &slint::Weak<OptionsWindow>,
    target: Target,
    saved: Rc<dyn Fn()>,
) -> Result<Owned<ShortcutSetWindow>, slint::PlatformError> {
    let window = ShortcutSetWindow::new()?;
    let live = Rc::new(Cell::new(true));
    let settings = editor.borrow().edited_shortcuts();
    let name = target.name();
    let (merge, primary) = (settings.merge_numpad, settings.primary_labels);
    let draft = Rc::new(RefCell::new(
        settings.sets.get(&name).cloned().unwrap_or_default(),
    ));
    window.set_set_name(name.as_str().into());
    window.set_name_enabled(matches!(target, Target::Custom(_)));
    window.set_description(sets::description(&name).unwrap_or_default().into());
    show_set(&window, &draft.borrow(), primary);
    let close: Rc<dyn Fn()> = Rc::new({
        let slots = Rc::downgrade(slots);
        let live = live.clone();
        let weak = window.as_weak();
        let parent = parent.clone();
        move || {
            if !live.replace(false) {
                return;
            }
            if let Some(slots) = slots.upgrade() {
                slots.cancel_command();
                slots.set.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(parent) = parent.upgrade() {
                parent.set_shortcuts_child_open(false);
            }
        }
    });
    window.on_selected_row({
        let weak = window.as_weak();
        let draft = draft.clone();
        let live = live.clone();
        let parent_live = parent_live.clone();
        let slots = slots.clone();
        move |row| {
            if !live.get() || !parent_live.get() || slots.command.borrow().is_some() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                window.set_selected(row);
                show_set(&window, &draft.borrow(), primary);
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let draft = draft.clone();
        let live = live.clone();
        let parent_live = parent_live.clone();
        let slots = slots.clone();
        let name = name.clone();
        move |action| {
            if !live.get() || !parent_live.get() || slots.command.borrow().is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_remove_question() {
                return;
            }
            let selected = usize::try_from(window.get_selected()).ok();
            if action == "remove" {
                window.set_remove_question(true);
                return;
            }
            let index = if action == "add" {
                None
            } else if action == "edit" {
                selected
            } else {
                return;
            };
            let value = index
                .and_then(|i| draft.borrow().get(i).cloned())
                .unwrap_or(Command {
                    gesture: Gesture::default(),
                    action: commands(&name)[0].0,
                    text: None,
                });
            if let Ok(child) = open_command(
                value,
                &name,
                merge,
                primary,
                parent_live.clone(),
                live.clone(),
                &slots,
                Rc::new({
                    let weak = weak.clone();
                    let draft = draft.clone();
                    move |value| {
                        let mut bindings = draft.borrow_mut();
                        if let Some(index) = index {
                            bindings[index] = value;
                        } else {
                            bindings.push(value);
                        }
                        if let Some(window) = weak.upgrade() {
                            window.set_selected(
                                i32::try_from(index.unwrap_or(bindings.len() - 1)).unwrap_or(-1),
                            );
                            show_set(&window, &bindings, primary);
                        }
                    }
                }),
            ) {
                *slots.command.borrow_mut() = Some(child);
                window.set_child_open(true);
            }
        }
    });
    window.on_remove_chosen({
        let weak = window.as_weak();
        let draft = draft.clone();
        let live = live.clone();
        let parent_live = parent_live.clone();
        move |yes| {
            if !live.get() || !parent_live.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.get_remove_question() {
                return;
            }
            window.set_remove_question(false);
            if yes && let Ok(index) = usize::try_from(window.get_selected()) {
                let mut bindings = draft.borrow_mut();
                if index < bindings.len() {
                    bindings.remove(index);
                }
                window.set_selected(-1);
                show_set(&window, &bindings, primary);
            }
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let draft = draft.clone();
        let live = live.clone();
        let parent_live = parent_live.clone();
        let slots = slots.clone();
        let close = close.clone();
        move || {
            if !live.get() || !parent_live.get() || slots.command.borrow().is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_remove_question() {
                return;
            }
            let bindings = draft.borrow().clone();
            for (index, binding) in bindings.iter().enumerate() {
                if let Some(previous) = bindings[..index].iter().find(|b| b.gesture == binding.gesture) {
                    window.set_error(
                        format!(
                            "The shortcut:\n\n{}\n\nis mapped twice:\n\n{}\n\n{}\n\nThe system only supports one command per shortcut in a set for now, please remove one.",
                            binding.gesture.text(primary),
                            sets::command_text(binding),
                            sets::command_text(previous)
                        )
                        .into(),
                    );
                    return;
                }
            }
            let mut settings = editor.borrow().edited_shortcuts();
            match &target {
                Target::Reserved(name) => {
                    settings.sets.insert(name.clone(), bindings);
                }
                Target::Custom(old) => {
                    let chosen = window.get_set_name();
                    let chosen = if chosen.trim().is_empty() {
                        sets::NEW_NAME
                    } else {
                        chosen.as_str()
                    };
                    sets::save_custom(&mut settings, old.as_deref(), chosen, bindings);
                }
            }
            editor.borrow_mut().set_shortcuts(settings);
            close();
            saved();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || {
            close();
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    LAST_SET.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    window.show()?;
    Ok(Owned { window, live })
}
fn show_capture(window: &ShortcutCommandWindow, capture: &Capture, primary: bool) {
    window.set_keyboard_text(capture.keyboard.text(primary).into());
    window.set_mouse_text(capture.mouse.text(primary).into());
    window.set_mode(i32::from(capture.mouse_selected));
    window.set_release_choice(i32::from(capture.release));
    window.set_release_enabled(capture.mouse.appropriate_for_release());
}
// The capture child receives both owner guards and the two reference policies.
#[allow(clippy::too_many_arguments)]
fn open_command(
    value: Command,
    scope: &str,
    merge: bool,
    primary: bool,
    parent: Rc<Cell<bool>>,
    set_live: Rc<Cell<bool>>,
    slots: &Rc<Slots>,
    applied: Rc<dyn Fn(Command)>,
) -> Result<Owned<ShortcutCommandWindow>, slint::PlatformError> {
    let window = ShortcutCommandWindow::new()?;
    let live = Rc::new(Cell::new(true));
    let capture = Rc::new(RefCell::new(Capture::new(value.gesture, merge)));
    let actions = commands(scope);
    window.set_commands(ModelRc::new(VecModel::from(
        actions
            .iter()
            .map(|(_, name)| (*name).into())
            .collect::<Vec<_>>(),
    )));
    window.set_command_index(
        i32::try_from(
            actions
                .iter()
                .position(|(action, _)| *action == value.action)
                .unwrap_or(0),
        )
        .unwrap_or(0),
    );
    show_capture(&window, &capture.borrow(), primary);
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let live = live.clone();
        move || live.get() && parent.get() && set_live.get()
    });
    let update: Rc<dyn Fn(u8, u32, u8)> = Rc::new({
        let capture = capture.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |kind, key, bits| {
            if !valid() {
                return;
            }
            capture.borrow_mut().keyboard(kind, key, bits);
            if let Some(window) = weak.upgrade() {
                show_capture(&window, &capture.borrow(), primary);
            }
        }
    });
    window.on_key_capture({
        let update = update.clone();
        move |text, bits| {
            if let Some((kind, key, bits)) = crate::shortcut_input::slint_key(&text, bits as u8) {
                update(kind, key, bits);
            }
        }
    });
    window.on_mouse_capture({
        let capture = capture.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |key, press, bits| {
            if !valid() {
                return;
            }
            capture
                .borrow_mut()
                .mouse(key as u32, press as u8, bits as u8);
            if let Some(window) = weak.upgrade() {
                show_capture(&window, &capture.borrow(), primary);
            }
        }
    });
    window.on_double_capture({
        let capture = capture.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move || {
            if !valid() {
                return;
            }
            let mut capture = capture.borrow_mut();
            let key = capture.mouse.key;
            let bits = capture.mouse.bits();
            capture.mouse(key, 2, bits);
            if let Some(window) = weak.upgrade() {
                show_capture(&window, &capture, primary);
            }
        }
    });
    window.on_wheel_capture({
        let capture = capture.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |delta, bits| {
            if !valid() {
                return;
            }
            WHEEL.with(|wheel| {
                capture
                    .borrow_mut()
                    .wheel(delta as i32, bits as u8, &mut wheel.borrow_mut());
            });
            if let Some(window) = weak.upgrade() {
                show_capture(&window, &capture.borrow(), primary);
            }
        }
    });
    window.on_release_changed({
        let capture = capture.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |choice| {
            if !valid() {
                return;
            }
            capture.borrow_mut().choose_release(choice == 1);
            if let Some(window) = weak.upgrade() {
                show_capture(&window, &capture.borrow(), primary);
            }
        }
    });
    let mut input = crate::shortcut_input::Input::default();
    let mut cursor = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut previous: Option<(MouseButton, Instant, (f64, f64))> = None;
    window.window().on_winit_window_event({let valid=valid.clone();let weak=window.as_weak();move |platform,event|{
        if !valid(){return EventResult::Propagate;}input.observe(event);let Some(window)=weak.upgrade() else{return EventResult::Propagate;};
        if matches!(event,WindowEvent::KeyboardInput{event,..} if event.state==ElementState::Pressed) && window.get_mode()==0 && window.get_keyboard_focused(){if let Some((kind,key,bits))=input.pending.take(){update(kind,key,bits);}return EventResult::PreventDefault;}
        if let WindowEvent::CursorMoved{position,..}=event {cursor=(position.x/f64::from(platform.scale_factor()),position.y/f64::from(platform.scale_factor()));}
        let inside=cursor.0>=f64::from(window.get_capture_x()) && cursor.1>=f64::from(window.get_capture_y()) && cursor.0<f64::from(window.get_capture_x()+window.get_capture_width()) && cursor.1<f64::from(window.get_capture_y()+window.get_capture_height());
        if !inside{return EventResult::Propagate;}
        match event {
            WindowEvent::MouseInput{state,button,..}=>{
                let key=match button {MouseButton::Left=>0,MouseButton::Right=>1,MouseButton::Middle=>2,MouseButton::Back=>7,MouseButton::Forward=>8,MouseButton::Other(5 | 10 | 0x117)=>9,MouseButton::Other(_)=>return EventResult::PreventDefault};
                let mut press=u8::from(*state==ElementState::Released);
                if *state==ElementState::Pressed {let now=Instant::now();if previous.as_ref().is_some_and(|(old,time,position)|old==button && now.duration_since(*time)<=Duration::from_millis(400) && (position.0-cursor.0).abs()+(position.1-cursor.1).abs()<=5.0){press=2;previous=None;}else{previous=Some((*button,now,cursor));}}
                window.invoke_mouse_capture(key,i32::from(press),i32::from(input.bits));EventResult::PreventDefault
            }
            WindowEvent::MouseWheel{delta,..}=>{let delta=match delta {MouseScrollDelta::LineDelta(_,y)=>y*120.0,MouseScrollDelta::PixelDelta(point)=>point.y as f32};window.invoke_wheel_capture(delta,i32::from(input.bits));EventResult::PreventDefault}
            _=>EventResult::Propagate,
        }
    }});
    let close: Rc<dyn Fn()> = Rc::new({
        let slots = Rc::downgrade(slots);
        let weak = window.as_weak();
        let live = live.clone();
        move || {
            if !live.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slots) = slots.upgrade() {
                slots.command.borrow_mut().take();
                if let Some(set) = slots.set.borrow().as_ref() {
                    set.window.set_child_open(false);
                }
            }
        }
    });
    window.on_apply({
        let valid = valid.clone();
        let capture = capture.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some((action, _)) = usize::try_from(window.get_command_index())
                .ok()
                .and_then(|i| actions.get(i))
            else {
                return;
            };
            applied(Command {
                gesture: if window.get_mode() == 1 {
                    capture.borrow().mouse.clone()
                } else {
                    capture.borrow().keyboard.clone()
                },
                action: *action,
                text: None,
            });
            close();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || {
            close();
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    LAST_COMMAND.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    window.show()?;
    Ok(Owned { window, live })
}
