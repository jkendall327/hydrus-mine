//! "Remember the last action" (files and trash options), set in the real
//! options window and met by the real advanced deletion dialog, replaying
//! `remember_action` in `oracle/fixtures/files_trash.json`: with the option on,
//! the action accepted is saved and is the one chosen when the dialog is next
//! opened; with it off, nothing is saved and the dialog opens on its first
//! action.

use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui::delete_files_window;
use hydrus_gui::media_actions;
use hydrus_store::Store;
use hydrus_store::settings::{DeletionAction, DeletionPreferences};
use slint::Model as _;

use super::options_system_consumers::{client, row, store};

fn decode(value: &serde_json::Value) -> Option<DeletionAction> {
    value.as_str().map(|value| match value {
        "physical_delete" => DeletionAction::Physical,
        "clear_delete" => DeletionAction::ClearRecord,
        key => DeletionAction::Domain(hydrus_core::ServiceKey::new(hex::decode(key).unwrap())),
    })
}

/// The recording's pair: a file in one local domain, and one in two.
fn pair(store: &Arc<Store>, single: &str) -> [HashId; 2] {
    let snapshot = store.snapshot();
    let roles = hydrus_store::content::DomainRoles::new(&snapshot.services).unwrap();
    let mut page = super::common::all_local_page(store.clone());
    page.enter();
    let files = page.results().to_vec();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &files))
        .unwrap();
    let hash: hydrus_core::Sha256 = single.parse().unwrap();
    let single = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let double = batch
        .results
        .iter()
        .find(|m| {
            m.current
                .iter()
                .filter(|c| roles.local.contains(&c.service))
                .count()
                == 2
        })
        .unwrap()
        .hash_id;
    [single, double]
}

fn open(store: &Arc<Store>, files: &[HashId]) -> hydrus_gui::DeleteFilesWindow {
    delete_files_window::open(
        &delete_files_window::Slot::default(),
        store,
        files,
        None,
        media_actions::DELETE_REASON,
        Rc::new(|| true),
        Rc::new(|| {}),
    )
    .unwrap()
    .expect("the dialog is asked, not resolved")
}

// leaf: audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-remember-the-last-action
#[test]
fn the_remember_row_decides_whether_the_accepted_action_is_saved_and_recalled_as_recorded() {
    let fixture = hydrus_testkit::fixture_json("files_trash.json");
    let labels: Vec<String> = fixture["action_cases"][0]["state"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let single = fixture["advanced"][0]["hash"].as_str().unwrap().to_owned();
    // (one window for the options; each dialog on a store of its own, as the
    // files it deletes are gone afterwards)
    let client = client();
    let first_look = client.options("files and trash");
    assert!(
        !row(&first_look, "Remember the last action: ").1.checked,
        "off unless asked for"
    );
    first_look.invoke_cancel();
    for case in fixture["remember_action"].as_array().unwrap() {
        let remember = case["remember"].as_bool().unwrap();
        let selected = i32::try_from(case["selected"].as_u64().unwrap()).unwrap();
        let window = client.options("files and trash");
        let (remember_row, _) = row(&window, "Remember the last action: ");
        let (advanced, _) = row(&window, "Use the advanced file deletion dialog: ");
        window.invoke_check_toggled(advanced, true);
        window.invoke_check_toggled(remember_row, remember);
        window.invoke_apply();
        let prefs: DeletionPreferences = client.get();
        assert_eq!((prefs.advanced, prefs.remember_action), (true, remember));

        let (_dirs, first) = store();
        first
            .write({
                let prefs = prefs.clone();
                move |ctx| hydrus_store::settings::set(ctx.conn(), &prefs)
            })
            .unwrap();
        let files = pair(&first, &single);
        let dialog = open(&first, &files);
        let shown: Vec<String> = dialog
            .get_actions()
            .iter()
            .map(|label| label.to_string())
            .collect();
        assert_eq!(shown, labels, "{case}");
        assert_eq!(dialog.get_selected_action(), 0, "nothing remembered yet");
        assert_eq!(shown[selected as usize], case["label"].as_str().unwrap());
        dialog.invoke_action_selected(selected);
        dialog.invoke_accept_deletion();
        let saved: DeletionPreferences = first.read(hydrus_store::settings::get).unwrap();
        assert_eq!(saved.last_action, decode(&case["saved_action"]), "{case}");

        // the options as they were left, over files still there to be asked about
        let (_dirs_again, again) = store();
        again
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &saved))
            .unwrap();
        let files = pair(&again, &single);
        let dialog = open(&again, &files);
        assert_eq!(
            i64::from(dialog.get_selected_action()),
            case["reopened"].as_i64().unwrap(),
            "{case}"
        );
        assert_eq!(
            dialog
                .get_actions()
                .row_data(usize::try_from(dialog.get_selected_action()).unwrap())
                .unwrap(),
            case["reopened_label"].as_str().unwrap()
        );
    }
}
