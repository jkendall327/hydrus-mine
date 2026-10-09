//! The media playback page's "fetch mpv audio devices" button: libmpv is
//! asked for the devices it can see, offered in a chooser whose choice fills
//! the "Preferred audio output device:" draft; without libmpv it says so.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hydrus_gui_model::mpv_audio_devices::{self as model, Device};

use crate::ChoiceButtonsWindow;

/// Asks mpv for its audio devices; none without libmpv.
pub type Fetch = fn() -> Option<Vec<Device>>;

thread_local! {
    static FETCH: Cell<Fetch> = const { Cell::new(crate::mpv::audio_devices) };
    /// The open chooser or message, kept alive until answered.
    static DIALOG: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// Ask `fetch` instead of libmpv (tests).
pub fn set_fetch(fetch: Fetch) {
    FETCH.with(|f| f.set(fetch));
}

/// The last chooser or message shown, for tests.
pub fn last_dialog() -> Option<ChoiceButtonsWindow> {
    DIALOG.with(|d| d.borrow().as_ref().map(slint::ComponentHandle::clone_strong))
}

/// The button was clicked: `chosen` gets the device's option value.
pub fn clicked(chosen: impl Fn(Option<String>) + 'static) {
    let opened = match FETCH.with(Cell::get)() {
        None => crate::choice_buttons::open(
            &crate::choice_buttons::Ask {
                title: "Information",
                message: model::UNAVAILABLE,
                choices: Vec::new(),
                no_label: "OK",
            },
            |_| {},
        ),
        Some(devices) => {
            let devices = Rc::new(model::offered(devices));
            crate::choice_buttons::open(
                &crate::choice_buttons::Ask {
                    title: model::TITLE,
                    message: model::MESSAGE,
                    choices: devices.iter().map(model::label).collect(),
                    no_label: "",
                },
                move |choice| {
                    if let Some(device) = choice.and_then(|i| devices.get(i)) {
                        chosen(model::value(device));
                    }
                },
            )
        }
    };
    match opened {
        Ok(window) => DIALOG.with(|d| *d.borrow_mut() = window),
        Err(e) => eprintln!("could not show the mpv audio devices: {e}"),
    }
}
