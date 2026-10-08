//! The network job control inside a popup (`PopupMessage.UpdateMessage`
//! with `NetworkJobControl`): shown as soon as the popup has a network job,
//! and, once the job is gone, cleared at once but left showing, blank, for
//! ten seconds before it hides.

use hydrus_store::live::JobLine;

/// How long the control stays, blank, after its job goes.
pub const LINGER_SECONDS: i64 = 10;

/// One popup's network job control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Control {
    shown: bool,
    has_job: bool,
    gone_at: i64,
}

impl Control {
    /// Update with the popup's network job line (`None` without a job) at
    /// `now` (seconds); what the control shows, if it is shown.
    pub fn update(&mut self, line: Option<JobLine>, now: i64) -> Option<JobLine> {
        if let Some(line) = line {
            self.has_job = true;
            self.shown = true;
            Some(line)
        } else {
            if self.has_job {
                self.has_job = false;
                self.gone_at = now;
            }
            // (`TimeHasPassed`: strictly after)
            if self.shown && now > self.gone_at + LINGER_SECONDS {
                self.shown = false;
            }
            self.shown.then(JobLine::default)
        }
    }
}
