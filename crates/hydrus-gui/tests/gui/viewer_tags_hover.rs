//! The media viewer's tags hover list (the reference's
//! `ListBoxTagsMediaHoverFrame`), replayed step by step from
//! `oracle/record_viewer_tags_hover.py` through a real viewer opened from
//! the real main window: real pointer presses at the rows where the hover
//! lays them out, and real key presses on the list.
use serde_json::{Value as Json, json};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle as _, Model as _};

use super::selection_tags_box::{Harness, MenuDriver, replay_shared_entries, strings};
use crate::common::input::{self, CTRL, Mods, NONE, SHIFT};
use crate::common::widgets;
use hydrus_gui::{MediaViewerWindow, headless};

const DOWN: &str = "\u{F701}";
const UP: &str = "\u{F700}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";
const ESCAPE: &str = "\u{1b}";
const RETURN: &str = "\n";

const LEFT: PointerEventButton = PointerEventButton::Left;
const RIGHT: PointerEventButton = PointerEventButton::Right;
const MIDDLE: PointerEventButton = PointerEventButton::Middle;

/// The box's page, with the viewer open on the recorded file.
struct Hover {
    h: Harness,
    fixture: Json,
    viewer: MediaViewerWindow,
    drawn: std::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
}

impl Hover {
    fn new() -> Self {
        let h = Harness::new(1600.0);
        let fixture = hydrus_testkit::fixture_json("viewer_tags_hover.json");
        let file: hydrus_core::Sha256 = fixture["file"].as_str().unwrap().parse().unwrap();
        let id = h
            .store
            .read(|conn| hydrus_store::master::hash_ids(conn, &[file]))
            .unwrap()[&file];
        let index = h
            .page()
            .borrow()
            .files()
            .iter()
            .position(|f| *f == id)
            .unwrap();
        h.ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
        let viewer = h.bound.viewer.borrow().as_ref().unwrap().clone_strong();
        let drawn = h.windows.get(h.windows.count() - 1).unwrap();
        headless::render(&drawn, 1000, 650);
        let hover = Self {
            h,
            fixture,
            viewer,
            drawn,
        };
        assert_eq!(hover.rows(), strings(&hover.fixture["drawn"]), "the rows");
        hover
    }

    fn rows(&self) -> Vec<String> {
        self.viewer
            .get_tags()
            .iter()
            .map(|r| r.text.to_string())
            .collect()
    }

    fn tags(&self) -> Vec<String> {
        self.viewer
            .get_tag_identities()
            .iter()
            .map(|t| t.to_string())
            .collect()
    }

