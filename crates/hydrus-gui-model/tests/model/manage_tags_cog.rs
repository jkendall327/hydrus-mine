//! The Manage Tags cog, typed and list entry, the remove button and the copy
//! button, replayed from `manage_tags_cog.json` (recorded from the reference's
//! real `ManageTagsPanel`: `oracle/record_manage_tags_cog.py`).
use hydrus_core::{HashId, Sha256, Tag};
use hydrus_gui_model::manage_tags::{Entered, ManageTags, Removal};
use hydrus_store::{Store, content::MappingAction};
use serde_json::Value;
use std::sync::Arc;

pub(super) fn seed(recorded: &Value) -> (tempfile::TempDir, Arc<Store>, Vec<HashId>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    let hashes: Vec<Sha256> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse().unwrap())
        .collect();
    let files = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            Ok(hashes.iter().map(|h| ids[h]).collect::<Vec<_>>())
        })
        .unwrap();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = recorded["corpus"].clone();
    let selected = files.clone();
    store
        .write_content(move |writer| {
            for row in corpus.as_array().unwrap() {
                let [tag, indexes] = row.as_array().unwrap().as_slice() else {
                    panic!("corpus row");
                };
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new(tag.as_str().unwrap()).unwrap(),
                )?;
                let files: Vec<_> = indexes
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| selected[usize::try_from(i.as_u64().unwrap()).unwrap()])
                    .collect();
                writer.update_mappings(service, &MappingAction::Add, tag, &files)?;
            }
            Ok(())
        })
        .unwrap();
    (directory, store, files)
}

