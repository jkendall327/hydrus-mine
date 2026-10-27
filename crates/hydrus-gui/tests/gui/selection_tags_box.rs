//! A search page's "selection tags" box (the reference's
//! `ListBoxTagsMediaSidebar`), replayed step by step from
//! `oracle/record_selection_tags_box.py` through the real main window.
//!
//! Every press is a real pointer press at the row where the window laid it
//! out (found by the text the row shows), and every key a real key press
//! on the focused list, so these prove the rows receive the clicks: before
//! #286 the box took only double-clicks, and tests that invoked its
//! callbacks could not tell.
use std::{cell::RefCell, rc::Rc};

use serde_json::{Value as Json, json};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle as _, Model as _};

use crate::common::input::{self, CTRL, CTRL_SHIFT, Mods, NONE, SHIFT};
use crate::common::widgets;
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, SelectionTagsBox, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext, predicate_text};

const DOWN: &str = "\u{F701}";
const UP: &str = "\u{F700}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";
const PAGE_UP: &str = "\u{F72C}";
const PAGE_DOWN: &str = "\u{F72D}";
const ESCAPE: &str = "\u{1b}";
const RETURN: &str = "\n";
const DELETE: &str = "\u{7f}";

const LEFT: PointerEventButton = PointerEventButton::Left;
const RIGHT: PointerEventButton = PointerEventButton::Right;
const MIDDLE: PointerEventButton = PointerEventButton::Middle;

/// The recorder's page (my files, searching its anchor tag) on the basic
/// fixture with its corpus, in a laid-out main window.
pub(crate) struct Harness {
    pub fixture: Json,
    pub ui: MainWindow,
    pub bound: Bound,
    pub store: std::sync::Arc<hydrus_store::Store>,
    pub copies: Rc<RefCell<Vec<String>>>,
    pub differences: RefCell<Vec<String>>,
    _dirs: [tempfile::TempDir; 2],
    _windows: headless::Windows,
}

impl Harness {
    pub fn new(height: f32) -> Self {
        let fixture = hydrus_testkit::fixture_json("selection_tags_box.json");
        let seed = json!({
            "corpus": fixture["corpus"],
            "siblings": fixture["siblings"],
            "parents": fixture["parents"],
            "events": [{ "favourites": [] }],
        });
        let (dirs, store, _) = super::read_autocomplete::seeded(&seed);
        let windows = headless::init();
        let copies = Rc::new(RefCell::new(Vec::new()));
        hydrus_gui::set_clipper({
            let copies = copies.clone();
            move |clip| {
                if let hydrus_gui::Clip::Text(text) = clip {
                    copies.borrow_mut().push(text.clone());
                }
            }
        });
        let context = FileSearchContext {
            location: LocationContext::single(hydrus_core::ServiceKey::new(
                b"local files".to_vec(),
            )),
            predicates: hydrus_search::parse_api_search(&json!([fixture["search"]])).unwrap(),
            ..Default::default()
        };
        let ui = MainWindow::new().unwrap();
        ui.window()
            .set_size(slint::LogicalSize::new(1200.0, height));
        ui.show().unwrap();
        let bound = bind(
            &ui,
            Pages::single(SearchPage::restored(
                store.clone(),
                context,
                true,
                None,
                Vec::new(),
            )),
        );
        // (the page searches as the reference's new page does)
        ui.invoke_refresh_page();
        let harness = Self {
            fixture,
            ui,
            bound,
            store,
            copies,
            differences: RefCell::default(),
            _dirs: dirs,
            _windows: windows,
        };
        harness.wait_for_rows("the rows as the reference draws them");
        harness
    }

