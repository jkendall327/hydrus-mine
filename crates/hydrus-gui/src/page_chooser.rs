//! The "new page" chooser (the reference's `DialogPageChooser`): a 3×3 grid
//! of buttons, laid out as a number pad, through a few menus (file search,
//! download, special) to the kind of page to open.

use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_store::Store;

/// What a button does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Menu(Menu),
    Page(NewPage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Home,
    FileSearch,
    Download,
    Special,
}

impl Menu {
    fn name(self) -> &'static str {
        match self {
            Menu::Home => "home",
            Menu::FileSearch => "file search",
            Menu::Download => "download",
            Menu::Special => "special",
        }
    }
}

/// The page chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewPage {
    /// A file search page on this file domain.
    Search {
        domain: ServiceKey,
        name: String,
    },
    Urls,
    Watcher,
    Gallery,
    SimpleDownloader,
    /// An empty page of pages.
    Pages,
    Duplicates,
}

impl NewPage {
    fn label(&self) -> &str {
        match self {
            NewPage::Search { name, .. } => name,
            NewPage::Urls => "urls",
            NewPage::Watcher => "watcher",
            NewPage::Gallery => "gallery",
            NewPage::SimpleDownloader => "simple downloader",
            NewPage::Pages => "page of pages",
            NewPage::Duplicates => "duplicates processing",
        }
    }
}

/// The chooser's state: which button (1 to 9, as on a number pad, 7 at the
/// top left) does what.
#[derive(Debug, Clone)]
pub struct PageChooser {
    /// File search pages it offers: the local file domains, then "all my
    /// files", then the trash (the reference's defaults).
    domains: Vec<(ServiceKey, String)>,
    /// Indexed by button number - 1.
    buttons: [Option<Entry>; 9],
}

impl PageChooser {
    pub fn new(store: &Store) -> Self {
        let snapshot = store.snapshot();
        let services = &snapshot.services;
        let trash = ServiceKey::new(builtin_keys::TRASH.to_vec());
        let mut domains: Vec<(ServiceKey, String)> = services
            .of_type(ServiceType::LocalFileDomain)
            .filter(|s| s.key != trash)
            .map(|s| (s.key.clone(), s.name.clone()))
            .collect();
        for key in [
            builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
            builtin_keys::TRASH,
        ] {
            if let Ok(service) = services.builtin(key) {
                domains.push((service.key.clone(), service.name.clone()));
            }
        }
        let mut chooser = Self {
            domains,
            buttons: Default::default(),
        };
        chooser.show(Menu::Home);
        chooser
    }

    fn show(&mut self, menu: Menu) {
        let entries: Vec<Entry> = match menu {
            Menu::Home => [Menu::FileSearch, Menu::Download, Menu::Special]
                .into_iter()
                .map(Entry::Menu)
                .collect(),
            Menu::FileSearch => self
                .domains
                .iter()
                .map(|(domain, name)| {
                    Entry::Page(NewPage::Search {
                        domain: domain.clone(),
                        name: name.clone(),
                    })
                })
                .collect(),
            Menu::Download => [
                NewPage::Urls,
                NewPage::Watcher,
                NewPage::Gallery,
                NewPage::SimpleDownloader,
            ]
            .into_iter()
            .map(Entry::Page)
            .collect(),
            Menu::Special => [NewPage::Pages, NewPage::Duplicates]
                .into_iter()
                .map(Entry::Page)
                .collect(),
        };
        // up to four in a cross, else the whole grid from the top left
        let order: &[usize] = if entries.len() <= 4 {
            &[8, 4, 6, 2]
        } else {
            &[7, 8, 9, 4, 5, 6, 1, 2, 3]
        };
        self.buttons = Default::default();
        for (entry, &button) in entries.into_iter().zip(order) {
            self.buttons[button - 1] = Some(entry);
        }
    }

    /// Each button's label, by button number - 1 (empty: not shown).
    pub fn labels(&self) -> [String; 9] {
        std::array::from_fn(|i| match &self.buttons[i] {
            Some(Entry::Menu(menu)) => menu.name().to_owned(),
            Some(Entry::Page(page)) => page.label().to_owned(),
            None => String::new(),
        })
    }

    /// Press button `number` (1 to 9): a menu shows its buttons; a page is
    /// the choice.
    pub fn press(&mut self, number: usize) -> Option<NewPage> {
        let entry = self.buttons.get(number.checked_sub(1)?)?.clone()?;
        match entry {
            Entry::Menu(menu) => {
                self.show(menu);
                None
            }
            Entry::Page(page) => Some(page),
        }
    }

    /// Enter presses the first button, scanning from the top left.
    pub fn enter(&mut self) -> Option<NewPage> {
        let first = [7, 8, 9, 4, 5, 6, 1, 2, 3]
            .into_iter()
            .find(|&n| self.buttons[n - 1].is_some())?;
        self.press(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_lead_to_pages_as_in_the_reference() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let mut chooser = PageChooser::new(&store);
        // up to four entries make a cross: 8, 4, 6, 2
        let labels = chooser.labels();
        assert_eq!(labels[7], "file search");
        assert_eq!(labels[3], "download");
        assert_eq!(labels[5], "special");
        assert!(labels[6].is_empty() && labels[0].is_empty());

        assert_eq!(chooser.press(5), None, "an empty button does nothing");
        assert_eq!(chooser.press(8), None, "a menu");
        assert_eq!(chooser.labels()[7], "my files");
        assert_eq!(chooser.labels()[3], "combined local file domains");
        assert_eq!(chooser.labels()[5], "trash");
        let Some(NewPage::Search { domain, .. }) = chooser.enter() else {
            panic!("enter chooses the first button");
        };
        assert_eq!(domain.as_bytes(), builtin_keys::MY_FILES);

        let mut chooser = PageChooser::new(&store);
        chooser.press(6);
        assert_eq!(chooser.press(4), Some(NewPage::Duplicates));
    }
}
