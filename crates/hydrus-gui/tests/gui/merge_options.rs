//! The duplicate metadata merge options editor: the duplicates page's
//! defaults edited (a tag service taken out, one added through its action
//! and the tag filter editor, a sync changed) and written, and a rule's
//! custom merge options edited in it. What the editor says and does at each
//! step is tested against the reference's in hydrus-gui-model's tests.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_gui::merge_options_window::last_opened;
use hydrus_gui::{MainWindow, MergeOptionsWindow, Pages, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use hydrus_store::duplicates::auto;
use hydrus_store::duplicates::merge::{DuplicateMergeSettings, MergeAction};
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::duplicate_filter::{my_files, store_with_pairs};

fn duplicates_page(store: &Store) {
    let (_, key) = my_files(store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..FileSearchContext::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
}

fn cells(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<Vec<String>> {
    (0..rows.row_count())
        .map(|r| {
            let cells = rows.row_data(r).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

fn choices(editor: &MergeOptionsWindow) -> Vec<String> {
    let choices = editor.get_asking_choices();
    (0..choices.row_count())
        .map(|i| choices.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn the_default_merge_options_are_edited_and_written() {
    let windows = headless::init();
    let (_dir, store) = store_with_pairs();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());

    // "this is better"'s
    ui.invoke_duplicates_action("merge options".into(), 0, false, false);
    let editor = last_opened().expect("it opens");
    assert_eq!(editor.get_window_title(), "edit duplicate merge options");
    assert_eq!(
        editor.get_note(),
        "Editing for \"this is a better duplicate\"."
    );
    assert!(editor.get_edit_action_shown());
    assert_eq!(
        cells(&editor.get_tag_rows()),
        [
            ["downloader tags", "move from worse to better", "all tags"],
            ["my tags", "move from worse to better", "all tags"],
        ]
    );
    // downloader tags taken out, asking first
    editor.invoke_row_clicked(0, 0, false, false);
    assert!(editor.get_tag_one_selected());
    editor.invoke_list_button(0, "delete".into());
    assert_eq!(editor.get_asking_message(), "Remove all selected?");
    editor.invoke_chosen(0);
    assert_eq!(cells(&editor.get_tag_rows()).len(), 1);
    // and back: the only one left is taken without asking; its action is
    // asked, then its tags in the tag filter editor
    editor.invoke_list_button(0, "add".into());
    assert_eq!(editor.get_asking_title(), "select action");
    assert_eq!(
        choices(&editor),
        [
            "copy from worse to better",
            "copy in both directions",
            "move from worse to better"
        ]
    );
    editor.invoke_chosen(0);
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("the filter editor opens");
    assert_eq!(filter.get_window_title(), "edit which tags will be merged");
    filter.invoke_typed(1, "creator:".into());
    filter.invoke_apply();
    assert_eq!(
        cells(&editor.get_tag_rows())[0],
        [
            "downloader tags",
            "copy from worse to better",
            "all tags except 'creator' tags"
        ]
    );
    // urls no longer copied
    editor.set_urls_index(0);
    editor.invoke_choice_changed();
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last - 1).unwrap(), 720, 720);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("merge_options.png"), &pixels, 720, 720).unwrap();
    editor.invoke_apply();
    assert!(last_opened().is_none());
    let written: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(written.better.urls, None);
    let downloader = written
        .better
        .tags
        .iter()
        .find(|t| t.action == MergeAction::Copy)
        .expect("downloader tags, copied");
    assert_eq!(
        downloader.filter.to_filter_string(),
        "all tags except 'creator' tags"
    );
    // (the others' are as they were)
    assert_eq!(
        written.same_quality,
        DuplicateMergeSettings::default().same_quality
    );

    // "same quality"'s: one action only, so none asked
    ui.invoke_duplicates_action("merge options".into(), 1, false, false);
    let editor = last_opened().expect("it opens");
    assert!(!editor.get_edit_action_shown());
    assert_eq!(
        (0..editor.get_urls_options().row_count())
            .map(|i| editor.get_urls_options().row_data(i).unwrap().to_string())
            .collect::<Vec<_>>(),
        ["make no change", "copy in both directions"]
    );
    // the note merge settings
    editor.invoke_note_settings();
    assert!(editor.get_note_settings_open());
    editor.set_note_extend(false);
    editor.set_note_conflict(2);
    editor.invoke_note_settings_done(true);
    editor.invoke_apply();
    let written: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    let merge = written.same_quality.note_merge.unwrap();
    assert!(!merge.extend_existing);
    assert_eq!(merge.conflict, hydrus_core::notes::NoteConflict::Append);
    assert_eq!(written.better.urls, None, "still");
}

#[test]
fn a_rules_custom_merge_options_are_edited() {
    let _windows = headless::init();
    let (_dir, store) = store_with_pairs();
    duplicates_page(&store);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(Arc::clone(&store)).unwrap());
    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
    let list = bound
        .auto_resolution
        .list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add_suggested();
    list.invoke_suggested_chosen(0);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let rule = bound
        .auto_resolution
        .rule
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    // custom options: the client's for the action, to start from
    rule.set_default_merge(false);
    rule.invoke_changed();
    rule.invoke_edit_merge();
    let editor = last_opened().expect("it opens");
    assert_eq!(
        editor.get_note(),
        "Editing for \"this is a better duplicate\"."
    );
    editor.set_archive_index(1);
    editor.invoke_choice_changed();
    editor.invoke_apply();
    rule.invoke_apply();
    list.invoke_apply();
    let written = store.read(auto::rules).unwrap();
    let custom = written[0].1.custom_merge.as_ref().expect("custom options");
    assert_eq!(
        custom.archive,
        hydrus_store::duplicates::merge::ArchiveSync::IfEither
    );
}