    /// Wait until the rows are the recorder's first rows.
    pub fn wait_for_rows(&self, what: &str) {
        let wanted = strings(&self.fixture["drawn"]);
        let started = std::time::Instant::now();
        while self.rows() != wanted {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "{what}: {:?}",
                self.rows()
            );
            widgets::settle();
        }
    }

    pub fn window(&self) -> &slint::Window {
        self.ui.window()
    }

    pub fn page(&self) -> Rc<std::cell::RefCell<SearchPage>> {
        self.bound.current.borrow().clone()
    }

    /// The rows as drawn.
    pub fn rows(&self) -> Vec<String> {
        self.ui
            .get_tags()
            .iter()
            .map(|r| r.text.to_string())
            .collect()
    }

    /// The tags drawn selected, in the list's order.
    pub fn selected(&self) -> Vec<String> {
        let mask = self.ui.global::<SelectionTagsBox<'_>>().get_selected();
        let page = self.page();
        let page = page.borrow();
        page.tag_list()
            .iter()
            .enumerate()
            .filter(|(i, _)| mask.row_data(*i).unwrap_or(false))
            .map(|(_, (tag, _))| tag.clone())
            .collect()
    }

    /// Where the row of `tag` is (or below the rows, for none).
    pub fn at(&self, tag: Option<&str>) -> slint::LogicalPosition {
        let page = self.page();
        let rows = page.borrow().tag_list().to_vec();
        if let Some(tag) = tag {
            let text = &rows
                .iter()
                .find(|(t, _)| t == tag)
                .unwrap_or_else(|| panic!("{tag} is not listed"))
                .1;
            widgets::position(self.window(), text, 0)
        } else {
            let last = widgets::position(self.window(), &rows.last().unwrap().1, 0);
            slint::LogicalPosition::new(last.x, last.y + 40.0)
        }
    }

    pub fn press(&self, tag: Option<&str>, button: PointerEventButton, mods: Mods) {
        input::press(self.window(), self.at(tag), button, mods);
    }

    pub fn click(&self, tag: Option<&str>, mods: Mods) {
        self.press(tag, LEFT, mods);
    }

    pub fn key(&self, text: &str, mods: Mods) {
        input::key(self.window(), text, mods);
    }

    /// Select just `tags`: a click below the rows (which selects nothing),
    /// then ctrl+click each. (The recorder presses escape instead of the
    /// first click: the same, but escape needs the list to have the
    /// keyboard, which the tag menu keeps once it has closed.)
    pub fn select(&self, tags: &[&str]) {
        self.click(None, NONE);
        for tag in tags {
            self.click(Some(tag), CTRL);
        }
    }

    /// Escape, then ctrl+click each of `tags`, as the recorder selects (with
    /// the list holding the keyboard).
    pub fn select_after_escape(&self, tags: &[&str]) {
        self.key(ESCAPE, NONE);
        for tag in tags {
            self.click(Some(tag), CTRL);
        }
    }

    /// The recorded step named `name`.
    pub fn recorded(&self, name: &str) -> Json {
        self.fixture["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["step"] == name)
            .unwrap_or_else(|| panic!("no recorded step {name}"))
            .clone()
    }

    pub fn differ(&self, what: &str, ours: &Json, theirs: &Json) {
        if ours != theirs {
            self.differences
                .borrow_mut()
                .push(format!("{what}:\n {ours}\nnot\n {theirs}"));
        }
    }

    /// Compare the selection with the recorded step's.
    pub fn step(&self, name: &str) {
        let recorded = self.recorded(name);
        self.differ(name, &json!(self.selected()), &recorded["selected"]);
    }

    /// How predicates are written out (`ToString`, not for display).
    pub fn text(&self) -> hydrus_search::TextContext {
        let viewing: hydrus_store::settings::FileViewingStatistics = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        hydrus_search::TextContext::from_store(&self.store.snapshot().services, &viewing)
    }

    /// The page's search, written out and sorted as the recorder writes it.
    pub fn searched(&self) -> Json {
        let text = self.text();
        let page = self.page();
        let page = page.borrow();
        let mut out: Vec<String> = page
            .active_predicates()
            .iter()
            .map(|p| predicate_text(p, &text))
            .collect();
        out.sort();
        json!(out)
    }

    /// Put the recorder's search back, as the recorder does after each step
    /// that changed it, and wait for the rows to come back.
    pub fn restore(&self) {
        let page = self.page();
        let original = hydrus_search::parse_api_search(&json!([self.fixture["search"]])).unwrap();
        let current = page.borrow().active_predicates().to_vec();
        page.borrow_mut()
            .edit_active_predicates(&current, &original);
        self.ui.invoke_refresh_page();
        self.wait_for_rows("the rows back");
    }

    /// The pages opened since there were `before`, as the recorder writes
    /// them, closed again.
    pub fn launched(&self, before: usize) -> Json {
        let original = self.page();
        let original_name = "files";
        let mut out = Vec::new();
        while self.bound.pages.borrow().page_count() > before {
            let page = self.page();
            let (predicates, location, duplicates) = {
                let page = page.borrow();
                let text = self.text();
                // (a duplicates page searches its first file search)
                let searched = page.duplicates().map_or_else(
                    || page.active_predicates().to_vec(),
                    |d| d.search.search_1.predicates.clone(),
                );
                let mut predicates: Vec<String> =
                    searched.iter().map(|p| predicate_text(p, &text)).collect();
                predicates.sort();
                (
                    predicates,
                    page.location().clone(),
                    page.duplicates().is_some(),
                )
            };
            let name = self.bound.pages.borrow().shown().name.clone();
            let topic = if duplicates {
                "new_page_duplicates"
            } else {
                "new_page_query"
            };
            out.push(json!({
                "topic": topic,
                "predicates": predicates,
                "page_name": name,
                "location": if location == original.borrow().location().clone() { "my files" } else { "elsewhere" },
            }));
            self.ui.invoke_close_page();
            widgets::settle();
        }
        assert_eq!(
            self.bound.pages.borrow().shown().name,
            original_name,
            "back on the page"
        );
        // (the recorder pushes them as opened; the last opened is closed first)
        out.reverse();
        json!(out)
    }

    pub fn finish(&self) {
        let differences = self.differences.borrow();
        assert!(
            differences.is_empty(),
            "{} differences from the reference:\n{}",
            differences.len(),
            differences.join("\n\n")
        );
    }
}

