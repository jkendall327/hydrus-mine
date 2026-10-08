//! Help > about opens the about window: the name, version and site over
//! the description, optional libraries, credits and license tabs. (What
//! they say is tested against the reference's in hydrus-gui-model.)

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};

use crate::subscriptions::store;

/// A line's label: before its ": ", or its first two words ("running on").
fn label(line: &str) -> String {
    line.split_once(": ").map_or_else(
        || line.split(' ').take(2).collect::<Vec<_>>().join(" "),
        |(l, _)| l.to_owned(),
    )
}

// leaf: audit-options-menu-menu-help-about
#[test]
fn help_about_opens_the_about_window() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let titles = ui.get_menu_titles();
    let help = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(help).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let about = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "about")
        .unwrap();
    assert!(lines.row_data(about).unwrap().usable);
    ui.invoke_menu_line_clicked(0, i32::try_from(about).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.about.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(window.get_window_title(), "about hydrus");
    assert_eq!(window.get_name(), "hydrus-rs");
    assert!(
        window
            .get_version()
            .ends_with(", porting hydrus v688, using network version 20")
    );
    assert_eq!(window.get_site(), "https://hydrusnetwork.github.io/hydrus/");
    let tabs: Vec<String> = window.get_tabs().iter().map(|t| t.to_string()).collect();
    assert_eq!(
        tabs,
        ["Description", "Optional Libraries", "Credits", "License"]
    );
    let texts = window.get_texts();
    let description = texts.row_data(0).unwrap();
    // (the store's own directory and database settings)
    assert!(description.contains(&format!("db dir: {}", store.dir().display())));
    assert!(description.contains("db journal mode: wal"));
    assert!(description.contains("db cache size per file: 256MB"));
    // each line of the description is one of the reference's, by label
    let recorded = hydrus_testkit::fixture_json("about_window.json");
    let theirs = recorded["tabs"][0][1].as_str().unwrap();
    let their_labels: Vec<String> = theirs
        .split_once("\n\n")
        .unwrap()
        .1
        .lines()
        .map(label)
        .collect();
    let lines: Vec<String> = description
        .split_once("\n\n")
        .unwrap()
        .1
        .lines()
        .map(str::to_owned)
        .collect();
    for line in &lines {
        assert!(
            their_labels.contains(&label(line)),
            "{line:?} isn't one of {their_labels:?}"
        );
    }
    for wanted in ["locale", "db using memory for temp?", "temp dir"] {
        assert!(lines.iter().any(|l| label(l) == wanted), "{wanted}");
    }
    // the libraries tab is the reference's form, a line each, in groups
    let libraries = texts.row_data(1).unwrap();
    let forms = ["yes", "not available", "no - error"];
    for line in libraries.lines().filter(|l| !l.is_empty()) {
        let (_, state) = line.split_once(": ").unwrap();
        assert!(
            forms.contains(&state) || (state.starts_with("yes (") && state.ends_with(')')),
            "{line:?}"
        );
    }
    assert!(libraries.contains("\n\n"), "in groups");
    assert!(libraries.contains("mpv: not available"));
    assert!(libraries.lines().any(|l| l.starts_with("ffmpeg: ")));
    assert!(
        texts
            .row_data(3)
            .unwrap()
            .starts_with("           DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE")
    );
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 620, 560);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("about.png"), &pixels, 620, 560).unwrap();
    window.invoke_close_clicked();
}
