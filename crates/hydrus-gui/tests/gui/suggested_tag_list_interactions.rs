//! The suggested-tag lists' own mouse, keyboard and right-click menus (the
//! reference's `ListBoxTagsStrings`), replayed step by step from
//! `oracle/record_suggested_tag_list_interactions.py` through the real
//! Manage Tags window: selection by click, ctrl, shift and keys, ctrl+a,
//! ctrl+c and ctrl+shift+c, the sibling and parent decorators, the menu's
//! entries and what its copy, open, favourites and decorator entries do,
//! middle-click searches, and activation by enter and double-click.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::options_gui_support::basic_store;
use hydrus_core::search::predicate::Predicate;
use hydrus_core::{HashId, Sha256, Tag};
use hydrus_gui::{MainWindow, ManageTagsWindow, Pages, SearchPage, bind};
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;
use hydrus_store::settings::{FavouriteTags, TagAutocompleteTabs, TagSuggestionSettings};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

const DOWN: &str = "\u{F701}";
const UP: &str = "\u{F700}";
const HOME: &str = "\u{F729}";
const END: &str = "\u{F72B}";
const PAGE_UP: &str = "\u{F72C}";
const PAGE_DOWN: &str = "\u{F72D}";
const ESCAPE: &str = "\u{1b}";
const RETURN: &str = "\n";
const DELETE: &str = "\u{7f}";

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn seeded(
    f: &Value,
) -> (
    Vec<tempfile::TempDir>,
    std::sync::Arc<hydrus_store::Store>,
    Vec<HashId>,
) {
    let (dirs, store) = basic_store();
    let hashes: Vec<Sha256> = strings(&f["files"])
        .iter()
        .map(|h| h.parse().unwrap())
        .collect();
    let ids = store
        .read(|conn| hydrus_store::master::hash_ids(conn, &hashes))
        .unwrap();
    let files: Vec<HashId> = hashes.iter().map(|h| ids[h]).collect();
    let mine = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    for row in f["mapped"].as_array().unwrap() {
        let tag = Tag::new(row[0].as_str().unwrap()).unwrap();
        let with: Vec<HashId> = row[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        let service = mine.id;
        store
            .write_content(move |w| {
                let id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    id,
                    &with,
                )
            })
            .unwrap();
    }
    for (kind, key) in [
        (RelationKind::Siblings, "siblings"),
        (RelationKind::Parents, "parents"),
    ] {
        let updates = f[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| RelationUpdate {
                service: mine.id,
                left: Tag::new(p[0].as_str().unwrap()).unwrap(),
                right: Tag::new(p[1].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            })
            .collect();
        tag_relations::apply(&store, kind, updates).unwrap();
    }
    let recent = strings(&f["recent_pushed"]);
    let most_used = strings(&f["most_used"]);
    let (service, hex) = (mine.id, mine.key.to_hex());
    store
        .write(move |ctx| {
            ctx.conn().execute("DELETE FROM recent_tags", [])?;
            for (i, tag) in recent.iter().enumerate() {
                let id = hydrus_store::master::intern_tag(ctx.conn(), &Tag::new(tag).unwrap())?;
                ctx.conn().execute(
                    "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                    rusqlite::params![service, id, 1_000_000 + i64::try_from(i).unwrap()],
                )?;
            }
            let mut tabs: TagAutocompleteTabs = hydrus_store::settings::get(ctx.conn())?;
            tabs.most_used.insert(hex, most_used);
            hydrus_store::settings::set(ctx.conn(), &tabs)?;
            hydrus_store::settings::set(ctx.conn(), &FavouriteTags::default())?;
            hydrus_store::settings::set(
                ctx.conn(),
                &TagSuggestionSettings {
                    columns: true,
                    ..TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    (dirs, store, files)
}

/// A menu as the recording has it: a line per entry ("---" a separator),
/// a submenu's entries indented under it.
fn recorded_tree(tree: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree.as_array().unwrap() {
        if e["separator"].as_bool().unwrap() {
            out.push("---".into());
            continue;
        }
        out.push(e["label"].as_str().unwrap().to_owned());
        if !e["children"].is_null() {
            for line in recorded_tree(&e["children"]) {
                out.push(format!("  {line}"));
            }
        }
    }
    out
}

struct Window {
    tags: Vec<String>,
    m: ManageTagsWindow,
}

impl Window {
    /// The most-used list's drawn rows, a tag's parent rows after it.
    fn rows(&self, panel: i32) -> Vec<(String, bool)> {
        let rows = match panel {
            0 => self.m.get_most_used_rows(),
            1 => self.m.get_recent_tag_rows(),
            _ => self.m.get_related_tag_rows(),
        };
        rows.iter()
            .map(|r| (r.cells.row_data(0).unwrap().to_string(), r.selected))
            .collect()
    }
    fn drawn(&self) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = Vec::new();
        for (text, _) in self.rows(0) {
            match out.last_mut() {
                Some(group) if text.starts_with("    ") => group.push(text),
                _ => out.push(vec![text]),
            }
        }
        out
    }
    fn tag_of(&self, text: &str) -> String {
        self.tags
            .iter()
            .find(|t| text == t.as_str() || text.starts_with(&format!("{t} ")))
            .unwrap_or_else(|| panic!("no tag drawn as {text:?}"))
            .clone()
    }
    fn selected(&self) -> Vec<String> {
        self.rows(0)
            .into_iter()
            .filter(|(text, selected)| *selected && !text.starts_with("    "))
            .map(|(text, _)| self.tag_of(&text))
            .collect()
    }
    fn listed(&self) -> Vec<String> {
        self.rows(0)
            .into_iter()
            .filter(|(text, _)| !text.starts_with("    "))
            .map(|(text, _)| self.tag_of(&text))
            .collect()
    }
    fn row(&self, tag: &str) -> i32 {
        let at = self
            .rows(0)
            .iter()
            .position(|(text, _)| !text.starts_with("    ") && self.tag_of(text) == tag)
            .unwrap_or_else(|| panic!("{tag} is not listed"));
        i32::try_from(at).unwrap()
    }
    /// A row past the last: the empty space below the rows.
    fn below(&self) -> i32 {
        i32::try_from(self.rows(0).len() + 2).unwrap()
    }
    fn click(&self, tag: &str, ctrl: bool, shift: bool) {
        self.m.invoke_side_clicked(0, self.row(tag), ctrl, shift);
    }
    fn key(&self, text: &str, ctrl: bool, shift: bool) -> bool {
        self.m.invoke_side_key(0, text.into(), ctrl, shift, 20)
    }
    fn panes(&self) -> Vec<Vec<(String, i32)>> {
        let panes = self.m.get_tag_menu_panes();
        (0..panes.row_count())
            .map(|i| {
                let lines = panes.row_data(i).unwrap().lines;
                (0..lines.row_count())
                    .map(|j| {
                        let line = lines.row_data(j).unwrap();
                        (line.label.to_string(), line.kind)
                    })
                    .collect()
            })
            .collect()
    }
    /// The open menu, each submenu opened in turn to read it.
    fn tree(&self) -> Vec<String> {
        let mut out = Vec::new();
        let top = self.panes().remove(0);
        for (line, (label, kind)) in top.iter().enumerate() {
            match kind {
                2 => out.push("---".into()),
                3 => {
                    out.push(label.clone());
                    self.m.invoke_tag_menu_clicked(
                        0,
                        i32::try_from(line).unwrap(),
                        100.0,
                        50.0,
                        10.0,
                    );
                    for (item, kind) in self.panes().remove(1) {
                        out.push(format!("  {}", if kind == 2 { "---".into() } else { item }));
                    }
                }
                _ => out.push(label.clone()),
            }
        }
        out
    }
    /// Choose `path` (a top entry, or a submenu's entry) in the open menu.
    fn choose(&self, path: &[&str]) {
        let top = self.panes().remove(0);
        let at = top
            .iter()
            .position(|(label, _)| label == path[0])
            .unwrap_or_else(|| panic!("no {:?} in {top:?}", path[0]));
        self.m
            .invoke_tag_menu_clicked(0, i32::try_from(at).unwrap(), 100.0, 50.0, 10.0);
        if let [_, entry] = path {
            let sub = self.panes().remove(1);
            let at = sub
                .iter()
                .position(|(label, _)| label == entry)
                .unwrap_or_else(|| panic!("no {entry:?} in {sub:?}"));
            self.m
                .invoke_tag_menu_clicked(1, i32::try_from(at).unwrap(), 100.0, 50.0, 10.0);
        }
    }
    fn menu(&self, row: i32) {
        self.m.invoke_side_menu(0, row, 10.0, 10.0);
    }
}

fn settle() {
    for _ in 0..10 {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A launched page as the recording words it: its predicates (an OR as its
/// tags joined, sorted) and whether it is a duplicates page.
fn launched(predicates: &[Predicate], duplicate: bool) -> (BTreeSet<String>, bool) {
    let text = |p: &Predicate| match p {
        Predicate::Tag { tag, .. } => tag.as_str().to_owned(),
        Predicate::Or(inner) => {
            let mut tags: Vec<String> = inner
                .iter()
                .map(|p| match p {
                    Predicate::Tag { tag, .. } => tag.as_str().to_owned(),
                    other => format!("{other:?}"),
                })
                .collect();
            tags.sort();
            tags.join(" OR ")
        }
        other => format!("{other:?}"),
    };
    (predicates.iter().map(text).collect(), duplicate)
}

fn recorded_launches(v: &Value) -> Vec<(BTreeSet<String>, bool)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                strings(&l["predicates"]).into_iter().collect(),
                l["topic"] == "new_page_duplicates",
            )
        })
        .collect()
}

// leaf: suggested-tag-list-interactions
#[test]
fn suggestion_lists_select_copy_decorate_search_and_activate_as_the_recording() {
    let f = hydrus_testkit::fixture_json("suggested_tag_list_interactions.json");
    let (_dirs, store, files) = seeded(&f);
    let _windows = hydrus_gui::headless::init();
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().clone_from(text);
            }
        }
    });
    let mut search = SearchPage::new(store.clone());
    search.choose_location(hydrus_core::search::context::LocationContext::default());
    search.enter();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(search));
    ui.show().unwrap();
    let launches = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::write_tag_menu::install_search_launcher(Rc::new({
        let launches = launches.clone();
        move |_location, _context, predicates: Vec<Predicate>, duplicate| {
            launches.borrow_mut().push(launched(&predicates, duplicate));
        }
    }));
    bound.current.borrow().borrow_mut().select_files(&files);
    ui.invoke_manage_tags_selected();
    let m = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mine = m
        .get_service_names()
        .iter()
        .position(|n| n == "my tags")
        .unwrap();
    m.invoke_service_chosen(i32::try_from(mine).unwrap());
    settle();
    let w = Window {
        tags: strings(&f["tags"]),
        m: m.clone_strong(),
    };
    // the list, and its rows as drawn with their sibling and parent decorators
    assert_eq!(w.listed(), strings(&f["tags"]));
    let drawn: Vec<Vec<String>> = f["drawn"].as_array().unwrap().iter().map(strings).collect();
    assert_eq!(w.drawn(), drawn);

    let steps = f["steps"].as_array().unwrap();
    let mut at = 0;
    let mut next = |name: &str| {
        let step = &steps[at];
        at += 1;
        assert_eq!(step["step"], name, "the steps run in the recording's order");
        step.clone()
    };
    let check = |step: &Value| {
        settle();
        assert_eq!(w.selected(), strings(&step["selected"]), "{}", step["step"]);
    };
    // replays a menu step: its entries, then each copy and open entry chosen
    let menu_step = |step: &Value, row: i32| {
        w.menu(row);
        settle();
        check(step);
        let recorded = &step["menu"];
        assert_eq!(
            w.tree(),
            recorded_tree(&recorded["tree"]),
            "{}",
            step["step"]
        );
        w.m.invoke_tag_menu_dismissed();
        for copy in recorded["copies"].as_array().unwrap() {
            w.menu(row);
            w.choose(&["copy", copy["label"].as_str().unwrap()]);
            assert_eq!(
                *copied.borrow(),
                copy["payload"].as_str().unwrap(),
                "{}",
                step["step"]
            );
        }
        for launch in recorded["launches"].as_array().unwrap() {
            launches.borrow_mut().clear();
            w.menu(row);
            w.choose(&["open", launch["label"].as_str().unwrap()]);
            // (the reference opens a page per tag in its set's order)
            let mut native = launches.borrow().clone();
            let mut recorded = recorded_launches(&launch["launched"]);
            native.sort();
            recorded.sort();
            assert_eq!(native, recorded, "{}: {}", step["step"], launch["label"]);
        }
        check(step);
    };

    check(&next("initial"));
    w.click("alpha:first", false, false);
    check(&next("click alpha:first"));
    assert!(w.key(DOWN, false, false));
    check(&next("down"));
    assert!(w.key(DOWN, false, true));
    check(&next("shift down"));
    w.click("plain tag", true, false);
    check(&next("ctrl click plain tag adds it"));
    w.click("parity:漢字", true, false);
    check(&next("ctrl click parity takes it away"));
    w.click("old:sibling", false, false);
    check(&next("plain click on a selected row keeps the selection"));
    assert!(w.key("c", true, false));
    let step = next("ctrl c");
    check(&step);
    assert_eq!(*copied.borrow(), step["copied"].as_str().unwrap());
    menu_step(
        &next("menu on a selected row (two selected)"),
        w.row("old:sibling"),
    );
    w.click("alpha:first", true, false);
    check(&next("ctrl click alpha:first adds it"));
    assert!(w.key("c", true, true));
    let step = next("ctrl shift c includes parents");
    check(&step);
    assert_eq!(*copied.borrow(), step["copied"].as_str().unwrap());
    menu_step(
        &next("menu (three selected, one with a parent)"),
        w.row("plain tag"),
    );
    w.click("series:spaced name", false, false);
    check(&next("plain click on an unselected row selects only it"));
    menu_step(
        &next("menu (one selected, with namespace and spaces)"),
        w.row("series:spaced name"),
    );
    assert!(w.key(ESCAPE, false, false));
    check(&next("escape"));
    assert!(
        !w.key(ESCAPE, false, false),
        "escape with nothing selected goes on to the window"
    );
    assert!(w.key("c", true, false));
    let step = next("ctrl c with none selected");
    check(&step);
    assert_eq!(*copied.borrow(), step["copied"].as_str().unwrap());
    assert!(w.key("a", true, false));
    check(&next("ctrl a"));
    menu_step(&next("menu with all selected"), w.row("zeta:last"));
    assert!(w.key(ESCAPE, false, false));
    check(&next("escape again"));
    menu_step(
        &next("menu on alpha:first (unselected, so it is selected first)"),
        w.row("alpha:first"),
    );
    menu_step(&next("menu on old:sibling"), w.row("old:sibling"));
    menu_step(&next("menu on parity:漢字"), w.row("parity:漢字"));
    // the decorator entries change what is drawn, for this list only
    for label in [
        "collapse parent rows",
        "expand parent rows",
        "hide parent decorators",
        "show parent decorators",
        "hide sibling decorators",
        "show sibling decorators",
    ] {
        let step = next(&format!("decorator {label}"));
        w.click("alpha:first", false, false);
        w.menu(w.row("alpha:first"));
        let top: Vec<String> = w
            .panes()
            .remove(0)
            .into_iter()
            .filter(|(_, kind)| *kind != 2)
            .map(|(label, _)| label)
            .take(3)
            .collect();
        assert_eq!(top, strings(&step["offered"]), "{label}");
        w.choose(&[label]);
        settle();
        let drawn: Vec<Vec<String>> = step["drawn"]
            .as_array()
            .unwrap()
            .iter()
            .map(strings)
            .collect();
        assert_eq!(w.drawn(), drawn, "{label}");
        check(&step);
    }
    assert!(
        w.rows(1).iter().all(|(text, _)| !text.contains("parents)")),
        "another list keeps its own decorators"
    );
    menu_step(&next("menu on empty space"), w.below());
    w.click("alpha:first", false, false);
    check(&next("click alpha:first again"));
    for (name, key, ctrl, shift) in [
        ("up wraps to the end", UP, false, false),
        ("down wraps to the start", DOWN, false, false),
        ("end", END, false, false),
        ("home", HOME, false, false),
        ("ctrl n", "n", true, false),
        ("ctrl p", "p", true, false),
        ("page down", PAGE_DOWN, false, false),
        ("page up", PAGE_UP, false, false),
        ("ctrl down", DOWN, true, false),
        ("shift down after ctrl down", DOWN, false, true),
    ] {
        assert!(w.key(key, ctrl, shift), "{name}");
        check(&next(name));
    }
    w.click("zeta:last", false, true);
    check(&next("shift click zeta:last"));
    w.click("series:spaced name", true, true);
    check(&next("ctrl shift click series"));
    w.m.invoke_side_clicked(0, w.below(), false, false);
    check(&next("click empty space"));
    // a middle click hits its row and searches for the selection
    launches.borrow_mut().clear();
    w.m.invoke_side_middle(0, w.row("plain tag"), false, false);
    let step = next("middle click plain tag");
    check(&step);
    assert_eq!(*launches.borrow(), recorded_launches(&step["launched"]));
    w.click("series:spaced name", true, false);
    launches.borrow_mut().clear();
    w.m.invoke_side_middle(0, w.row("plain tag"), false, true);
    let step = next("shift middle click with two selected");
    check(&step);
    assert_eq!(*launches.borrow(), recorded_launches(&step["launched"]));
    // enter adds the selection, which leaves the list; the row it was at is hit
    w.click("zeta:last", false, false);
    assert!(w.key(RETURN, false, false));
    let step = next("return");
    check(&step);
    assert_eq!(w.listed(), strings(&step["tags"]));
    let staged = |tag: &str| w.m.get_tags().iter().any(|r| r.text.starts_with(tag));
    for activation in step["activated"].as_array().unwrap() {
        assert_eq!(activation["only_add"], true);
        for tag in strings(&activation["tags"]) {
            assert!(staged(&tag), "{tag} is staged");
        }
    }
    // a double-click's press hits the row, and the second activates it
    let row = w.row("parity:漢字");
    w.m.invoke_side_clicked(0, row, false, false);
    w.m.invoke_side_activated(0, row);
    let step = next("double click");
    check(&step);
    assert_eq!(w.listed(), strings(&step["tags"]));
    for tag in strings(&step["activated"][0]["tags"]) {
        assert!(staged(&tag), "{tag} is staged");
    }
    assert!(w.key(DELETE, false, false));
    let step = next("delete does nothing");
    check(&step);
    assert_eq!(w.listed(), strings(&step["tags"]));
    // favourites: added at once, removed after a question
    let step = next("menu on plain tag");
    w.menu(w.row("plain tag"));
    settle();
    check(&step);
    assert_eq!(w.tree(), recorded_tree(&step["menu"]["tree"]));
    w.m.invoke_tag_menu_dismissed();
    w.menu(w.row("plain tag"));
    w.choose(&["favourites", "add \"plain tag\" to favourites"]);
    let step = next("favourite added");
    check(&step);
    let favourites = || {
        store
            .read(hydrus_store::settings::get::<FavouriteTags>)
            .unwrap()
            .0
    };
    assert_eq!(favourites(), strings(&step["favourites"]));
    let step = next("menu on plain tag after favouriting");
    w.menu(w.row("plain tag"));
    settle();
    assert_eq!(w.tree(), recorded_tree(&step["menu"]["tree"]));
    w.m.invoke_tag_menu_dismissed();
    w.menu(w.row("plain tag"));
    w.choose(&["favourites", "remove \"plain tag\" from favourites"]);
    let step = next("favourite removed");
    assert_eq!(
        w.m.get_tag_menu_question(),
        step["questions"][0].as_str().unwrap()
    );
    w.m.invoke_tag_menu_answered(true);
    check(&step);
    assert_eq!(favourites(), strings(&step["favourites"]));
    w.menu(w.row("plain tag"));
    w.choose(&[
        "favourites",
        "remove \"plain tag\" from most used for \"my tags\"",
    ]);
    let step = next("most used toggled from the menu");
    assert_eq!(
        w.m.get_tag_menu_question(),
        step["questions"][1].as_str().unwrap()
    );
    w.m.invoke_tag_menu_answered(true);
    settle();
    check(&step);
    let hex = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    let most_used: BTreeSet<String> = store
        .read(hydrus_store::settings::get::<TagAutocompleteTabs>)
        .unwrap()
        .most_used[&hex]
        .iter()
        .cloned()
        .collect();
    assert_eq!(most_used, strings(&step["most_used"]).into_iter().collect());
    assert_eq!(at, steps.len(), "every recorded step replayed");

    // the recent list is the same kind of list, with the same menu
    let other = &f["other_lists"]["recent"];
    let recent: Vec<String> = w.rows(1).into_iter().map(|(text, _)| text).collect();
    assert_eq!(recent, strings(&other["tags"]));
    m.invoke_side_clicked(1, 0, false, false);
    m.invoke_side_menu(1, 0, 10.0, 10.0);
    settle();
    assert_eq!(w.tree(), recorded_tree(&other["tree"]));
    m.invoke_tag_menu_dismissed();
    hydrus_gui::write_tag_menu::clear_search_launcher();
    m.invoke_cancel();
}
