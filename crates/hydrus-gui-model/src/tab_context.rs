//! Notebook tab actions and stable ordering, matching `PagesNotebook`.

use crate::main_menu::{Command, Entry};

pub use hydrus_store::sessions::NotebookSettings;

/// What a notebook's sibling pages are sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// File count, then importer total and completed counts.
    Files,
    /// The sum of the sizes of all files under each page.
    Size,
    /// Case-sensitive name, with file count descending as a secondary key.
    Name,
}

/// The facts used to sort one page, including nested notebooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub name: String,
    pub files: usize,
    pub progress: (usize, usize),
    pub size: u64,
}

/// Stable sibling indices in the reference's order. Names deliberately use
/// ordinary lexical order rather than the human order used by other lists.
pub fn order(pages: &[Summary], by: Sort, ascending: bool) -> Vec<usize> {
    let mut indices: Vec<_> = (0..pages.len()).collect();
    let counts = |i: usize| (pages[i].files, pages[i].progress.1, pages[i].progress.0);
    if by == Sort::Name {
        indices.sort_by_key(|&i| std::cmp::Reverse(counts(i)));
    }
    indices.sort_by(|&a, &b| {
        let comparison = match by {
            Sort::Files => counts(a).cmp(&counts(b)),
            Sort::Size => pages[a].size.cmp(&pages[b].size),
            Sort::Name => pages[a].name.cmp(&pages[b].name),
        };
        if ascending {
            comparison
        } else {
            comparison.reverse()
        }
    });
    indices
}

/// A move within the clicked tab's notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    First,
    Left,
    Right,
    Last,
}

/// The destination of a move, absent at the notebook's boundaries.
pub fn destination(index: usize, count: usize, movement: Move) -> Option<usize> {
    if index >= count {
        return None;
    }
    let target = match movement {
        Move::First => 0,
        Move::Left => index.checked_sub(1)?,
        Move::Right => index.checked_add(1)?,
        Move::Last => count.checked_sub(1)?,
    };
    (target < count && target != index).then_some(target)
}

/// Which clicked siblings are moved into a fresh notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Send {
    This,
    FromHere,
    Right,
}

/// Frozen sibling indices for a send-down action.
pub fn send_indices(index: usize, count: usize, scope: Send) -> Vec<usize> {
    if index >= count {
        return Vec::new();
    }
    match scope {
        Send::This => vec![index],
        Send::FromHere => (index..count).collect(),
        Send::Right => (index + 1..count).collect(),
    }
}

/// Which siblings a bulk-close action targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    Other,
    Left,
    Right,
}

/// Sibling indices to close, in tab order; execution closes right to left.
pub fn close_indices(index: usize, count: usize, side: Close) -> Vec<usize> {
    if index >= count {
        return Vec::new();
    }
    (0..count)
        .filter(|&i| match side {
            Close::Other => i != index,
            Close::Left => i < index,
            Close::Right => i > index,
        })
        .collect()
}

/// The reference's description included in each bulk-close question.
pub fn close_description(side: Close) -> &'static str {
    match side {
        Close::Other => "other pages",
        Close::Left => "pages to the left",
        Close::Right => "pages to the right",
    }
}

/// Tab context commands scoped to the clicked row. Navigation uses the
/// selected tab's position, while moves and close groups use the clicked tab.
pub fn menu(depth: usize, index: usize, count: usize, selected: usize) -> Vec<Entry> {
    if index >= count {
        return Vec::new();
    }
    let item = |label: &str, command| Entry::Item {
        label: label.into(),
        enabled: true,
        command: Some(command),
    };
    let mut entries = vec![item("close page", Command::CloseTab { depth, index })];
    let close = |label: &str, side| item(label, Command::CloseTabs { depth, index, side });
    if count > 1 {
        if index == 0 || index == count - 1 {
            entries.push(close(
                if count == 2 {
                    "close other page"
                } else {
                    "close other pages"
                },
                Close::Other,
            ));
        } else {
            entries.push(Entry::Menu {
                label: "close".into(),
                entries: vec![
                    close("other pages", Close::Other),
                    close("pages to the left", Close::Left),
                    close("pages to the right", Close::Right),
                ],
                enabled: true,
            });
        }
        entries.push(Entry::Separator);
        let possible = |at: usize, movement| match movement {
            Move::First => at > 1,
            Move::Left => at > 0,
            Move::Right => at + 1 < count,
            Move::Last => at + 2 < count,
        };
        let navigation = [
            ("first page", Move::First),
            ("page to the left", Move::Left),
            ("page to the right", Move::Right),
            ("last page", Move::Last),
        ]
        .into_iter()
        .filter(|&(_, movement)| possible(selected, movement))
        .map(|(label, movement)| item(label, Command::NavigateTabs { depth, movement }))
        .collect();
        entries.push(Entry::Menu {
            label: "select".into(),
            entries: navigation,
            enabled: true,
        });
        let moves = [
            ("to left end", Move::First),
            ("left", Move::Left),
            ("right", Move::Right),
            ("to right end", Move::Last),
        ]
        .into_iter()
        .filter(|&(_, movement)| possible(index, movement))
        .map(|(label, movement)| {
            item(
                label,
                Command::MoveTab {
                    depth,
                    index,
                    movement,
                },
            )
        })
        .collect();
        entries.push(Entry::Menu {
            label: "move page".into(),
            entries: moves,
            enabled: true,
        });
        entries.push(sort_menu(depth));
    }
    entries.push(item("rename page", Command::RenameTab { depth, index }));
    entries.push(send_menu(depth, index, count));
    entries
}

fn sort_menu(depth: usize) -> Entry {
    let sorts = [
        ("by most files first", Sort::Files, false),
        ("by fewest files first", Sort::Files, true),
        ("by largest total file size first", Sort::Size, false),
        ("by smallest total file size first", Sort::Size, true),
        ("by name a-z", Sort::Name, true),
        ("by name z-a", Sort::Name, false),
    ]
    .into_iter()
    .map(|(label, by, ascending)| Entry::Item {
        label: label.into(),
        enabled: true,
        command: Some(Command::SortTabs {
            depth,
            by,
            ascending,
        }),
    })
    .collect();
    Entry::Menu {
        label: "sort pages".into(),
        entries: sorts,
        enabled: true,
    }
}

fn send_menu(depth: usize, index: usize, count: usize) -> Entry {
    let item = |label: &str, scope| Entry::Item {
        label: label.into(),
        enabled: true,
        command: Some(Command::SendTabs {
            depth,
            index,
            scope,
        }),
    };
    let mut entries = vec![item("this page", Send::This)];
    if index + 1 < count {
        entries.push(item("pages from here to the right", Send::FromHere));
        entries.push(item("pages to the right", Send::Right));
    }
    Entry::Menu {
        label: "send down to a new page of pages".into(),
        entries,
        enabled: true,
    }
}
