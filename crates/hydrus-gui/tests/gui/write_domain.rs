//! Tags > display/search's write tag service and file-domain override, replayed
//! from `oracle/record_write_domain.py`: set in the real window, read back from
//! a real Manage tags autocomplete's domain and suggestions.
use crate::options_gui_support::Client;
use hydrus_core::{Sha256, Tag};
use slint::{ComponentHandle as _, Model as _};

fn open_display(client: &Client) -> hydrus_gui::TagDisplayWindow {
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
        .position(|r| r.label.starts_with("display/search"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
    client
        .bound
        .tag_display
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn hash_id(w: &hydrus_store::content::ContentWriter<'_>, hex: &str) -> hydrus_core::HashId {
    let hash: Sha256 = hex.parse().unwrap();
    hydrus_store::master::hash_id(w.conn(), &hash)
        .unwrap()
        .unwrap()
}

// leaf: audit-media-tag-display-write-domain
#[test]
fn write_service_and_location_override_reach_the_manage_tags_autocomplete() {
    let recording = hydrus_testkit::fixture_json("write_domain.json");
    let client = Client::basic();
    let snapshot = client.store.snapshot();
    let mappings = recording["mappings"].clone();
    let trashed = recording["trashed"].clone();
    let my_files = snapshot.services.by_name("my files").unwrap().id;
    let services: Vec<_> = mappings
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            snapshot
                .services
                .by_name(m[0].as_str().unwrap())
                .unwrap()
                .id
        })
        .collect();
    client
        .store
        .write_content(move |w| {
            for (m, service) in mappings.as_array().unwrap().iter().zip(services) {
                let tag = hydrus_store::master::intern_tag(
                    w.conn(),
                    &Tag::new(m[1].as_str().unwrap()).unwrap(),
                )?;
                let files: Vec<_> = m[2]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| hash_id(w, h.as_str().unwrap()))
                    .collect();
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &files,
                )?;
            }
            let files: Vec<_> = trashed
                .as_array()
                .unwrap()
                .iter()
                .map(|h| hash_id(w, h.as_str().unwrap()))
                .collect();
            w.delete_files(my_files, &files, None)
        })
        .unwrap();
    let ui = &client.ui;
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let mut failures = Vec::new();
    for case in recording["cases"].as_array().unwrap() {
        let w = open_display(&client);
        let at = w
            .get_services()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(at).unwrap());
        let domain = w
            .get_services()
            .iter()
            .position(|s| s == case["domain"].as_str().unwrap())
            .unwrap();
        w.set_write_service(i32::try_from(domain).unwrap());
        let overridden = !case["override"].is_null();
        w.set_override_location(overridden);
        w.invoke_options_changed();
        if overridden {
            w.invoke_location();
            let child = hydrus_gui::locations_window::last_opened().unwrap();
            let wanted = case["override"].as_str().unwrap();
            // tick only the wanted domain (its first, current-files, tick),
            // re-reading the ticks a change can cascade to
            let ticks = child.get_ticks();
            let labels: Vec<String> = ticks.iter().map(|t| t.label.to_string()).collect();
            let at = labels
                .iter()
                .position(|l| l == wanted)
                .unwrap_or_else(|| panic!("{labels:?}"));
            // untick the others first: a covering domain swallows the wanted one
            for _ in 0..20 {
                let ticks = child.get_ticks();
                let next = ticks
                    .iter()
                    .enumerate()
                    .find(|(i, t)| *i != at && t.checked)
                    .map(|(i, _)| (i, false))
                    .or_else(|| (!ticks.row_data(at).unwrap().checked).then_some((at, true)));
                let Some((i, on)) = next else {
                    break;
                };
                child.invoke_toggled(i32::try_from(i).unwrap(), on);
            }
            child.invoke_apply();
        }
        w.invoke_apply();
        assert!(client.bound.tag_display.borrow().is_none(), "{case}");
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
        if manage.get_file_label() != case["location_label"].as_str().unwrap()
            || manage.get_tag_label() != case["tag_label"].as_str().unwrap()
        {
            failures.push(format!(
                "{} {}: labels {} / {}, recorded {} / {}",
                case["domain"],
                case["override"],
                manage.get_file_label(),
                manage.get_tag_label(),
                case["location_label"],
                case["tag_label"]
            ));
        }
        manage.invoke_text_edited(case["text"].as_str().unwrap().into());
        let shown: Vec<String> = manage
            .get_suggestions()
            .iter()
            .map(|r| r.text.to_string())
            .collect();
        if serde_json::json!(shown) != case["rows"] {
            failures.push(format!(
                "{} {} {}: shown {shown:?}, recorded {}",
                case["domain"], case["override"], case["text"], case["rows"]
            ));
        }
        manage.invoke_cancel();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
