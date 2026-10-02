//! Saving the open pages as a session (pages > sessions > save), as the
//! reference does it (`ProposeSaveGUISession`): "as new session…" asks a
//! name, a reserved one is refused with a warning and asked again, an
//! existing one asks whether to overwrite it ("no, choose another name"
//! asks again); a session's own entry asks whether to overwrite it.
//! Checked against the reference, recorded by
//! `oracle/record_sessions_menu.py`.

/// Names a session can't be saved as (`RESERVED_SESSION_NAMES`).
pub const RESERVED: [&str; 4] = ["", "just a blank page", "last session", "exit session"];

/// The name's dialog: its title and message.
pub const NAME_TITLE: &str = "Enter Text";
pub const NAME_MESSAGE: &str = "Enter a name for the new session.";
/// Said of a reserved name.
pub const RESERVED_WARNING: &str = "Sorry, you cannot have that name! Try another.";
pub const OVERWRITE_TITLE: &str = "Overwrite existing session?";

/// A yes/no question, as the reference words it and its buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub message: String,
    pub title: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
}

/// What saving does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Ask a name (after this warning, if any).
    AskName {
        warning: Option<&'static str>,
    },
    Ask(Question),
    /// Save the open pages as this session.
    Save(String),
    /// Nothing more.
    Stop,
}

/// A session being saved.
#[derive(Debug, Clone)]
pub struct Saving {
    /// The sessions saved already.
    existing: Vec<String>,
    /// The name asked about.
    name: String,
    /// Saving as a new session ("no" asks another name).
    new_session: bool,
}

impl Saving {
    /// "as new session…": a name is asked.
    pub fn new_session(existing: Vec<String>) -> (Self, Step) {
        let saving = Self {
            existing,
            name: String::new(),
            new_session: true,
        };
        (saving, Step::AskName { warning: None })
    }

    /// A session's own entry: whether to overwrite it is asked.
    pub fn over(name: &str) -> (Self, Step) {
        let saving = Self {
            existing: Vec::new(),
            name: name.to_owned(),
            new_session: false,
        };
        let question = Question {
            message: format!("Overwrite \"{name}\" session?"),
            title: OVERWRITE_TITLE,
            yes: "yes, overwrite",
            no: "no",
        };
        (saving, Step::Ask(question))
    }

    /// A name entered (`None`: the dialog cancelled). Blank text is as
    /// cancelling, as the reference's text entry refuses it.
    pub fn named(&mut self, name: Option<&str>) -> Step {
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            return Step::Stop;
        };
        if RESERVED.contains(&name) {
            return Step::AskName {
                warning: Some(RESERVED_WARNING),
            };
        }
        name.clone_into(&mut self.name);
        if self.existing.iter().any(|e| e == name) {
            return Step::Ask(Question {
                message: format!("Session \"{name}\" already exists! Do you want to overwrite it?"),
                title: OVERWRITE_TITLE,
                yes: "yes, overwrite",
                no: "no, choose another name",
            });
        }
        Step::Save(name.to_owned())
    }

    /// The question answered (`None`: closed without an answer).
    pub fn answered(&mut self, answer: Option<bool>) -> Step {
        match answer {
            Some(true) => Step::Save(self.name.clone()),
            Some(false) if self.new_session => Step::AskName { warning: None },
            _ => Step::Stop,
        }
    }
}
