//! The media viewer's slideshow, as the reference's
//! (`CanvasMediaListBrowser`'s): once a period has passed since a file
//! was shown, the next is shown (or, shuffling, a random other one). A
//! file with a duration may bend the period
//! (`_CalculateAnySpecialSlideshowPeriodForCurrentMedia`): a short one
//! may move on early once it has played, or stop at its end and move on
//! then; a long one may run over a little to finish. With "play media once
//! through", a file with a duration is shown until it has played through
//! at least once. Plain Rust, timed by the seconds it is given: the window
//! asks it each moment whether to move on, and tells it of each file shown.

use hydrus_core::media_viewer::SlideshowSettings;
use hydrus_core::time::pretty_time_delta_f64;

use crate::thumbnail_menu::{Action, Entry};
use crate::viewer_menu::{Seconds, ViewerAction};

/// The slideshow options now.
pub fn settings(store: &hydrus_store::Store) -> SlideshowSettings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}

/// Change them, and keep the change; says what they are now.
pub fn change(
    store: &hydrus_store::Store,
    change: impl FnOnce(&mut SlideshowSettings),
) -> SlideshowSettings {
    let mut settings = settings(store);
    change(&mut settings);
    let kept = settings.clone();
    if let Err(e) = store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &kept)) {
        eprintln!("could not keep the slideshow options: {e}");
    }
    settings
}

/// The "very fast" period, in seconds.
pub const VERY_FAST: f64 = 0.08;

/// What the file shown is, for the slideshow's timing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    /// A still, or a file the viewer doesn't play.
    Still,
    /// A file the viewer plays (`CurrentlyPresentingMediaWithDuration`),
    /// with its duration in seconds, if known.
    Playing(Option<f64>),
}

/// The period a file with a duration bends the slideshow's to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Special {
    /// The slideshow's own.
    None,
    /// This many seconds; and whether the file stops at its end rather
    /// than play again (`StopForSlideshow`).
    Period { seconds: f64, stop_at_end: bool },
    /// Its duration isn't known: the slideshow stops.
    Stop,
}

/// The period a file of `duration` seconds (if known) bends a slideshow
/// of `normal` seconds' to (`_CalculateAnySpecialSlideshowPeriodForCurrentMedia`).
/// A shorter one moves on once it has played through in the shortest of
/// the short loop periods (a part of the period, or seconds) it fits in,
/// no longer than the period; or, if it is longer than the cutoff part of
/// the period, as soon as it has played once. One as long or longer moves
/// on as it ends if that is within the overspill part past the period.
#[allow(clippy::cast_precision_loss)] // (percentages and seconds, as the reference's)
pub fn special_period(duration: Option<f64>, normal: f64, settings: &SlideshowSettings) -> Special {
    let Some(duration) = duration else {
        return Special::Stop;
    };
    let mut special = None;
    let mut stop_at_end = false;
    if duration < normal {
        let mut cutoffs = Vec::new();
        if let Some(percentage) = settings.short_loop_percentage {
            cutoffs.push(normal * (percentage as f64 / 100.0));
        }
        if let Some(seconds) = settings.short_loop_seconds {
            cutoffs.push(seconds as f64);
        }
        cutoffs.sort_by(|a, b| b.total_cmp(a));
        for cutoff in cutoffs {
            if cutoff > normal {
                continue;
            }
            // (will it play once in this shorter time?)
            if duration <= cutoff {
                special = Some(cutoff);
            }
        }
        if let Some(percentage) = settings.short_cutoff_percentage
            && normal * (percentage as f64 / 100.0) < duration
            && duration < normal
        {
            special = Some(duration);
            stop_at_end = true;
        }
    } else if let Some(percentage) = settings.long_overspill_percentage {
        let quotient = 1.0 + percentage as f64 / 100.0;
        if duration < normal * quotient {
            special = Some(duration);
            stop_at_end = true;
        }
    }
    match special {
        Some(seconds) => Special::Period {
            seconds,
            stop_at_end,
        },
        None => Special::None,
    }
}

/// A viewer's slideshow: whether it runs, and its period, timing,
/// shuffling and playing through, the last two this viewer's own (from
/// the options when it opened).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Slideshow {
    running: bool,
    /// When the file shown was shown, in seconds.
    last_switch: f64,
    /// The period, in seconds; 0 before any slideshow.
    period: f64,
    /// The file shown's own period, if it bends the slideshow's.
    special: Option<f64>,
    shuffling: bool,
    once_through: bool,
    /// Whether the player is told to stop at the file's end.
    stops_player: bool,
}

