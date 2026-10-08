//! Modal popups (the reference's `FrameGUI.AddModalMessage` and
//! `PopupMessageDialogPanel`): a long job thrown up in a dialog of its own
//! until it finishes, instead of in the popup stack. The dialog is titled
//! with the job's title, has a close button only if the job can be
//! cancelled, and closes itself when the job finishes, releasing the job
//! to the ordinary popups. Jobs that arrive while the main window is
//! minimised or hidden, another dialog is open, or the main window is not
//! active wait, one retried at a time.

/// A popup's key.
pub type Key = [u8; 32];

/// The title of a job with none.
pub const DEFAULT_TITLE: &str = "important job";
/// What closing a running, cancellable job asks.
pub const CANCEL_QUESTION: &str = "Cancel/stop job?";
/// What closing a running job that cannot be cancelled says.
pub const CANNOT_CANCEL: &str = "Unfortunately, this job cannot be cancelled. If it really is taking too long, please kill the client through task manager.";

/// What a modal needs to know of its job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFacts {
    pub key: Key,
    pub title: Option<String>,
    pub done: bool,
    pub dismissed: bool,
    pub cancellable: bool,
}

/// The window state a modal waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conditions {
    /// `_CurrentlyMinimisedOrHidden`: minimised, or hidden to the tray.
    pub minimised_or_hidden: bool,
    /// Another dialog is open.
    pub dialog_open: bool,
    /// The main window, or a window of its own, is the active one.
    pub active: bool,
}

/// What the GUI does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Pause all media (`pub( 'pause_all_media' )`) as the dialog opens.
    PauseAllMedia,
    /// Open the dialog.
    Show {
        key: Key,
        title: String,
        hide_close_button: bool,
    },
    /// Release the job to the ordinary popups (`pub( 'message' )`).
    Release(Key),
    /// Cancel the job.
    Cancel(Key),
    /// Ask [`CANCEL_QUESTION`]; the answer comes to [`Modals::answered`].
    Ask,
    /// Warn [`CANNOT_CANCEL`].
    Warn,
    /// Close the dialog.
    Close,
}

/// The dialog's title.
pub fn title(job: &JobFacts) -> String {
    job.title
        .clone()
        .unwrap_or_else(|| DEFAULT_TITLE.to_owned())
}

#[derive(Debug, Clone)]
struct Open {
    key: Key,
    question_open: bool,
}

/// The modal jobs waiting, and the dialog open.
#[derive(Debug, Default)]
pub struct Modals {
    pending: Vec<Key>,
    open: Option<Open>,
}

impl Modals {
    /// How many wait (`_pending_modal_job_statuses`).
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The key of the job whose dialog is open.
    pub fn open_key(&self) -> Option<Key> {
        self.open.as_ref().map(|open| open.key)
    }

    /// `AddModalMessage`.
    pub fn add(&mut self, job: &JobFacts, around: &Conditions) -> Vec<Effect> {
        if job.dismissed {
            return Vec::new();
        }
        if job.done {
            return vec![Effect::Release(job.key)];
        }
        if around.minimised_or_hidden || around.dialog_open || self.open.is_some() || !around.active
        {
            if !self.pending.contains(&job.key) {
                self.pending.push(job.key);
            }
            return Vec::new();
        }
        self.open = Some(Open {
            key: job.key,
            question_open: false,
        });
        vec![
            Effect::PauseAllMedia,
            Effect::Show {
                key: job.key,
                title: title(job),
                hide_close_button: !job.cancellable,
            },
        ]
    }

    /// `REPEATINGPageUpdate`'s retry: the job that has waited longest (the
    /// reference's is a set, in no order) comes off the waiting, to be
    /// added again, by [`add`](Self::add), with the job as it is now.
    pub fn page_update(&mut self) -> Option<Key> {
        (!self.pending.is_empty()).then(|| self.pending.remove(0))
    }

    /// `UserIsOKToCancel`, the dialog's close.
    pub fn close_requested(&mut self, job: &JobFacts) -> Vec<Effect> {
        let Some(open) = self.open.as_mut() else {
            return Vec::new();
        };
        if job.done {
            self.open = None;
            vec![Effect::Release(job.key), Effect::Close]
        } else if job.cancellable {
            open.question_open = true;
            vec![Effect::Ask]
        } else {
            vec![Effect::Warn]
        }
    }

    /// The answer to [`Effect::Ask`].
    pub fn answered(&mut self, yes: bool) -> Vec<Effect> {
        let Some(open) = self.open.as_mut() else {
            return Vec::new();
        };
        open.question_open = false;
        if !yes {
            return Vec::new();
        }
        let key = open.key;
        self.open = None;
        vec![Effect::Cancel(key), Effect::Release(key), Effect::Close]
    }

    /// `REPEATINGUpdate` (a quarter second): a job that has finished closes
    /// its dialog, unless a question is open on it.
    pub fn tick(&mut self, job: &JobFacts) -> Vec<Effect> {
        match &self.open {
            Some(open) if open.key == job.key && job.done && !open.question_open => {
                self.close_requested(job)
            }
            _ => Vec::new(),
        }
    }
}