    fn selected(&self) -> Vec<String> {
        let mask = self.viewer.get_tag_selected();
        self.tags()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| mask.row_data(*i).unwrap_or(false))
            .map(|(_, t)| t)
            .collect()
    }

    /// Where the row of `tag` is laid out in the hover (or below the rows).
    fn at(&self, tag: Option<&str>) -> slint::LogicalPosition {
        let tags = self.tags();
        let row = match tag {
            Some(tag) => tags
                .iter()
                .position(|t| t == tag)
                .unwrap_or_else(|| panic!("{tag} is not listed")),
            None => tags.len() + 2,
        };
        #[allow(clippy::cast_precision_loss)]
        let y = 6.0 + (row as f32 + 0.5) * self.viewer.get_tag_row_height();
        slint::LogicalPosition::new(20.0, y)
    }

    /// Press where the hover shows `tag`, the pointer over it first (the
    /// hover shows while it is there).
    fn press(&self, tag: Option<&str>, button: PointerEventButton, mods: Mods) {
        let at = self.at(tag);
        self.viewer
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: at });
        headless::render(&self.drawn, 1000, 650);
        assert!(self.viewer.get_tags_showing(), "the hover shows");
        input::press(self.viewer.window(), at, button, mods);
        headless::render(&self.drawn, 1000, 650);
    }

    fn click(&self, tag: Option<&str>, mods: Mods) {
        self.press(tag, LEFT, mods);
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

    /// Whether the viewer's manage tags opened (closed again).
    fn managed(&self) -> Json {
        let window = self
            .h
            .bound
            .manage_tags
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        match window {
            Some(window) => {
                window.invoke_cancel();
                widgets::settle();
                json!([true])
            }
            None => json!([]),
        }
    }

    fn tree(&self, pane: usize) -> Json {
        let lines = self
            .viewer
            .get_tag_menu_panes()
            .row_data(pane)
            .unwrap()
            .lines;
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            out.push(match line.kind {
                2 => json!("---"),
                3 => {
                    self.viewer.invoke_tag_menu_hovered(
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
                .viewer
                .get_tag_menu_panes()
                .row_data(pane)
                .unwrap()
                .lines;
            let line = lines
                .iter()
                .position(|row| row.label == *label)
                .unwrap_or_else(|| panic!("no {label} in {path:?}"));
            self.viewer.invoke_tag_menu_clicked(
                i32::try_from(pane).unwrap(),
                i32::try_from(line).unwrap(),
                300.0,
                20.0,
                10.0,
            );
        }
        widgets::settle();
    }

    /// The question the hover's tag menu asks, as the recorder writes it,
    /// answered yes.
    fn answer(&self) -> Json {
        if self.viewer.get_tag_menu_question().is_empty() {
            return json!([]);
        }
        let asked = json!([{
            "message": self.viewer.get_tag_menu_question().to_string(),
            "title": self.viewer.get_tag_menu_question_title().to_string(),
            "yes": self.viewer.get_tag_menu_yes_label().to_string(),
            "no": self.viewer.get_tag_menu_no_label().to_string(),
        }]);
        self.viewer.invoke_tag_menu_answered(true);
        widgets::settle();
        asked
    }

    fn open_menu(&self, tags: &[String]) {
        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
        self.select(&tags);
        self.press(tags.last().copied(), RIGHT, NONE);
        assert!(
            self.viewer.get_tag_menu_panes().row_count() > 0,
            "no menu for {tags:?}"
        );
    }
}

// leaf: viewer-tags-hover-click-select
#[test]
fn hover_clicks_select_rows_as_the_references_do() {
    let v = Hover::new();
    v.step("initial");
    v.click(Some("series:spaced name"), NONE);
    v.step("click series:spaced name");
    v.click(Some("plain tag"), CTRL);
    v.step("ctrl click plain tag adds it");
    v.click(Some("box:anchor"), SHIFT);
    v.step("shift click box:anchor");
    v.click(Some("series:spaced name"), CTRL);
    v.step("ctrl click series:spaced name takes it away");
    v.click(Some("plain tag"), NONE);
    v.step("plain click on a selected row keeps the selection");
    v.press(Some("alpha:first"), RIGHT, NONE);
    v.viewer.invoke_tag_menu_dismissed();
    v.step("right press on a selected row keeps the selection");
    v.click(None, NONE);
    v.step("click below the rows");
    v.h.finish();
}

// leaf: viewer-tags-hover-middle-click
#[test]
fn hover_middle_clicks_open_search_pages_as_the_references_do() {
    let v = Hover::new();
    let middle = |tag: Option<&str>, mods: Mods, name: &str| {
        let before = v.h.bound.pages.borrow().page_count();
        v.press(tag, MIDDLE, mods);
        let recorded: Vec<Json> = v.recorded(name)["launched"]
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
        v.h.differ(name, &v.h.launched(before), &json!(recorded));
        v.step(name);
    };
    middle(
        Some("series:spaced name"),
        NONE,
        "middle click series:spaced name",
    );
    v.select(&["plain tag", "box:anchor"]);
    middle(
        Some("plain tag"),
        SHIFT,
        "shift middle click with two selected",
    );
    v.h.finish();
}

// leaf: viewer-tags-hover-double-click
#[test]
fn hover_double_clicks_manage_the_files_tags_as_the_references_do() {
    let v = Hover::new();
    // (the selection the recorder had)
    v.select(&["plain tag", "safe", "alpha:first", "box:anchor"]);
    let at = v.at(Some("alpha:first"));
    input::double_click(v.viewer.window(), at, NONE);
    let name = "double click alpha:first";
    v.h.differ(name, &v.managed(), &v.recorded(name)["managed"]);
    v.step(name);
    v.click(None, NONE);
    input::double_click(v.viewer.window(), v.at(None), NONE);
    let name = "double click below the rows";
    v.h.differ(name, &v.managed(), &v.recorded(name)["managed"]);
    v.step(name);
    v.h.finish();
}

// leaf: viewer-tags-hover-keyboard
#[test]
fn hover_keys_select_copy_and_manage_as_the_references_do() {
    let v = Hover::new();
    v.click(Some("plain tag"), NONE);
    v.step("click plain tag");
    let key = |text: &str, mods: Mods| {
        input::key(v.viewer.window(), text, mods);
        headless::render(&v.drawn, 1000, 650);
    };
    for (text, mods, name) in [
        (DOWN, NONE, "down"),
        (DOWN, SHIFT, "shift down"),
        (HOME, NONE, "home"),
        (UP, NONE, "up wraps to the end"),
        (END, NONE, "end"),
        ("a", CTRL, "ctrl a"),
        (ESCAPE, NONE, "escape"),
    ] {
        key(text, mods);
        v.step(name);
    }
    let n = v.h.copies.borrow().len();
    key("c", CTRL);
    let name = "ctrl c with none selected copies all";
    v.step(name);
    v.h.differ(
        name,
        &json!(v.h.copies.borrow()[n..].to_vec()),
        &v.recorded(name)["copied"],
    );
    v.click(Some("safe"), NONE);
    let n = v.h.copies.borrow().len();
    key("c", CTRL);
    let name = "ctrl c copies the selection";
    v.step(name);
    v.h.differ(
        name,
        &json!(v.h.copies.borrow()[n..].to_vec()),
        &v.recorded(name)["copied"],
    );
    key(RETURN, NONE);
    let name = "return";
    v.h.differ(name, &v.managed(), &v.recorded(name)["managed"]);
    v.step(name);
    v.h.finish();
}

// leaf: viewer-tags-hover-menu
#[test]
fn the_hovers_menu_and_its_entries_are_the_references() {
    let v = Hover::new();
    for case in v.fixture["menus"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let tags = strings(&case["selected"]);
        v.open_menu(&tags);
        v.h.differ(
            &format!("{name} selected"),
            &json!(v.selected()),
            &case["selected"],
        );
        v.h.differ(&format!("{name} menu"), &v.tree(0), &case["menu"]);
        v.viewer.invoke_tag_menu_dismissed();
        for copy in case["copies"].as_array().unwrap() {
            let label = copy["label"].as_str().unwrap();
            v.open_menu(&tags);
            let n = v.h.copies.borrow().len();
            v.choose(&["copy", label]);
            v.h.differ(
                &format!("{name} copy {label}"),
                &json!(v.h.copies.borrow()[n..].to_vec()),
                &copy["payload"],
            );
        }
        for launch in case["launches"].as_array().unwrap() {
            let label = launch["label"].as_str().unwrap();
            v.open_menu(&tags);
            let before = v.h.bound.pages.borrow().page_count();
            v.choose(&["open", label]);
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
            v.h.differ(
                &format!("{name} {label}"),
                &sort(v.h.launched(before)),
                &sort(launch["launched"].clone()),
            );
        }
    }
    // the entries it shares with the other tag lists, from its own menu
    replay_shared_entries(
        &v.h,
        &v.fixture["triggers"],
        &MenuDriver {
            open: &|tags| v.open_menu(tags),
            choose: &|path| v.choose(path),
            answer: &|| v.answer(),
            dialog: &|| {
                v.h.bound
                    .viewer_tag_relationships
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            },
        },
    );
    // hiding a tag from the hover's view of the file's tags
    let hide = &v.fixture["hide"];
    v.open_menu(&strings(&hide["selected"]));
    let path = strings(&hide["path"]);
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    v.choose(&path);
    v.h.differ("hide question", &v.answer(), &hide["questions"]);
    v.h.differ("hide tags", &json!(v.tags()), &hide["tags_after"]);
    v.h.finish();
}

// (#297 review: a hover that took the keyboard and then lowered kept it
// from the viewer's keys)
#[test]
fn the_viewers_keys_work_after_a_tag_was_clicked_and_the_hover_lowered() {
    let v = Hover::new();
    v.click(Some("plain tag"), NONE);
    // the pointer onto the canvas: the hover lowers
    let canvas = slint::LogicalPosition::new(600.0, 300.0);
    v.viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: canvas });
    headless::render(&v.drawn, 1000, 650);
    assert!(!v.viewer.get_tags_showing(), "the hover lowered");
    let before = v.viewer.get_caption();
    input::key(v.viewer.window(), "\u{F703}", NONE);
    headless::render(&v.drawn, 1000, 650);
    assert_ne!(
        v.viewer.get_caption(),
        before,
        "right arrow shows the next file"
    );
}

// (#297 review: escape with the hover's menu open closes the menu, as the
// reference's menu takes it, and leaves the selection as it was)
#[test]
fn escape_closes_the_hovers_menu_and_keeps_its_selection() {
    let v = Hover::new();
    v.open_menu(&["plain tag".to_owned(), "box:anchor".to_owned()]);
    let selected = v.selected();
    input::key(v.viewer.window(), ESCAPE, NONE);
    headless::render(&v.drawn, 1000, 650);
    assert_eq!(
        v.viewer.get_tag_menu_panes().row_count(),
        0,
        "the menu closed"
    );
    assert_eq!(v.selected(), selected, "the selection kept");
    // and the hover had stayed up while its menu was open, as the reference's
    v.open_menu(&["plain tag".to_owned()]);
    let away = slint::LogicalPosition::new(600.0, 300.0);
    v.viewer
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: away });
    headless::render(&v.drawn, 1000, 650);
    assert!(
        v.viewer.get_tags_showing(),
        "the hover stays up under its menu"
    );
    v.viewer.invoke_tag_menu_dismissed();
}
