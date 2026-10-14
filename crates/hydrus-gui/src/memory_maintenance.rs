//! The reference's memory maintenance, on its schedules: the fast pulse every
//! minute (`MaintainMemoryFast`: every cache trims itself) and the slow one
//! every five minutes (`MaintainMemorySlow`: closed pages over an hour old
//! are deleted, if no page has changed for half an hour). Help > debug >
//! memory actions runs either at once.
use std::rc::Rc;
use std::time::Duration;

/// The two repeating jobs; they stop when this is dropped.
pub(crate) struct Timers {
    _fast: slint::Timer,
    _slow: slint::Timer,
}

pub(crate) fn start(fast: Rc<dyn Fn()>, slow: Rc<dyn Fn()>) -> Timers {
    let timer = |period: u64, job: Rc<dyn Fn()>| {
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(period),
            move || job(),
        );
        timer
    };
    Timers {
        _fast: timer(60, fast),
        _slow: timer(300, slow),
    }
}
