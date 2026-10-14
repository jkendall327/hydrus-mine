//! Help pages, as the reference's `OpenDocumentation` opens them: the page
//! from the local help folder where there is one, otherwise the question
//! "You do not have a local help!" with the online page or the guide to
//! building one as its answers.
use std::cell::RefCell;
use std::path::PathBuf;

use crate::ChoiceButtonsWindow;

/// The online help (`HC.REMOTE_HELP`).
pub const REMOTE_HELP: &str = "https://hydrusnetwork.github.io/hydrus";
/// The page about the help (`HC.DOCUMENTATION_ABOUT_DOCS`), which says how to build it.
pub const ABOUT_DOCS: &str = "about_docs.html";
/// The reference's pages that hydrus-rs has a button for.
pub const INDEX: &str = "index.html";
pub const CHANGELOG: &str = "changelog.html#version_688";
pub const EXTERNAL_PROGRAMS: &str = "external_programs.html";
pub const IMPORT_OPTIONS: &str = "getting_started_import_options.html";

/// The question asked when the page is not in the local help.
pub const NO_LOCAL_HELP: &str = "You do not have a local help! Are you running from source? Would you like to open the online help or see a guide on how to build your own?";

thread_local! {
    static HELP_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static QUESTION: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// Look for the local help in `dir` rather than the `help` folder beside the
/// program (for tests).
pub fn set_help_dir(dir: Option<PathBuf>) {
    HELP_DIR.with(|d| *d.borrow_mut() = dir);
}

fn help_dir() -> PathBuf {
    HELP_DIR
        .with(|d| d.borrow().clone())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("help")))
        })
        .unwrap_or_else(|| PathBuf::from("help"))
}

/// The question window that is open, if any (for tests).
pub fn last_question() -> Option<ChoiceButtonsWindow> {
    QUESTION.with(|q| {
        q.borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    })
}

/// Open the help page `page` (a path under the help, possibly with a `#fragment`).
pub fn open(page: &str) {
    let local = help_dir().join(page.split('#').next().unwrap_or(page));
    if local.is_file() {
        let launch = help_dir().join(page);
        crate::launch(&format!("file:///{}", launch.display()));
        return;
    }
    eprintln!(
        "Was asked to open \"{page}\", which appeared to be \"{}\" locally, but it did not seem to exist!",
        local.display()
    );
    let page = page.to_owned();
    let asked = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: "Are you sure?",
            message: NO_LOCAL_HELP,
            choices: vec![
                "open online help".to_owned(),
                "open how to build guide".to_owned(),
            ],
            no_label: "forget it",
        },
        move |choice| {
            QUESTION.with(|q| q.borrow_mut().take());
            let url = match choice {
                Some(0) => format!("{REMOTE_HELP}/{}", page.trim_start_matches('/')),
                Some(1) => format!("{REMOTE_HELP}/{ABOUT_DOCS}"),
                _ => return,
            };
            crate::launch(&url);
        },
    );
    match asked {
        Ok(window) => QUESTION.with(|q| *q.borrow_mut() = window),
        Err(error) => eprintln!("could not ask about the help: {error}"),
    }
}
