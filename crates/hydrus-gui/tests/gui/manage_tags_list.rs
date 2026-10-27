//! The manage tags dialog's tag list (the reference's
//! `ListBoxTagsMediaTagsDialog`: stored tags, with counts and sibling and
//! parent decorators), replayed step by step from
//! `oracle/record_manage_tags_list.py` through the real dialog opened from
//! the real main window: real pointer presses at the rows where the dialog
//! lays them out, and real key presses on the list.
//!
//! The reference's recorder replaced the list's enter and delete callables
//! with recorders of the tags they were given; here the same tags are given
//! to a fresh dialog model of the same files, and what it asks or stages is
//! compared with what the window asks or shows.
use serde_json::{Value as Json, json};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle as _, Model as _};

use super::selection_tags_box::{Harness, MenuDriver, replay_shared_entries, strings};
use crate::common::input::{self, CTRL, CTRL_SHIFT, Mods, NONE, SHIFT};
use crate::common::widgets;
use hydrus_core::HashId;
use hydrus_gui::ManageTagsWindow;

const DOWN: &str = "\u{F701}";
const UP: &str = "\u{F700}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";
const PAGE_UP: &str = "\u{F72C}";
const PAGE_DOWN: &str = "\u{F72D}";
const ESCAPE: &str = "\u{1b}";
const RETURN: &str = "\n";
const DELETE: &str = "\u{7f}";

fn combined_local_file_domains() -> hydrus_search::LocationContext {
    hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
    ))
}

const LEFT: PointerEventButton = PointerEventButton::Left;
const RIGHT: PointerEventButton = PointerEventButton::Right;
const MIDDLE: PointerEventButton = PointerEventButton::Middle;

/// The box's page, its three files selected, with the dialog open on them.
struct Dialog {
    h: Harness,
    fixture: Json,
    window: ManageTagsWindow,
    files: Vec<HashId>,
}

/// The dialog opened on the page's selected files, laid out.
fn open(h: &Harness) -> ManageTagsWindow {
    h.ui.invoke_manage_tags_selected();
    let window = h
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    widgets::lay_out(window.window(), 900.0, 1100.0);
    widgets::settle();
    window
}

impl Dialog {
    /// Cancel the dialog (it stages nothing until applied) and open it
    /// again.
    fn reopen(&mut self) {
        self.window.invoke_cancel();
        widgets::settle();
        assert!(self.h.bound.manage_tags.borrow().is_none(), "closed");
        self.window = open(&self.h);
        assert_eq!(
            self.rows(),
            strings(&self.fixture["drawn"]),
            "the rows again"
        );
    }

    fn new() -> Self {
        let h = Harness::new(1600.0);
        let fixture = hydrus_testkit::fixture_json("manage_tags_list.json");
        h.page().borrow_mut().select_all();
        let files = h.page().borrow().selected_files();
        let window = open(&h);
        let dialog = Self {
            h,
            fixture,
            window,
            files,
        };
        assert_eq!(dialog.rows(), strings(&dialog.fixture["drawn"]), "the rows");
        dialog
    }

    fn rows(&self) -> Vec<String> {
        self.window
            .get_tags()
            .iter()
            .map(|r| r.text.to_string())
            .collect()
    }

    /// The tag each drawn row belongs to (a parent row to the tag above).
    fn row_tags(&self) -> Vec<String> {
        let tags = strings(&self.fixture["tags"]);
        let mut out = Vec::new();
        let mut next = 0;
        for row in self.rows() {
            if row.starts_with("    ") {
                out.push(tags[next - 1].clone());
            } else {
                out.push(tags[next].clone());
                next += 1;
            }
        }
        out
    }

    /// The tags drawn selected, in the list's order.
    fn selected(&self) -> Vec<String> {
        let mask = self.window.get_tag_selected();
        let mut out: Vec<String> = Vec::new();
        for (i, tag) in self.row_tags().into_iter().enumerate() {
            if mask.row_data(i).unwrap_or(false) && !out.contains(&tag) {
                out.push(tag);
            }
        }
        out
    }