fn set_cog(store: &Store, allow: bool, confirm: bool) {
    store
        .write(move |ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            o.allow_remove_on_input = allow;
            o.confirm_remove = confirm;
            o.default_service = hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::MY_TAGS.to_vec(),
            );
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn staged(model: &ManageTags, files: &[HashId]) -> Vec<(String, String, Vec<usize>)> {
    model
        .staged_changes()
        .into_iter()
        .map(|(add, tag, ids)| {
            (
                if add { "add" } else { "delete" }.to_owned(),
                tag,
                ids.iter()
                    .map(|id| files.iter().position(|f| f == id).unwrap())
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Run one recorded case and compare everything the reference did.
fn replay(store: &Arc<Store>, files: &[HashId], case: &Value) {
    let name = case["name"].as_str().unwrap();
    set_cog(
        store,
        case["allow_remove"].as_bool().unwrap(),
        case["confirm"].as_bool().unwrap(),
    );
    let mut model = ManageTags::new(store.clone(), files.to_vec()).unwrap();
    for (n, tag) in strings(&case["selected"]).iter().enumerate() {
        let row = model.rows().iter().position(|(t, _)| t == tag).unwrap();
        model.click_tag(row, n > 0, false);
    }
    let asked = strings(&case["asked"]);
    let prompts = case["choices_asked"].as_array().unwrap();
    let answer_yes = case["answer_yes"].as_bool().unwrap();
    let choice = case["choice_index"].as_u64().map(|i| i as usize);
    let mut seen_prompt = 0;
    let mut seen_question = Vec::new();
    let mut copied = None;
    let call = &case["calls"][0];
    let tags = call["args"]
        .get(0)
        .filter(|a| a.is_array())
        .map(strings)
        .unwrap_or_default();
    let only_add = call["kwargs"]["only_add"].as_bool().unwrap_or(false);
    let outcome = match call["method"].as_str().unwrap() {
        "AddTags" => model.add_tags(&tags, only_add).map(Some),
        "EnterTags" => model.enter_tags(&tags, only_add, false).map(Some),
        _ => Ok(None),
    };
    let removal = match call["method"].as_str().unwrap() {
        "RemoveTags" => Some(model.remove_tags(&tags).unwrap()),
        "_RemoveTagsButton" => Some(model.remove_button().unwrap()),
        "_Copy" => {
            copied = model.copy_button();
            None
        }
        _ => None,
    };
    if let Ok(Some(Entered::Ask(prompt))) = outcome {
        seen_prompt += 1;
        assert_eq!(prompt.message, prompts[0]["message"], "{name}: message");
        let texts: Vec<_> = prompts[0]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["text"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(prompt.choices, texts, "{name}: buttons");
        // (the tag lines of a tooltip follow Python set order, which is unspecified)
        let unordered = |tips: Vec<String>| -> Vec<(String, Vec<String>)> {
            tips.into_iter()
                .map(|t| {
                    let mut lines = t.lines().map(str::to_owned);
                    let head = lines.next().unwrap();
                    let mut rest: Vec<_> = lines.collect();
                    rest.sort();
                    (head, rest)
                })
                .collect()
        };
        let tooltips: Vec<_> = prompts[0]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["tooltip"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            unordered(prompt.tooltips.clone()),
            unordered(tooltips),
            "{name}: tooltips"
        );
        model.answer_prompt(&prompt, choice.filter(|i| *i < prompt.choices.len()));
    } else {
        outcome.unwrap();
    }
    if let Some(Removal::Confirm { message, tags }) = removal {
        seen_question.push(message);
        if answer_yes {
            model.confirm_removal(&tags).unwrap();
        }
    }
    assert_eq!(seen_prompt, prompts.len(), "{name}: prompts asked");
    assert_eq!(seen_question, asked, "{name}: yes/no questions");
    let recorded: Vec<(String, String, Vec<usize>)> = case["staged"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["action"].as_str().unwrap().to_owned(),
                s["tag"].as_str().unwrap().to_owned(),
                s["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i.as_u64().unwrap() as usize)
                    .collect(),
            )
        })
        .collect();
    assert_eq!(staged(&model, files), recorded, "{name}: staged changes");
    match (case["clipboard"].as_array().unwrap().first(), copied) {
        (None, None) => {}
        (Some(clip), Some((text, notice))) => {
            assert_eq!(text, clip[1].as_str().unwrap(), "{name}: clipboard");
            assert_eq!(notice, case["notices"][0].as_str().unwrap(), "{name}: notice");
        }
        (recorded, got) => panic!("{name}: clipboard {recorded:?} vs {got:?}"),
    }
}

fn cases(selector: impl Fn(&str) -> bool) {
    let recorded = hydrus_testkit::fixture_json("manage_tags_cog.json");
    let (_directory, store, files) = seed(&recorded);
    let mut ran = 0;
    for case in recorded["cases"].as_array().unwrap() {
        if selector(case["name"].as_str().unwrap()) {
            replay(&store, &files, case);
            ran += 1;
        }
    }
    assert!(ran > 0);
}

#[test]
fn cog_defaults_and_menu_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_cog.json");
    let defaults = hydrus_store::tag_editing::TagEditingSettings::default();
    assert_eq!(
        recorded["defaults"]["allow_remove_on_manage_tags_input"],
        defaults.allow_remove_on_input
    );
    assert_eq!(
        recorded["defaults"]["yes_no_on_remove_on_manage_tags"],
        defaults.confirm_remove
    );
    assert_eq!(
        recorded["defaults"]["ac_select_first_with_count"],
        defaults.select_first_with_count
    );
    let (_directory, store, files) = seed(&recorded);
    let model = ManageTags::new(store, files).unwrap();
    let menu: Vec<String> = model
        .cog_entries()
        .into_iter()
        .map(|entry| match entry {
            hydrus_gui_model::write_tag_menu::Entry::Check(label, ..)
            | hydrus_gui_model::write_tag_menu::Entry::Item(label, _) => label,
            _ => "separator".to_owned(),
        })
        .collect();
    let recorded_menu: Vec<String> = recorded["cog_menu"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(menu, recorded_menu);
}

// leaf: audit-media-tags-missing-cog
#[test]
fn typed_and_list_entry_follow_the_cog_and_ask_when_only_some_have_the_tag() {
    cases(|n| n.starts_with("add_") || n.starts_with("list_activate"));
}

// leaf: audit-media-tags-missing-cog
#[test]
fn remove_confirmation_remove_button_and_copy_replay_the_reference() {
    cases(|n| n.starts_with("remove_") || n.starts_with("copy_"));
}
