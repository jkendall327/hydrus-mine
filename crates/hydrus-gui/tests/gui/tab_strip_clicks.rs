//! The page tabs' clicks, replayed through the real main window with real
//! pointer events against the reference's notebook
//! (`oracle/fixtures/tab_strip_interactions.json`, from
//! `oracle/record_tab_strip_interactions.py`, which drove the real client's
//! tab bar with Qt mouse events): a double-click on a tab renames its page
//! ("Enter the new name.", the page's name to start with), a double-click on
//! a tab row's empty space or a middle click there chooses a new page for
//! that row's notebook, and the right button let go over a tab or over empty
//! space shows that notebook's tab menu (its entries as recorded).

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use serde_json::{Value as Json, json};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition, Model as _};

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{Store, sessions, settings};

use crate::common::widgets;

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: hydrus_search::FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

/// The recording's pages: alpha, a notebook "nested" holding "inner one"
/// and "inner two", and omega; "inner one" shown.
pub(crate) fn recorded_pages(store: &Arc<Store>) -> Session {
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![
            search("alpha"),
            Page {
                key: PageKey::random(),
                name: "nested".into(),
                content: PageContent::Pages(vec![search("inner one"), search("inner two")]),
            },
            search("omega"),
        ],
    };
    let saved = session.clone();
    let PageContent::Pages(inner) = &session.pages[1].content else {
        unreachable!()
    };
    let shown = inner[0].key;
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &saved, 1)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&shown))?;
            // (the recording's tabs show no file counts)
            settings::set(
                ctx.conn(),
                &hydrus_core::pages::PageNameSettings {
                    file_counts: hydrus_core::pages::FileCountDisplay::None,
                    ..hydrus_core::pages::PageNameSettings::default()
                },
            )
        })
        .unwrap();
    session
}

/// The pages as the recorder writes them: names, a notebook's children
/// under it.
pub(crate) fn tree(pages: &[Page]) -> Json {
    json!(
        pages
            .iter()
            .map(|page| match &page.content {
                PageContent::Pages(children) =>
                    json!({ "name": page.name, "children": tree(children) }),
                _ => json!({ "name": page.name }),
            })
            .collect::<Vec<_>>()
    )
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Where each tab (by page key) and each tab row (by "row <depth>") is, as
/// the window reports it.
type Rects = Rc<RefCell<HashMap<String, Rect>>>;

fn rects(ui: &MainWindow) -> Rects {
    let rects = Rects::default();
    ui.on_tab_geometry_measured({
        let rects = rects.clone();
        move |key, _, depth, _, x, y, w, h| {
            let name = if key.is_empty() {
                format!("row {depth}")
            } else {
                key.to_string()
            };
            rects.borrow_mut().insert(name, Rect { x, y, w, h });
        }
    });
    rects
}

/// Draw a few frames, so the tabs' timers report where they now are.
pub(crate) fn settle(native: &MinimalSoftwareWindow) {
    for _ in 0..12 {
        headless::render(native, 900, 600);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

pub(crate) fn press(
    native: &MinimalSoftwareWindow,
    at: LogicalPosition,
    button: PointerEventButton,
) {
    native.dispatch_event(WindowEvent::PointerMoved { position: at });
    native.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button,
    });
}

pub(crate) fn release(
    native: &MinimalSoftwareWindow,
    at: LogicalPosition,
    button: PointerEventButton,
) {
    native.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button,
    });
}

pub(crate) fn click(
    native: &MinimalSoftwareWindow,
    at: LogicalPosition,
    button: PointerEventButton,
) {
    press(native, at, button);
    release(native, at, button);
}

/// Two clicks in quick succession, the window drawn once between them (well
/// inside the double-click interval).
pub(crate) fn double_click(
    native: &MinimalSoftwareWindow,
    at: LogicalPosition,
    button: PointerEventButton,
) {
    click(native, at, button);
    headless::render(native, 900, 600);
    click(native, at, button);
}