    /// Where the row of `tag` is laid out (or below the rows).
    fn at(&self, tag: Option<&str>) -> slint::LogicalPosition {
        let rows = self.rows();
        let row_tags = self.row_tags();
        if let Some(tag) = tag {
            let i = row_tags
                .iter()
                .position(|t| t == tag)
                .unwrap_or_else(|| panic!("{tag} is not listed"));
            widgets::position(self.window.window(), &rows[i], 0)
        } else {
            let last = widgets::position(self.window.window(), rows.last().unwrap(), 0);
            slint::LogicalPosition::new(last.x, last.y + 40.0)
        }
    }

    fn press(&self, tag: Option<&str>, button: PointerEventButton, mods: Mods) {
        input::press(self.window.window(), self.at(tag), button, mods);
    }

    fn click(&self, tag: Option<&str>, mods: Mods) {
        self.press(tag, LEFT, mods);
    }

    fn key(&self, text: &str, mods: Mods) {
        input::key(self.window.window(), text, mods);
    }

    fn select(&self, tags: &[&str]) {
        self.click(None, NONE);
        for tag in tags {
            self.click(Some(tag), CTRL);
        }
    }

    fn recorded(&self, name: &str) -> Json {
        self.fixture["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["step"] == name)
            .unwrap_or_else(|| panic!("no recorded step {name}"))
            .clone()
    }

    fn step(&self, name: &str) {
        self.h.differ(
            name,
            &json!(self.selected()),
            &self.recorded(name)["selected"],
        );
    }

    /// What the window asks or shows once something was entered (or a
    /// removal asked), then put back.
    fn outcome(&self) -> Json {
        let asked = json!({
            "question": self.window.get_tag_menu_question().to_string(),
            "rows": self.rows(),
        });
        self.window.invoke_tag_menu_dismissed();
        widgets::settle();
        asked
    }

    /// What a fresh dialog model of the same files asks or shows when
    /// `entered` (the reference's) are entered, or (`remove`) removed.
    fn expected(&self, recorded: &Json, remove: bool) -> Json {
        let mut fresh = self.fresh();
        let mut question = String::new();
        for tags in recorded.as_array().unwrap() {
            let tags = strings(tags);
            if remove {
                if let Ok(hydrus_gui_model::manage_tags::Removal::Confirm { message, .. }) =
                    fresh.remove_tags(&tags)
                {
                    question = message;
                }
            } else if let Ok(hydrus_gui_model::manage_tags::Entered::Ask(prompt)) =
                fresh.enter_tags(&tags, false, false)
            {
                question = prompt.message;
            }
        }
        json!({
            "question": question,
            "rows": fresh.display_rows().into_iter().map(|r| r.label).collect::<Vec<_>>(),
        })
    }

    fn fresh(&self) -> hydrus_gui_model::manage_tags::ManageTags {
        hydrus_gui_model::manage_tags::ManageTags::new(self.h.store.clone(), self.files.clone())
            .unwrap()
    }

    /// What the window asks once tags were entered, and the rows it stages
    /// once `yes` (its first choice) or no is answered.
    fn answered(&self, yes: bool) -> Json {
        let asked = json!({
            "question": self.window.get_tag_menu_question().to_string(),
            "yes": self.window.get_tag_menu_yes_label().to_string(),
            "no": self.window.get_tag_menu_no_label().to_string(),
        });
        self.window.invoke_tag_menu_answered(yes);
        widgets::settle();
        json!({ "asked": asked, "rows": self.rows() })
    }

    /// The same of a fresh dialog model given the tags the reference was
    /// given (`entered`), answered the same way.
    fn expected_answered(&self, entered: &Json, yes: bool) -> Json {
        let mut fresh = self.fresh();
        let mut asked = json!({ "question": "", "yes": "yes", "no": "no" });
        for tags in entered.as_array().unwrap() {
            if let Ok(hydrus_gui_model::manage_tags::Entered::Ask(prompt)) =
                fresh.enter_tags(&strings(tags), false, false)
            {
                asked = json!({
                    "question": prompt.message,
                    "yes": prompt.choices[0],
                    "no": prompt.choices[1],
                });
                fresh.answer_prompt(&prompt, Some(usize::from(!yes)));
            }
        }
        json!({
            "asked": asked,
            "rows": fresh.display_rows().into_iter().map(|r| r.label).collect::<Vec<_>>(),
        })
    }

    fn tree(&self, pane: usize) -> Json {
        let lines = self
            .window
            .get_tag_menu_panes()
            .row_data(pane)
            .unwrap()
            .lines;
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            out.push(match line.kind {
                1 => json!(["check", line.label.to_string(), line.checked]),
                2 => json!("---"),
                3 => {
                    self.window.invoke_tag_menu_hovered(
                        i32::try_from(pane).unwrap(),
                        i32::try_from(i).unwrap(),
                        300.0,
                        20.0,
                        10.0,
                    );
                    json!({ line.label.to_string(): self.tree(pane + 1) })
                }
                _ if !line.usable => json!(["label", line.label.to_string()]),
                _ => json!(line.label.to_string()),
            });
        }
        json!(out)
    }

