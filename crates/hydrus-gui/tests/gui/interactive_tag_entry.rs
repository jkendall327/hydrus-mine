//! The interactive tag shortcut (the reference's "interactive 'edit value'
//! manual set command" on a tag service) and its quick entry dialog,
//! replayed from `oracle/record_interactive_tag_entry.py`: the command made
//! in the shortcut editor and its text, then each recorded use of it from
//! the media viewer (one file) or the thumbnails (two): the dialog's title,
//! message, recent-tag buttons and starting text, a button pressed, a tag
//! typed, the default applied, cancel, blank and whitespace refused, and
//! what each does to the files' tags and the recent tags. The quick dialog
//! count set in Options reaches the dialog as
//! `oracle/record_suggestion_options_consumers.py` recorded.
use crate::options_gui_support::{basic_store, row, show_page};
use hydrus_core::shortcuts::{Binding, ContentCommand, Gesture, Settings};
use hydrus_core::{HashId, ServiceKey, Sha256, Tag};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind};
use hydrus_store::settings::TagSuggestionSettings;
use serde_json::Value;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle as _, Model as _};

const QUICK: &str = "number of recent tags to show in quick entry dialogs: ";
const RECENT: &str = "number of recent tags to show: ";

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn files_of(store: &hydrus_store::Store, f: &Value) -> Vec<HashId> {
    let hashes: Vec<Sha256> = strings(&f["files"])
        .iter()
        .map(|h| h.parse().unwrap())
        .collect();
    let ids = store
        .read(|conn| hydrus_store::master::hash_ids(conn, &hashes))
        .unwrap();
    hashes.iter().map(|h| ids[h]).collect()
}

fn my_tags(store: &hydrus_store::Store) -> (hydrus_core::ServiceId, ServiceKey) {
    let s = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    (s.id, s.key.clone())
}

/// Recent tags pushed in this order (the last newest), before anything else.
fn push_recent(store: &hydrus_store::Store, tags: Vec<String>) {
    let (service, _) = my_tags(store);
    store
        .write(move |ctx| {
            ctx.conn().execute("DELETE FROM recent_tags", [])?;
            for (i, tag) in tags.iter().enumerate() {
                let id = hydrus_store::master::intern_tag(ctx.conn(), &Tag::new(tag).unwrap())?;
                ctx.conn().execute(
                    "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                    rusqlite::params![service, id, 1_000_000 + i64::try_from(i).unwrap()],
                )?;
            }
            Ok(())
        })
        .unwrap();
}

/// "v" in the media set runs the interactive command for "my tags".
fn bind_v(store: &hydrus_store::Store) {
    let (_, key) = my_tags(store);
    store
        .write(move |ctx| {
            let mut settings: Settings = hydrus_store::settings::get(ctx.conn())?;
            settings.sets.get_mut("media").unwrap().push(Binding {
                gesture: Gesture::new(0, 118, 0, 0),
                action: 0,
                text: None,
                content: Some(ContentCommand::Interactive { service: key }),
            });
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

fn set_counts(store: &hydrus_store::Store, main: Option<usize>, quick: Option<usize>) {
    store
        .write(move |ctx| {
            let mut s: TagSuggestionSettings = hydrus_store::settings::get(ctx.conn())?;
            s.recent_limit = main;
            s.quick_recent_limit = quick;
            hydrus_store::settings::set(ctx.conn(), &s)
        })
        .unwrap();
}

fn tags_of(store: &hydrus_store::Store, file: HashId) -> Vec<String> {
    let (service, _) = my_tags(store);
    let table = hydrus_store::schema::MappingTables::new(service).current;
    let mut tags: Vec<String> = store
        .read(|conn| {
            let ids: Vec<hydrus_core::TagId> = conn
                .prepare(&format!("SELECT tag_id FROM {table} WHERE hash_id = ?1"))?
                .query_map([file], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(hydrus_store::master::tags(conn, &ids)?
                .into_values()
                .map(|t| t.as_str().to_owned())
                .collect())
        })
        .unwrap();
    tags.sort();
    tags
}

/// file > options…
fn open_options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> hydrus_gui::OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    bound.options.borrow().as_ref().unwrap().clone_strong()
}

fn press(ui: &MainWindow, keys: &[slint::SharedString]) {
    for text in keys {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    }
    for text in keys.iter().rev() {
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: text.clone() });
    }
}

struct Client {
    _dirs: Vec<tempfile::TempDir>,
    store: std::sync::Arc<hydrus_store::Store>,
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    files: Vec<HashId>,
    windows: hydrus_gui::headless::Windows,
}

impl Client {
    fn new(f: &Value) -> Self {
        let (dirs, store) = basic_store();
        let files = files_of(&store, f);
        Self::with(dirs, store, files)
    }
    fn with(
        dirs: Vec<tempfile::TempDir>,
        store: std::sync::Arc<hydrus_store::Store>,
        files: Vec<HashId>,
    ) -> Self {
        bind_v(&store);
        let windows = hydrus_gui::headless::init();
        let mut search = SearchPage::new(store.clone());
        search.choose_location(hydrus_core::search::context::LocationContext::default());
        search.enter();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(search));
        ui.show().unwrap();
        Self {
            _dirs: dirs,
            store,
            ui,
            bound,
            files,
            windows,
        }
    }
    /// Press "v" on these files: in the viewer opened on the one, or on the
    /// thumbnails (given the keyboard with ctrl+m) with them selected.
    fn press_v(&self, files: &[HashId]) -> Option<hydrus_gui::MediaViewerWindow> {
        if let [one] = files {
            let at = self
                .bound
                .current
                .borrow()
                .borrow()
                .results()
                .iter()
                .position(|f| f == one)
                .unwrap();
            self.ui
                .invoke_thumbnail_activated(i32::try_from(at).unwrap());
            let viewer = self.bound.viewer.borrow().as_ref().unwrap().clone_strong();
            assert!(
                viewer.invoke_shortcut_key("v".into(), 0),
                "the viewer takes it"
            );
            Some(viewer)
        } else {
            self.bound.current.borrow().borrow_mut().select_files(files);
            // the search box takes the keyboard by a rendered click, as a
            // user's would, then ctrl+m hands it to the thumbnails
            let _image = hydrus_gui::headless::render(&self.windows.get(0).unwrap(), 1000, 700);
            let position = slint::LogicalPosition::new(
                self.ui.get_autocomplete_anchor_x() + 24.0,
                self.ui.get_autocomplete_anchor_y() - 12.0,
            );
            self.ui
                .window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            for event in [
                WindowEvent::PointerPressed {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                },
                WindowEvent::PointerReleased {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                },
            ] {
                self.ui.window().dispatch_event(event);
            }
            assert!(self.ui.get_search_focused());
            press(&self.ui, &[Key::Control.into(), "m".into()]);
            // (a `changed` handler runs with the event loop's next turn)
            slint::platform::update_timers_and_animations();
            assert!(
                self.ui.get_grid_focused(),
                "ctrl+m gives the thumbnails the keyboard"
            );
            press(&self.ui, &["v".into()]);
            None
        }
    }
}

