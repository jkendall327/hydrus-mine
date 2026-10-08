//! The Manage Tags window's remove button, tag selection, copy button and
//! cog menu, driven as a user would; what the reference answers is replayed
//! from `oracle/fixtures/manage_tags_cog.json` (`record_manage_tags_cog.py`).

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, ManageTagsWindow, Pages, bind, headless};
use hydrus_store::content::MappingAction;

struct Opened {
    _dirs: [tempfile::TempDir; 2],
    store: std::sync::Arc<hydrus_store::Store>,
    files: Vec<hydrus_core::HashId>,
    _ui: MainWindow,
    _bound: hydrus_gui::Bound,
    manage: ManageTagsWindow,
}

/// Every file of the fixture has `cog:all`; the first alone has `cog:some`.
fn open() -> Opened {
    open_with(|_| {})
}

fn open_with(before: impl FnOnce(&hydrus_store::Store)) -> Opened {
    let (dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let mut page = hydrus_gui::SearchPage::new(store.clone());
    page.enter();
    let files = page.results().to_vec();
    assert!(files.len() > 1);
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let (all, first) = (files.clone(), files[0]);
    store
        .write_content(move |writer| {
            for (tag, on) in [("cog:all", all), ("cog:some", vec![first])] {
                let id = hydrus_store::master::intern_tag(writer.conn(), &Tag::new(tag).unwrap())?;
                writer.update_mappings(service, &MappingAction::Add, id, &on)?;
            }
            Ok(())
        })
        .unwrap();
    before(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    Opened {
        _dirs: dirs,
        store,
        files,
        _ui: ui,
        _bound: bound,
        manage,
    }
}

fn row_of(manage: &ManageTagsWindow, tag: &str) -> i32 {
    let at = manage
        .get_tags()
        .iter()
        .position(|row| row.text.starts_with(tag))
        .unwrap_or_else(|| panic!("{tag} is listed"));
    i32::try_from(at).unwrap()
}

fn listed(manage: &ManageTagsWindow, tag: &str) -> bool {
    manage
        .get_tags()
        .iter()
        .any(|row| row.text.starts_with(tag))
}

fn recorded_case(recorded: &serde_json::Value, name: &str, confirm: bool) -> serde_json::Value {
    recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name && c["confirm"] == confirm && c["allow_remove"] == false)
        .unwrap()
        .clone()
}

fn set_cog(
    store: &hydrus_store::Store,
    f: impl FnOnce(&mut hydrus_store::tag_editing::TagEditingSettings) + Send + 'static,
) {
    store
        .write(move |ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            f(&mut o);
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
}

// leaf: audit-media-tags-missing-cog
#[test]
fn remove_button_confirms_as_the_reference_does_and_stages_the_removal() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_cog.json");
    let o = open();
    let m = &o.manage;
    assert_eq!(
        m.get_remove_button_text().as_str(),
        recorded["remove_button_text"].as_str().unwrap()
    );
    // nothing selected: every tag is removable, and the question counts them
    m.invoke_remove_pressed();
    assert!(
        m.get_tag_menu_question()
            .starts_with("Are you sure you want to remove these ")
    );
    assert_eq!(m.get_tag_menu_yes_label().as_str(), "yes");
    m.invoke_tag_menu_answered(false);
    assert!(m.get_tag_menu_question().is_empty());
    assert!(listed(m, "cog:all"), "'no' removes nothing");
    // one tag selected: the question is the reference's, with the tag
    m.invoke_tag_clicked(row_of(m, "cog:some"), false, false);
    assert!(
        m.get_tag_selected()
            .row_data(usize::try_from(row_of(m, "cog:some")).unwrap())
            .unwrap()
    );
    m.invoke_remove_pressed();
    let asked = recorded_case(&recorded, "remove_button_selected_declined", true)["asked"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(m.get_tag_menu_question().as_str(), asked);
    m.invoke_tag_menu_answered(true);
    assert!(!listed(m, "cog:some"));
    assert!(listed(m, "cog:all"));
    // staged only: the store still has it until apply
    m.invoke_apply();
    let service = o.store.snapshot().services.by_name("my tags").unwrap().id;
    let snapshot = o.store.snapshot();
    let batch = o
        .store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &o.files[..1]))
        .unwrap();
    let current: Vec<String> = batch.results[0].tags[&service]
        .by_status
        .get(&hydrus_core::ContentStatus::Current)
        .into_iter()
        .flatten()
        .map(|id| batch.tags[id].to_string())
        .collect();
    assert!(current.contains(&"cog:all".to_owned()));
    assert!(!current.contains(&"cog:some".to_owned()));
}

