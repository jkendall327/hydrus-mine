//! The write autocomplete as a user meets it in Manage tags and the sibling and
//! parent editors: typing shows the recorded rows with the recorded first
//! highlight under each option set, Enter or a double-click enters what is
//! highlighted, and without fetch-as-you-type only Ctrl+Space fetches.
use crate::options_gui_support::basic_store;
use hydrus_core::{Sha256, Tag};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::{Store, settings, tag_editing::TagEditingSettings};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};
use std::collections::BTreeSet;
use std::sync::Arc;

fn seeded(fixture: &Value) -> (Vec<tempfile::TempDir>, Arc<Store>) {
    let (dirs, store) = basic_store();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = fixture["corpus"].clone();
    store
        .write_content(move |w| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    w.conn(),
                    &Tag::from_clean(row["tag"].as_str().unwrap()),
                )?;
                let files = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| {
                        let hash: Sha256 = h.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(w.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &files,
                )?;
            }
            Ok(())
        })
        .unwrap();
    for (kind, pairs) in [
        (
            hydrus_store::display::RelationKind::Siblings,
            vec![("parity:amber old", "parity:amber")],
        ),
        (
            hydrus_store::display::RelationKind::Parents,
            vec![
                ("parity:amber", "parity:colour"),
                ("parity:colour", "parity:root"),
            ],
        ),
    ] {
        tag_relations::apply(
            &store,
            kind,
            pairs
                .into_iter()
                .map(|(left, right)| RelationUpdate {
                    service,
                    left: Tag::new(left).unwrap(),
                    right: Tag::new(right).unwrap(),
                    action: RelationAction::Add,
                })
                .collect(),
        )
        .unwrap();
    }
    // (the recorded menu is on a tag that is a favourite)
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::FavouriteTags(vec!["parity:amber old".into()]),
            )
        })
        .unwrap();
    (dirs, store)
}

/// Manage tags on a file the corpus does not tag, opened from a page.
fn open_manage_tags(
    store: &Arc<Store>,
    fixture: &Value,
) -> (MainWindow, hydrus_gui::Bound, hydrus_gui::ManageTagsWindow) {
    let tagged: BTreeSet<hydrus_core::HashId> = store
        .read(|conn| {
            let mut ids = BTreeSet::new();
            for row in fixture["corpus"].as_array().unwrap() {
                for hash in row["hashes"].as_array().unwrap() {
                    let hash: Sha256 = hash.as_str().unwrap().parse().unwrap();
                    ids.extend(hydrus_store::master::hash_id(conn, &hash)?);
                }
            }
            Ok(ids)
        })
        .unwrap();
    let file = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<hydrus_core::HashId>>>()?)
        })
        .unwrap()
        .into_iter()
        .find(|id| !tagged.contains(id))
        .expect("a file outside the corpus");
    let mut page = SearchPage::new(store.clone());
    page.choose_location(hydrus_core::search::context::LocationContext::default());
    page.enter();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    bound.current.borrow().borrow_mut().select_files(&[file]);
    ui.invoke_manage_tags_selected();
    let w = bound
        .manage_tags
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    (ui, bound, w)
}

/// Whether pasting many lines asks first, as the option says.
fn set_skip_paste_question(store: &Store, skip: bool) {
    store
        .write(move |ctx| {
            let mut tags: TagEditingSettings = settings::get(ctx.conn())?;
            tags.skip_multiline_paste_confirmation = skip;
            settings::set(ctx.conn(), &tags)
        })
        .unwrap();
}

/// The recorded option set of one query, as the options write it.
fn apply_query(store: &Store, query: &Value) {
    let query = query.clone();
    let snapshot = store.snapshot();
    let key = snapshot.services.by_name("my tags").unwrap().key.clone();
    let count = snapshot
        .services
        .by_name(query["service"].as_str().unwrap())
        .unwrap()
        .key
        .clone();
    store
        .write(move |ctx| {
            let mut tags: TagEditingSettings = settings::get(ctx.conn())?;
            tags.select_first_with_count = query["first_with_count"].as_bool().unwrap();
            tags.autocomplete_expand_parents = query["expanded"].as_bool().unwrap();
            tags.autocomplete_show_parents = query["parents"].as_bool().unwrap();
            tags.autocomplete_show_siblings = query["siblings"].as_bool().unwrap();
            settings::set(ctx.conn(), &tags)?;
            let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                settings::get(ctx.conn())?;
            let mut options = widgets.options(&key);
            options.write_tag_service = count;
            options.fetch_automatically = true;
            widgets.services.insert(key.to_hex(), options);
            settings::set(ctx.conn(), &widgets)
        })
        .unwrap();
}

