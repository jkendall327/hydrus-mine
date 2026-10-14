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
    let deadline = Instant::now() + Duration::from_secs(60);
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

fn tree(base: &Path) -> Vec<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(base, base, &mut out);
    out.sort();
    out
}

/// The reference's `ClearOrphans` (oracle/record_clear_orphans.py): the same strays, the same
/// answers, then what the popup said and what was left on disk compared; and the job cancelled
/// from its popup while it waits to delete, which leaves everything where it was.
// leaf: audit-media-database-clear-orphan-files
#[test]
fn clearing_orphan_files_replays_the_reference_s_runs_and_its_cancel() {
    let recording = hydrus_testkit::fixture_json("clear_orphans.json");
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let media = home.path().join("client_files");
    let away_root = tempfile::tempdir().unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let picked: Rc<std::cell::RefCell<(Vec<String>, Vec<PathBuf>)>> = Rc::default();
    {
        let picked = picked.clone();
        hydrus_gui::set_picker(move |_, title| {
            let mut picked = picked.borrow_mut();
            picked.0.push(title.to_owned());
            std::mem::take(&mut picked.1)
        });
    }
    let popup_text = || {
        shown(&store)
            .into_iter()
            .find(|(title, _)| title.as_deref() == Some(model::POPUP_TITLE))
            .and_then(|(_, text)| text)
    };
    for step in recording["steps"].as_array().unwrap() {
        let name = step["step"].as_str().unwrap();
        let inputs = &step["inputs"];
        let wanted = |key: &str| -> Vec<String> {
            step[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap().to_owned())
                .collect()
        };
        // what is in the media folder to begin with: the strays, unless the step has none
        let _ = std::fs::remove_dir_all(&media);
        if name != "nothing to clear" {
            self::strays(&media);
        }
        let _ = std::fs::remove_dir_all(away_root.path());
        std::fs::create_dir_all(away_root.path().join("away")).unwrap();
        picked.borrow_mut().0.clear();
        let chooser = {
            from_menu_path(&ui, &["file maintenance"], "clear orphan files\u{2026}");
            orphan_files_chooser().expect("the question is asked")
        };
        let asked = &step["asked"][0];
        assert_eq!(chooser.get_message(), asked[1].as_str().unwrap(), "{name}");
        assert_eq!(
            slint::Model::iter(&chooser.get_choices())
                .map(|c| c.to_string())
                .collect::<Vec<_>>(),
            asked[2]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(chooser.get_no_label(), asked[3].as_str().unwrap(), "{name}");
        match inputs["choice"].as_str() {
            None => {
                chooser.invoke_cancelled();
                pump(Duration::from_millis(300));
                assert!(shown(&store).is_empty(), "{name}");
            }
            Some(choice) => {
                if inputs["pick"].is_string() {
                    picked.borrow_mut().1 = vec![away_root.path().join("away")];
                }
                chooser.invoke_chosen(if choice == "move" { 0 } else { 1 });
                if choice == "move" {
                    let title = &step["asked"][1][1];
                    assert_eq!(
                        picked.borrow().0.last().unwrap(),
                        title.as_str().unwrap(),
                        "{name}"
                    );
                }
                if inputs["pick"].is_null() && choice == "move" {
                    pump(Duration::from_millis(300));
                    assert!(shown(&store).is_empty(), "{name}");
                } else {
                    // watch what the popup says until it is done (or, cancelled, stays put)
                    let wait_for = if inputs["cancel_when"].is_string() {
                        inputs["cancel_when"].as_str().unwrap().to_owned()
                    } else {
                        String::new()
                    };
                    let mut seen: Vec<String> = Vec::new();
                    let deadline = Instant::now() + Duration::from_secs(90);
                    loop {
                        assert!(
                            Instant::now() < deadline,
                            "{name}: never finished, saw {seen:?}"
                        );
                        pump(Duration::from_millis(5));
                        if let Some(text) = popup_text() {
                            if seen.last() != Some(&text) {
                                seen.push(text.clone());
                            }
                            if !wait_for.is_empty() && text.contains(&wait_for) {
                                let at = shown(&store)
                                    .iter()
                                    .position(|(t, _)| t.as_deref() == Some(model::POPUP_TITLE))
                                    .unwrap();
                                ui.invoke_popup_cancel(i32::try_from(at).unwrap());
                                pump(Duration::from_millis(1500));
                                break;
                            }
                            if text.ends_with("cleared!") || text == "no orphans found!" {
                                break;
                            }
                        }
                    }
                    let recorded: Vec<String> = step["statuses"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s[0].as_str().unwrap().to_owned())
                        .collect();
                    // every non-scan line it said is one the reference said (the scan's own
                    // "checking ..." lines are the folders, sampled)
                    let moving = |t: &str| t.starts_with("moving orphan");
                    for text in seen.iter().filter(|t| {
                        !t.starts_with("checking ") && t.as_str() != "preparing" && !moving(t)
                    }) {
                        assert!(
                            recorded.contains(text),
                            "{name}: {text:?} not in {recorded:?}"
                        );
                    }
                    if wait_for.is_empty() {
                        assert_eq!(seen.last(), recorded.last(), "{name}");
                    } else {
                        assert_eq!(
                            popup_text().as_deref(),
                            recorded.last().map(String::as_str),
                            "the popup is left as it was: {name}"
                        );
                    }
                }
            }
        }
        let place = |base: &Path| tree(base).into_iter().collect::<Vec<_>>();
        assert_eq!(place(&media), wanted("default_files"), "{name}");
        // (the reference's folder of moved files is `<root>/away/...`; here it is the same)
        let moved: Vec<String> = place(away_root.path());
        let expected_moved: Vec<String> = wanted("moved_files")
            .into_iter()
            .filter(|f| inputs["choice"] == "move" || name.contains("cancelled"))
            .collect();
        if inputs["choice"] == "move" && inputs["pick"].is_string() {
            assert_eq!(moved, expected_moved, "{name}");
        }
        // clear the popup for the next step
        store
            .write(|ctx| Ok(ctx.conn().execute("DELETE FROM popups", []).map(|_| ())?))
            .unwrap();
    }
}
