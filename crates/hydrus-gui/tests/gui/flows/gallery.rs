//! Flow (`oracle/record_flow_gallery.py`): a new gallery downloader page,
//! the downloader chosen, a file limit of five and a query entered; the
//! search runs through its result pages until the limit and the files are
//! fetched; a second query is entered and runs. After each step what the
//! user sees (each query's file log and search log, the files with their
//! tags and URLs, the list's rows and totals, the highlighted query's box,
//! the thumbnails) must be what the reference showed.

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues::{self, SeedStatus};

use super::{Daemon, Known, Normaliser, Site, State, compare, hashes_in_log, install};

const KNOWN: &[Known] = &[];

const FILE_LIMIT: i32 = 5;

#[test]
fn search_a_gallery_with_a_file_limit_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_gallery.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_gallery.json", &recording);
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

    // pages > new page: download, gallery (the chooser's buttons by label)
    let choose = |label: &str| {
        let labels = ui.get_chooser_labels();
        let at = labels
            .iter()
            .position(|l| l == label)
            .unwrap_or_else(|| panic!("a \"{label}\" button in the page chooser"));
        ui.invoke_chooser_pressed(i32::try_from(at + 1).unwrap());
    };
    ui.invoke_new_page();
    choose("download");
    choose("gallery");

    let queries = || -> Vec<(i64, String)> {
        bound
            .current
            .borrow()
            .borrow()
            .gallery()
            .map(|g| {
                g.queries
                    .iter()
                    .map(|q| (q.queue, q.query.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let shown = || bound.current.borrow().borrow().files().len();
    // `count` queries made, their searches finished and their files done
    // and shown (the client refreshes its downloader pages twice a second)
    let searched = |count: usize| {
        (bound.sync)();
        let queries = queries();
        queries.len() == count
            && ui.get_gallery_data().highlighted
            && queries.iter().all(|(queue, _)| {
                let searches = store.read(|c| queues::gallery_seeds(c, *queue)).unwrap();
                let files = store.read(|c| queues::file_seeds(c, *queue)).unwrap();
                !searches.is_empty()
                    && searches.iter().all(|g| g.status != SeedStatus::Unknown)
                    && files.iter().all(|f| f.status != SeedStatus::Unknown)
            })
            && shown() == highlighted_hashes(&state, &bound).len()
    };
    let texts = || -> Json {
        (bound.sync)();
        let data = ui.get_gallery_data();
        let rows: Vec<Vec<String>> = ui
            .get_gallery_rows()
            .iter()
            .map(|r| r.cells.iter().map(|c| n.text(&c)).collect())
            .collect();
        json!({
            "rows": rows,
            "top": n.text(&data.top_status),
            "bottom": n.text(&data.bottom_status),
            "query": n.text(&data.highlighted_query),
            "files status": n.text(&data.files_line),
            "file log summary": n.text(&ui.get_import_status()),
            "file log progress": n.text(&ui.get_import_progress()),
            "search status": n.text(&data.search_line),
            "search log summary": n.text(&ui.get_search_status()),
        })
    };
    let step = |name: &str| -> Json {
        let mut hashes = std::collections::BTreeSet::new();
        let mut states = Vec::new();
        for (queue, text) in queries() {
            let files = state.file_log(queue);
            hashes.extend(hashes_in_log(&files));
            let searches = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
            let query = bound
                .current
                .borrow()
                .borrow()
                .gallery()
                .and_then(|g| g.query(queue).cloned())
                .unwrap();
            states.push(json!({
                "text": text,
                "source": query.source,
                "files": files,
                "searches": searches.iter().map(|g| json!({
                    "url": n.text(&g.url),
                    "status": g.status.code(),
                    "note": n.text(&g.note),
                })).collect::<Vec<_>>(),
                "finished": query.gallery_finished(),
                "files finished": query.files_finished(),
            }));
        }
        json!({
            "step": name,
            "dialog": {},
            "page": {
                "queries": states,
                "texts": texts(),
                "thumbnails": shown(),
            },
            "files": state.files(hashes),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. search: the downloader chosen, the limit set, the query entered
    let gugs = ui.get_gallery_data().gug_names;
    let gug = gugs
        .iter()
        .position(|g| g == "local booru tag search")
        .expect("the downloader is offered");
    ui.invoke_gallery_gug(i32::try_from(gug).unwrap());
    ui.invoke_gallery_limit(false, FILE_LIMIT);
    ui.invoke_gallery_queries("blue_eyes".into());
    site.wait_for_a_request(20, "the gallery page");
    super::wait_until_idle(&daemon, || searched(1));
    steps.push(step("search"));

    // 2. second search
    ui.invoke_gallery_queries("red_hair".into());
    super::wait_until_idle(&daemon, || searched(2));
    steps.push(step("second search"));

    // 3. raise the limit: the highlighted query's own, to 20; its search
    // has no page left to read (the limit stopped it adding the next), so
    // nothing more is fetched, but the status "hit file limit" clears
    ui.invoke_gallery_shown_limit(false, 20);
    super::wait_until_idle(&daemon, || {
        searched(2) && ui.get_gallery_data().search_line.is_empty()
    });
    steps.push(step("raise the limit"));

    compare(&steps, &recording, KNOWN);
}

/// The files of the query the page shows.
fn highlighted_hashes(
    state: &State<'_>,
    bound: &hydrus_gui::Bound,
) -> std::collections::BTreeSet<String> {
    let queue = bound
        .current
        .borrow()
        .borrow()
        .gallery()
        .and_then(|g| g.state.highlighted);
    queue.map_or_else(Default::default, |q| hashes_in_log(&state.file_log(q)))
}
