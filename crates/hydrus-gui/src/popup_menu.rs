//! A window's own popup menu (a right-click menu, a button's menu), drawn
//! by `main_menu.slint`'s `MenuPanes` over the window as the menu bar's
//! are, with the menu bar's open menus ([`OpenMenus`]) doing the work: a
//! window holds a [`Popup`], gives its `MenuPanes` the popup's model, and
//! passes the panes' callbacks on to it.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model as _, ModelRc, VecModel};

use crate::MenuPane;
use crate::main_menu::{Command, Entry, LineKind, OpenMenus};

/// What choosing a popup's entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen<A> {
    /// The window's action.
    Action(A),
    /// A label's text, to the clipboard.
    Copy(String),
}

/// A popup menu, and its window's panes.
pub struct Popup<A> {
    open: RefCell<OpenMenus>,
    actions: RefCell<Vec<A>>,
    model: Rc<VecModel<MenuPane>>,
}

impl<A: Clone> Popup<A> {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            open: RefCell::default(),
            actions: RefCell::default(),
            model: Rc::new(VecModel::default()),
        })
    }

    /// The panes, for the window's `MenuPanes`.
    pub fn model(&self) -> ModelRc<MenuPane> {
        ModelRc::from(self.model.clone())
    }

    /// Open the menu (as `main_menu::popup` made it) at `x`, `y`.
    pub fn open(&self, entries: Vec<Entry>, actions: Vec<A>, x: f32, y: f32) {
        *self.actions.borrow_mut() = actions;
        self.open.borrow_mut().open_popup(entries, x, y);
        self.show();
    }

    pub fn close(&self) {
        self.open.borrow_mut().close();
        self.show();
    }

    pub fn placed(&self, pane: i32, x: f32, y: f32, width: f32) {
        if let Ok(pane) = usize::try_from(pane) {
            self.open.borrow_mut().placed(pane, x, y, width);
        }
    }

    pub fn hover(&self, pane: i32, line: i32, right: f32, top: f32, left: f32) {
        if let (Ok(pane), Ok(line)) = (usize::try_from(pane), usize::try_from(line)) {
            self.open.borrow_mut().hover(pane, line, right, top, left);
            self.show();
        }
    }

    /// A line clicked: what it chose, if anything (the menu closes).
    pub fn click(
        &self,
        pane: i32,
        line: i32,
        right: f32,
        top: f32,
        left: f32,
    ) -> Option<Chosen<A>> {
        let (Ok(pane), Ok(line)) = (usize::try_from(pane), usize::try_from(line)) else {
            return None;
        };
        let command = self.open.borrow_mut().click(pane, line, right, top, left);
        self.show();
        match command? {
            Command::Popup(i) => self.actions.borrow().get(i).cloned().map(Chosen::Action),
            Command::Copy(text) => Some(Chosen::Copy(text)),
            _ => None,
        }
    }

    /// Show the open menus in the panes' model, changing only what
    /// changed (lines made anew under the pointer would lose its press).
    fn show(&self) {
        let view = self.open.borrow().view();
        let model = &self.model;
        for (p, pane) in view.iter().enumerate() {
            let lines: Vec<crate::MenuLine> = pane
                .lines
                .iter()
                .map(|(label, kind, usable, checked)| crate::MenuLine {
                    label: label.as_str().into(),
                    kind: match kind {
                        LineKind::Item => 0,
                        LineKind::Check => 1,
                        LineKind::Separator => 2,
                        LineKind::Menu => 3,
                    },
                    usable: *usable,
                    checked: *checked,
                })
                .collect();
            let same_lines = model.row_data(p).is_some_and(|row| {
                row.lines.row_count() == lines.len()
                    && lines
                        .iter()
                        .enumerate()
                        .all(|(i, l)| row.lines.row_data(i).as_ref() == Some(l))
            });
            let row = MenuPane {
                x: pane.x,
                y: pane.y,
                left: pane.left,
                lines: if same_lines {
                    model.row_data(p).map(|r| r.lines).unwrap_or_default()
                } else {
                    ModelRc::new(VecModel::from(lines))
                },
                current: pane
                    .current
                    .and_then(|c| i32::try_from(c).ok())
                    .unwrap_or(-1),
            };
            if p < model.row_count() {
                if model.row_data(p).as_ref() != Some(&row) {
                    model.set_row_data(p, row);
                }
            } else {
                model.push(row);
            }
        }
        while model.row_count() > view.len() {
            model.remove(model.row_count() - 1);
        }
    }
}
