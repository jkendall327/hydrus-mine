//! A simple downloader page: made on the formula last chosen, URLs typed
//! becoming jobs with the formula chosen (full URLs only, each once), the
//! formula chosen kept for new pages, and its jobs moved, deleted and its
//! parsing paused, all in its queue for the daemon to work.

use slint::Model as _;

use hydrus_gui::page_chooser::NewPage;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues::{self, SimpleDownloader};
use hydrus_store::settings::SimpleDownloaderFormulae;

use crate::subscriptions::store;

fn jobs(ui: &MainWindow) -> Vec<String> {
    let rows = ui.get_simple_jobs();
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn a_simple_downloader_page_queues_pages_with_its_formula() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (bound.open_page)(&NewPage::SimpleDownloader);
    assert_eq!(bound.pages.borrow().shown().name, "simple downloader");
    assert!(ui.get_importing() && ui.get_simple_downloader());
    let formulae: Vec<String> = ui
        .get_simple_formulae()
        .iter()
        .map(|f| f.to_string())
        .collect();
    assert_eq!(
        formulae,
        [
            "all files linked by images in page",
            "all images embedded in page"
        ]
    );
    assert_eq!(ui.get_simple_formula(), 0);
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let state = || {
        let made = store
            .read(move |c| queues::queue(c, queue))
            .unwrap()
            .unwrap();
        SimpleDownloader::of(&made).unwrap()
    };
    assert_eq!(state().formula_name, "all files linked by images in page");

    // URLs typed: full ones only, each once
    ui.invoke_url_entered("https://example.com/a\nnot a url\nhttps://example.com/a\n".into());
    assert_eq!(
        jobs(&ui),
        ["all files linked by images in page: https://example.com/a"]
    );
    // the other formula: new jobs use it, and new pages start on it
    ui.set_simple_formula(1);
    ui.invoke_simple_formula_chosen(1);
    assert_eq!(state().formula_name, "all images embedded in page");
    let kept: SimpleDownloaderFormulae = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(kept.favourite, "all images embedded in page");
    ui.invoke_url_entered("https://example.com/b".into());
    ui.invoke_url_entered("https://example.com/c".into());
    assert_eq!(
        jobs(&ui),
        [
            "all files linked by images in page: https://example.com/a",
            "all images embedded in page: https://example.com/b",
            "all images embedded in page: https://example.com/c",
        ]
    );
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 1100, 760);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("simple_downloader.png"), &pixels, 1100, 760).unwrap();

    // the last moved up, staying selected, then deleted
    assert!(!ui.get_simple_job_selected());
    ui.invoke_simple_job_clicked(2, false, false);
    assert!(ui.get_simple_job_selected());
    ui.invoke_simple_move_jobs(-1);
    assert_eq!(
        jobs(&ui)[1],
        "all images embedded in page: https://example.com/c"
    );
    let rows = ui.get_simple_jobs();
    assert!(rows.row_data(1).unwrap().selected);
    ui.invoke_simple_delete_jobs();
    let urls: Vec<String> = state().pending.into_iter().map(|j| j.url).collect();
    assert_eq!(urls, ["https://example.com/a", "https://example.com/b"]);

    // the parsing paused alone, then the files
    ui.invoke_simple_pause_play_queue();
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert!(made.gallery_paused && !made.files_paused);
    assert!(ui.get_simple_queue_paused());
    assert!(ui.get_simple_parser_status().starts_with("paused"));
    ui.invoke_pause_play_files();
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert!(made.gallery_paused && made.files_paused);

    // a new page starts on the formula last chosen
    (bound.open_page)(&NewPage::SimpleDownloader);
    assert_eq!(ui.get_simple_formula(), 1);
}
