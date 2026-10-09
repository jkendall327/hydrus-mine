//! Options > tag editing's three write-autocomplete decoration switches,
//! replayed from `oracle/record_autocomplete_decoration_flags.py`: each of the
//! eight combinations is set in the real Options window and read back from a
//! real Manage tags autocomplete.
use crate::options_gui_support::{Client, row, show_page};
use hydrus_core::{Sha256, Tag};
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;
use slint::{ComponentHandle as _, Model as _};

const EXPAND: &str = "Show parents expanded by default on edit/write autocomplete taglists: ";
const PARENTS: &str = "Show parent info by default on edit/write autocomplete taglists: ";
const SIBLINGS: &str = "Show sibling info by default on edit/write autocomplete taglists: ";

pub(crate) fn seed(client: &Client, recording: &serde_json::Value) {
    let service = client
        .store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .id;
    let corpus = recording["corpus"].clone();
    client
        .store
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
    for (kind, key) in [
        (RelationKind::Siblings, "siblings"),
        (RelationKind::Parents, "parents"),
    ] {
        let updates = recording[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| RelationUpdate {
                service,
                left: Tag::new(p[0].as_str().unwrap()).unwrap(),
                right: Tag::new(p[1].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            })
            .collect();
        tag_relations::apply(&client.store, kind, updates).unwrap();
    }
}

/// Rows grouped by suggestion, each suggestion's expanded parent rows sorted:
/// the reference keeps a tag's parents in a Python set, so their order is
/// unspecified (as `write_autocomplete.rs`'s replay also treats them).
fn groups(rows: Vec<String>) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for row in rows {
        match out.last_mut() {
            Some(group) if row.starts_with("    ") => group.push(row),
            _ => out.push(vec![row]),
        }
    }
    for group in &mut out {
        group[1..].sort();
    }
    out
}

// leaf: audit-options-tag-editing-tag-edit-autocomplete-show-parent-info-by-default-on-edit-write-autocomplete-taglists
// leaf: audit-options-tag-editing-tag-edit-autocomplete-show-sibling-info-by-default-on-edit-write-autocomplete-taglists
#[test]
fn each_decoration_switch_works_alone_in_the_manage_tags_autocomplete() {
    let recording = hydrus_testkit::fixture_json("autocomplete_decoration_flags.json");
    let client = Client::basic();
    seed(&client, &recording);
    let ui = &client.ui;
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let mut checked = 0;
    for case in recording["cases"].as_array().unwrap() {
        let flags = [
            (EXPAND, case["expand"].as_bool().unwrap()),
            (PARENTS, case["parents"].as_bool().unwrap()),
            (SIBLINGS, case["siblings"].as_bool().unwrap()),
        ];
        let options = client.open_options();
        show_page(&options, "tag editing");
        for (label, on) in flags {
            let (at, _) = row(&options, label);
            options.invoke_check_toggled(at, on);
        }
        options.invoke_apply();
        ui.invoke_select_all();
        ui.invoke_manage_tags_selected();
        let manage = client
            .bound
            .manage_tags
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let mine = manage
            .get_service_names()
            .iter()
            .position(|n| n.starts_with("my tags"))
            .unwrap();
        manage.invoke_service_chosen(i32::try_from(mine).unwrap());
        manage.invoke_text_edited(case["text"].as_str().unwrap().into());
        let shown = groups(
            manage
                .get_suggestions()
                .iter()
                .map(|r| r.text.to_string())
                .collect(),
        );
        let expected = groups(
            case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|term| term["rows"].as_array().unwrap().clone())
                .map(|r| r.as_str().unwrap().to_owned())
                .collect(),
        );
        assert_eq!(shown, expected, "{case}");
        manage.invoke_cancel();
        checked += 1;
    }
    assert_eq!(checked, 24);
}