/// The shortcut editor makes the interactive command for a tag service, and
/// lists it in the reference's words; editing it again shows it so.
// leaf: shortcut-interactive-tag-entry
#[test]
fn the_shortcut_editor_makes_an_interactive_tag_command_as_the_reference() {
    let f = hydrus_testkit::fixture_json("interactive_tag_entry.json");
    let (_dirs, store) = basic_store();
    let options_client = crate::options_gui_support::Client::with(Vec::new(), store.clone());
    let options = options_client.open_options();
    show_page(&options, "shortcuts");
    let label = hydrus_gui_model::shortcut_sets::pretty_name("media");
    let set_row = options
        .get_shortcut_reserved_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap().as_str() == label)
        .unwrap();
    options.invoke_shortcut_set_clicked(false, i32::try_from(set_row).unwrap(), false, false);
    options.invoke_shortcut_set_action("edit-reserved".into());
    let sets = hydrus_gui::shortcut_windows::last_set().unwrap();
    sets.invoke_action("add".into());
    let editor = hydrus_gui::shortcut_windows::last_command().unwrap();
    // the reference also offers its numerical and inc/dec rating services
    // ("stars", "counter"); hydrus-rs's interactive commands take tags only
    let services: Vec<String> = editor
        .get_interactive_services()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let recorded: Vec<String> = strings(&f["interactive_services"])
        .into_iter()
        .filter(|s| s.ends_with(" tags"))
        .collect();
    assert_eq!(services, recorded);
    editor.invoke_key_capture("v".into(), 0);
    editor.set_command_type(2);
    let mine = services.iter().position(|s| s == "my tags").unwrap();
    editor.set_interactive_service(i32::try_from(mine).unwrap());
    editor.invoke_apply();
    let texts: Vec<String> = sets
        .get_rows()
        .iter()
        .map(|r| r.cells.row_data(1).unwrap_or_default().to_string())
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t == f["command_text"].as_str().unwrap()),
        "{texts:?}"
    );
    // edited again, it is the interactive command it was
    let at = texts
        .iter()
        .position(|t| t == f["command_text"].as_str().unwrap())
        .unwrap();
    sets.set_selected(i32::try_from(at).unwrap());
    sets.invoke_selected_row(i32::try_from(at).unwrap());
    sets.invoke_action("edit".into());
    let again = hydrus_gui::shortcut_windows::last_command().unwrap();
    assert_eq!(again.get_command_type(), 2, "{}", f["command_round_trip"]);
    assert_eq!(
        again.get_interactive_service(),
        i32::try_from(mine).unwrap()
    );
    again.invoke_cancel();
    sets.invoke_apply();
    options.invoke_apply();
    let saved: Settings = store.read(hydrus_store::settings::get).unwrap();
    let (_, key) = my_tags(&store);
    assert!(
        saved.sets["media"]
            .iter()
            .any(|b| b.gesture == Gesture::new(0, 118, 0, 0)
                && b.content
                    == Some(ContentCommand::Interactive {
                        service: key.clone()
                    }))
    );
}

