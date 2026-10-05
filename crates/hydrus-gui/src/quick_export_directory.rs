//! File > open uses the saved manual-export directory, without a chooser.
use crate::MainWindow;
use hydrus_gui_model::export_files;
use hydrus_store::{Store, popups, settings};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

const UNDETERMINED: &str = "Unfortunately, your export path could not be determined!";
type Home = Rc<dyn Fn() -> Option<PathBuf>>;
struct State {
    window: slint::Weak<MainWindow>,
    active: Rc<Cell<bool>>,
    store: Arc<Store>,
    home: RefCell<Home>,
}
impl State {
    fn available(&self) -> bool {
        self.active.get()
            && self.window.upgrade().is_some_and(|window| {
                window.window().is_visible() && window.get_question().is_empty()
            })
    }
    fn report(&self, text: String, error: bool) {
        if !self.available() {
            return;
        }
        eprintln!("{text}");
        let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
        #[allow(clippy::cast_precision_loss)] // Popup timestamps are seconds.
        let mut job = popups::Job::text(&text, now as f64);
        if error {
            job.had_error = true;
            job.traceback = Some(text);
        }
        if let Err(error) = self.store.write(move |c| popups::add(c.conn(), &job, now)) {
            eprintln!("could not report the export-directory error: {error}");
        }
    }
}
// Python expanduser uses HOME, then the POSIX account database. This stable
// std API supplies the account fallback only after checking HOME ourselves,
// because it ignores an explicitly empty HOME. Its deprecation concerns Windows,
// where we instead use Python's USERPROFILE/HOMEDRIVE+HOMEPATH rules below.
#[allow(deprecated)]
fn default_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        home_or_account(
            std::env::var_os("HOME").map(PathBuf::from),
            std::env::home_dir,
        )
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .or_else(|| {
                std::env::var_os("HOMEPATH").map(|path| {
                    let mut drive = std::env::var_os("HOMEDRIVE").unwrap_or_default();
                    drive.push(path);
                    drive
                })
            })
            .map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
// Pure owner-local resolution: an explicit empty path is a value, not absence.
#[cfg(any(unix, test))]
fn home_or_account(
    home: Option<PathBuf>,
    account: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    home.or_else(account)
}
/// An action owned by one main-window binding, including its home resolver.
#[derive(Clone)]
pub struct Control(Rc<State>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickExportDirectory")
            .field("active", &self.0.active.get())
            .finish_non_exhaustive()
    }
}
impl Control {
    pub(crate) fn new(window: &MainWindow, store: Arc<Store>, active: Rc<Cell<bool>>) -> Self {
        Self(Rc::new(State {
            window: window.as_weak(),
            store,
            active,
            home: RefCell::new(Rc::new(default_home)),
        }))
    }
    /// Substitute this owner's home lookup without changing process environment.
    pub fn set_home_resolver(&self, home: Rc<dyn Fn() -> Option<PathBuf>>) {
        if self.0.active.get() {
            *self.0.home.borrow_mut() = home;
        }
    }
    /// Re-read the saved directory on each trigger. Configured paths are passed
    /// to the existing OS opener even if missing; only the fallback is created.
    pub fn open(&self) {
        if !self.0.available() {
            return;
        }
        let naming: settings::ExportSettings = match self.0.store.read(settings::get) {
            Ok(naming) => naming,
            Err(error) => {
                self.0.report(
                    format!("Could not read the export directory: {error}"),
                    true,
                );
                return;
            }
        };
        let directory = if naming.default_directory.is_none() {
            let home = self.0.home.borrow().clone();
            let Some(home) = home() else {
                self.0.report(UNDETERMINED.to_owned(), false);
                return;
            };
            // expanduser concatenates the slash after ~ even for an explicitly
            // empty home, rather than making a relative directory.
            let path = if home.as_os_str().is_empty() {
                PathBuf::from(std::path::MAIN_SEPARATOR_STR).join("hydrus_export")
            } else {
                home.join("hydrus_export")
            };
            if !self.0.available() {
                return;
            }
            let created = if path.exists() && !path.is_dir() {
                Err(format!(
                    "Cannot create the directory \"{}\" because it already exists as a normal file!",
                    path.display()
                ))
            } else {
                std::fs::create_dir_all(&path).map_err(|error| error.to_string())
            };
            if let Err(error) = created {
                self.0.report(error, true);
                return;
            }
            path.to_string_lossy().into_owned()
        } else {
            let path = export_files::default_directory(&self.0.store, &naming);
            // Reference portable-path recovery is conditional on the original
            // spelling not existing, and does not check the replacement path.
            if cfg!(not(windows)) && !std::path::Path::new(&path).exists() {
                path.replace('\\', "/")
            } else {
                path
            }
        };
        if self.0.available() {
            crate::launch(&directory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_empty_home_wins_and_account_lookup_only_runs_for_absent_home() {
        let calls = Cell::new(0);
        let account = || {
            calls.set(calls.get() + 1);
            Some(PathBuf::from("synthetic account home"))
        };
        assert_eq!(
            home_or_account(Some(PathBuf::new()), account),
            Some(PathBuf::new())
        );
        assert_eq!(calls.get(), 0, "empty HOME must not consult the account");
        assert_eq!(
            home_or_account(Some(PathBuf::from("synthetic explicit home")), account),
            Some(PathBuf::from("synthetic explicit home"))
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            home_or_account(None, account),
            Some(PathBuf::from("synthetic account home"))
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(home_or_account(None, || None), None);
    }
}
