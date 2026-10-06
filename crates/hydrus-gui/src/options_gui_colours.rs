//! Options owns staged RGB pickers and the colour buttons' clipboard menu.
use crate::{GuiColourPickerWindow, GuiColourRow, OptionsWindow, SessionDialog};
use hydrus_gui_model::options::Editor;
use hydrus_store::{gui_colours, services::Rgb};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Default)]
struct Children {
    picker: Option<GuiColourPickerWindow>,
    notice: Option<SessionDialog>,
}
impl Drop for Children {
    fn drop(&mut self) {
        if let Some(picker) = self.picker.take() {
            let _ = picker.hide();
        }
        if let Some(notice) = self.notice.take() {
            let _ = notice.hide();
        }
    }
}
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
thread_local! { static LAST: RefCell<Option<slint::Weak<GuiColourPickerWindow>>> = const { RefCell::new(None) }; }
/// The most recent visible Options RGB picker, without extending its lifetime.
pub fn last_opened() -> Option<GuiColourPickerWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|window| window.window().is_visible())
}
fn rgb_channels(red: i32, green: i32, blue: i32) -> Option<Rgb> {
    Some(Rgb([
        u8::try_from(red).ok()?,
        u8::try_from(green).ok()?,
        u8::try_from(blue).ok()?,
    ]))
}
pub(crate) fn paste_rgb(text: &str) -> Option<Rgb> {
    let hex: String = text
        .strip_prefix('#')
        .unwrap_or(text)
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect();
    if hex.len() != 6 {
        return None;
    }
    let channel = |i| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Rgb([channel(0)?, channel(2)?, channel(4)?]))
}
pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
) -> Binding {
    let children = Rc::new(RefCell::new(Children::default()));
    let has_open: Rc<dyn Fn() -> bool> = Rc::new({
        let children = Rc::downgrade(&children);
        move || {
            children.upgrade().is_some_and(|children| {
                let children = children.borrow();
                children.picker.is_some() || children.notice.is_some()
            })
        }
    });
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let editor = editor.clone();
        let has_open = has_open.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let settings = editor.borrow().edited_gui_colours();
            let set = usize::try_from(window.get_gui_colour_tab())
                .ok()
                .filter(|set| *set < 2)
                .unwrap_or(0);
            let spans = [(0, 4), (4, 4), (8, 1), (9, 1), (10, 1), (11, 1), (12, 1)];
            let rows = gui_colours::ROW_LABELS
                .into_iter()
                .zip(spans)
                .map(|(label, (first, count))| {
                    let values = &settings.sets[set][first..first + count];
                    GuiColourRow {
                        label: label.into(),
                        first: i32::try_from(first).expect("thirteen roles"),
                        colours: ModelRc::new(VecModel::from(
                            values
                                .iter()
                                .map(|rgb| slint::Color::from_rgb_u8(rgb.0[0], rgb.0[1], rgb.0[2]))
                                .collect::<Vec<_>>(),
                        )),
                        hex: ModelRc::new(VecModel::from(
                            values
                                .iter()
                                .map(|rgb| rgb.to_string().to_ascii_lowercase().into())
                                .collect::<Vec<_>>(),
                        )),
                    }
                })
                .collect::<Vec<_>>();
            window.set_gui_colour_rows(ModelRc::new(VecModel::from(rows)));
            window.set_gui_colour_enabled(settings.override_stylesheet);
            window.set_gui_colour_child_open(has_open());
        }
    });
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let active = active.clone();
        let editor = editor.clone();
        move || {
            active.get()
                && weak
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
                && {
                    let editor = editor.borrow();
                    editor
                        .page_names()
                        .get(editor.page())
                        .is_some_and(|name| *name == "colours")
                        && editor.edited_gui_colours().override_stylesheet
                }
        }
    });
    window.on_gui_colour_tab_changed({
        let show = show.clone();
        move || show()
    });
    window.on_gui_colour_chosen({
        let children = children.clone();
        let has_open = has_open.clone();
        let valid = valid.clone();
        let editor = editor.clone();
        let show = show.clone();
        move |set, role| {
            if !valid() || has_open() {
                return;
            }
            let (Ok(set), Ok(role)) = (usize::try_from(set), usize::try_from(role)) else {
                return;
            };
            let Some(rgb) = editor
                .borrow()
                .edited_gui_colours()
                .sets
                .get(set)
                .and_then(|roles| roles.get(role))
                .copied()
            else {
                return;
            };
            let Ok(picker) = GuiColourPickerWindow::new() else {
                return;
            };
            let [red, green, blue] = rgb.0;
            picker.set_red(i32::from(red));
            picker.set_green(i32::from(green));
            picker.set_blue(i32::from(blue));
            let alive = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let alive = alive.clone();
                let weak = picker.as_weak();
                let children = Rc::downgrade(&children);
                let show = show.clone();
                move || {
                    // A retained retired picker can be shown again. Always hide
                    // this exact window; owner cleanup and refresh happen once.
                    if let Some(picker) = weak.upgrade() {
                        let _ = picker.hide();
                    }
                    if !alive.replace(false) {
                        return;
                    }
                    if let Some(children) = children.upgrade() {
                        children.borrow_mut().picker.take();
                    }
                    show();
                }
            });
            picker.on_accepted({
                let close = close.clone();
                let editor = editor.clone();
                let valid = valid.clone();
                let alive = alive.clone();
                let weak = picker.as_weak();
                let children = Rc::downgrade(&children);
                move |r, g, b| {
                    if alive.get()
                        && valid()
                        && weak.upgrade().is_some_and(|picker| {
                            picker.window().is_visible()
                                && children.upgrade().is_some_and(|children| {
                                    children.borrow().picker.as_ref().is_some_and(|current| {
                                        std::ptr::eq(current.window(), picker.window())
                                    })
                                })
                        })
                        && let Some(rgb) = rgb_channels(r, g, b)
                    {
                        editor.borrow_mut().set_gui_colour(set, role, rgb);
                        close();
                    }
                }
            });
            picker.on_cancelled({
                let close = close.clone();
                move || close()
            });
            picker.window().on_close_requested(move || {
                close();
                slint::CloseRequestResponse::HideWindow
            });
            LAST.with(|last| *last.borrow_mut() = Some(picker.as_weak()));
            children.borrow_mut().picker = Some(picker.clone_strong());
            show();
            if picker.show().is_err() {
                picker.invoke_cancelled();
            }
        }
    });
    window.on_gui_colour_clipboard({
        let children = children.clone();
        let has_open = has_open.clone();
        let valid = valid.clone();
        let editor = editor.clone();
        let show = show.clone();
        move |set, role, paste| {
            if !valid() || has_open() {
                return;
            }
            let (Ok(set), Ok(role)) = (usize::try_from(set), usize::try_from(role)) else {
                return;
            };
            let Some(rgb) = editor
                .borrow()
                .edited_gui_colours()
                .sets
                .get(set)
                .and_then(|roles| roles.get(role))
                .copied()
            else {
                return;
            };
            if !paste {
                crate::copy_to_clipboard(&rgb.to_string().to_ascii_lowercase());
                return;
            }
            let message = match crate::clipboard_text() {
                Ok(text) => {
                    let text = text.unwrap_or_default();
                    if let Some(rgb) = paste_rgb(&text) {
                        editor.borrow_mut().set_gui_colour(set, role, rgb);
                        show();
                        return;
                    }
                    format!("\"{text}\" did not appear to be a hex string!")
                }
                Err(error) => error,
            };
            let Ok(notice) = SessionDialog::new() else {
                return;
            };
            notice.set_window_title("Problem pasting!".into());
            notice.set_message(message.into());
            notice.set_notice_only(true);
            notice.set_notice_ok_label("OK".into());
            let close: Rc<dyn Fn()> = Rc::new({
                let alive = Cell::new(true);
                let weak = notice.as_weak();
                let children = Rc::downgrade(&children);
                let show = show.clone();
                move || {
                    if !alive.replace(false) {
                        return;
                    }
                    if let Some(notice) = weak.upgrade() {
                        let _ = notice.hide();
                    }
                    if let Some(children) = children.upgrade() {
                        children.borrow_mut().notice.take();
                    }
                    show();
                }
            });
            notice.on_cancelled({
                let close = close.clone();
                move || close()
            });
            notice.window().on_close_requested(move || {
                close();
                slint::CloseRequestResponse::HideWindow
            });
            children.borrow_mut().notice = Some(notice.clone_strong());
            show();
            if notice.show().is_err() {
                notice.invoke_cancelled();
            }
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new(move || {
        let picker = children
            .borrow()
            .picker
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(picker) = picker {
            picker.invoke_cancelled();
        }
        let notice = children
            .borrow()
            .notice
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(notice) = notice {
            notice.invoke_cancelled();
        }
    });
    Binding {
        show,
        cancel,
        has_open,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clipboard_hex_acceptance_matches_actual_qt_cases() {
        let fixture = hydrus_testkit::fixture_json("gui_coloursets.json");
        let cases = fixture["clipboard"].as_array().unwrap();
        assert_eq!(cases.len(), 5);
        for case in cases {
            let expected = if case["critical"].is_null() {
                Some(hydrus_store::services::Rgb(
                    serde_json::from_value(case["after"].clone()).unwrap(),
                ))
            } else {
                assert_eq!(case["before"], case["after"]);
                None
            };
            assert_eq!(super::paste_rgb(case["text"].as_str().unwrap()), expected);
        }
    }
}