/// Each recorded use of the command, in order (the recent tags and the
/// files' tags carry from one to the next).
// leaf: shortcut-interactive-tag-entry
#[test]
fn the_quick_dialog_offers_the_recent_tags_and_sets_the_tag_as_the_recording() {
    let f = hydrus_testkit::fixture_json("interactive_tag_entry.json");
    let client = Client::new(&f);
    push_recent(&client.store, strings(&f["pushed"]));
    for step in f["steps"].as_array().unwrap() {
        let name = step["name"].as_str().unwrap();
        set_counts(
            &client.store,
            step["main"].as_u64().map(|n| usize::try_from(n).unwrap()),
            step["quick"].as_u64().map(|n| usize::try_from(n).unwrap()),
        );
        let files: Vec<HashId> = step["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| client.files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        let viewer = client.press_v(&files);
        let dialog = hydrus_gui::quick_tag_entry::last_window().expect("the quick dialog");
        let seen = &step["dialog"];
        assert_eq!(
            dialog.get_composed_title(),
            format!(
                "{}{}",
                seen["title"].as_str().unwrap(),
                dialog.get_title_suffix()
            ),
            "{name}"
        );
        assert_eq!(
            dialog.get_message(),
            seen["message"][0].as_str().unwrap(),
            "{name}"
        );
        let suggestions: Vec<String> = dialog
            .get_suggestions()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(suggestions, strings(&seen["suggestions"]), "{name}");
        assert_eq!(dialog.get_text(), seen["text"].as_str().unwrap(), "{name}");
        assert_eq!(
            dialog.get_selected_all(),
            !seen["selected"].as_str().unwrap().is_empty(),
            "{name}: the starting tag is selected"
        );
        let act = &step["act"];
        match act[0].as_str().unwrap() {
            "button" => dialog.invoke_chosen(i32::try_from(act[1].as_i64().unwrap()).unwrap()),
            "type" => {
                dialog.set_text(act[1].as_str().unwrap().into());
                dialog.invoke_applied(dialog.get_text());
                if let Some(refused) = seen["refused"].as_array() {
                    assert_eq!(
                        dialog.get_refusal(),
                        refused[0]["message"].as_str().unwrap(),
                        "{name}"
                    );
                    assert!(dialog.window().is_visible(), "{name}: still asking");
                    dialog.invoke_cancelled();
                }
            }
            "apply" => dialog.invoke_applied(dialog.get_text()),
            _ => dialog.invoke_cancelled(),
        }
        assert!(!dialog.window().is_visible(), "{name}: the dialog closed");
        let tags: Vec<Vec<String>> = client
            .files
            .iter()
            .map(|f| tags_of(&client.store, *f))
            .collect();
        let expected: Vec<Vec<String>> = step["tags_after"]
            .as_array()
            .unwrap()
            .iter()
            .map(strings)
            .collect();
        assert_eq!(tags, expected, "{name}: the files' tags");
        set_counts(&client.store, Some(20), Some(10));
        let (service, _) = my_tags(&client.store);
        assert_eq!(
            hydrus_gui_model::tag_suggestions::recent(&client.store, service, 20).unwrap(),
            strings(&step["recent_after"]),
            "{name}: the recent tags"
        );
        if let Some(viewer) = viewer {
            viewer.invoke_close_requested();
        }
    }
}

/// The quick dialog count set through Options (with the recent count it is
/// read under) reaches the dialog, as recorded with the reference's Options
/// page and quick dialog.
// leaf: shortcut-interactive-tag-entry
#[test]
fn the_options_quick_dialog_count_reaches_the_dialog_as_the_recording() {
    let f = hydrus_testkit::fixture_json("suggestion_options_consumers.json");
    let (dirs, store, files) = crate::suggestion_options_consumers::seeded(&f);
    let client = Client::with(dirs, store.clone(), files);
    for scenario in f["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let options = open_options(&client.ui, &client.bound);
        show_page(&options, "tag suggestions");
        for (label, value) in [
            (RECENT, &scenario["set"]["recent"]),
            (QUICK, &scenario["set"]["quick"]),
        ] {
            if let Some(count) = value.as_i64() {
                options.invoke_none_toggled(row(&options, label).0, false);
                options.invoke_number_edited(row(&options, label).0, i32::try_from(count).unwrap());
            } else {
                options.invoke_none_toggled(row(&options, label).0, true);
            }
        }
        options.invoke_apply();
        let viewer = client.press_v(&client.files[..1]).unwrap();
        let dialog = hydrus_gui::quick_tag_entry::last_window().expect("the quick dialog");
        let seen = &scenario["quick_dialog"];
        let suggestions: Vec<String> = dialog
            .get_suggestions()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(suggestions, strings(&seen["suggestions"]), "{name}");
        assert_eq!(dialog.get_text(), seen["text"].as_str().unwrap(), "{name}");
        dialog.invoke_cancelled();
        viewer.invoke_close_requested();
    }
}