// leaf: audit-media-tags-missing-cog
#[test]
fn cog_menu_toggles_are_written_and_the_confirmation_obeys_them() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_cog.json");
    let o = open();
    let m = &o.manage;
    m.invoke_cog_pressed(0.0, 0.0);
    let lines = m.get_tag_menu_panes().row_data(0).unwrap().lines;
    let labels: Vec<String> = lines.iter().map(|l| l.label.to_string()).collect();
    let titles: Vec<String> = recorded["cog_menu"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] != "MenuTemplateItemSeparator")
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        labels
            .iter()
            .filter(|l| !l.is_empty())
            .cloned()
            .collect::<Vec<_>>(),
        titles
    );
    // "select the first tag result with actual count" is the options' own setting
    let at = labels
        .iter()
        .position(|l| l.starts_with("select the first"))
        .unwrap();
    m.invoke_tag_menu_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let saved: hydrus_store::tag_editing::TagEditingSettings =
        o.store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(
        saved.select_first_with_count,
        !recorded["defaults"]["ac_select_first_with_count"]
            .as_bool()
            .unwrap()
    );
    m.invoke_cog_pressed(0.0, 0.0);
    // "confirm remove/petition tags…" is on by default: turn it off
    let at = labels
        .iter()
        .position(|l| l.starts_with("confirm remove"))
        .unwrap();
    m.invoke_tag_menu_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let saved: hydrus_store::tag_editing::TagEditingSettings =
        o.store.read(hydrus_store::settings::get).unwrap();
    assert!(!saved.confirm_remove);
    // now the remove button does not ask
    m.invoke_tag_clicked(row_of(m, "cog:some"), false, false);
    m.invoke_remove_pressed();
    assert!(m.get_tag_menu_question().is_empty());
    assert!(!listed(m, "cog:some"));
    // the cog shows its state
    m.invoke_cog_pressed(0.0, 0.0);
    let lines = m.get_tag_menu_panes().row_data(0).unwrap().lines;
    assert!(
        lines
            .iter()
            .any(|l| l.label.starts_with("confirm remove") && !l.checked)
    );
    m.invoke_tag_menu_dismissed();
    // typing a tag all the files have removes it only with the first item on
    set_cog(&o.store, |o| o.allow_remove_on_input = true);
    m.set_text("cog:all".into());
    m.invoke_text_edited("cog:all".into());
    m.invoke_entered();
    assert!(!listed(m, "cog:all"));
}

#[test]
fn copy_button_copies_selected_or_all_tags_with_the_reference_notice() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_cog.json");
    let o = open();
    let m = &o.manage;
    m.invoke_tag_clicked(row_of(m, "cog:all"), false, false);
    m.invoke_tag_clicked(row_of(m, "cog:some"), true, false);
    m.invoke_copy_pressed();
    let notice = recorded_case(&recorded, "copy_selected", true)["notices"][0]
        .as_str()
        .unwrap()
        .replace('3', "2");
    assert_eq!(
        m.get_notice().as_str(),
        notice,
        "(the recorded wording, two tags)"
    );
    m.invoke_tag_clicked(row_of(m, "cog:all"), false, false);
    m.invoke_tag_clicked(row_of(m, "cog:all"), true, false);
    m.invoke_copy_pressed();
    assert!(
        m.get_notice().starts_with("Copied ") && m.get_notice().ends_with(" tags!"),
        "nothing selected copies all of them"
    );
}