pub(crate) struct Strip {
    pub(crate) ui: MainWindow,
    pub(crate) native: Rc<MinimalSoftwareWindow>,
    pub(crate) bound: hydrus_gui::Bound,
    rects: Rects,
}

impl Strip {
    pub(crate) fn open(store: &Arc<Store>) -> Self {
        let windows = headless::init();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let rects = rects(&ui);
        ui.show().unwrap();
        let native = windows.get(0).unwrap();
        native.dispatch_event(WindowEvent::WindowActiveChanged(true));
        settle(&native);
        Self {
            ui,
            native,
            bound,
            rects,
        }
    }

    pub(crate) fn key(&self, name: &str) -> PageKey {
        self.bound
            .pages
            .borrow()
            .session()
            .all_pages()
            .into_iter()
            .find(|page| page.name == name)
            .unwrap_or_else(|| panic!("no page {name:?}"))
            .key
    }

    /// The middle of the tab of the page `name`.
    fn tab(&self, name: &str) -> LogicalPosition {
        settle(&self.native);
        let rect = self.rects.borrow()[&self.key(name).to_hex()];
        LogicalPosition::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
    }

    /// Empty space at the end of the tab row `depth`, past its last tab
    /// (and short of the row's scroll arrows' place).
    fn space(&self, depth: usize) -> LogicalPosition {
        settle(&self.native);
        let row = self.rects.borrow()[&format!("row {depth}")];
        LogicalPosition::new(row.x + row.w - 60.0, row.y + row.h / 2.0)
    }

    /// Empty space in the last 40 pixels of the tab row `depth` (where the
    /// row's scroll arrows go when its tabs overflow, and the row's own
    /// touch area takes the pointer).
    fn row_end(&self, depth: usize) -> LogicalPosition {
        settle(&self.native);
        let row = self.rects.borrow()[&format!("row {depth}")];
        LogicalPosition::new(row.x + row.w - 20.0, row.y + row.h / 2.0)
    }

    pub(crate) fn state(&self) -> Json {
        let pages = self.bound.pages.borrow();
        json!({ "pages": tree(&pages.session().pages), "shown": pages.shown().name })
    }

    /// The open popup menu as the recorder writes it, each submenu opened by
    /// pointing at it.
    pub(crate) fn menu(&self, pane: usize) -> Json {
        let lines = self.ui.get_menu_panes().row_data(pane).unwrap().lines;
        let window = self.ui.window();
        let mut out = Vec::new();
        for line in lines.iter() {
            let label = line.label.to_string();
            out.push(match line.kind {
                2 => json!("---"),
                3 => {
                    // (the open menus are drawn over everything: the last
                    // match)
                    let at = widgets::position(window, &label, widgets::count(window, &label) - 1);
                    self.native
                        .dispatch_event(WindowEvent::PointerMoved { position: at });
                    settle(&self.native);
                    json!({ "menu": label, "entries": self.menu(pane + 1) })
                }
                _ => json!(label),
            });
        }
        json!(out)
    }

    /// Close the open menu as a user does: a click away from it.
    pub(crate) fn dismiss(&self) {
        if self.ui.get_menu_panes().row_count() > 0 {
            click(
                &self.native,
                LogicalPosition::new(880.0, 590.0),
                PointerEventButton::Left,
            );
            settle(&self.native);
        }
        assert_eq!(self.ui.get_menu_panes().row_count(), 0, "menu closed");
    }

    /// The rename window's question, answered with `answer` (typed and
    /// applied) or cancelled.
    pub(crate) fn rename(&self, answer: Option<&str>) -> Json {
        let Some(dialog) = self
            .bound
            .tab_name_dialog
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
        else {
            return json!([]);
        };
        let asked = json!([{
            "message": dialog.get_message().to_string(),
            "default": dialog.get_text().to_string(),
            "answer": answer,
        }]);
        let window = dialog.window();
        widgets::lay_out(window, 400.0, 200.0);
        match answer {
            Some(name) => {
                dialog.set_text(name.into());
                widgets::click(window, "apply");
            }
            None => widgets::click(window, "cancel"),
        }
        assert!(self.bound.tab_name_dialog.borrow().is_none(), "answered");
        asked
    }