    fn choose(&self, path: &[&str]) {
        for (pane, label) in path.iter().enumerate() {
            let lines = self
                .window
                .get_tag_menu_panes()
                .row_data(pane)
                .unwrap()
                .lines;
            let line = lines
                .iter()
                .position(|row| row.label == *label)
                .unwrap_or_else(|| panic!("no {label} in {path:?}"));
            self.window.invoke_tag_menu_clicked(
                i32::try_from(pane).unwrap(),
                i32::try_from(line).unwrap(),
                300.0,
                20.0,
                10.0,
            );
        }
        widgets::settle();
    }

    fn open_menu(&self, tags: &[String]) {
        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
        self.select(&tags);
        self.press(tags.last().copied(), RIGHT, NONE);
        assert!(
            self.window.get_tag_menu_panes().row_count() > 0,
            "no menu for {tags:?}"
        );
    }

    fn answer(&self) -> Json {
        if self.window.get_tag_menu_question().is_empty() {
            return json!([]);
        }
        let asked = json!([{
            "message": self.window.get_tag_menu_question().to_string(),
            "title": self.window.get_tag_menu_question_title().to_string(),
            "yes": self.window.get_tag_menu_yes_label().to_string(),
            "no": self.window.get_tag_menu_no_label().to_string(),
        }]);
        self.window.invoke_tag_menu_answered(true);
        widgets::settle();
        asked
    }
}

