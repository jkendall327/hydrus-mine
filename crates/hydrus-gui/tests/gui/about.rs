//! Help > about opens the about window: the name, version and site over
//! the description, optional libraries, credits and license tabs. (What
//! they say is tested against the reference's in hydrus-gui-model.)

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};

use crate::subscriptions::store;

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