    /// The notebook the open page chooser is for, as the recorder names it
    /// (none if it is not open); it is then cancelled with Escape.
    fn chooser(&self) -> Option<String> {
        if self.ui.get_chooser_labels().row_count() == 0 {
            return None;
        }
        let notebook = self.bound.pages.borrow().new_page_notebook();
        let name = notebook.map_or_else(
            || "top page notebook".to_string(),
            |key| {
                self.bound
                    .pages
                    .borrow()
                    .session()
                    .all_pages()
                    .into_iter()
                    .find(|page| page.key == key)
                    .unwrap()
                    .name
                    .clone()
            },
        );
        widgets::key(self.ui.window(), slint::platform::Key::Escape);
        settle(&self.native);
        assert_eq!(self.ui.get_chooser_labels().row_count(), 0);
        Some(name)
    }
}

fn step<'a>(recorded: &'a Json, action: &str) -> &'a Json {
    recorded["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["action"] == action)
        .unwrap_or_else(|| panic!("no recorded step {action:?}"))
}

/// The recorded step's menu (the first captured), its pages, the page
/// shown and the chooser opened, compared with the window's.
fn compare(strip: &Strip, recorded: &Json, action: &str, menu: Option<&Json>, chose: Option<&str>) {
    let step = step(recorded, action);
    let expected = step["menus"]
        .as_array()
        .unwrap()
        .first()
        .map(|m| &m["entries"]);
    assert_eq!(menu, expected, "{action}: menu");
    // (the chooser for the same notebook, the page going where the
    // notebook's option puts it: no insertion place in each recording)
    let chosen = step["chosen"].as_array().unwrap();
    assert!(chosen.iter().all(|c| c["insertion_index"].is_null()));
    assert_eq!(
        chose,
        chosen.first().map(|c| c["notebook"].as_str().unwrap()),
        "{action}: chooser"
    );
    assert_eq!(
        strip.state(),
        json!({ "pages": step["pages"], "shown": step["shown"] }),
        "{action}"
    );
}

