//! The options window's shortcuts page: the two switches above the set lists
//! (as the reference's `ShortcutsPanel` words them) and what each does to
//! the shortcuts once applied.

use slint::Model as _;

use hydrus_core::shortcuts::{Binding, Gesture, Settings};

use crate::options_gui_support::{Client, show_page};

/// The words of the two checkboxes, as the reference's panel has them (the
/// page's controls are made in its Slint file, so the words are read there).
fn checkbox_labels_are_the_references() {
    let page = include_str!("../../ui/options.slint");
    for label in [
        r#"text:"Treat all non-number numpad inputs as \"normal\": ""#,
        r#"text:"Replace \"left/right\"-click labels with \"primary/secondary\": ""#,
    ] {
        assert!(page.contains(label), "{label}");
    }
}

// leaf: audit-options-shortcuts-treat-all-non-number-numpad-inputs-as-normal
#[test]
fn numpad_variants_run_the_normal_keys_binding_only_while_merged() {
    checkbox_labels_are_the_references();
    // ctrl+shift+q, flipping whether the page searches as it is edited
    let binding = Binding {
        gesture: Gesture::new(0, u32::from('q'), 0, 5),
        action: 78,
        text: None,
        content: None,
    };
    let (dirs, store) = crate::options_gui_support::basic_store();
    store
        .write(move |ctx| {
            let mut settings: Settings = hydrus_store::settings::get(ctx.conn())?;
            settings
                .sets
                .get_mut("main_gui")
                .unwrap()
                .push(binding.clone());
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    let client = Client::with(dirs, store);
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let page = client.bound.current.borrow().clone();
    let numpad_bit = 1 << 3;
    // (the binding's command makes the page search as it is edited: look
    // for that after turning it off)
    let off = || {
        if page.borrow().synchronised() {
            client.ui.invoke_flip_synchronised();
        }
        assert!(!page.borrow().synchronised());
    };
    let press = |bits: i32| client.ui.invoke_shortcut_key("q".into(), bits);

    // merged by default: the numpad variant of the key counts as the normal one
    assert!(client.setting::<Settings>().merge_numpad);
    off();
    assert!(press(5 | numpad_bit));
    assert!(page.borrow().synchronised());

    let options = client.open_options();
    show_page(&options, "shortcuts");
    assert!(
        options.get_shortcuts_merge_numpad(),
        "on in the reference's defaults"
    );
    options.set_shortcuts_merge_numpad(false);
    options.invoke_shortcuts_policy(false, options.get_shortcuts_primary_labels());
    assert!(client.setting::<Settings>().merge_numpad, "waits for apply");
    options.invoke_apply();
    assert!(!client.setting::<Settings>().merge_numpad);
    // not merged: the numpad variant is its own key, with no binding
    off();
    assert!(!press(5 | numpad_bit));
    assert!(!page.borrow().synchronised());
    assert!(press(5), "the normal key still does");
    assert!(page.borrow().synchronised());
}

// leaf: audit-options-shortcuts-replace-left-right-click-labels-with-primary-secondary
#[test]
fn mouse_buttons_are_called_primary_and_secondary_if_asked() {
    checkbox_labels_are_the_references();
    let client = Client::basic();
    let set_rows = |client: &Client, options: &hydrus_gui::OptionsWindow| -> Vec<String> {
        let _ = client;
        let label = hydrus_gui_model::shortcut_sets::pretty_name("media_viewer_media_window");
        let row = options
            .get_shortcut_reserved_rows()
            .iter()
            .position(|row| row.cells.row_data(0).unwrap().as_str() == label)
            .unwrap();
        options.invoke_shortcut_set_clicked(false, i32::try_from(row).unwrap(), false, false);
        options.invoke_shortcut_set_action("edit-reserved".into());
        let set = hydrus_gui::shortcut_windows::last_set().unwrap();
        let rows = set
            .get_rows()
            .iter()
            .map(|row| row.cells.row_data(0).unwrap().to_string())
            .collect();
        set.invoke_cancel();
        rows
    };

    let options = client.open_options();
    show_page(&options, "shortcuts");
    assert!(
        !options.get_shortcuts_primary_labels(),
        "off in the reference's defaults"
    );
    assert_eq!(set_rows(&client, &options), ["left-click"]);
    options.set_shortcuts_primary_labels(true);
    options.invoke_shortcuts_policy(options.get_shortcuts_merge_numpad(), true);
    assert_eq!(
        set_rows(&client, &options),
        ["primary-click"],
        "the edited set is labelled at once"
    );
    options.invoke_apply();
    assert!(client.setting::<Settings>().primary_labels);

    // and back, from the saved value
    let options = client.open_options();
    show_page(&options, "shortcuts");
    assert!(options.get_shortcuts_primary_labels());
    assert_eq!(set_rows(&client, &options), ["primary-click"]);
    options.set_shortcuts_primary_labels(false);
    options.invoke_shortcuts_policy(options.get_shortcuts_merge_numpad(), false);
    assert_eq!(set_rows(&client, &options), ["left-click"]);
    options.invoke_apply();
    assert!(!client.setting::<Settings>().primary_labels);
}
