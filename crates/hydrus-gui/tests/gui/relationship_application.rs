//! Sibling/parent source precedence, replayed from
//! `oracle/record_relationship_application.py` through the real application
//! window, and checked on the display graph the whole client reads.
use crate::options_gui_support::Client;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn open(client: &Client) -> hydrus_gui::TagDisplayWindow {
    let ui = &client.ui;
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let row = pane
        .lines
        .iter()
        .position(|r| r.label.starts_with("advanced"))
        .unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(row).unwrap(), 200.0, 22.0, 0.0);
    ui.invoke_menu_line_clicked(1, 0, 0.0, 0.0, 0.0);
    client
        .bound
        .tag_display
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn queue(w: &hydrus_gui::TagDisplayWindow, parents: bool) -> Vec<String> {
    let rows = if parents {
        w.get_parent_rows()
    } else {
        w.get_sibling_rows()
    };
    rows.iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}

/// Set a queue as a user would: remove every entry, then add each in order.
fn set_queue(w: &hydrus_gui::TagDisplayWindow, parents: bool, services: &Value) {
    while !queue(w, parents).is_empty() {
        w.invoke_source_click(parents, 0);
        w.invoke_source_change(parents, 0);
    }
    for name in services.as_array().unwrap() {
        let at = w
            .get_source_services()
            .iter()
            .position(|s| s == name.as_str().unwrap())
            .unwrap();
        w.set_source_index(i32::try_from(at).unwrap());
        w.invoke_source_add(parents);
    }
}

// leaf: audit-media-application-parents
#[test]
fn source_queues_apply_services_rules_in_the_reference_order() {
    let recording = hydrus_testkit::fixture_json("relationship_application.json");
    let client = Client::basic();
    let snapshot = client.store.snapshot();
    let mine = snapshot.services.by_name("my tags").unwrap().id;
    for pair in recording["pairs"].as_array().unwrap() {
        let service = snapshot
            .services
            .by_name(pair[0].as_str().unwrap())
            .unwrap()
            .id;
        let kind = if pair[1] == "siblings" {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        tag_relations::apply(
            &client.store,
            kind,
            vec![RelationUpdate {
                service,
                left: hydrus_core::Tag::new(pair[2].as_str().unwrap()).unwrap(),
                right: hydrus_core::Tag::new(pair[3].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            }],
        )
        .unwrap();
    }
    for case in recording["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let w = open(&client);
        let at = w
            .get_services()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        if name != "default" {
            set_queue(&w, false, &case["siblings"]);
            set_queue(&w, true, &case["parents"]);
        }
        assert_eq!(
            serde_json::json!(queue(&w, false)),
            case["siblings"],
            "{name}"
        );
        assert_eq!(
            serde_json::json!(queue(&w, true)),
            case["parents"],
            "{name}"
        );
        w.invoke_apply();
        assert!(client.bound.tag_display.borrow().is_none(), "{name}");
        let graph = client.store.snapshot().display.get(mine);
        let names = |ids: &[hydrus_core::TagId]| -> Vec<String> {
            let mut tags: Vec<String> = client
                .store
                .read(|conn| hydrus_store::master::tags(conn, ids))
                .unwrap()
                .into_values()
                .map(|t| t.to_string())
                .collect();
            tags.sort();
            tags
        };
        for (tag, expected) in case["tags"].as_object().unwrap() {
            let id = client
                .store
                .read(|conn| {
                    hydrus_store::master::tag_id(conn, &hydrus_core::Tag::new(tag).unwrap())
                })
                .unwrap()
                .unwrap();
            let ideal = graph.ideal(id);
            let mut chain = graph.chain(id);
            if chain.is_empty() {
                chain.push(id);
            }
            let shown = serde_json::json!({
                "ideal": names(&[ideal])[0],
                "chain": names(&chain),
                "parents": names(graph.ancestors(ideal)),
                "children": names(graph.descendants(ideal)),
            });
            assert_eq!(&shown, expected, "{name}: {tag}");
        }
    }
}
