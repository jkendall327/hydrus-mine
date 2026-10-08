//! Database > file maintenance > clear orphan files…, from the real menu:
//! `ClientGUI._ClearOrphanFiles` asks whether to move the strays somewhere or
//! delete them (or forget it), asks where to when moving, and the job then
//! reports in a "clearing orphans" popup, taking only what the database does
//! not expect to find in the file folders.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::database_menu_jobs::{from_menu_path, pump, shown};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless, orphan_files_chooser};
use hydrus_gui_model::orphan_files as model;
use hydrus_store::Store;
use hydrus_store::storage::prefix_dir;
use slint::ComponentHandle as _;

struct Strays {
    file: PathBuf,
    thumbnail: PathBuf,
}

fn strays(media: &Path) -> Strays {
    let file = prefix_dir(media, "fab").join(format!("{}.png", "ab".repeat(32)));
    let thumbnail = prefix_dir(media, "t00").join("Thumbs.db");
    for path in [&file, &thumbnail] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    Strays { file, thumbnail }
}

fn ask(ui: &MainWindow) -> hydrus_gui::ChoiceButtonsWindow {
    from_menu_path(ui, &["file maintenance"], "clear orphan files\u{2026}");
    let chooser = orphan_files_chooser().expect("the question is asked");
    assert!(chooser.window().is_visible());
    assert_eq!(chooser.get_message(), model::QUESTION);
    assert_eq!(chooser.get_no_label(), model::NO);
    let choices: Vec<String> = slint::Model::iter(&chooser.get_choices())
        .map(|c| c.to_string())
        .collect();
    assert_eq!(choices, model::CHOICES);
    chooser
}

fn wait_cleared(store: &Store, text: &str, times: usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while shown(store)
        .iter()
        .filter(|(title, t)| {
            title.as_deref() == Some(model::POPUP_TITLE) && t.as_deref() == Some(text)
        })
        .count()
        < times
    {
        assert!(Instant::now() < deadline, "never said {text:?}");
        pump(Duration::from_millis(20));
    }
}

// leaf: audit-media-database-clear-orphan-files
#[test]
fn clearing_orphan_files_moves_or_deletes_strays_after_the_reference_s_questions() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let media = home.path().join("client_files");
    let strays = strays(&media);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let picked: std::rc::Rc<std::cell::RefCell<(Vec<String>, Vec<PathBuf>)>> = Rc::default();
    {
        let picked = picked.clone();
        hydrus_gui::set_picker(move |_, title| {
            let mut picked = picked.borrow_mut();
            picked.0.push(title.to_owned());
            std::mem::take(&mut picked.1)
        });
    }

    // "forget it" touches nothing
    ask(&ui).invoke_cancelled();
    pump(Duration::from_millis(300));
    assert!(strays.file.is_file() && strays.thumbnail.is_file());
    assert!(shown(&store).is_empty());

    // moving, but cancelling the folder choice, touches nothing either
    ask(&ui).invoke_chosen(0);
    assert_eq!(picked.borrow().0, [model::PICK_TITLE]);
    pump(Duration::from_millis(300));
    assert!(strays.file.is_file() && strays.thumbnail.is_file());
    assert!(shown(&store).is_empty());

    // moving takes the orphans out, thumbnails into a subdirectory
    let away = tempfile::tempdir().unwrap();
    picked.borrow_mut().1 = vec![away.path().to_path_buf()];
    ask(&ui).invoke_chosen(0);
    wait_cleared(&store, &model::final_text(1, 1), 1);
    assert!(!strays.file.exists() && !strays.thumbnail.exists());
    assert!(away.path().join(strays.file.file_name().unwrap()).is_file());
    assert!(away.path().join("thumbnails/Thumbs.db").is_file());

    // deleting removes them
    let again = self::strays(&media);
    ask(&ui).invoke_chosen(1);
    wait_cleared(&store, &model::final_text(1, 1), 2);
    assert!(!again.file.exists() && !again.thumbnail.exists());
}