pub(crate) fn strings(v: &Json) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

/// The recorded launches, less what this harness cannot tell apart
/// (whether the window was raised).
fn launches(recorded: &Json) -> Json {
    json!(
        recorded
            .as_array()
            .unwrap()
            .iter()
            .map(|l| json!({
                "topic": l["topic"],
                "predicates": l["predicates"],
                "page_name": l["page_name"],
                "location": l["location"],
            }))
            .collect::<Vec<_>>()
    )
}

// leaf: selection-tags-click-select
#[test]
fn clicks_select_rows_as_the_references_hit_rules_do() {
    let h = Harness::new(1600.0);
    h.step("initial");
    h.click(Some("series:spaced name"), NONE);
    h.step("click series:spaced name");
    h.click(Some("solo"), CTRL);
    h.step("ctrl click solo adds it");
    h.click(Some("series:spaced name"), CTRL);
    h.step("ctrl click series:spaced name takes it away");
    h.click(Some("zeta:last"), SHIFT);
    h.step("shift click zeta:last");
    h.click(Some("alpha:first"), CTRL_SHIFT);
    h.step("ctrl shift click alpha:first");
    h.click(Some("zeta:last"), NONE);
    h.step("plain click on a selected row keeps the selection");
    h.click(Some("plain tag"), NONE);
    h.step("plain click on an unselected row selects only it");
    h.press(Some("box:anchor"), RIGHT, NONE);
    h.ui.invoke_tag_menu_dismissed();
    h.step("right press on an unselected row selects only it");
    h.click(Some("plain tag"), CTRL);
    h.press(Some("plain tag"), RIGHT, NONE);
    h.ui.invoke_tag_menu_dismissed();
    h.step("right press on a selected row keeps the selection");
    h.click(None, NONE);
    h.step("click below the rows");
    h.click(Some("solo"), SHIFT);
    h.step("shift click with nothing before");
    h.click(None, SHIFT);
    h.step("shift click below the rows selects to the end");
    h.finish();
}

