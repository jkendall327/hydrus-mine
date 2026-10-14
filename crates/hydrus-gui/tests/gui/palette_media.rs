//! The command palette's media menu provider on a real page with files selected, against
//! the reference's (`oracle/record_palette_media_rows.py`).
use hydrus_gui::CommandPaletteWindow;
use hydrus_store::command_palette::CommandPaletteSettings;
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};

/// The non-heading rows the palette shows for `query`: (text, menu path), once they have settled.
fn rows_for(window: &CommandPaletteWindow, query: &str) -> Vec<(String, String)> {
    window.invoke_query_edited(query.into());
    let read = || {
        let rows = window.get_rows();
        let mut under_media = false;
        let mut found = Vec::new();
        for r in (0..rows.row_count()).map(|i| rows.row_data(i).unwrap()) {
            if r.heading {
                under_media = r.primary.as_str() == "Media";
            } else if under_media {
                found.push((r.primary.to_string(), r.secondary.to_string()));
            }
        }
        found
    };
    let started = Instant::now();
    let mut last = Vec::new();
    let mut stable = 0;
    loop {
        window.invoke_poll();
        let now = read();
        if now == last {
            stable += 1;
        } else {
            stable = 0;
            last = now;
        }
        if stable >= 100 || started.elapsed() > Duration::from_secs(5) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The reference's single-file copy entries for the bitmap exchange (not ported: DIFFERENCES.md).
const KNOWN_MISSING: [&str; 2] = ["copy bitmap", "copy thumbnail bitmap"];

// (not tagged: the two bitmap rows are missing natively; the media-menu leaf stays weak)
#[test]
fn media_menu_rows_for_selected_files_are_the_reference_s_but_two() {
    let recorded = hydrus_testkit::fixture_json("palette_media_rows.json");
    let (dirs, store) = crate::options_gui_support::basic_store();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &CommandPaletteSettings {
                    show_media_menu: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let client = crate::options_gui_support::Client::with(dirs, store.clone());
    // a page of exactly the recorded files
    let wanted: Vec<hydrus_core::Sha256> = recorded["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse().unwrap())
        .collect();
    let ids: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            wanted
                .iter()
                .map(|h| Ok(hydrus_store::master::hash_id(conn, h)?.unwrap()))
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    let location = client.bound.current.borrow().borrow().location().clone();
    client
        .bound
        .pages
        .borrow_mut()
        .open_files(location, ids.clone(), None, None);
    (client.bound.sync)();
    let last = client.bound.pages.borrow().page_count() - 1;
    client.ui.invoke_tab_chosen(0, i32::try_from(last).unwrap());
    let page = client.bound.current.borrow().clone();
    assert_eq!(page.borrow().results().len(), ids.len());
    for selection in recorded["selections"].as_array().unwrap() {
        let count = usize::try_from(selection["count"].as_u64().unwrap()).unwrap();
        // one file: a click on the first thumbnail selects and focuses it (the reference's
        // `_HitMedia`); three: all selected, none focused (its `_Select`)
        page.borrow_mut().select_files(&[]);
        if count == 1 {
            client.ui.invoke_thumbnail_clicked(0, false, false);
        } else if count == 3 {
            page.borrow_mut().select_files(&ids[..3]);
        }
        client.ui.invoke_command_palette_requested();
        let window = client
            .bound
            .command_palette
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        for query in recorded["queries"].as_array().unwrap() {
            let q = query.as_str().unwrap();
            let expected: Vec<(String, String)> = selection["rows"][q]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    (
                        r[0].as_str()
                            .unwrap()
                            .replace("<b>", "")
                            .replace("</b>", "")
                            .replace("&#x27;", "'"),
                        r[1].as_str().unwrap().to_owned(),
                    )
                })
                .filter(|row| !(count == 1 && KNOWN_MISSING.contains(&row.0.as_str())))
                .collect();
            assert_eq!(
                rows_for(&window, q),
                expected,
                "{} / {q}",
                selection["label"]
            );
        }
        window.invoke_cancel();
    }

    // activating a row runs its menu action: the recorded effect on the selection
    for activation in recorded["activations"].as_array().unwrap() {
        page.borrow_mut().select_files(&[]);
        client.ui.invoke_command_palette_requested();
        let window = client
            .bound
            .command_palette
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let query = activation["query"].as_str().unwrap();
        let text = activation["row"][0]
            .as_str()
            .unwrap()
            .replace("<b>", "")
            .replace("</b>", "");
        rows_for(&window, query);
        let rows = window.get_rows();
        let at = (0..rows.row_count())
            .find(|&i| {
                let r = rows.row_data(i).unwrap();
                !r.heading && r.primary.as_str() == text
            })
            .unwrap_or_else(|| panic!("no {text:?} row"));
        window.invoke_activate(i32::try_from(at).unwrap());
        assert_eq!(
            page.borrow().selected_files().len() as u64,
            activation["selected"].as_u64().unwrap(),
            "{text}"
        );
    }
}
