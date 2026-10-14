//! "Do not permit archived files to be deleted from the trash:" set in the real
//! options window and met by the real deletion dialog, replaying
//! `delete_lock_dialog` in `oracle/fixtures/files_trash.json` (the reference's
//! `EditDeleteFilesPanel` over an inboxed or archived file, in the trash and in
//! a local domain, with the lock off and on): which deletions the dialog
//! offers, and that it offers none when nothing is left to delete.

use std::rc::Rc;

use hydrus_gui::delete_files_window;
use hydrus_gui::media_actions::{self, Deletion};
use slint::Model as _;

use super::options_system_consumers::{check, client};

// leaf: audit-options-files-and-trash-delete-lock-do-not-permit-archived-files-to-be-deleted-from-the-trash
#[test]
fn the_lock_row_decides_which_deletions_the_dialog_offers_for_archived_files_as_recorded() {
    let fixture = hydrus_testkit::fixture_json("files_trash.json");
    let client = client();
    let hash: hydrus_core::Sha256 = fixture["advanced"][0]["hash"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let file = client
        .store
        .read(move |c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let window = client.options("files and trash");
    check(&window, "Use the advanced file deletion dialog: ", true);
    window.invoke_apply();

    for lock in [false, true] {
        let window = client.options("files and trash");
        check(
            &window,
            "Do not permit archived files to be deleted from the trash: ",
            lock,
        );
        window.invoke_apply();
        for case in fixture["delete_lock_dialog"].as_array().unwrap() {
            if case["lock"] != lock {
                continue;
            }
            let in_trash = case["where"] == "trash";
            if in_trash {
                media_actions::delete(&client.store, &[file], &Deletion::ToTrash).unwrap();
            }
            if case["inbox"].as_bool().unwrap() {
                media_actions::inbox(&client.store, &[file]).unwrap();
            } else {
                media_actions::archive(&client.store, &[file]).unwrap();
            }
            let opened = delete_files_window::open(
                &delete_files_window::Slot::default(),
                &client.store,
                &[file],
                None,
                media_actions::DELETE_REASON,
                Rc::new(|| true),
                Rc::new(|| {}),
            );
            match (case["error"].as_str(), opened) {
                (Some(error), Err(message)) => {
                    assert_eq!(message, "No valid delete choices!", "{case}");
                    assert!(error.ends_with(&message), "{case}");
                }
                (None, Ok(Some(dialog))) => {
                    // (the recorded dialog was asked, not resolved: `resolved` is false
                    // and the advanced dialog was on, as here)
                    assert_eq!(case["resolved"], false, "{case}");
                    let offered: Vec<String> =
                        dialog.get_actions().iter().map(|a| a.to_string()).collect();
                    let recorded: Vec<String> = case["actions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| a.as_str().unwrap().to_owned())
                        .collect();
                    assert_eq!(offered, recorded, "{case}");
                    dialog.invoke_cancel();
                }
                (_, other) => panic!("{case}: {:?}", other.map(|d| d.is_some())),
            }
            if in_trash {
                let art = client.store.snapshot().services.by_name("art").unwrap().id;
                media_actions::undelete_to(&client.store, &[file], art).unwrap();
            }
        }
    }
}
