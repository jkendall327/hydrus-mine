//! Popup messages in the main window (the reference's
//! `PopupMessageManager`): the store's popups (`hydrus_store::popups`), the
//! oldest ten shown at the bottom right, each as the reference's
//! `PopupMessage.UpdateMessage` shows it, with a line under them saying
//! how many there are and a button to dismiss those done. A popup that is
//! done dismisses with a right click.

use hydrus_core::numbers::human_int;
use hydrus_store::live::JobLine;
use hydrus_store::popups::Job;

/// A gauge: how far along, or going with no end known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gauge {
    At(f32),
    Going,
}

/// A popup as shown.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupView {
    pub key: [u8; 32],
    /// In bold, centred.
    pub title: Option<String>,
    /// "paused" while it is.
    pub text_1: Option<String>,
    /// Gauges: their value of their range.
    pub gauge_1: Option<Gauge>,
    pub text_2: Option<String>,
    pub gauge_2: Option<Gauge>,
    /// The download it is doing (its network job), if any.
    pub download: Option<JobLine>,
    /// Its files' button: "{label} - show 3 files".
    pub files: Option<String>,
    /// An error's traceback, behind its button.
    pub traceback: Option<String>,
    pub pausable: bool,
    pub paused: bool,
    pub cancellable: bool,
    pub done: bool,
}

/// Text longer than this shows its start (`TEXT_CUTOFF`).
const TEXT_CUTOFF: usize = 1024;

/// `_ProcessText`.
fn cut(text: &str) -> String {
    if text.chars().count() <= TEXT_CUTOFF {
        return text.to_owned();
    }
    let start: String = text.chars().take(TEXT_CUTOFF).collect();
    format!(
        "The text is too long to display here. Here is the start of it (the rest is printed to the log):\n{start}"
    )
}

fn gauge(g: Option<(i64, i64)>) -> Option<Gauge> {
    g.map(|(value, range)| {
        if range > 0 {
            #[allow(clippy::cast_precision_loss)] // (a gauge)
            let f = value.clamp(0, range) as f32 / range as f32;
            Gauge::At(f)
        } else {
            Gauge::Going
        }
    })
}

/// A popup as `PopupMessage.UpdateMessage` shows it: paused, its text is
/// "paused" and its gauges and second text hide.
pub fn view(job: &Job) -> PopupView {
    let paused = job.paused;
    let text_1 = if paused {
        Some("paused".to_owned())
    } else {
        job.status_text_1.clone()
    };
    PopupView {
        key: job.key,
        title: job.status_title.clone(),
        text_1,
        gauge_1: if paused {
            None
        } else {
            gauge(job.popup_gauge_1)
        },
        text_2: if paused {
            None
        } else {
            job.status_text_2.as_deref().map(cut)
        },
        gauge_2: if paused {
            None
        } else {
            gauge(job.popup_gauge_2)
        },
        // (whose stop button is the popup's own, below)
        download: job.network_job.as_ref().map(|network| JobLine {
            can_cancel: false,
            ..network.line()
        }),
        files: job.files.as_ref().map(|(hashes, label)| {
            format!(
                "{} - show {} files",
                label.as_deref().unwrap_or("None"),
                human_int(hashes.len() as u64)
            )
        }),
        traceback: job.traceback.as_deref().map(cut),
        pausable: job.pausable,
        paused,
        cancellable: job.cancellable,
        done: job.done,
    }
}

/// The popups shown (the oldest, as many as fit), and the summary line
/// under them (`PopupMessageSummaryBar.SetNumMessages`), for all `jobs`.
pub fn shown(jobs: &[Job]) -> (Vec<PopupView>, String) {
    let views = jobs
        .iter()
        .take(hydrus_store::popups::IN_VIEW)
        .map(view)
        .collect();
    let summary = match jobs.len() {
        1 => "1 message".to_owned(),
        n => format!("{} messages", human_int(n as u64)),
    };
    (views, summary)
}

