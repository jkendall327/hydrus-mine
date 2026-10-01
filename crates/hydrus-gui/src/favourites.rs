//! The favourite searches menu, laid out as the reference's
//! (`_FavouriteSearchesMenu` over `GetNestedFoldersToNames`): a folder name
//! with `/` in it nests; at each level its searches come first, by name,
//! then its subfolders, by name.

use std::collections::BTreeMap;

use hydrus_core::pages::FavouriteSearch;

/// A row of the menu: a folder's name, or a search to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavouriteRow {
    pub label: String,
    /// How deep in folders it is (0: the top).
    pub depth: usize,
    /// The search it loads (its index among the favourites); none for a
    /// folder.
    pub search: Option<usize>,
}

#[derive(Default)]
struct Folder<'a> {
    /// (name, index) of the searches here.
    searches: Vec<(&'a str, usize)>,
    folders: BTreeMap<&'a str, Folder<'a>>,
}

/// The menu's rows for `favourites`.
pub fn favourite_rows(favourites: &[FavouriteSearch]) -> Vec<FavouriteRow> {
    let mut top = Folder::default();
    for (index, favourite) in favourites.iter().enumerate() {
        let mut folder = &mut top;
        for part in favourite
            .folder
            .as_deref()
            .unwrap_or_default()
            .split('/')
            .filter(|p| !p.is_empty())
        {
            folder = folder.folders.entry(part).or_default();
        }
        folder.searches.push((&favourite.name, index));
    }
    let mut rows = Vec::new();
    lay_out(&mut top, 0, &mut rows);
    rows
}

fn lay_out(folder: &mut Folder<'_>, depth: usize, rows: &mut Vec<FavouriteRow>) {
    // (a stable sort, as Python's, keeping stored order among equal names)
    folder.searches.sort_by_key(|&(name, _)| name);
    for &(name, index) in &folder.searches {
        rows.push(FavouriteRow {
            label: name.to_owned(),
            depth,
            search: Some(index),
        });
    }
    for (name, sub) in &mut folder.folders {
        rows.push(FavouriteRow {
            label: (*name).to_owned(),
            depth,
            search: None,
        });
        lay_out(sub, depth + 1, rows);
    }
}

#[cfg(test)]
mod tests {
    use hydrus_core::search::context::FileSearchContext;

    use super::*;

    fn favourite(folder: Option<&str>, name: &str) -> FavouriteSearch {
        FavouriteSearch {
            folder: folder.map(str::to_owned),
            name: name.to_owned(),
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
        }
    }

    #[test]
    fn folders_nest_after_the_searches_beside_them() {
        let favourites = [
            favourite(Some("art/sketches"), "pencil"),
            favourite(None, "zebra"),
            favourite(Some("art"), "paintings"),
            favourite(Some("/art/"), "colour"),
            favourite(Some("Beta"), "b"),
            favourite(None, "apple"),
        ];
        let rows: Vec<(String, usize, Option<usize>)> = favourite_rows(&favourites)
            .into_iter()
            .map(|r| (r.label, r.depth, r.search))
            .collect();
        let row = |label: &str, depth, search| (label.to_owned(), depth, search);
        assert_eq!(
            rows,
            [
                row("apple", 0, Some(5)),
                row("zebra", 0, Some(1)),
                // (code point order, as Python sorts: capitals first)
                row("Beta", 0, None),
                row("b", 1, Some(4)),
                row("art", 0, None),
                row("colour", 1, Some(3)),
                row("paintings", 1, Some(2)),
                row("sketches", 1, None),
                row("pencil", 2, Some(0)),
            ]
        );
    }
}