// leaf: interaction-manage-tags-list-menu-keys
#[test]
fn the_dialogs_list_selects_enters_and_removes_as_the_references_does() {
    let d = Dialog::new();
    // clicks
    d.step("initial");
    d.click(Some("series:spaced name"), NONE);
    d.step("click series:spaced name");
    d.click(Some("plain tag"), CTRL);
    d.step("ctrl click plain tag adds it");
    d.click(Some("box:anchor"), SHIFT);
    d.step("shift click box:anchor");
    d.click(Some("series:spaced name"), CTRL);
    d.step("ctrl click series:spaced name takes it away");
    d.click(Some("box:anchor"), NONE);
    d.step("plain click on a selected row keeps the selection");
    d.press(Some("solo"), RIGHT, NONE);
    d.window.invoke_tag_menu_dismissed();
    d.step("right press on a selected row keeps the selection");
    d.click(None, NONE);
    d.step("click below the rows");
    // a double-click enters the selection
    d.select(&["plain tag", "box:anchor"]);
    input::double_click(d.window.window(), d.at(Some("box:anchor")), NONE);
    // (what it entered: the_dialogs_list_enters_the_tags_the_reference_does)
    d.step("double click with two selected");
    d.window.invoke_tag_menu_dismissed();
    widgets::settle();
    d.click(None, NONE);
    input::double_click(d.window.window(), d.at(None), NONE);
    let name = "double click below the rows";
    d.h.differ(
        name,
        &d.outcome(),
        &d.expected(&d.recorded(name)["entered"], false),
    );
    // keys
    d.click(Some("plain tag"), NONE);
    d.step("click plain tag");
    for (text, mods, name) in [
        (DOWN, NONE, "down"),
        (DOWN, SHIFT, "shift down"),
        (HOME, NONE, "home"),
        (UP, NONE, "up wraps to the end"),
        (END, NONE, "end"),
        ("a", CTRL, "ctrl a"),
        (ESCAPE, NONE, "escape"),
    ] {
        d.key(text, mods);
        d.step(name);
    }
    let copies = |n: usize| json!(d.h.copies.borrow()[n..].to_vec());
    let n = d.h.copies.borrow().len();
    d.key("c", CTRL);
    let name = "ctrl c with none selected copies all";
    d.h.differ(name, &copies(n), &d.recorded(name)["copied"]);
    d.select(&["alpha:first", "safe"]);
    for (mods, name) in [
        (CTRL, "ctrl c copies the selection"),
        (CTRL_SHIFT, "ctrl shift c copies the selection with parents"),
    ] {
        let n = d.h.copies.borrow().len();
        d.key("c", mods);
        d.step(name);
        d.h.differ(name, &copies(n), &d.recorded(name)["copied"]);
    }
    d.key(RETURN, NONE);
    d.step("return enters the selection");
    d.window.invoke_tag_menu_dismissed();
    widgets::settle();
    d.key(DELETE, NONE);
    let name = "delete removes the selection";
    d.h.differ(
        name,
        &d.outcome(),
        &d.expected(&d.recorded(name)["deleted"], true),
    );
    // page keys, the list showing as many rows as the reference's did
    d.click(Some("plain tag"), NONE);
    d.step("click plain tag before the page keys");
    let wanted = d.fixture["rows_per_page"].as_i64().unwrap();
    for _ in 0..4 {
        let rows = i64::from(d.window.get_tag_page_rows());
        if rows == wanted {
            break;
        }
        let size = d
            .window
            .window()
            .size()
            .to_logical(d.window.window().scale_factor());
        #[allow(clippy::cast_precision_loss)]
        d.window.window().set_size(slint::LogicalSize::new(
            size.width,
            size.height + (wanted - rows) as f32 * 20.0,
        ));
        widgets::settle();
    }
    assert_eq!(i64::from(d.window.get_tag_page_rows()), wanted);
    for (text, mods, name) in [
        (PAGE_DOWN, NONE, "page down"),
        (PAGE_DOWN, NONE, "page down again"),
        (PAGE_UP, NONE, "page up"),
        (PAGE_DOWN, SHIFT, "shift page down"),
    ] {
        d.key(text, mods);
        d.step(name);
    }
    d.h.finish();
}

// leaf: interaction-manage-tags-list-menu-keys
#[test]
fn the_dialogs_list_enters_the_tags_the_reference_does() {
    // (each answer to the question in a dialog opened anew: an answer
    // stages changes the next steps would see)
    let mut d = Dialog::new();
    for yes in [true, false] {
        d.select(&["plain tag", "box:anchor"]);
        input::double_click(d.window.window(), d.at(Some("box:anchor")), NONE);
        let name = "double click with two selected";
        d.h.differ(
            &format!("{name}, answered {yes}"),
            &d.answered(yes),
            &d.expected_answered(&d.recorded(name)["entered"], yes),
        );
        d.reopen();
        d.select(&["alpha:first", "safe"]);
        d.key(RETURN, NONE);
        let name = "return enters the selection";
        d.h.differ(
            &format!("{name}, answered {yes}"),
            &d.answered(yes),
            &d.expected_answered(&d.recorded(name)["entered"], yes),
        );
        d.reopen();
    }
    d.h.finish();
}