/// What the popups work with.
pub(crate) struct Hooks {
    pub pages: std::rc::Rc<std::cell::RefCell<crate::pages::Pages>>,
    pub change_pages: crate::ChangePages,
}

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn data(view: &PopupView) -> crate::PopupData {
    let gauge = |g: Option<Gauge>| match g {
        None => (false, 0.0, false),
        Some(Gauge::Going) => (true, 0.0, true),
        Some(Gauge::At(f)) => (true, f, false),
    };
    let (has_gauge_1, gauge_1, gauge_1_going) = gauge(view.gauge_1);
    let (has_gauge_2, gauge_2, gauge_2_going) = gauge(view.gauge_2);
    let text = |t: &Option<String>| t.clone().unwrap_or_default().into();
    crate::PopupData {
        title: text(&view.title),
        text_1: text(&view.text_1),
        has_gauge_1,
        gauge_1,
        gauge_1_going,
        text_2: text(&view.text_2),
        has_gauge_2,
        gauge_2,
        gauge_2_going,
        has_download: view.download.is_some(),
        download: view
            .download
            .as_ref()
            .map(crate::download_line)
            .unwrap_or_default(),
        files: text(&view.files),
        traceback: text(&view.traceback),
        pausable: view.pausable,
        paused: view.paused,
        cancellable: view.cancellable,
    }
}

/// Show the store's popups in `window`, four times a second as the
/// reference's manager looks, and do what their buttons say. Returns the
/// timer that shows them (to hold).
pub(crate) fn bind(window: &crate::MainWindow, hooks: Hooks) -> std::rc::Rc<slint::Timer> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use slint::{ComponentHandle as _, Model as _, ModelRc, VecModel};

    let hooks = Rc::new(hooks);
    let model: Rc<VecModel<crate::PopupData>> = Rc::new(VecModel::default());
    window.set_popups(ModelRc::from(model.clone()));
    // (those shown, to act on by their place)
    let shown: Rc<RefCell<Vec<PopupView>>> = Rc::default();
    let store = move |hooks: &Hooks| hooks.pages.borrow().store().clone();
    let refresh: Rc<dyn Fn()> = {
        let hooks = hooks.clone();
        let shown = shown.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            let Some(window) = weak.upgrade() else { return };
            let jobs = match store(&hooks).read(|conn| hydrus_store::popups::all(conn, now())) {
                Ok(jobs) => jobs,
                Err(e) => {
                    eprintln!("could not read the popups: {e}");
                    return;
                }
            };
            let (views, summary) = self::shown(&jobs);
            // (changed in place: rows made anew under the pointer would
            // lose its press)
            for (i, view) in views.iter().enumerate() {
                let row = data(view);
                if i < model.row_count() {
                    if model.row_data(i).as_ref() != Some(&row) {
                        model.set_row_data(i, row);
                    }
                } else {
                    model.push(row);
                }
            }
            while model.row_count() > views.len() {
                model.remove(model.row_count() - 1);
            }
            let summary = if jobs.is_empty() {
                String::new()
            } else {
                summary
            };
            window.set_popup_summary(summary.into());
            *shown.borrow_mut() = views;
        })
    };
    refresh();
    // change the popup shown at `i`, then show them again
    let change = {
        let hooks = hooks.clone();
        let shown = shown.clone();
        let refresh = refresh.clone();
        move |i: i32, f: fn(&mut Job)| {
            let key = usize::try_from(i)
                .ok()
                .and_then(|i| shown.borrow().get(i).map(|v| v.key));
            if let Some(key) = key {
                let done = store(&hooks).write(move |ctx| {
                    hydrus_store::popups::update(ctx.conn(), &key, now(), f).map(|_| ())
                });
                if let Err(e) = done {
                    eprintln!("could not change the popup: {e}");
                }
            }
            refresh();
        }
    };
    window.on_popup_dismiss({
        let change = change.clone();
        // (`TryToDismiss`: only one that is done)
        move |i| {
            change(i, |job| {
                if job.done {
                    job.finish_and_dismiss(None, now());
                }
            });
        }
    });
    window.on_popup_pause_play({
        let change = change.clone();
        move |i| change(i, Job::pause_play)
    });
    window.on_popup_cancel({
        let change = change.clone();
        move |i| {
            change(i, |job| {
                if job.cancellable {
                    job.cancel();
                }
            });
        }
    });
    window.on_popups_dismiss_all({
        let hooks = hooks.clone();
        let refresh = refresh.clone();
        move || {
            let done = store(&hooks)
                .write(|ctx| hydrus_store::popups::dismiss_all_done(ctx.conn(), now()));
            if let Err(e) = done {
                eprintln!("could not dismiss the popups: {e}");
            }
            refresh();
        }
    });
    window.on_popup_copy_traceback({
        let hooks = hooks.clone();
        let shown = shown.clone();
        move |i| {
            let Some(key) = usize::try_from(i)
                .ok()
                .and_then(|i| shown.borrow().get(i).map(|v| v.key))
            else {
                return;
            };
            let job = store(&hooks)
                .read(|conn| hydrus_store::popups::all(conn, now()))
                .ok()
                .and_then(|jobs| jobs.into_iter().find(|j| j.key == key));
            if let Some(job) = job {
                // (`CopyTB`: the version and system, then the job)
                crate::copy_to_clipboard(&format!(
                    "hydrus-rs {}, {}\n{}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    job.nice_string()
                ));
            }
        }
    });
    // "show files": a page of those still in the client, named for them
    // (`ShowFiles`); with none left, the popup goes
    window.on_popup_show_files({
        let hooks = hooks.clone();
        let shown = shown.clone();
        let refresh = refresh.clone();
        move |i| {
            let Some(key) = usize::try_from(i)
                .ok()
                .and_then(|i| shown.borrow().get(i).map(|v| v.key))
            else {
                return;
            };
            let store = store(&hooks);
            let job = store
                .read(|conn| hydrus_store::popups::all(conn, now()))
                .ok()
                .and_then(|jobs| jobs.into_iter().find(|j| j.key == key));
            let Some((hashes, label)) = job.and_then(|j| j.files) else {
                return;
            };
            let location = hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
            ));
            let ids: Vec<hydrus_core::HashId> = store
                .read(|conn| {
                    let mut ids = Vec::new();
                    for hash in &hashes {
                        if let Some(id) = hydrus_store::master::hash_id(conn, hash)? {
                            ids.push(id);
                        }
                    }
                    Ok(ids)
                })
                .unwrap_or_default();
            let present = crate::media_actions::still_in(&store, &location, &ids);
            if present.is_empty() {
                let done = store.write(move |ctx| {
                    hydrus_store::popups::update(ctx.conn(), &key, now(), |job| {
                        job.files = None;
                        if job.done {
                            job.finish_and_dismiss(None, now());
                        }
                    })
                    .map(|_| ())
                });
                if let Err(e) = done {
                    eprintln!("could not change the popup: {e}");
                }
            } else {
                let name = label.unwrap_or_else(|| "files".to_owned());
                (hooks.change_pages)(&|pages| {
                    pages.open_files(location.clone(), present.clone(), None, None);
                    pages.rename_shown(&name);
                    Ok(())
                });
            }
            refresh();
        }
    });
    let timer = Rc::new(slint::Timer::default());
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(250),
        move || refresh(),
    );
    timer
}