// leaf: selection-tags-keyboard
#[test]
fn keys_select_copy_and_activate_as_the_references_do() {
    let h = Harness::new(1600.0);
    // (the click that gives the list the keyboard; the recorder's
    // "shift click below the rows" left everything from solo selected)
    h.click(Some("solo"), NONE);
    h.click(None, SHIFT);
    h.click(Some("box:anchor"), NONE);
    h.step("click box:anchor");
    // the list holding as many rows as the reference's did, for page up and down
    let wanted = h.fixture["rows_per_page"].as_i64().unwrap();
    for _ in 0..4 {
        let rows = i64::from(h.ui.get_selection_tags_page_rows());
        if rows == wanted {
            break;
        }
        let size = h.window().size().to_logical(h.window().scale_factor());
        #[allow(clippy::cast_precision_loss)]
        h.window().set_size(slint::LogicalSize::new(
            size.width,
            size.height + (wanted - rows) as f32 * 20.0,
        ));
        widgets::settle();
    }
    assert_eq!(i64::from(h.ui.get_selection_tags_page_rows()), wanted);
    for (key, mods, name) in [
        (DOWN, NONE, "down"),
        (DOWN, SHIFT, "shift down"),
        (DOWN, CTRL, "ctrl down"),
        (UP, NONE, "up"),
        (HOME, NONE, "home"),
        (UP, NONE, "up wraps to the end"),
        (DOWN, NONE, "down wraps to the start"),
        (END, NONE, "end"),
        ("n", CTRL, "ctrl n"),
        ("p", CTRL, "ctrl p"),
        (PAGE_UP, NONE, "page up"),
        (PAGE_DOWN, NONE, "page down"),
        (ESCAPE, NONE, "escape"),
    ] {
        h.key(key, mods);
        h.step(name);
    }
    let copied = |h: &Harness, n: usize| json!(h.copies.borrow()[n..].to_vec());
    let n = h.copies.borrow().len();
    h.key("c", CTRL);
    h.step("ctrl c with none selected copies all");
    let name = "ctrl c with none selected copies all";
    h.differ(name, &copied(&h, n), &h.recorded(name)["copied"]);
    h.key("a", CTRL);
    h.step("ctrl a");
    h.select_after_escape(&["alpha:first", "plain tag"]);
    for (mods, name) in [
        (CTRL, "ctrl c copies the selection"),
        (CTRL_SHIFT, "ctrl shift c"),
    ] {
        let n = h.copies.borrow().len();
        h.key("c", mods);
        h.step(name);
        h.differ(name, &copied(&h, n), &h.recorded(name)["copied"]);
    }
    h.key(DELETE, NONE);
    h.step("delete does nothing");
    h.differ(
        "delete does nothing",
        &h.searched(),
        &h.recorded("delete does nothing")["searched"],
    );
    // enter activates the selection, as a double-click does
    for (tags, mods, name) in [
        (&["alpha:first"][..], NONE, "return"),
        (
            &["alpha:first", "solo"][..],
            CTRL,
            "ctrl return with two selected",
        ),
    ] {
        h.select_after_escape(tags);
        h.key(RETURN, mods);
        h.differ(name, &h.searched(), &h.recorded(name)["searched"]);
        // (a new search empties the list, and its selection with it)
        h.differ(name, &json!(h.selected()), &h.recorded(name)["selected"]);
        h.restore();
    }
    h.finish();
}

// leaf: selection-tags-middle-click
#[test]
fn middle_clicks_open_search_pages_as_the_references_do() {
    let h = Harness::new(1600.0);
    let middle = |tag: Option<&str>, mods: Mods, name: &str| {
        let before = h.bound.pages.borrow().page_count();
        h.press(tag, MIDDLE, mods);
        h.differ(
            name,
            &h.launched(before),
            &launches(&h.recorded(name)["launched"]),
        );
        // (the press selected as any press does; the page it opened is
        // closed again, so this is the list it was pressed in)
        h.step(name);
    };
    h.key(ESCAPE, NONE);
    middle(
        Some("series:spaced name"),
        NONE,
        "middle click series:spaced name",
    );
    h.select(&["plain tag", "solo"]);
    middle(
        Some("solo"),
        NONE,
        "middle click on a selected row with two selected",
    );
    middle(
        Some("plain tag"),
        SHIFT,
        "shift middle click with two selected",
    );
    h.key(ESCAPE, NONE);
    middle(
        Some("zeta:last"),
        SHIFT,
        "shift middle click with one selected",
    );
    middle(None, NONE, "middle click below the rows");
    h.finish();
}

// leaf: selection-tags-double-click
#[test]
fn double_clicks_put_the_selection_in_the_search_as_the_references_do() {
    let h = Harness::new(1600.0);
    let double = |tag: &str, mods: Mods, name: &str| {
        input::double_click(h.window(), h.at(Some(tag)), mods);
        h.differ(name, &h.searched(), &h.recorded(name)["searched"]);
        h.differ(name, &json!(h.selected()), &h.recorded(name)["selected"]);
        h.restore();
    };
    h.key(ESCAPE, NONE);
    double(
        "series:spaced name",
        NONE,
        "double click series:spaced name",
    );
    h.key(ESCAPE, NONE);
    double("zeta:last", CTRL, "ctrl double click zeta:last");
    h.select(&["plain tag", "solo"]);
    double("plain tag", CTRL, "ctrl double click on a selected row");
    h.select(&["plain tag", "solo"]);
    double("solo", SHIFT, "shift double click with two selected");
    h.select(&["box:anchor"]);
    double("box:anchor", NONE, "double click on the searched tag");
    h.finish();
}

