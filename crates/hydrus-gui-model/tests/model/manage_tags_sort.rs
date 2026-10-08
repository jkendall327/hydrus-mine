//! Real Qt default widgets and captured storage-list sort consumers.
use super::tag_dialog_preferences::fixture;
use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
use hydrus_gui_model::{
    manage_tags::ManageTags,
    manage_tags_sort::Control,
    options::{Editor, Row, Settings},
};
use hydrus_store::{
    Store,
    manage_tags_sort::{Context, Settings as Sorts, Sort},
    settings,
};
use serde_json::{Value, json};

pub(super) const LABELS: [&str; 2] = [
    "Default tag sort in search page manage tags dialogs: ",
    "Default tag sort in media viewer manage tags dialogs: ",
];
pub(super) fn sort(value: &Value) -> Sort {
    Sort {
        order: TagSort {
            sort_type: match value["sort_type"].as_u64().unwrap() {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => panic!("unknown recorded type"),
            },
            ascending: value["sort_order"] == 0,
            group_by: match value["group_by"].as_u64().unwrap() {
                0 => TagGroupBy::Nothing,
                1 => TagGroupBy::NamespaceAz,
                2 => TagGroupBy::NamespaceUser,
                _ => panic!("unknown recorded group"),
            },
        },
        use_siblings: value["use_siblings"].as_bool().unwrap(),
    }
}
fn api(value: Sort) -> Value {
    json!({"sort_type":match value.order.sort_type {TagSortType::Tag=>0,TagSortType::Subtag=>1,TagSortType::Count=>2},"sort_order":i32::from(!value.order.ascending),"group_by":match value.order.group_by {TagGroupBy::Nothing=>0,TagGroupBy::NamespaceAz=>1,TagGroupBy::NamespaceUser=>2},"use_siblings":value.use_siblings})
}
pub(super) fn context(value: &Value) -> Context {
    match value.as_u64().unwrap() {
        1 => Context::SearchPage,
        3 => Context::MediaViewer,
        _ => panic!("unknown context"),
    }
}
pub(super) fn policy(store: &Store, context: Context, value: Sort) {
    store
        .write(move |ctx| {
            let mut saved: Sorts = settings::get(ctx.conn())?;
            match context {
                Context::SearchPage => saved.search_page = value,
                Context::MediaViewer => saved.media_viewer = value,
            }
            settings::set(ctx.conn(), &saved)
        })
        .unwrap();
}
fn rows(model: &ManageTags, recorded: &Value) -> Value {
    json!(
        model
            .display_rows()
            .into_iter()
            .filter(|row| !row.parent_row
                && recorded["corpus"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["tag"] == row.tag))
            .map(|row| row.tag)
            .collect::<Vec<_>>()
    )
}
fn edit(editor: &mut Editor, index: usize, value: &Value) {
    let row = editor
        .rows()
        .iter()
        .position(|row| matches!(row,Row::Opt{option,..} if option.label==LABELS[index]))
        .unwrap();
    editor.tag_sort(row, 0, value["sort_type"].as_u64().unwrap() as usize);
    editor.tag_sort(
        row,
        1,
        if value["sort_type"] == 2 {
            usize::from(value["sort_order"] == 0)
        } else {
            usize::from(value["sort_order"] != 0)
        },
    );
    editor.tag_sort(row, 2, value["group_by"].as_u64().unwrap() as usize);
    editor.tag_sort(row, 3, usize::from(value["use_siblings"] != true));
}
fn editor(settings: Settings) -> Editor {
    let mut editor = Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|page| *page == "tag sort")
        .unwrap();
    editor.show_page(page);
    editor
}
#[test]
fn independent_defaults_stage_cancel_apply_reopen_and_merge_only_changed_context() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (directory, store, _files) = fixture::seed(&recorded);
    let before = store.read(Settings::load).unwrap();
    assert_eq!(
        json!([
            api(before.manage_tags_sort.search_page),
            api(before.manage_tags_sort.media_viewer)
        ]),
        recorded["options"]["initial"]
    );
    let mut draft = editor(before.clone());
    for index in 0..2 {
        edit(&mut draft, index, &recorded["options"]["applied"][index]);
    }
    assert_eq!(
        store.read::<Sorts>(settings::get).unwrap(),
        before.manage_tags_sort
    );
    drop(draft);
    assert_eq!(
        store.read::<Sorts>(settings::get).unwrap(),
        before.manage_tags_sort
    );
    let mut search = editor(before.clone());
    let mut viewer = editor(before.clone());
    edit(&mut search, 0, &recorded["options"]["applied"][0]);
    edit(&mut viewer, 1, &recorded["options"]["applied"][1]);
    for draft in [viewer, search] {
        let (after, original, problems) = draft.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &original))
            .unwrap();
    }
    let reopened = Store::open(directory.path()).unwrap();
    let saved = reopened.read::<Sorts>(settings::get).unwrap();
    assert_eq!(
        json!([api(saved.search_page), api(saved.media_viewer)]),
        recorded["options"]["reopened"]
    );
    assert_eq!(
        reopened.read(Settings::load).unwrap().tag_presentation,
        before.tag_presentation,
        "unrelated sidebar/viewer sorts and colours are preserved"
    );
    let mut control = Control::new(saved.search_page);
    control.choose(0, 0);
    control.choose(1, 1);
    control.choose(0, 2);
    control.choose(1, 1);
    assert_eq!(
        api(control.value),
        recorded["options"]["remembered_orders"][0]
    );
    control.choose(0, 0);
    assert_eq!(
        api(control.value),
        recorded["options"]["remembered_orders"][1]
    );
}
#[test]
fn both_contexts_replay_all_real_sort_types_orders_siblings_and_groupings() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    for case in recorded["cases"].as_array().unwrap() {
        let context = context(&case["context"]);
        policy(&store, context, sort(&case["sort"]));
        let selected = if context == Context::MediaViewer {
            files[..1].to_vec()
        } else {
            files.clone()
        };
        let model = ManageTags::new_at(store.clone(), selected, context).unwrap();
        assert_eq!(api(model.sort_control().value), case["sort"]);
        assert_eq!(rows(&model, &recorded), case["rows"], "{case}");
        assert_eq!(
            case["siblings_visible"],
            model.sort_control().value.order.sort_type != TagSortType::Count
        );
        assert_eq!(
            case["grouping_visible"],
            model.sort_control().value.order.sort_type != TagSortType::Subtag
        );
    }
}
#[test]
fn open_service_tabs_keep_local_sort_after_options_change_and_new_dialog_reads_default() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    for case in recorded["lifetime"].as_array().unwrap() {
        let context = context(&case["context"]);
        policy(&store, context, sort(&case["before"]["sort"]));
        let selected = if context == Context::MediaViewer {
            files[..1].to_vec()
        } else {
            files.clone()
        };
        let mut model = ManageTags::new_at(store.clone(), selected.clone(), context).unwrap();
        assert_eq!(rows(&model, &recorded), case["before"]["rows"]);
        model.choose_sort(0, 1);
        model.choose_sort(1, 0);
        model.choose_sort(3, 0);
        assert_eq!(api(model.sort_control().value), case["local"]["sort"]);
        let mine = model.service();
        let other = (mine + 1) % model.service_names().len();
        model.choose_service(other).unwrap();
        assert_eq!(model.sort_control().value, sort(&case["before"]["sort"]));
        model.choose_service(mine).unwrap();
        policy(&store, context, sort(&case["reopened"]["sort"]));
        model.refresh_stored();
        assert_eq!(
            api(model.sort_control().value),
            case["after_options"]["sort"]
        );
        assert_eq!(rows(&model, &recorded), case["after_options"]["rows"]);
        let next = ManageTags::new_at(store.clone(), selected, context).unwrap();
        assert_eq!(api(next.sort_control().value), case["reopened"]["sort"]);
        assert_eq!(rows(&next, &recorded), case["reopened"]["rows"]);
    }
}

#[test]
fn sibling_sort_uses_raw_tags_when_storage_sibling_information_is_disabled() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_sort.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let raw = recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["context"] == 1
                && case["sort"]["sort_type"] == 0
                && case["sort"]["sort_order"] == 0
                && case["sort"]["group_by"] == 2
                && case["sort"]["use_siblings"] == false
        })
        .unwrap();
    policy(&store, Context::SearchPage, Sort::default());
    store
        .write(|ctx| {
            let mut preferences: hydrus_store::tag_editing::TagEditingSettings =
                settings::get(ctx.conn())?;
            preferences.tag_list_show_siblings = false;
            settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    let model = ManageTags::new(store, files).unwrap();
    assert!(model.sort_control().value.use_siblings);
    assert!(!model.dialog_preferences().tag_list_show_siblings);
    assert_eq!(
        rows(&model, &recorded),
        raw["rows"],
        "Qt GetBestTag returns its logical tag when sibling decorators are disabled"
    );
}