impl Slideshow {
    /// A viewer's, before any slideshow: shuffling and playing through as
    /// the options have them.
    pub fn new(settings: &SlideshowSettings) -> Self {
        Self {
            shuffling: settings.shuffle,
            once_through: settings.once_through,
            ..Self::default()
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// The period, in seconds, if a slideshow has had one.
    pub fn period(&self) -> Option<f64> {
        (self.period > 0.0).then_some(self.period)
    }

    pub fn shuffling(&self) -> bool {
        self.shuffling
    }

    pub fn set_shuffling(&mut self, shuffling: bool) {
        self.shuffling = shuffling;
    }

    /// Whether a file with a duration plays through before it moves on.
    pub fn once_through(&self) -> bool {
        self.once_through
    }

    pub fn set_once_through(&mut self, once_through: bool) {
        self.once_through = once_through;
    }

    /// Whether the file shown's player is to stop at its end rather than
    /// play again (`StopForSlideshow`).
    pub fn stops_player(&self) -> bool {
        self.stops_player
    }

    /// Stop (`_StopSlideshow`): the period is kept, to resume at.
    pub fn stop(&mut self) {
        if self.running {
            self.running = false;
            self.special = None;
            self.stops_player = false;
        }
    }

    /// Start at `period` seconds from now, with `shown` shown
    /// (`_StartSlideshow`); a period of 0 or less (or none) only stops it.
    pub fn start(&mut self, period: f64, now: f64, shown: Shown, settings: &SlideshowSettings) {
        self.stop();
        if period > 0.0 {
            self.period = period;
            self.running = true;
            self.shown(now, shown, settings);
        }
    }

    /// Stop if running, else start at the last period, or the first of the
    /// options' (`_PausePlaySlideshow`).
    pub fn pause_play(&mut self, now: f64, shown: Shown, settings: &SlideshowSettings) {
        if self.running {
            self.stop();
        } else {
            if self.period == 0.0 {
                self.period = settings.durations.first().copied().unwrap_or(1.0);
            }
            self.start(self.period, now, shown, settings);
        }
    }

    /// A file is shown, by the slideshow or the user: its period starts
    /// now (`_RegisterNextSlideshowPresentation`).
    pub fn shown(&mut self, now: f64, shown: Shown, settings: &SlideshowSettings) {
        if !self.running {
            return;
        }
        self.last_switch = now;
        self.special = None;
        // (a new file plays on, unless told to stop)
        self.stops_player = false;
        if let Shown::Playing(duration) = shown {
            match special_period(duration, self.period, settings) {
                Special::None => {}
                Special::Period {
                    seconds,
                    stop_at_end,
                } => {
                    self.special = Some(seconds);
                    self.stops_player = stop_at_end;
                }
                Special::Stop => self.stop(),
            }
        }
    }

    /// Whether to move on at `now` (`_DoSlideshowWork`): the period is
    /// past, and a file with a duration has played through if it must.
    pub fn due(&self, now: f64, shown: Shown, played_once_through: bool) -> bool {
        if !self.running {
            return false;
        }
        if matches!(shown, Shown::Playing(_)) && self.once_through && !played_once_through {
            return false;
        }
        now > self.last_switch + self.special.unwrap_or(self.period)
    }
}

/// The slideshow submenu (`AppendSlideshowMenu`): stop or resume, the
/// options' periods, very fast and a custom one, then shuffling and
/// playing through, this viewer's and every new one's (checked as they
/// are).
pub fn slideshow_menu(slideshow: &Slideshow, settings: &SlideshowSettings) -> Entry {
    let item = |label: String, action: ViewerAction| Entry::Item(label, Action::Viewer(action));
    let check = |label: &str, action: ViewerAction, checked: bool| {
        Entry::Check(label.into(), Action::Viewer(action), checked)
    };
    let mut inner = Vec::new();
    let period = slideshow.period();
    if slideshow.running() {
        let label = match period {
            Some(period) => format!("stop ({})", pretty_time_delta_f64(period)),
            None => "stop".into(),
        };
        inner.push(item(label, ViewerAction::PausePlaySlideshow));
    } else if let Some(period) = period {
        inner.push(item(
            format!("resume at {}", pretty_time_delta_f64(period)),
            ViewerAction::PausePlaySlideshow,
        ));
    }
    if !inner.is_empty() {
        inner.push(Entry::Separator);
    }
    for &duration in &settings.durations {
        inner.push(item(
            pretty_time_delta_f64(duration),
            ViewerAction::StartSlideshow(Some(Seconds(duration))),
        ));
    }
    inner.push(item(
        "very fast".into(),
        ViewerAction::StartSlideshow(Some(Seconds(VERY_FAST))),
    ));
    inner.push(item(
        "custom interval".into(),
        ViewerAction::StartSlideshow(None),
    ));
    inner.push(Entry::Separator);
    inner.push(check(
        "shuffle this slideshow",
        ViewerAction::FlipShuffle,
        slideshow.shuffling(),
    ));
    inner.push(check(
        "all slideshows shuffle",
        ViewerAction::FlipGlobalShuffle,
        settings.shuffle,
    ));
    inner.push(Entry::Separator);
    inner.push(check(
        "this slideshow plays media once through",
        ViewerAction::FlipOnceThrough,
        slideshow.once_through(),
    ));
    inner.push(check(
        "always play media once through",
        ViewerAction::FlipGlobalOnceThrough,
        settings.once_through,
    ));
    let title = if slideshow.running() {
        "slideshow running"
    } else {
        "start slideshow"
    };
    Entry::Menu(title.into(), inner)
}

/// A custom period as typed (`float()`), if it reads as one.
pub fn parse_period(text: &str) -> Option<f64> {
    text.trim().replace('_', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> SlideshowSettings {
        SlideshowSettings::default()
    }

    #[test]
    fn a_slideshow_moves_on_once_its_period_has_passed() {
        let settings = defaults();
        let mut slideshow = Slideshow::new(&settings);
        assert!(!slideshow.due(100.0, Shown::Still, false));
        // pause/play with none before: the options' first period
        slideshow.pause_play(10.0, Shown::Still, &settings);
        assert!(slideshow.running());
        assert_eq!(slideshow.period(), Some(1.0));
        assert!(!slideshow.due(11.0, Shown::Still, false));
        assert!(slideshow.due(11.01, Shown::Still, false));
        // a file shown (by the user too) starts the period again
        slideshow.shown(11.5, Shown::Still, &settings);
        assert!(!slideshow.due(12.4, Shown::Still, false));
        // stopped: never; resumed: at the same period
        slideshow.pause_play(13.0, Shown::Still, &settings);
        assert!(!slideshow.running());
        assert!(!slideshow.due(100.0, Shown::Still, false));
        slideshow.start(30.0, 13.0, Shown::Still, &settings);
        slideshow.pause_play(14.0, Shown::Still, &settings);
        slideshow.pause_play(14.0, Shown::Still, &settings);
        assert_eq!(slideshow.period(), Some(30.0));
        assert!(!slideshow.due(44.0, Shown::Still, false));
        assert!(slideshow.due(44.5, Shown::Still, false));
        // a period of none only stops it
        slideshow.start(0.0, 50.0, Shown::Still, &settings);
        assert!(!slideshow.running());
        assert_eq!(slideshow.period(), Some(30.0));
        // with no periods in the options, a second
        let none = SlideshowSettings {
            durations: Vec::new(),
            ..defaults()
        };
        let mut slideshow = Slideshow::new(&none);
        slideshow.pause_play(0.0, Shown::Still, &none);
        assert_eq!(slideshow.period(), Some(1.0));
    }

    #[test]
    fn files_with_durations_bend_the_period() {
        let settings = defaults();
        let mut slideshow = Slideshow::new(&settings);
        // a 2s video in a 30s slideshow: on after the shortest loop period
        // it fits (20% of 30 seconds, under the 10 seconds)
        slideshow.start(30.0, 0.0, Shown::Playing(Some(2.0)), &settings);
        assert!(!slideshow.stops_player());
        assert!(!slideshow.due(6.0, Shown::Playing(Some(2.0)), true));
        assert!(slideshow.due(6.1, Shown::Playing(Some(2.0)), true));
        // a 25s one: on as it ends, stopping there
        slideshow.shown(0.0, Shown::Playing(Some(25.0)), &settings);
        assert!(slideshow.stops_player());
        assert!(slideshow.due(25.1, Shown::Playing(Some(25.0)), true));
        // a still after it plays on, at the slideshow's period
        slideshow.shown(0.0, Shown::Still, &settings);
        assert!(!slideshow.stops_player());
        assert!(!slideshow.due(29.0, Shown::Still, false));
        // playing through first, if asked
        slideshow.set_once_through(true);
        slideshow.shown(0.0, Shown::Playing(Some(2.0)), &settings);
        assert!(!slideshow.due(60.0, Shown::Playing(Some(2.0)), false));
        assert!(slideshow.due(60.0, Shown::Playing(Some(2.0)), true));
        // a file playing of no known duration stops it
        slideshow.shown(0.0, Shown::Playing(None), &settings);
        assert!(!slideshow.running());
        assert!(!slideshow.stops_player());
    }

    #[test]
    fn the_special_periods_as_the_reference_bends_them() {
        let settings = defaults();
        let period = |duration, normal| special_period(Some(duration), normal, &settings);
        // shorter: the 10 seconds (20% of 60 is 12, which it fits too, but
        // the shortest it fits wins)
        assert_eq!(
            period(5.0, 60.0),
            Special::Period {
                seconds: 10.0,
                stop_at_end: false
            }
        );
        // 11s fits 20% of 60 only
        assert_eq!(
            period(11.0, 60.0),
            Special::Period {
                seconds: 12.0,
                stop_at_end: false
            }
        );
        // between those and 75%: the period
        assert_eq!(period(30.0, 60.0), Special::None);
        // past 75%: as it ends
        assert_eq!(
            period(50.0, 60.0),
            Special::Period {
                seconds: 50.0,
                stop_at_end: true
            }
        );
        // as long or longer, within 50% over: as it ends; beyond: the period
        assert_eq!(
            period(60.0, 60.0),
            Special::Period {
                seconds: 60.0,
                stop_at_end: true
            }
        );
        assert_eq!(period(90.0, 60.0), Special::None);
        // a loop period longer than the slideshow's doesn't count
        assert_eq!(
            period(0.5, 5.0),
            Special::Period {
                seconds: 1.0,
                stop_at_end: false
            }
        );
        assert_eq!(special_period(None, 5.0, &settings), Special::Stop);
    }

    #[test]
    fn the_menu_offers_what_the_slideshow_can_do() {
        fn titles(entry: &Entry) -> (String, Vec<String>) {
            let Entry::Menu(title, inner) = entry else {
                panic!("a menu");
            };
            let inner = inner
                .iter()
                .map(|e| match e {
                    Entry::Item(label, _) | Entry::Label(label) => label.clone(),
                    Entry::Check(label, _, checked) => {
                        format!("[{}] {label}", if *checked { "x" } else { " " })
                    }
                    Entry::Menu(title, _) => format!("{title} >"),
                    Entry::Separator => "---".into(),
                })
                .collect();
            (title.clone(), inner)
        }
        let settings = defaults();
        let mut slideshow = Slideshow::new(&settings);
        let (title, inner) = titles(&slideshow_menu(&slideshow, &settings));
        assert_eq!(title, "start slideshow");
        assert_eq!(
            inner,
            [
                "1 second",
                "5 seconds",
                "10 seconds",
                "30 seconds",
                "1 minute",
                "very fast",
                "custom interval",
                "---",
                "[ ] shuffle this slideshow",
                "[ ] all slideshows shuffle",
                "---",
                "[ ] this slideshow plays media once through",
                "[ ] always play media once through",
            ]
        );
        slideshow.start(2.5, 0.0, Shown::Still, &settings);
        slideshow.set_shuffling(true);
        let (title, inner) = titles(&slideshow_menu(&slideshow, &settings));
        assert_eq!(title, "slideshow running");
        assert_eq!(inner[..2], ["stop (2.5 seconds)", "---"]);
        assert_eq!(inner[10], "[x] shuffle this slideshow");
        slideshow.start(VERY_FAST, 0.0, Shown::Still, &settings);
        slideshow.stop();
        let (title, inner) = titles(&slideshow_menu(&slideshow, &settings));
        assert_eq!(title, "start slideshow");
        assert_eq!(inner[0], "resume at 80 milliseconds");
    }

    #[test]
    fn custom_periods_read_as_the_reference_reads_them() {
        assert_eq!(parse_period("15.0"), Some(15.0));
        assert_eq!(parse_period(" 2 "), Some(2.0));
        assert_eq!(parse_period("1_000"), Some(1000.0));
        assert_eq!(parse_period("1e1"), Some(10.0));
        assert_eq!(parse_period("ten"), None);
        assert_eq!(parse_period(""), None);
    }
}