// leaf: interaction-tab-double-click-rename
// leaf: interaction-tab-strip-clicks
#[test]
fn tab_strip_clicks_replay_the_references_notebook_with_real_pointer_events() {
    use PointerEventButton::{Left, Middle, Right};
    let recorded = hydrus_testkit::fixture_json("tab_strip_interactions.json");
    let (_dirs, store) = crate::subscriptions::store();
    recorded_pages(&store);
    let strip = Strip::open(&store);
    let native = strip.native.clone();
    let menu = |strip: &Strip| {
        settle(&strip.native);
        if strip.ui.get_menu_panes().row_count() == 0 {
            return None;
        }
        let menu = strip.menu(0);
        strip.dismiss();
        Some(menu)
    };

    // double-clicks on a tab: rename it, the first click showing it
    for (action, name, answer) in [
        (
            "left double-click tab alpha, rename",
            "alpha",
            Some("renamed alpha"),
        ),
        ("left double-click tab omega, cancel", "omega", None),
        (
            "left double-click tab nested (a page of pages)",
            "nested",
            Some("renamed nested"),
        ),
    ] {
        double_click(&native, strip.tab(name), Left);
        settle(&native);
        let asked = strip.rename(answer);
        assert_eq!(asked, step(&recorded, action)["texts"], "{action}");
        compare(&strip, &recorded, action, None, None);
    }
    // a nested notebook's tab: its row is the second
    let action = "left double-click inner tab inner two";
    double_click(&native, strip.tab("inner two"), Left);
    settle(&native);
    assert_eq!(
        strip.rename(Some("renamed two")),
        step(&recorded, action)["texts"]
    );
    compare(&strip, &recorded, action, None, None);

    // one click on a tab shows it and asks nothing
    let action = "left click tab omega";
    click(&native, strip.tab("omega"), Left);
    compare(&strip, &recorded, action, None, None);
    assert!(strip.bound.tab_name_dialog.borrow().is_none());
    // a press elsewhere between two clicks on a tab: no double-click
    let omega = strip.tab("omega");
    let top = strip.space(0);
    click(&native, omega, Left);
    headless::render(&native, 900, 600);
    click(&native, top, Left);
    headless::render(&native, 900, 600);
    click(&native, omega, Left);
    settle(&native);
    assert!(strip.bound.tab_name_dialog.borrow().is_none());
    compare(&strip, &recorded, action, None, None);

    // empty space: one left click does nothing, a double-click or a single
    // middle click opens the chooser
    click(&native, top, Left);
    compare(
        &strip,
        &recorded,
        "left click empty space",
        menu(&strip).as_ref(),
        strip.chooser().as_deref(),
    );
    double_click(&native, top, Left);
    compare(
        &strip,
        &recorded,
        "left double-click empty space",
        None,
        strip.chooser().as_deref(),
    );
    click(&native, top, Middle);
    compare(
        &strip,
        &recorded,
        "middle click empty space",
        None,
        strip.chooser().as_deref(),
    );
    // (a middle double-click: the first opens the chooser, which the second
    // lands on)
    double_click(&native, top, Middle);
    compare(
        &strip,
        &recorded,
        "middle double-click empty space",
        None,
        strip.chooser().as_deref(),
    );

    // the right button: nothing until it is let go, then the menu
    let alpha = strip.tab("renamed alpha");
    press(&native, alpha, Right);
    let action = "right press tab alpha (no release)";
    compare(&strip, &recorded, action, menu(&strip).as_ref(), None);
    release(&native, alpha, Right);
    compare(
        &strip,
        &recorded,
        "right release tab alpha",
        menu(&strip).as_ref(),
        None,
    );
    click(&native, strip.tab("renamed nested"), Right);
    compare(
        &strip,
        &recorded,
        "right click tab nested",
        menu(&strip).as_ref(),
        None,
    );
    click(&native, top, Right);
    compare(
        &strip,
        &recorded,
        "right click empty space",
        menu(&strip).as_ref(),
        None,
    );
    // the same at the row's end, where the row's own touch area is: nothing
    // on the press, the menu or the chooser when let go
    let end = strip.row_end(0);
    press(&native, end, Right);
    settle(&native);
    assert_eq!(
        strip.ui.get_menu_panes().row_count(),
        0,
        "no menu on a press"
    );
    release(&native, end, Right);
    compare(
        &strip,
        &recorded,
        "right click empty space",
        menu(&strip).as_ref(),
        None,
    );
    press(&native, end, Middle);
    settle(&native);
    assert_eq!(
        strip.ui.get_chooser_labels().row_count(),
        0,
        "no chooser on a press"
    );
    release(&native, end, Middle);
    compare(
        &strip,
        &recorded,
        "middle click empty space",
        None,
        strip.chooser().as_deref(),
    );

    // the nested notebook's row: its own menus and chooser
    click(&native, strip.tab("renamed nested"), Left);
    settle(&native);
    let action = "right click inner tab inner one";
    click(&native, strip.tab("inner one"), Right);
    compare(&strip, &recorded, action, menu(&strip).as_ref(), None);
    let inner = strip.space(1);
    click(&native, inner, Right);
    compare(
        &strip,
        &recorded,
        "right click inner empty space",
        menu(&strip).as_ref(),
        None,
    );
    double_click(&native, inner, Left);
    let chose = strip.chooser();
    compare(
        &strip,
        &recorded,
        "left double-click inner empty space",
        None,
        chose.as_deref(),
    );

    // a middle double-click on a tab: the first closes it, the second lands
    // on the empty space it leaves and chooses a new page
    let omega = strip.tab("omega");
    click(&native, omega, Middle);
    settle(&native);
    click(&native, omega, Middle);
    let chose = strip.chooser();
    compare(
        &strip,
        &recorded,
        "middle double-click tab omega",
        None,
        chose.as_deref(),
    );
}