/// The rows grouped by suggestion (a row indented four spaces belongs to the one
/// before it), the parents of each as an unordered bag as Qt lists them.
fn grouped(rows: &[String]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for row in rows {
        if row.starts_with("    ") && !out.is_empty() {
            out.last_mut().unwrap().push(row.clone());
        } else {
            out.push(vec![row.clone()]);
        }
    }
    for group in &mut out {
        group[1..].sort();
    }
    out
}

fn expected_groups(query: &Value) -> Vec<Vec<String>> {
    let rows: Vec<String> = query["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["rows"].as_array().unwrap().clone())
        .map(|r| r.as_str().unwrap().to_owned())
        .collect();
    grouped(&rows)
}

macro_rules! panes_of {
    ($window:expr) => {
        || {
            let panes = $window.get_tag_menu_panes();
            (0..panes.row_count())
                .map(|i| {
                    let lines = panes.row_data(i).unwrap().lines;
                    (0..lines.row_count())
                        .map(|j| {
                            let line = lines.row_data(j).unwrap();
                            (line.label.to_string(), line.kind)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        }
    };
}

/// The menu's entries as paths ("copy", "amber old"), a submenu's opened to list
/// its entries: the first two levels, which is all the reference's menu has.
fn menu_paths(
    panes: impl Fn() -> Vec<Vec<(String, i32)>>,
    click: impl Fn(i32, i32),
) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let top = panes().remove(0);
    for (line, (label, kind)) in top.iter().enumerate() {
        match kind {
            2 => {}
            3 => {
                click(0, i32::try_from(line).unwrap());
                for (item, kind) in panes().remove(1) {
                    if kind != 2 {
                        out.push(vec![label.clone(), item]);
                    }
                }
            }
            _ => out.push(vec![label.clone()]),
        }
    }
    out
}

fn recorded_menu_paths(f: &Value) -> Vec<Vec<String>> {
    f["menus"][0]["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

/// Whether two sets of menu paths are the same entries (Qt lists a parent's
/// favourites and service entries in its own order).
fn same_paths(mut a: Vec<Vec<String>>, mut b: Vec<Vec<String>>) -> bool {
    a.sort();
    b.sort();
    a == b
}

/// The tag a listed row is for ("tag (3)", "tag (1) → ideal" and "tag" are all "tag").
fn tag_of(row: &str) -> String {
    row.split(" (").next().unwrap().to_owned()
}

/// The tags a Manage tags list shows (its indented parent rows are not tags of the files).
fn listed_tags(w: &hydrus_gui::ManageTagsWindow) -> BTreeSet<String> {
    w.get_tags()
        .iter()
        .filter(|r| !r.text.starts_with(' '))
        .map(|r| tag_of(&r.text))
        .collect()
}

/// The tags listed on one side of a sibling or parent editor.
fn side_tags(w: &hydrus_gui::TagRelationshipsWindow, right: bool) -> BTreeSet<String> {
    let rows = if right {
        w.get_right_tags()
    } else {
        w.get_left_tags()
    };
    rows.iter().map(|r| tag_of(&r.text)).collect()
}

/// The tags an event pasted, by the recording.
fn recorded_pasted(event: &Value) -> BTreeSet<String> {
    event["pasted"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|batch| batch.as_array().unwrap())
        .map(|t| t.as_str().unwrap().to_owned())
        .collect()
}

fn recorded_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split_once("\n\n")
        .map_or("", |(_, lines)| lines)
        .lines()
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

// leaf: audit-media-tags-autocomplete
#[test]
fn manage_tags_typing_highlighting_entering_and_fetching_follow_the_recording() {
    let f = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dirs, store) = seeded(&f);
    let _windows = headless::init();
    for query in f["queries"].as_array().unwrap() {
        apply_query(&store, query);
        let (_ui, _bound, w) = open_manage_tags(&store, &f);
        w.invoke_text_edited(query["text"].as_str().unwrap().into());
        let rows: Vec<String> = w
            .get_suggestions()
            .iter()
            .map(|r| r.text.to_string())
            .collect();
        assert_eq!(grouped(&rows), expected_groups(query), "{query}");
        // Enter enters what is highlighted
        let before = listed_tags(&w);
        w.invoke_entered();
        let after = listed_tags(&w);
        // exactly what the recording had highlighted is added
        let entered = query["selected"][0].as_str().unwrap();
        assert_eq!(
            after.difference(&before).cloned().collect::<Vec<_>>(),
            [entered.to_owned()],
            "{query}"
        );
        // (the input is cleared by an entry: typed again for the rest)
        w.invoke_text_edited(query["text"].as_str().unwrap().into());
        let rows: Vec<String> = w
            .get_suggestions()
            .iter()
            .map(|r| r.text.to_string())
            .collect();
        if query == &f["queries"][0] {
            // the context menu on a result is the recorded one
            let at = rows
                .iter()
                .position(|r| r.starts_with("parity:amber old"))
                .unwrap();
            w.invoke_context_menu(i32::try_from(at).unwrap(), 10.0, 10.0);
            let paths = menu_paths(panes_of!(w), |pane, line| {
                w.invoke_tag_menu_clicked(pane, line, 100.0, 50.0, 10.0);
            });
            let recorded = recorded_menu_paths(&f);
            assert!(
                same_paths(paths.clone(), recorded.clone()),
                "only native {:?}; only recorded {:?}",
                paths
                    .iter()
                    .filter(|p| !recorded.contains(p))
                    .collect::<Vec<_>>(),
                recorded
                    .iter()
                    .filter(|p| !paths.contains(p))
                    .collect::<Vec<_>>()
            );
            w.invoke_tag_menu_dismissed();
            // pasting asks about many lines, and only ever adds
            for event in f["paste"].as_array().unwrap() {
                set_skip_paste_question(&store, event["skip"].as_bool().unwrap());
                let text = event["text"].as_str().unwrap().to_owned();
                hydrus_gui::set_paster(move || text.clone());
                let before: BTreeSet<String> = listed_tags(&w);
                let consumed = w.invoke_paste_requested(event["button"].as_bool().unwrap());
                // (a question is raised before the paste is known to be consumed)
                if !event["consumed"].is_null() && event["asked"].as_array().unwrap().is_empty() {
                    assert_eq!(consumed, event["consumed"].as_bool().unwrap(), "{event}");
                }
                let asked = event["asked"].as_array().unwrap();
                if asked.is_empty() {
                    assert!(w.get_question().is_empty(), "{event}");
                } else {
                    assert_eq!(
                        recorded_lines(&w.get_question()),
                        recorded_lines(asked[0]["message"].as_str().unwrap()),
                        "{event}"
                    );
                    w.invoke_paste_answered(event["answer"].as_bool().unwrap());
                }
                // exactly the tags the recording pasted are added, and nothing else
                let after: BTreeSet<String> = listed_tags(&w);
                let added: BTreeSet<String> = after.difference(&before).cloned().collect();
                assert_eq!(added, recorded_pasted(event), "{event}");
            }
            w.invoke_text_edited(query["text"].as_str().unwrap().into());
        }
        w.invoke_cancel();
    }
    // without fetch-as-you-type only the typed tag is offered until Ctrl+Space
    let query = &f["queries"][0];
    apply_query(&store, query);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    store
        .write(move |ctx| {
            let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                settings::get(ctx.conn())?;
            let mut options = widgets.options(&key);
            options.fetch_automatically = false;
            widgets.services.insert(key.to_hex(), options);
            settings::set(ctx.conn(), &widgets)
        })
        .unwrap();
    let (_ui, _bound, w) = open_manage_tags(&store, &f);
    w.invoke_text_edited("parity:amb".into());
    assert_eq!(w.get_suggestions().row_count(), 1);
    w.invoke_fetch();
    let fetched: Vec<String> = w
        .get_suggestions()
        .iter()
        .map(|r| r.text.to_string())
        .collect();
    assert_eq!(grouped(&fetched).len(), 3);
    w.invoke_cancel();
}

fn relationship_editor(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
    kind: &str,
) -> hydrus_gui::TagRelationshipsWindow {
    let top = (0..ui.get_menu_titles().row_count())
        .find(|&i| ui.get_menu_titles().row_data(i).unwrap().label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let line = (0..pane.lines.row_count())
        .find(|&i| pane.lines.row_data(i).unwrap().label.starts_with(kind))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(line).unwrap(), 0.0, 0.0, 0.0);
    bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn relationship_inputs(kind: &str) {
    let f = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dirs, store) = seeded(&f);
    let _windows = headless::init();
    for query in f["queries"].as_array().unwrap() {
        apply_query(&store, query);
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let w = relationship_editor(&ui, &bound, kind);
        let mine = w
            .get_service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(mine).unwrap());
        for right in [false, true] {
            w.invoke_autocomplete_edited(right, query["text"].as_str().unwrap().into());
            let suggestions = if right {
                w.get_right_suggestions()
            } else {
                w.get_left_suggestions()
            };
            let rows: Vec<String> = suggestions.iter().map(|r| r.text.to_string()).collect();
            assert_eq!(grouped(&rows), expected_groups(query), "{kind} {query}");
            // Enter, or a double-click on the highlight, enters it
            let before = side_tags(&w, right);
            w.invoke_autocomplete_chosen(right, -1);
            // exactly what the recording had highlighted is added
            let after = side_tags(&w, right);
            assert_eq!(
                after.difference(&before).cloned().collect::<Vec<_>>(),
                [query["selected"][0].as_str().unwrap().to_owned()],
                "{kind} right={right} {query}"
            );
            // (the input is cleared by an entry: typed again for the rest)
            w.invoke_autocomplete_edited(right, query["text"].as_str().unwrap().into());
            let rows: Vec<String> = if right {
                w.get_right_suggestions()
            } else {
                w.get_left_suggestions()
            }
            .iter()
            .map(|r| r.text.to_string())
            .collect();
            if query == &f["queries"][0] {
                let at = rows
                    .iter()
                    .position(|r| r.starts_with("parity:amber old"))
                    .unwrap();
                w.invoke_context_menu(right, i32::try_from(at).unwrap(), 10.0, 10.0);
                let paths = menu_paths(panes_of!(w), |pane, line| {
                    w.invoke_tag_menu_clicked(pane, line, 100.0, 50.0, 10.0);
                });
                let recorded = recorded_menu_paths(&f);
                assert!(
                    same_paths(paths.clone(), recorded.clone()),
                    "{kind}: only native {:?}; only recorded {:?}",
                    paths
                        .iter()
                        .filter(|p| !recorded.contains(p))
                        .collect::<Vec<_>>(),
                    recorded
                        .iter()
                        .filter(|p| !paths.contains(p))
                        .collect::<Vec<_>>()
                );
                w.invoke_tag_menu_dismissed();
                for event in f["paste"].as_array().unwrap() {
                    set_skip_paste_question(&store, event["skip"].as_bool().unwrap());
                    let text = event["text"].as_str().unwrap().to_owned();
                    hydrus_gui::set_paster(move || text.clone());
                    let before = side_tags(&w, right);
                    let consumed =
                        w.invoke_autocomplete_paste(right, event["button"].as_bool().unwrap());
                    // (a question is raised before the paste is known to be consumed)
                    if !event["consumed"].is_null() && event["asked"].as_array().unwrap().is_empty()
                    {
                        assert_eq!(consumed, event["consumed"].as_bool().unwrap(), "{event}");
                    }
                    let asked = event["asked"].as_array().unwrap();
                    if asked.is_empty() {
                        assert!(w.get_question().is_empty(), "{event}");
                    } else {
                        assert_eq!(
                            recorded_lines(&w.get_question()),
                            recorded_lines(asked[0]["message"].as_str().unwrap()),
                            "{event}"
                        );
                        w.invoke_answered(event["answer"].as_bool().unwrap());
                    }
                    let after = side_tags(&w, right);
                    let added: BTreeSet<String> = after.difference(&before).cloned().collect();
                    let pasted = recorded_pasted(event);
                    // (a sibling's ideal side holds one tag, so many pasted there
                    // leave one of them; every other side keeps them all)
                    if kind == "siblings" && right {
                        assert!(added.is_subset(&pasted), "{kind}: {added:?} of {pasted:?}");
                        assert_eq!(added.len(), usize::from(!pasted.is_empty()), "{event}");
                    } else {
                        assert_eq!(added, pasted, "{kind} right={right} {event}");
                    }
                }
                w.invoke_autocomplete_edited(right, query["text"].as_str().unwrap().into());
            }
        }
        w.invoke_cancel();
    }
    // without fetch-as-you-type only the typed tag is offered until Ctrl+Space
    apply_query(&store, &f["queries"][0]);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    store
        .write(move |ctx| {
            let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                settings::get(ctx.conn())?;
            let mut options = widgets.options(&key);
            options.fetch_automatically = false;
            widgets.services.insert(key.to_hex(), options);
            settings::set(ctx.conn(), &widgets)
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let w = relationship_editor(&ui, &bound, kind);
    let mine = w
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    w.invoke_autocomplete_edited(false, "parity:amb".into());
    assert_eq!(w.get_left_suggestions().row_count(), 1);
    w.invoke_autocomplete_fetch(false);
    let fetched: Vec<String> = w
        .get_left_suggestions()
        .iter()
        .map(|r| r.text.to_string())
        .collect();
    assert_eq!(grouped(&fetched).len(), 3);
    w.invoke_cancel();
}

// leaf: siblings-autocomplete
#[test]
fn sibling_editor_inputs_follow_the_recorded_autocomplete() {
    relationship_inputs("siblings");
}

// leaf: parents-autocomplete
#[test]
fn parent_editor_inputs_follow_the_recorded_autocomplete() {
    relationship_inputs("parents");
}
