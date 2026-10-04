//! Notebook tab actions and stable ordering, matching `PagesNotebook`.

use crate::main_menu::{Command, Entry};

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

/// The working portion of the reference's tab context menu, scoped to the
/// clicked notebook rather than the deepest selected leaf.
pub fn menu(depth: usize, index: usize, count: usize) -> Vec<Entry> {
    if index >= count || count < 2 {
        return Vec::new();
    }
    let moves = [
        ("to left end", Move::First),
        ("left", Move::Left),
        ("right", Move::Right),
        ("to right end", Move::Last),
    ]
    .into_iter()
    .filter(|&(_, movement)| destination(index, count, movement).is_some())
    .map(|(label, movement)| Entry::Item {
        label: label.into(),
        enabled: true,
        command: Some(Command::MoveTab {
            depth,
            index,
            movement,
        }),
    })
    .collect();
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
    vec![
        Entry::Menu {
            label: "move page".into(),
            entries: moves,
            enabled: true,
        },
        Entry::Menu {
            label: "sort pages".into(),
            entries: sorts,
            enabled: true,
        },
    ]
}