#[cfg(test)]
mod tests {
    use hydrus_core::Sha256;

    use super::*;

    #[test]
    fn a_popup_shows_as_the_reference_s() {
        let mut job = Job::new(true, true, 0.0);
        job.status_title = Some("subscription".into());
        job.status_text_1 = Some("downloading".into());
        job.popup_gauge_1 = Some((3, 12));
        job.status_text_2 = Some("file 3".into());
        job.popup_gauge_2 = Some((0, 0));
        job.set_files(
            vec![Sha256([1; 32]), Sha256([2; 32])],
            Some("artist".into()),
        );
        let shown = view(&job);
        assert_eq!(shown.title.as_deref(), Some("subscription"));
        assert_eq!(shown.text_1.as_deref(), Some("downloading"));
        assert_eq!(shown.gauge_1, Some(Gauge::At(0.25)));
        assert_eq!(shown.gauge_2, Some(Gauge::Going), "no range: going");
        assert_eq!(shown.files.as_deref(), Some("artist - show 2 files"));
        assert!(shown.pausable && shown.cancellable && !shown.done);
        // paused: "paused", and no gauges or second text
        job.pause_play();
        let shown = view(&job);
        assert_eq!(shown.text_1.as_deref(), Some("paused"));
        assert_eq!(
            (shown.gauge_1, shown.gauge_2, shown.text_2),
            (None, None, None)
        );
    }

    #[test]
    fn long_text_shows_its_start() {
        let mut job = Job::text("x", 0.0);
        job.status_text_2 = Some("y".repeat(2000));
        let text = view(&job).text_2.unwrap();
        assert!(text.starts_with(
            "The text is too long to display here. Here is the start of it (the rest is printed to the log):\n"
        ));
        assert_eq!(text.split_once('\n').unwrap().1, "y".repeat(1024));
    }

    #[test]
    fn ten_show_and_the_line_counts_them_all() {
        let jobs: Vec<Job> = (0..12).map(|i| Job::text(format!("{i}"), 0.0)).collect();
        let (views, summary) = shown(&jobs);
        assert_eq!(views.len(), 10);
        assert_eq!(views[0].text_1.as_deref(), Some("0"));
        assert_eq!(summary, "12 messages");
        assert_eq!(shown(&jobs[..1]).1, "1 message");
    }
}