#[test]
fn recent_panel_clear_button_asks_then_forgets_the_services_recent_tags() {
    let o = open_with(|store| {
        let service = store.snapshot().services.by_name("my tags").unwrap().id;
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::TagSuggestionSettings::default(),
                )?;
                let tag =
                    hydrus_store::master::intern_tag(ctx.conn(), &Tag::new("recent:one").unwrap())?;
                ctx.conn().execute(
                    "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                    rusqlite::params![service, tag, hydrus_core::time::TimestampMs::now().0],
                )?;
                Ok(())
            })
            .unwrap();
    });
    let service = o.store.snapshot().services.by_name("my tags").unwrap().id;
    let m = &o.manage;
    assert!(m.get_recent_tags_enabled());
    let recent = || {
        m.get_recent_tag_rows()
            .iter()
            .map(|r| r.cells.row_data(0).unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(recent(), ["recent:one"]);
    // (the reference: GetYesNo( 'Clear recent tags?' ))
    m.invoke_clear_recent();
    assert_eq!(m.get_tag_menu_question().as_str(), "Clear recent tags?");
    m.invoke_tag_menu_answered(false);
    assert_eq!(recent(), ["recent:one"]);
    m.invoke_clear_recent();
    m.invoke_tag_menu_answered(true);
    assert!(recent().is_empty());
    let left: i64 = o
        .store
        .read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM recent_tags WHERE service_id = ?",
                [service],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(left, 0);
}

fn press(window: &ManageTagsWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

#[test]
fn empty_input_keys_move_the_autocomplete_and_service_tabs_as_the_reference_does() {
    use slint::platform::Key;
    let recorded = hydrus_testkit::fixture_json("manage_tags_keys.json");
    let results = recorded["results"].as_array().unwrap();
    let recorded_move = |input: &str, filled: bool, command: &str| -> (i64, i64) {
        let r = results
            .iter()
            .find(|r| r["input"] == input && r["list_filled"] == filled && r["command"] == command)
            .unwrap();
        (
            r["tab"][1].as_i64().unwrap() - r["tab"][0].as_i64().unwrap(),
            r["service_page"][1].as_i64().unwrap() - r["service_page"][0].as_i64().unwrap(),
        )
    };
    let o = open();
    let m = &o.manage;
    assert_eq!(
        usize::try_from(recorded["info"]["num_tabs"].as_i64().unwrap()).unwrap(),
        3
    );
    m.invoke_focus_input();
    assert!(m.get_input_focused(), "the input has the keyboard");
    let services = i32::try_from(m.get_service_names().row_count()).unwrap();
    assert_eq!(
        services,
        recorded["info"]["services"].as_array().unwrap().len() as i32
    );
    // an empty input: Left / Right wrap through the three tabs
    assert_eq!(m.get_autocomplete_tab(), 0);
    press(m, Key::LeftArrow);
    assert_eq!(
        i64::from(m.get_autocomplete_tab()),
        recorded_move("", false, "tab_left").0.rem_euclid(3)
    );
    press(m, Key::RightArrow);
    assert_eq!(m.get_autocomplete_tab(), 0);
    press(m, Key::RightArrow);
    assert_eq!(
        i64::from(m.get_autocomplete_tab()),
        recorded_move("", false, "tab_right").0.rem_euclid(3)
    );
    press(m, Key::LeftArrow);
    assert_eq!(m.get_autocomplete_tab(), 0);
    // Up / Down wrap through the service tabs while the list is empty
    let start = m.get_service_index();
    press(m, Key::DownArrow);
    assert_eq!(
        i64::from((m.get_service_index() - start).rem_euclid(services)),
        recorded_move("", false, "page_right")
            .1
            .rem_euclid(i64::from(services))
    );
    press(m, Key::UpArrow);
    assert_eq!(m.get_service_index(), start);
    press(m, Key::UpArrow);
    assert_eq!(
        i64::from((m.get_service_index() - start).rem_euclid(services)),
        recorded_move("", false, "page_left")
            .1
            .rem_euclid(i64::from(services))
    );
    // typed text: the keys belong to the text
    m.invoke_service_chosen(start);
    m.set_text("blu".into());
    m.invoke_text_edited("blu".into());
    let index = m.get_service_index();
    press(m, Key::RightArrow);
    press(m, Key::DownArrow);
    assert_eq!(m.get_autocomplete_tab(), 0);
    assert_eq!(m.get_service_index(), index);
    assert!(
        results
            .iter()
            .filter(|r| r["input"] == "blu")
            .all(|r| r["matched"] == false),
        "the reference ignores them there too"
    );
}
