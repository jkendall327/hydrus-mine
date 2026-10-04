use hydrus_core::pages::{Page, PageContent, PageKey, Session};
pub fn search(name: &str) -> Page {
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
fn notebook(name: &str, children: Vec<Page>) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Pages(children),
    }
}
pub fn source() -> Session {
    Session {
        name: hydrus_store::sessions::LAST_SESSION.into(),
        pages: vec![
            notebook(
                "alpha",
                vec![
                    search("one"),
                    notebook("inner", vec![search("beta"), search("gamma")]),
                ],
            ),
            search("omega"),
        ],
    }
}
pub fn entries(pages: &[Page]) -> Vec<(PageKey, String)> {
    let mut result = Vec::new();
    for page in pages {
        result.push((page.key, page.name.clone()));
        if let PageContent::Pages(children) = &page.content {
            result.extend(entries(children));
        }
    }
    result
}
pub fn key(session: &Session, name: &str) -> PageKey {
    entries(&session.pages)
        .iter()
        .find(|(_, n)| n == name)
        .unwrap()
        .0
}
pub fn action(label: &str) -> Option<i32> {
    match label {
        "expand all" | "expand all again" => Some(8),
        "collapse all" => Some(7),
        "Up" => Some(0),
        "Down" => Some(1),
        "Left" | "collapse inner" => Some(2),
        "Right" | "expand alpha retains inner collapse" => Some(3),
        "Home" => Some(4),
        "End" => Some(5),
        "Return" | "activate beta" => Some(6),
        _ => None,
    }
}
