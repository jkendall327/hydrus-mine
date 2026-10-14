//! Flow (`oracle/record_flow_watcher.py`): a thread's URL entered on a new
//! watcher page; the watcher, highlighted, checks it and fetches its three
//! files; the site adds a post and "check now" is pressed; it checks again
//! and fetches only the new file. After each step what the user sees (the
//! watcher's logs, its checker's timing, the texts of the list and the
//! highlighted watcher box, the files) must be what the reference showed.

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::watchers::CheckerStatus;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues::{self, SeedStatus};

use super::{Daemon, Known, Normaliser, Site, State, compare, hashes_in_log, install};

const KNOWN: &[Known] = &[];

const THREAD: u32 = 100;

#[test]
fn watch_a_thread_then_check_it_again_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_watcher.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_watcher.json", &recording);
    let n = Normaliser::new(&site);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (popups and the thumbnail panel need a window that is shown)
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: &site,
        n: &n,
    };

    // pages > new page: download, watcher
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);

    let queue = || -> Option<i64> {
        bound
            .current
            .borrow()
            .borrow()
            .watchers()
            .and_then(|w| w.watchers.first().map(|r| r.queue))
    };
    let shown = || bound.current.borrow().borrow().files().len();
    // the watcher has made `checks` checks, all done, its files fetched and
    // shown (the client refreshes its downloader pages twice a second)
    let checked = |checks: usize| {
        (bound.sync)();
        let Some(queue) = queue() else {
            return false;
        };
        let searches = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
        let files = store.read(|c| queues::file_seeds(c, queue)).unwrap();
        searches.len() == checks
            && searches.iter().all(|g| g.status != SeedStatus::Unknown)
            && files.iter().all(|f| f.status != SeedStatus::Unknown)
            && ui.get_watcher_data().highlighted
            && shown() == hashes_in_log(&state.file_log(queue)).len()
    };
    let texts = || -> Json {
        (bound.sync)();
        let data = ui.get_watcher_data();
        let rows = ui.get_watcher_rows();
        let row: Vec<String> = rows
            .row_data(0)
            .unwrap()
            .cells
            .iter()
            // ("just added" shows for fifteen seconds: masked, as recorded)
            .map(|c| match n.text(&c).as_str() {
                "just added" => "DELTA".to_owned(),
                text => text.to_owned(),
            })
            .collect();
        json!({
            "row": row,
            "top": n.text(&data.top_status),
            "bottom": n.text(&data.bottom_status),
            "subject": n.text(&data.subject),
            "url": n.text(&data.url),
            "files status": n.text(&data.files_line),
            "file log summary": n.text(&ui.get_import_status()),
            "file log progress": n.text(&ui.get_import_progress()),
            "velocity": n.text(&data.velocity_line),
            "checker status": n.text(&data.checker_line),
            "check log summary": n.text(&ui.get_search_status()),
            "check now enabled": data.can_check_now,
            "checking pause enabled": data.can_pause_checking,
        })
    };
    let timing = |queue: i64| -> Json {
        let row = store
            .read(move |c| queues::queue(c, queue))
            .unwrap()
            .unwrap();
        let w = hydrus_store::watchers::watcher_state(&row).unwrap();
        let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
        json!({
            "checked": w.last_check_time > 0,
            "next check in the future": w.next_check_time > now,
            "checking status": match w.status {
                CheckerStatus::Ok => 0,
                CheckerStatus::Dead => 1,
                CheckerStatus::NotFound => 2,
            },
            "checking paused": w.checking_paused,
            "files paused": row.files_paused,
            "check now": w.check_now,
            "dead": w.status != CheckerStatus::Ok,
            "subject": w.subject,
        })
    };
    let step = |name: &str| -> Json {
        let queue = queue().expect("a watcher");
        let files = state.file_log(queue);
        let checks: Vec<Json> = store
            .read(|c| queues::gallery_seeds(c, queue))
            .unwrap()
            .iter()
            .map(|g| {
                json!({
                    "url": n.text(&g.url),
                    "status": g.status.code(),
                    "note": n.text(&g.note),
                })
            })
            .collect();
        json!({
            "step": name,
            "dialog": {},
            "page": {
                "files": files,
                "checks": checks,
                "timing": timing(queue),
                "texts": texts(),
                "thumbnails": shown(),
            },
            "files": state.files(hashes_in_log(&files)),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. watch: the thread's URL typed and entered
    ui.invoke_watcher_urls(format!("{}/thread/{THREAD}", site.base).into());
    site.wait_for_a_request(20, "the watcher page");
    super::wait_until_idle(&daemon, || checked(1));
    steps.push(step("watch"));

    // 2. check again: the site has a fourth post; "check now" on the
    // highlighted watcher
    site.set_phase(1);
    ui.invoke_watcher_check_now(true);
    super::wait_until_idle(&daemon, || checked(2));
    steps.push(step("check again"));

    compare(&steps, &recording, KNOWN);
}
