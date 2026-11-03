//! Flow (`oracle/record_flow_ratings_filter.py`): five still images sent to
//! the inbox, rated through manage > ratings (a like, 4/5 stars and two
//! counts on two of them; a dislike and 1/5 on a third), run through the
//! archive/delete filter (keep, delete, skip, keep, delete, the last
//! decision asking, "commit"), then searched for by inbox, archive and
//! rating. After each step the files' ratings, inbox and domains, and what
//! the windows showed, must be what the reference showed.

use std::collections::BTreeSet;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Sha256;
use hydrus_core::service::ServiceType;
use hydrus_gui::{MainWindow, ManageRatingsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::media::Rating;

use super::{Known, Normaliser, State, compare};
use crate::common::widgets;

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

const SEARCHES: &[&str] = &[
    "system:inbox",
    "system:archive",
    "system:rating for favourites is like",
    "system:rating for favourites is dislike",
    "system:rating for stars > 3/5",
    "system:rating for counter = 2",
];

fn sha(hex_text: &str) -> Sha256 {
    Sha256::from_slice(&hex::decode(hex_text).unwrap()).unwrap()
}

/// What to do on one of the dialog's controls.
enum Do {
    Left,
    Right,
    /// A numerical rating's stars, clicked this far along it.
    At(f32),
}

#[test]
#[allow(clippy::too_many_lines)] // (one flow, step by step)
fn files_rated_filtered_and_searched_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_ratings_filter.json");
    let (_dirs, store) = super::basic_store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let n = Normaliser::local();
    let state = State {
        store: &store,
        ui: &ui,
        site: None,
        n: &n,
    };
    let files: Vec<String> = recording["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let search = |predicates: &[&str]| {
        while ui.get_predicates().row_count() > 0 {
            ui.invoke_remove_predicate(0);
            widgets::settle();
        }
        for p in predicates {
            ui.invoke_search_edited((*p).into());
            ui.invoke_search_accepted();
            widgets::settle();
        }
    };
    search(&["system:everything"]);
    let hash_id = |hash: &str| {
        let sha = sha(hash);
        store
            .read(move |c| hydrus_store::master::hash_id(c, &sha))
            .unwrap()
            .unwrap()
    };
    let select = |hashes: &[&String]| {
        ui.invoke_select_none();
        for (i, hash) in hashes.iter().enumerate() {
            let id = hash_id(hash);
            let at = {
                let page = bound.current.borrow();
                let page = page.borrow();
                page.results().iter().position(|&r| r == id).unwrap()
            };
            ui.invoke_thumbnail_clicked(i32::try_from(at).unwrap(), i > 0, false);
        }
        widgets::settle();
    };
    let services = store.snapshot().services.clone();
    let rating_services: Vec<(hydrus_core::ServiceId, String, ServiceType)> = {
        let mut out: Vec<_> = services
            .all()
            .filter(|s| {
                matches!(
                    s.service_type(),
                    ServiceType::LocalRatingLike
                        | ServiceType::LocalRatingNumerical
                        | ServiceType::LocalRatingIncDec
                )
            })
            .map(|s| (s.id, s.name.clone(), s.service_type()))
            .collect();
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    };
    // each file's ratings (by service name: a like 1 or 0, numerical its
    // fraction, inc/dec its count, 0 when unset), inbox and domains
    let file_states = || -> Json {
        let shown = state.files(files.iter().cloned().collect());
        let mut out = serde_json::Map::new();
        for hash in &files {
            let id = hash_id(hash);
            let ratings = store
                .read(|c| hydrus_store::media::load(c, &services, None, &[id]))
                .unwrap()
                .results
                .remove(0)
                .ratings;
            let mut by_name = serde_json::Map::new();
            for (service, name, kind) in &rating_services {
                let value = match (ratings.get(service), kind) {
                    (Some(Rating::IncDec(count)), _) => json!(count),
                    (None, ServiceType::LocalRatingIncDec) => json!(0),
                    #[allow(clippy::cast_possible_truncation)] // (0.0 or 1.0)
                    (Some(Rating::Fraction(f)), ServiceType::LocalRatingLike) => {
                        json!(f.round() as i64)
                    }
                    (Some(Rating::Fraction(f)), _) => json!(f),
                    (None, _) => Json::Null,
                };
                by_name.insert(name.clone(), value);
            }
            out.insert(
                hash.clone(),
                json!({
                    "ratings": by_name,
                    "inbox": shown[hash]["inbox"],
                    "in": shown[hash]["in"],
                }),
            );
        }
        Json::Object(out)
    };
    let step =
        |name: &str, dialog: Json| json!({"step": name, "dialog": dialog, "files": file_states()});

    // (the five sent to the inbox, so the filter has something to archive)
    let all: Vec<&String> = files.iter().collect();
    select(&all);
    ui.invoke_inbox_selected();
    ui.invoke_answer(true);
    widgets::settle();

    // 1. rate: manage > ratings on F1 and F2, then F3
    let rate = |hashes: &[&String], actions: &[(&str, Do)]| -> Json {
        select(hashes);
        // (the thumbnails' menu, on the first: manage > ratings)
        let first = {
            let id = hash_id(hashes[0]);
            let page = bound.current.borrow();
            let page = page.borrow();
            i32::try_from(page.results().iter().position(|&r| r == id).unwrap()).unwrap()
        };
        ui.invoke_thumbnail_menu_requested(first);
        let manage = ui.get_thumbnail_menu().manage;
        let id = (0..manage.row_count())
            .map(|i| manage.row_data(i).unwrap())
            .find(|r| r.label == "ratings")
            .expect("manage > ratings")
            .id;
        ui.invoke_menu_chosen(id);
        let dialog: ManageRatingsWindow = bound
            .manage_ratings
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the manage ratings dialog");
        let names: Vec<String> = dialog.get_names().iter().map(|s| s.to_string()).collect();
        let title = dialog.get_window_title().to_string();
        for (name, what) in actions {
            let row = i32::try_from(names.iter().position(|n| n == name).unwrap()).unwrap();
            match what {
                Do::Left => dialog.invoke_rating_clicked(row, true, 0.0),
                Do::Right => dialog.invoke_rating_clicked(row, false, 0.0),
                Do::At(along) => dialog.invoke_rating_clicked(row, true, *along),
            }
        }
        dialog.invoke_apply();
        widgets::settle();
        assert!(bound.manage_ratings.borrow().is_none(), "the dialog closed");
        json!({"title": title})
    };
    let seen = json!({
        "F1 F2": rate(
            &[&files[0], &files[1]],
            &[
                ("favourites", Do::Left),
                ("stars", Do::At(0.75)),
                ("counter", Do::Left),
                ("counter", Do::Left),
            ],
        ),
        "F3": rate(&[&files[2]], &[("favourites", Do::Right), ("stars", Do::At(0.25))]),
    });
    let mut steps = vec![step("rate", seen)];

    // 2. filter: the five; keep, delete, skip, keep, delete as each shows;
    // the last decision asks; commit
    select(&all);
    ui.invoke_archive_delete_filter();
    let window = bound
        .archive_delete
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the filter");
    // (the filter shows them in the page's order)
    let order: Vec<usize> = {
        let ids: Vec<_> = files.iter().map(|h| hash_id(h)).collect();
        let page = bound.current.borrow();
        let page = page.borrow();
        page.results()
            .iter()
            .filter_map(|r| ids.iter().position(|i| i == r))
            .map(|i| i + 1)
            .collect()
    };
    for (i, f) in order.iter().enumerate() {
        assert_eq!(
            window.get_caption().as_str(),
            format!("{}/{}", i + 1, order.len()),
            "the filter on F{f}"
        );
        match f {
            1 | 4 => window.invoke_keep(),
            2 | 5 => window.invoke_delete(),
            _ => window.invoke_skip(),
        }
        widgets::settle();
    }
    // (as the window lays it out: its header, if any, then each commit
    // choice's text over its "commit" button)
    let commits: Vec<String> = window
        .get_commit_labels()
        .iter()
        .map(|l| l.to_string())
        .collect();
    let header = window.get_finish_header().to_string();
    let labels: Vec<String> = std::iter::once(header)
        .filter(|h| !h.is_empty())
        .chain(commits.iter().cloned())
        .collect();
    let question = json!({
        "question": labels[0],
        "commit buttons": vec!["commit"; commits.len()],
    });
    window.invoke_commit();
    widgets::settle();
    assert!(bound.archive_delete.borrow().is_none(), "the filter closed");
    steps.push(step(
        "filter",
        json!({"order": order, "question": question}),
    ));

    // 3. search: each on its own (the five among what it finds)
    let mut searches = serde_json::Map::new();
    for text in SEARCHES {
        search(&[text]);
        let predicate = ui
            .get_predicates()
            .row_data(0)
            .map(|p| p.text.to_string())
            .unwrap_or_default();
        let ids: BTreeSet<_> = bound
            .current
            .borrow()
            .borrow()
            .results()
            .iter()
            .copied()
            .collect();
        let found: Vec<usize> = files
            .iter()
            .enumerate()
            .filter(|(_, h)| ids.contains(&hash_id(h)))
            .map(|(i, _)| i + 1)
            .collect();
        searches.insert(
            (*text).to_owned(),
            json!({"predicate": predicate, "files": found}),
        );
    }
    let mut last = step("search", json!({}));
    last["searches"] = Json::Object(searches);
    steps.push(last);

    compare(&steps, &recording, KNOWN);
}