/// The recorded menu cases: their names and the tags they select (none for
/// "all", selected by ctrl+a).
fn cases(h: &Harness) -> Vec<(String, Option<Vec<String>>)> {
    h.fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let name = m["case"].as_str().unwrap().to_owned();
            let tags = (name != "all").then(|| strings(&m["selected"]));
            (name, tags)
        })
        .collect()
}

impl Harness {
    /// Select `tags` (all, for none) and right-press the last of them (or
    /// below the rows, for no tags), as the recorder does; the menu opens.
    pub fn open_menu(&self, tags: Option<&[String]>) {
        match tags {
            None => {
                self.click(Some("box:anchor"), NONE);
                self.key("a", CTRL);
                self.press(Some("zeta:last"), RIGHT, NONE);
            }
            Some(tags) => {
                let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
                // (the recorder ctrl+clicks them in the list's order)
                self.select(&tags);
                let mut wanted: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
                let mut shown = self.selected();
                wanted.sort();
                shown.sort();
                assert_eq!(shown, wanted, "selecting for the menu");
                self.press(tags.last().copied(), RIGHT, NONE);
            }
        }
        assert!(
            self.ui.get_tag_menu_panes().row_count() > 0,
            "no menu for {tags:?}"
        );
    }

    pub fn menu(&self) -> Json {
        super::active_predicate_menu::tree(&self.ui, 0)
    }

    pub fn choose(&self, path: &[&str]) {
        for (pane, label) in path.iter().enumerate() {
            let panes = self.ui.get_tag_menu_panes();
            let lines = panes.row_data(pane).map(|p| p.lines);
            let line = lines
                .as_ref()
                .and_then(|lines| lines.iter().position(|row| row.label == *label));
            let Some(line) = line else {
                let shown: Vec<Vec<String>> = panes
                    .iter()
                    .map(|p| p.lines.iter().map(|l| l.label.to_string()).collect())
                    .collect();
                panic!("no {label} in pane {pane} of {path:?}: {shown:?}");
            };
            self.ui.invoke_tag_menu_clicked(
                i32::try_from(pane).unwrap(),
                i32::try_from(line).unwrap(),
                300.0,
                20.0,
                10.0,
            );
        }
        widgets::settle();
    }

    pub fn close_menu(&self) {
        self.ui.invoke_tag_menu_dismissed();
    }

    /// The submenu titled `title` in a recorded or shown menu.
    pub fn submenu(menu: &Json, title: &str) -> Json {
        menu.as_array()
            .unwrap()
            .iter()
            .find_map(|e| e.get(title).cloned())
            .unwrap_or(Json::Null)
    }

    /// The question the tag menu asks, as the recorder writes it, answered
    /// yes.
    pub fn answer(&self) -> Json {
        if self.ui.get_tag_menu_question().is_empty() {
            return json!([]);
        }
        let asked = json!([{
            "message": self.ui.get_tag_menu_question().to_string(),
            "title": if self.ui.get_tag_menu_question_title().is_empty() {
                "Are you sure?".to_owned()
            } else {
                self.ui.get_tag_menu_question_title().to_string()
            },
            "yes": self.ui.get_tag_menu_yes_label().to_string(),
            "no": self.ui.get_tag_menu_no_label().to_string(),
        }]);
        self.ui.invoke_tag_menu_answered(true);
        widgets::settle();
        asked
    }

    pub fn tags(&self) -> Vec<String> {
        self.page()
            .borrow()
            .tag_list()
            .iter()
            .map(|(t, _)| t.clone())
            .collect()
    }
}

// leaf: selection-tags-menu-copy
#[test]
fn the_menu_and_its_copy_entries_are_the_references() {
    let h = Harness::new(1600.0);
    for (case, tags) in cases(&h) {
        let recorded = h.fixture["menus"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["case"] == case.as_str())
            .unwrap()
            .clone();
        h.open_menu(tags.as_deref());
        h.differ(
            &format!("{case} selected"),
            &json!(h.selected()),
            &recorded["selected"],
        );
        // (the whole menu, every group in its place)
        h.differ(&format!("{case} menu"), &h.menu(), &recorded["menu"]);
        h.close_menu();
        for copy in recorded["copies"].as_array().unwrap() {
            let label = copy["label"].as_str().unwrap();
            h.open_menu(tags.as_deref());
            let n = h.copies.borrow().len();
            h.choose(&["copy", label]);
            h.differ(
                &format!("{case} copy {label}"),
                &json!(h.copies.borrow()[n..].to_vec()),
                &copy["payload"],
            );
        }
    }
    h.finish();
}