// leaf: interaction-manage-tags-list-menu-keys
#[test]
fn the_dialogs_list_middle_clicks_open_search_pages_as_the_references_does() {
    let d = Dialog::new();
    let middle = |tag: Option<&str>, mods: Mods, name: &str| {
        let before = d.h.bound.pages.borrow().page_count();
        d.press(tag, MIDDLE, mods);
        assert_eq!(
            d.h.page().borrow().location(),
            &combined_local_file_domains(),
            "{name}: the new page's location"
        );
        let recorded: Vec<Json> = d.recorded(name)["launched"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                json!({
                    "topic": l["topic"],
                    "predicates": l["predicates"],
                    "page_name": l["page_name"],
                    "location": l["location"],
                })
            })
            .collect();
        d.h.differ(name, &d.h.launched(before), &json!(recorded));
        d.step(name);
    };
    middle(
        Some("series:spaced name"),
        NONE,
        "middle click series:spaced name",
    );
    d.select(&["plain tag", "box:anchor"]);
    middle(
        Some("plain tag"),
        SHIFT,
        "shift middle click on plain tag ranges from box:anchor",
    );
    d.h.finish();
}

// leaf: interaction-manage-tags-list-menu-keys
#[test]
fn the_dialogs_list_menu_and_its_entries_are_the_references() {
    let d = Dialog::new();
    for case in d.fixture["menus"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let tags = strings(&case["selected"]);
        d.open_menu(&tags);
        d.h.differ(
            &format!("{name} selected"),
            &json!(d.selected()),
            &case["selected"],
        );
        d.h.differ(&format!("{name} menu"), &d.tree(0), &case["menu"]);
        d.window.invoke_tag_menu_dismissed();
        for copy in case["copies"].as_array().unwrap() {
            let label = copy["label"].as_str().unwrap();
            d.open_menu(&tags);
            let n = d.h.copies.borrow().len();
            d.choose(&["copy", label]);
            d.h.differ(
                &format!("{name} copy {label}"),
                &json!(d.h.copies.borrow()[n..].to_vec()),
                &copy["payload"],
            );
        }
        for launch in case["launches"].as_array().unwrap() {
            let label = launch["label"].as_str().unwrap();
            d.open_menu(&tags);
            let before = d.h.bound.pages.borrow().page_count();
            d.choose(&["open", label]);
            let sort = |pages: Json| {
                let mut pages: Vec<Json> = pages
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| {
                        json!({
                            "topic": l["topic"],
                            "predicates": l["predicates"],
                            "page_name": l["page_name"],
                            "location": l["location"],
                        })
                    })
                    .collect();
                pages.sort_by_key(|p| p["page_name"].to_string());
                json!(pages)
            };
            d.h.differ(
                &format!("{name} {label}"),
                &sort(d.h.launched(before)),
                &sort(launch["launched"].clone()),
            );
        }
    }
    // the decorators the menu turns on and off change what is drawn
    for decorator in d.fixture["decorators"].as_array().unwrap() {
        let label = decorator["label"].as_str().unwrap();
        if decorator["chosen"] != true {
            continue;
        }
        d.open_menu(&["alpha:first".to_owned()]);
        let offered: Vec<String> = d
            .tree(0)
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| *e != "---")
            .take(3)
            .map(|e| match e {
                Json::String(s) => s.clone(),
                Json::Object(o) => o.keys().next().unwrap().clone(),
                other => other.to_string(),
            })
            .collect();
        d.h.differ(
            &format!("{label} offered"),
            &json!(offered),
            &decorator["offered"],
        );
        d.choose(&[label]);
        d.h.differ(
            &format!("{label} drawn"),
            &json!(d.rows()),
            &decorator["drawn"],
        );
    }
    // the entries it shares with the other tag lists
    replay_shared_entries(
        &d.h,
        &d.fixture["triggers"],
        &MenuDriver {
            open: &|tags| d.open_menu(tags),
            choose: &|path| d.choose(path),
            answer: &|| d.answer(),
            dialog: &|| {
                d.h.bound
                    .manage_tags_relationships
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            },
        },
    );
    d.h.finish();
}
