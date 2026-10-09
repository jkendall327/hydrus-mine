//! The checker options editor's window, bound to its model
//! ([`checker_options::Editor`]): a reasonable default's button, the
//! static checkbox and "apply" show the editor again whole; a time's field
//! typed in leaves that field as typed, showing again only the times the
//! editor changed otherwise (never slower than moved up to never faster
//! than). "apply" asks first if the editor says to, and gives the checker
//! options to `done`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::subscriptions::CheckerOptions;

use crate::checker_options::{self as checker, Editor, Time, Which};
use crate::{CheckerOptionsWindow, CheckerTexts, DurationField};

const TIMES: [Which; 4] = [Which::Faster, Which::Slower, Which::Period, Which::Velocity];

/// (fields and counts, none below 0)
fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn fields(time: &Time) -> ModelRc<DurationField> {
    let fields: Vec<DurationField> = time
        .fields
        .iter()
        .zip(time.units)
        .map(|(&value, unit)| DurationField {
            value: int(value),
            maximum: int(unit.max()),
            label: unit.label().into(),
        })
        .collect();
    ModelRc::new(VecModel::from(fields))
}

/// The intended new files per check as the reference's spin box shows it.
fn intended_text(n: f64) -> SharedString {
    format!("{n:.2}").into()
}

/// What the window shows of each time, so a field typed in is left be.
#[derive(Default)]
struct Shown {
    times: [Vec<i64>; 4],
}

fn show_time(window: &CheckerOptionsWindow, editor: &Editor, shown: &mut Shown, which: Which) {
    let time = editor.time(which);
    let model = fields(time);
    match which {
        Which::Faster => window.set_faster_fields(model),
        Which::Slower => window.set_slower_fields(model),
        Which::Period => window.set_period_fields(model),
        Which::Velocity => window.set_velocity_fields(model),
    }
    shown.times[which as usize].clone_from(&time.fields);
}

/// Show the editor whole.
fn show_all(window: &CheckerOptionsWindow, editor: &Editor, shown: &mut Shown) {
    for which in TIMES {
        show_time(window, editor, shown, which);
    }
    window.set_flat(editor.flat);
    window.set_intended(intended_text(editor.intended));
    window.set_velocity_files(editor.velocity_files.to_string().into());
}

/// Show the times the editor has otherwise than shown.
fn show_changed(window: &CheckerOptionsWindow, editor: &Editor, shown: &mut Shown) {
    for which in TIMES {
        if shown.times[which as usize] != editor.time(which).fields {
            show_time(window, editor, shown, which);
        }
    }
}

fn which(i: i32) -> Option<Which> {
    usize::try_from(i).ok().and_then(|i| TIMES.get(i).copied())
}

/// Open the editor on `options` (its least times tiny if `advanced`); it
/// forgets itself from `slot` when closed, and gives what was applied to
/// `done`.
pub(crate) fn open(
    options: &CheckerOptions,
    advanced: bool,
    slot: &Rc<RefCell<Option<CheckerOptionsWindow>>>,
    done: &Rc<dyn Fn(CheckerOptions)>,
) -> Result<CheckerOptionsWindow, String> {
    let window = crate::app_title::new::<crate::CheckerOptionsWindow>().map_err(|e| e.to_string())?;
    window.set_texts(CheckerTexts {
        warning: checker::WARNING.into(),
        defaults_box: checker::DEFAULTS_BOX.into(),
        velocity_label: checker::VELOCITY_LABEL.into(),
        velocity_per: checker::VELOCITY_PER.into(),
        static_label: checker::STATIC_LABEL.into(),
        advanced_warning: checker::ADVANCED_WARNING.into(),
        reactive_box: checker::REACTIVE_BOX.into(),
        reactive_text: checker::REACTIVE_TEXT.into(),
        intended_label: checker::INTENDED_LABEL.into(),
        faster_label: checker::FASTER_LABEL.into(),
        slower_label: checker::SLOWER_LABEL.into(),
        static_box: checker::STATIC_BOX.into(),
        period_label: checker::PERIOD_LABEL.into(),
    });
    let presets: Vec<SharedString> = checker::PRESETS
        .iter()
        .map(|(label, _)| (*label).into())
        .collect();
    window.set_presets(ModelRc::new(VecModel::from(presets)));
    window.set_advanced(advanced);
    let editor = Rc::new(RefCell::new(Editor::new(options, advanced)));
    let shown = Rc::new(RefCell::new(Shown::default()));
    show_all(&window, &editor.borrow(), &mut shown.borrow_mut());

    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    // (each edit, then the window shown again as it says)
    let edited = {
        let editor = editor.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        move |whole: bool, edit: &dyn Fn(&mut Editor)| {
            let Some(window) = weak.upgrade() else { return };
            edit(&mut editor.borrow_mut());
            window.set_question(SharedString::new());
            let editor = editor.borrow();
            if whole {
                show_all(&window, &editor, &mut shown.borrow_mut());
            } else {
                show_changed(&window, &editor, &mut shown.borrow_mut());
            }
        }
    };
    let edited = Rc::new(edited);
    window.on_preset({
        let edited = edited.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                edited(true, &|e| e.preset(i));
            }
        }
    });
    window.on_flat_toggled({
        let edited = edited.clone();
        move || edited(true, &Editor::toggle_flat)
    });
    window.on_field_edited({
        let edited = edited.clone();
        let shown = shown.clone();
        move |time, field, n| {
            let (Some(which), Ok(field)) = (which(time), usize::try_from(field)) else {
                return;
            };
            // (the field shows what was typed)
            if let Some(f) = shown.borrow_mut().times[which as usize].get_mut(field) {
                *f = i64::from(n);
            }
            edited(false, &|e| e.set_field(which, field, i64::from(n)));
        }
    });
    window.on_intended_edited({
        let edited = edited.clone();
        move |text| {
            // (text that isn't a number yet is left be, as typed)
            if let Ok(n) = text.trim().parse::<f64>() {
                edited(false, &|e| e.set_intended(n));
            }
        }
    });
    window.on_velocity_files_edited({
        let edited = edited.clone();
        let editor = editor.clone();
        let weak = window.as_weak();
        move |n| {
            edited(false, &|e| e.set_velocity_files(i64::from(n)));
            // (past its range, it shows where it was held)
            let files = editor.borrow().velocity_files;
            if files != i64::from(n)
                && let Some(window) = weak.upgrade()
            {
                window.set_velocity_files(files.to_string().into());
            }
        }
    });
    window.on_ok({
        let editor = editor.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        let done = done.clone();
        let close = close.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let question = editor.borrow_mut().ok();
            show_all(&window, &editor.borrow(), &mut shown.borrow_mut());
            if let Some(question) = question {
                window.set_question(question.into());
            } else {
                done(editor.borrow().value());
                close();
            }
        }
    });
    window.on_answered({
        let editor = editor.clone();
        let weak = window.as_weak();
        let done = done.clone();
        let close = close.clone();
        move |yes| {
            if yes {
                done(editor.borrow().value());
                close();
            } else if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