// leaf: selection-tags-menu-open
#[test]
fn the_menus_open_entries_open_the_references_pages() {
    let h = Harness::new(1600.0);
    for (case, tags) in cases(&h) {
        let recorded = h.fixture["menus"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["case"] == case.as_str())
            .unwrap()
            .clone();
        for launch in recorded["launches"].as_array().unwrap() {
            let label = launch["label"].as_str().unwrap();
            h.open_menu(tags.as_deref());
            let before = h.bound.pages.borrow().page_count();
            h.choose(&["open", label]);
            // (one page each opens in set order in the reference)
            let sort = |pages: Json| {
                let mut pages = pages.as_array().unwrap().clone();
                pages.sort_by_key(|p| p["page_name"].to_string());
                json!(pages)
            };
            h.differ(
                &format!("{case} {label}"),
                &sort(h.launched(before)),
                &sort(launches(&launch["launched"])),
            );
        }
    }
    h.finish();
}

// leaf: selection-tags-menu-search
#[test]
fn the_menus_search_entries_change_the_search_as_the_references_do() {
    let h = Harness::new(1600.0);
    for search in h.fixture["searches"].as_array().unwrap() {
        let tags = strings(&search["selected"]);
        let label = search["label"].as_str().unwrap();
        let what = format!("{} {label}", search["case"]);
        h.open_menu(Some(&tags));
        h.choose(&["search", label]);
        widgets::settle();
        // "start an OR predicate" opens the OR editor with the selection,
        // applied as it opened
        let editor = h
            .bound
            .search_or
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let or_editor = match editor {
            Some(or) => {
                let mut terms: Vec<String> =
                    or.get_predicates().iter().map(|p| p.to_string()).collect();
                terms.sort();
                or.invoke_apply();
                widgets::settle();
                json!([[terms.join(" OR ")]])
            }
            None => json!([]),
        };
        h.differ(&what, &or_editor, &search["or_editor"]);
        h.differ(&what, &h.searched(), &search["searched"]);
        h.restore();
    }
    h.finish();
}

// leaf: selection-tags-menu-select
#[test]
fn the_menus_select_entries_select_the_references_files() {
    let h = Harness::new(1600.0);
    for select in h.fixture["selects"].as_array().unwrap() {
        let tags = strings(&select["selected"]);
        let label = select["label"].as_str().unwrap();
        h.open_menu(Some(&tags));
        h.choose(&["select", label]);
        let files = h.page().borrow().selected_files();
        let mut hashes: Vec<String> = h
            .store
            .read(|conn| hydrus_store::master::hashes(conn, &files))
            .unwrap()
            .values()
            .map(ToString::to_string)
            .collect();
        hashes.sort();
        h.differ(label, &json!(hashes), &select["files"]);
        // (the box now lists the selected files' tags)
        h.differ(label, &json!(h.tags()), &select["tags_after"]);
        h.restore();
    }
    h.finish();
}

