//! Flow 2 (`oracle/record_flow_download.py`): a post's URL entered on a new
//! URL downloader page and downloaded by the daemon; its thumbnail then
//! tagged through manage tags, archived and deleted to the trash. After
//! each step what the user sees must be what the reference showed.

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues;

use super::{Daemon, Known, Normaliser, Site, State, compare, hashes_in_log, install};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

#[test]
fn download_a_url_then_tag_archive_and_delete_it_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_download.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_download.json", &recording);
    let n = Normaliser::new(&site);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (popups and the thumbnail actions need a window that is shown)
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: &site,
        n: &n,
    };

    // pages > new page: download, urls
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .expect("a url downloader page");
    // the page done with its URLs, and showing what it imported (the
    // client refreshes its downloader pages twice a second)
    let page_done = || {
        (bound.sync)();
        let files = store.read(|c| queues::file_seeds(c, queue)).unwrap();
        !files.is_empty()
            && files
                .iter()
                .all(|f| f.status != queues::SeedStatus::Unknown)
            && bound.current.borrow().borrow().files().len()
                == hashes_in_log(&state.file_log(queue)).len()
    };
    let step = |name: &str, dialog: Json| -> Json {
        let files = state.file_log(queue);
        json!({
            "step": name,
            "dialog": dialog,
            "page": {
                "files": files,
                "thumbnails": bound.current.borrow().borrow().files().len(),
            },
            "files": state.files(hashes_in_log(&files)),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. download: the post's URL typed and entered
    ui.invoke_url_entered(format!("{}/post/1", site.base).into());
    site.wait_for_a_request(20, "the url downloader page");
    super::wait_until_idle(&daemon, page_done);
    steps.push(step("download", json!({})));

    // 2. tag: the thumbnail clicked; manage tags, "favourite" entered in
    // my tags, apply
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_manage_tags_selected();
    let manage = bound
        .manage_tags
        .borrow()
        .as_ref()
        .expect("the manage tags dialog")
        .clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    // (the tabs as labelled; the title without the application's name,
    // which Qt adds to the window, not to its title)
    let tabs: Vec<String> = manage
        .get_service_labels()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let seen = json!({
        "title": manage.get_window_title().as_str(),
        "services": tabs,
        "current tab": tabs[mine],
    });
    manage.invoke_text_edited("favourite".into());
    manage.invoke_entered();
    manage.invoke_apply();
    super::wait_until_idle(&daemon, page_done);
    steps.push(step("tag", seen));

    // 3. archive
    ui.invoke_archive_selected();
    super::wait_until_idle(&daemon, page_done);
    steps.push(step("archive", json!({})));

    // 4. delete, answering what it asks
    ui.invoke_delete_selected();
    let dialog = super::super::common::delete_dialog(&bound).expect("the delete files dialog");
    let actions: Vec<String> = dialog.get_actions().iter().map(|a| a.to_string()).collect();
    let title = dialog.get_composed_title().to_string();
    let seen = json!({
        // (the title without the application's name, which Qt adds to the
        // window, not to its title)
        "title": title.strip_suffix(hydrus_gui::app_title::current().as_str()).unwrap_or(&title),
        "description": if actions.len() == 1 { json!(actions[0]) } else { json!(null) },
        "actions": if actions.len() == 1 { json!(null) } else { json!(actions) },
    });
    dialog.invoke_accept_deletion();
    super::wait_until_idle(&daemon, page_done);
    steps.push(step("delete", seen));

    compare(&steps, &recording, KNOWN);
}
