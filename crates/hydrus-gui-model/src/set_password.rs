//! Database > set a password (`FrameGUI._SetPassword`): the password asked,
//! then asked again (or, left blank, whether to clear the lock), and what is
//! saved. Recorded by `oracle/record_set_password.py`.

use hydrus_core::lock::LockPassword;

/// The first question.
pub const FIRST: &str = "You can set a password to be asked for whenever the client starts. This does not encrypt or truly lock the database files or media folders; it is just a simple check on boot. It will stop noobs from easily booting your client and poking around if you leave your machine unattended.\n\nDo not forget your password! If you do, you'll have to manually insert a yaml-dumped python dictionary into a sqlite database or run from edited source to regain access.\n\nThe password is cleartext here but obscured in the entry dialog. Enter a blank password to remove.";
/// The second.
pub const AGAIN: &str = "Please enter it again.";
/// Asked when the first entry is blank.
pub const CLEAR: &str = "Clear any existing password?";

/// What to show or do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Ask for text ("Enter Text"), maybe allowing it blank.
    Ask {
        message: &'static str,
        allow_blank: bool,
    },
    /// Ask [`CLEAR`], yes or no.
    ConfirmClear,
    /// Show a critical message, then stop.
    Problem {
        title: &'static str,
        message: &'static str,
    },
    /// Save this lock, then stop.
    Save(LockPassword),
    /// Stop with nothing saved.
    Done,
}

/// The flow between its dialogs.
#[derive(Debug, Clone, Default)]
pub struct SetPassword {
    first: Option<String>,
}

impl SetPassword {
    /// The first step.
    pub fn start(&mut self) -> Step {
        self.first = None;
        Step::Ask {
            message: FIRST,
            allow_blank: true,
        }
    }

    /// Text entered at the current [`Step::Ask`].
    pub fn entered(&mut self, text: &str) -> Step {
        match self.first.take() {
            None if text.is_empty() => Step::ConfirmClear,
            None => {
                self.first = Some(text.to_owned());
                Step::Ask {
                    message: AGAIN,
                    allow_blank: false,
                }
            }
            Some(first) if text.is_empty() => {
                // (blank isn't allowed: still asking)
                self.first = Some(first);
                Step::Ask {
                    message: AGAIN,
                    allow_blank: false,
                }
            }
            Some(first) if first == text => Step::Save(LockPassword::new(text)),
            Some(_) => Step::Problem {
                title: "Problem!",
                message: "Those passwords did not match!",
            },
        }
    }

    /// The answer to [`Step::ConfirmClear`].
    pub fn answered(&mut self, yes: bool) -> Step {
        if yes {
            Step::Save(LockPassword::default())
        } else {
            Step::Done
        }
    }
}