// leaf: selection-tags-menu-siblings-parents
// leaf: selection-tags-menu-favourites-maintenance
#[test]
fn the_menus_shared_tag_entries_act_as_the_references_do() {
    let h = Harness::new(1600.0);
    let favourites = || -> Json {
        json!(
            h.store
                .read(hydrus_store::settings::get::<hydrus_store::settings::FavouriteTags>)
                .unwrap()
                .0
        )
    };
    // a tag's stored count in all files, and setting it wrong, to see which
    // tags "regenerate tag display" repairs
    let snapshot = h.store.snapshot();
    let my_tags = snapshot.services.by_name("my tags").unwrap().id;
    let all_files = snapshot
        .services
        .of_type(hydrus_core::ServiceType::CombinedFile)
        .next()
        .unwrap()
        .id;
    let tag_id = |tag: &str| {
        let tag = hydrus_core::Tag::new(tag).unwrap();
        h.store
            .read(move |conn| hydrus_store::master::tag_id(conn, &tag))
            .unwrap()
            .unwrap()
    };
    let count = |tag: &str| {
        let id = tag_id(tag);
        h.store
            .read(move |conn| hydrus_store::counts::count(conn, my_tags, all_files, id, false))
            .unwrap()
            .current
    };
    let corrupt = |tag: &str| {
        let id = tag_id(tag);
        h.store
            .write(move |ctx| {
                let tables = hydrus_store::schema::MappingTables::new(my_tags);
                for table in [tables.counts, tables.display_counts] {
                    ctx.conn().execute(
                        &format!("UPDATE {table} SET current=77 WHERE tag_id=?1 AND domain_id=?2"),
                        rusqlite::params![id, all_files],
                    )?;
                }
                Ok(())
            })
            .unwrap();
    };
    for trigger in h.fixture["triggers"].as_array().unwrap() {
        let tags = strings(&trigger["selected"]);
        let path = strings(&trigger["path"]);
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        let what = format!("{tags:?} {path:?}");
        let regenerated: Vec<(String, u64)> = trigger["writes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|w| strings(&w["tags"]))
            .map(|tag| {
                let real = count(&tag);
                (tag, real)
            })
            .collect();
        let untouched = "zeta:last";
        if !regenerated.is_empty() {
            corrupt(untouched);
            for (tag, _) in &regenerated {
                corrupt(tag);
            }
        }
        let n = h.copies.borrow().len();
        h.open_menu(Some(&tags));
        h.choose(&path);
        h.differ(
            &format!("{what} questions"),
            &h.answer(),
            &trigger["questions"],
        );
        for (tag, real) in &regenerated {
            assert_eq!(count(tag), *real, "{what}: {tag} not regenerated");
        }
        if !regenerated.is_empty() {
            assert_eq!(count(untouched), 77, "{what}: {untouched} regenerated");
        }
        let dialog = h
            .bound
            .tag_menu_relationships
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let dialogs = match &dialog {
            Some(child) => {
                let title = child.get_composed_title();
                let title = title
                    .strip_suffix(child.get_title_suffix().as_str())
                    .unwrap_or(&title)
                    .to_owned();
                let mut given: Vec<String> = child
                    .get_left_tags()
                    .iter()
                    .map(|r| r.text.to_string())
                    .collect();
                given.sort();
                json!([{ "title": title, "tags": given }])
            }
            None => json!([]),
        };
        h.differ(&format!("{what} dialogs"), &dialogs, &trigger["dialogs"]);
        if let Some(child) = dialog {
            child.invoke_cancel();
        }
        h.differ(
            &format!("{what} copied"),
            &json!(h.copies.borrow()[n..].to_vec()),
            &trigger["copied"],
        );
        h.differ(
            &format!("{what} favourites"),
            &favourites(),
            &trigger["favourite_tags"],
        );
    }
    h.finish();
}

// leaf: selection-tags-menu-hide
#[test]
fn the_menus_hide_entries_ask_then_hide_as_the_references_do() {
    let h = Harness::new(1600.0);
    for hide in h.fixture["hides"].as_array().unwrap() {
        let tags = strings(&hide["selected"]);
        let path = strings(&hide["path"]);
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        h.open_menu(Some(&tags));
        h.differ(
            &format!("{path:?} menu"),
            &Harness::submenu(&h.menu(), "hide"),
            &hide["menu"],
        );
        h.choose(&path);
        h.differ(
            &format!("{path:?} question"),
            &h.answer(),
            &hide["questions"],
        );
        h.differ(
            &format!("{path:?} tags"),
            &json!(h.tags()),
            &hide["tags_after"],
        );
    }
    h.finish();
}

// leaf: selection-tags-menu-tag-display
#[test]
fn the_advanced_menus_tag_display_switch_shows_the_references_tags() {
    let h = Harness::new(1600.0);
    h.store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let advanced = &h.fixture["advanced"];
    let tags = strings(&advanced["selected"]);
    h.open_menu(Some(&tags));
    h.differ("advanced menu", &h.menu(), &advanced["menu"]);
    h.close_menu();
    for switch in advanced["switches"].as_array().unwrap() {
        let label = switch["label"].as_str().unwrap();
        h.open_menu(Some(&tags));
        h.choose(&["tag display (experimental)", label]);
        widgets::settle();
        h.differ(label, &json!(h.rows()), &switch["drawn"]);
    }
    h.finish();
}
