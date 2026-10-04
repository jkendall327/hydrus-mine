//! Staged, atomic downloader package imports and reference duplicate decisions.
//! Reference-only editor data is kept separately from executable definitions.
use crate::favourites::non_dupe_name;
use hydrus_core::{
    pages::PageKey,
    url::{AnyGug, UrlClassSettings, UrlClasses, UrlType},
};
pub use hydrus_downloader_exchange::{
    Definition, Native, decode_png, decode_text, encode_png, encode_text,
};
use hydrus_parse::Downloaders;
use hydrus_store::{
    Store, StoreError,
    settings::{self, Setting},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Original reference tuples, keyed by stable native definition identity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Auxiliary(pub BTreeMap<String, serde_json::Value>);
impl Setting for Auxiliary {
    const KEY: &'static str = "downloader_exchange_auxiliary";
}
impl Auxiliary {
    /// Export current native fields, retaining imported reference editor data.
    pub fn definition(&self, native: Native) -> Definition {
        let original = key(&native).and_then(|key| self.0.get(&key).cloned());
        Definition { native, original }
    }
    /// Merge auxiliary entries without deleting another editor's imports.
    pub fn save(&self, conn: &rusqlite::Connection) -> hydrus_store::Result<()> {
        let mut current: Self = settings::get(conn)?;
        current.0.extend(self.0.clone());
        settings::set(conn, &current)
    }
    /// Retain the original tuple after the native identity has been remapped.
    pub fn retain(&mut self, definition: &Definition) {
        if let (Some(key), Some(original)) = (key(&definition.native), &definition.original) {
            self.0.insert(key, original.clone());
        }
    }
}
fn key(native: &Native) -> Option<String> {
    match native {
        Native::Class(c) => Some(format!("class:{}", hex::encode(&c.key))),
        Native::Gug(g) => Some(format!("gug:{}", g.key())),
        Native::Page(p) => Some(format!("page:{}", p.key)),
        _ => None,
    }
}

/// A full package draft. Cancel simply drops this value.
#[derive(Debug, Clone)]
pub struct Draft {
    pub classes: UrlClassSettings,
    pub downloaders: Downloaders,
    pub auxiliary: Auxiliary,
    original_classes: UrlClassSettings,
    original_downloaders: Downloaders,
    original_auxiliary: Auxiliary,
}
/// The review displayed before staging an import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Review {
    pub added: Vec<String>,
    pub duplicates: usize,
}
impl Review {
    /// Concrete definitions and duplicate count for the confirmation panel.
    pub fn text(&self) -> String {
        format!(
            "{} definition(s) to add; {} exact duplicate(s) skipped.\n\n{}\n\nChanges are saved only when you apply the owning editor.",
            self.added.len(),
            self.duplicates,
            self.added.join("\n")
        )
    }
}
impl Draft {
    /// Read a consistent native settings snapshot without mutations.
    pub fn load(store: &Store) -> hydrus_store::Result<Self> {
        store.read(|conn| {
            Ok(Self::new(
                settings::get(conn)?,
                settings::get(conn)?,
                settings::get(conn)?,
            ))
        })
    }
    /// Construct an isolated draft from known settings.
    pub fn new(classes: UrlClassSettings, downloaders: Downloaders, auxiliary: Auxiliary) -> Self {
        Self {
            original_classes: classes.clone(),
            original_downloaders: downloaders.clone(),
            original_auxiliary: auxiliary.clone(),
            classes,
            downloaders,
            auxiliary,
        }
    }
    /// Decode and stage a package completely; failure keeps this draft intact.
    pub fn import(&mut self, definitions: Vec<Definition>) -> Result<Review, String> {
        if definitions.iter().any(|d| {
            !matches!(
                d.native,
                Native::Class(_) | Native::Gug(_) | Native::Page(_)
            )
        }) {
            return Err("Downloader bundles accept URL classes, generators and page parsers. Import formulas or content nodes from their own editor.".into());
        }
        let mut next = self.clone();
        let mut review = Review::default();
        let mut seen = BTreeSet::new();
        let mut remap = BTreeMap::new();
        let mut duplicate_classes = Vec::new();
        let mut new_classes = Vec::new();
        let mut new_parsers = Vec::new();
        // Single generators first so nested references map to their final keys
        // and names, including exact duplicates already present in the client.
        let mut definitions = definitions;
        definitions.sort_by_key(|d| matches!(d.native, Native::Gug(AnyGug::Nested(_))));
        for mut definition in definitions {
            let category = match definition.native {
                Native::Class(_) => "URL Class",
                Native::Page(_) => "Parser",
                _ => "GUG",
            };
            let name_seen = (category.to_owned(), definition.name().to_owned());
            if seen.contains(&name_seen) {
                continue;
            }
            match &mut definition.native {
                Native::Class(c) => {
                    let duplicate = next
                        .classes
                        .url_classes
                        .iter()
                        .find(|old| {
                            let mut old = (*old).clone();
                            old.name.clone_from(&c.name);
                            old.key.clone_from(&c.key);
                            old.example_url.clone_from(&c.example_url);
                            old == **c
                        })
                        .cloned();
                    if duplicate.is_some() {
                        duplicate_classes.push((**c).clone());
                        review.duplicates += 1;
                        continue;
                    }
                    // The reference keeps matching old classes and renames
                    // them; direct existing links remain on their old key.
                    for old in &mut next.classes.url_classes {
                        if !old.name.starts_with("zzz - renamed due to auto-import - ")
                            && c.test(&old.example_url, next.classes.collapse_leading_slashes)
                                .is_ok()
                            && old
                                .test(&c.example_url, next.classes.collapse_leading_slashes)
                                .is_ok()
                        {
                            old.name = format!("zzz - renamed due to auto-import - {}", old.name);
                        }
                    }
                    c.name = non_dupe_name(&c.name, &|name| {
                        next.classes.url_classes.iter().any(|old| old.name == name)
                    });
                    c.key = PageKey::random().0.to_vec();
                    new_classes.push((**c).clone());
                    next.classes.url_classes.push((**c).clone());
                }
                Native::Gug(g) => {
                    let incoming = g.key().to_owned();
                    if let AnyGug::Nested(n) = g {
                        for (key, name) in &mut n.gugs {
                            if let Some((k, n)) = remap.get(key) {
                                key.clone_from(k);
                                name.clone_from(n);
                            }
                        }
                    }
                    let duplicate = next
                        .downloaders
                        .gugs
                        .gugs
                        .iter()
                        .find(|old| {
                            let mut old = (*old).clone();
                            set_gug_identity(&mut old, g.key(), g.name());
                            old == *g
                        })
                        .cloned();
                    if let Some(old) = duplicate {
                        remap.insert(incoming, (old.key().to_owned(), old.name().to_owned()));
                        review.duplicates += 1;
                        continue;
                    }
                    let name = non_dupe_name(g.name(), &|name| {
                        next.downloaders
                            .gugs
                            .gugs
                            .iter()
                            .any(|old| old.name() == name)
                    });
                    let key = PageKey::random().to_hex();
                    set_gug_identity(g, &key, &name);
                    remap.insert(incoming, (key.clone(), name));
                    next.downloaders.gugs.keys_to_display.push(key);
                    next.downloaders.gugs.gugs.push(g.clone());
                }
                Native::Page(p) => {
                    let duplicate = next.downloaders.parsers.iter_mut().find(|old| {
                        let mut old = (*old).clone();
                        old.name.clone_from(&p.name);
                        old.key.clone_from(&p.key);
                        old.example_urls.clone_from(&p.example_urls);
                        semantic_eq(&old, p)
                    });
                    if let Some(old) = duplicate {
                        old.example_urls.extend(p.example_urls.iter().cloned());
                        old.example_urls.sort();
                        old.example_urls.dedup();
                        review.duplicates += 1;
                        continue;
                    }
                    p.name = non_dupe_name(&p.name, &|name| {
                        next.downloaders.parsers.iter().any(|old| old.name == name)
                    });
                    p.key = PageKey::random().to_hex();
                    new_parsers.push(p.clone());
                    next.downloaders.parsers.push(p.clone());
                }
                _ => unreachable!("definition kinds were checked before staging"),
            }
            seen.insert(name_seen);
            review
                .added
                .push(format!("{category}: {}", definition.name()));
            next.auxiliary.retain(&definition);
        }
        // Newly imported parsers replace links on duplicate classes; loose
        // new classes then use any existing parser with matching examples.
        let registry = UrlClasses::new(next.classes.clone());
        new_classes.extend(
            duplicate_classes
                .iter()
                .filter_map(|c| registry.class_for(&c.example_url).cloned()),
        );
        link(&mut next.classes, &new_classes, &new_parsers, true);
        let all = next.classes.url_classes.clone();
        let parsers = next.downloaders.parsers.clone();
        link(&mut next.classes, &all, &parsers, false);
        next.classes.parser_keys = next
            .downloaders
            .parsers
            .iter()
            .map(|p| p.key.clone())
            .collect();
        *self = next;
        Ok(review)
    }
    /// Save definitions, links and auxiliary data in one transaction, rejecting
    /// a stale snapshot so concurrent editors cannot lose each other's changes.
    pub fn save(&self, store: &Store) -> hydrus_store::Result<()> {
        let draft = self.clone();
        store.write_and_refresh(move|ctx|{let conn=ctx.conn();let classes:UrlClassSettings=settings::get(conn)?;let downloaders:Downloaders=settings::get(conn)?;let auxiliary:Auxiliary=settings::get(conn)?;if classes!=draft.original_classes||downloaders!=draft.original_downloaders||auxiliary!=draft.original_auxiliary{return Err(StoreError::Invalid("Downloader definitions changed in another editor. Reopen the import before applying.".into()));}settings::set(conn,&draft.classes)?;settings::set(conn,&draft.downloaders)?;settings::set(conn,&draft.auxiliary)})
    }
    /// Export every definition as a reference bundle for the package window.
    pub fn definitions(&self) -> Vec<Definition> {
        self.classes
            .url_classes
            .iter()
            .cloned()
            .map(|c| Native::Class(Box::new(c)))
            .chain(self.downloaders.gugs.gugs.iter().cloned().map(Native::Gug))
            .chain(self.downloaders.parsers.iter().cloned().map(Native::Page))
            .map(|n| self.auxiliary.definition(n))
            .collect()
    }
}
fn set_gug_identity(g: &mut AnyGug, key: &str, name: &str) {
    match g {
        AnyGug::Single(g) => {
            g.key = key.into();
            g.name = name.into();
        }
        AnyGug::Nested(g) => {
            g.key = key.into();
            g.name = name.into();
        }
    }
}
fn link(
    classes: &mut UrlClassSettings,
    targets: &[hydrus_core::url::UrlClass],
    parsers: &[hydrus_parse::content::PageParser],
    overwrite: bool,
) {
    let mut ordered = parsers.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));
    let registry = UrlClasses::new(UrlClassSettings {
        // Reference API-pair sources cannot own parsers. Remove them before
        // matching so an eligible target may recognise the same human URL.
        url_classes: targets
            .iter()
            .filter(|c| {
                !c.uses_api_url()
                    || !c
                        .api_url(&c.example_url, classes.collapse_leading_slashes)
                        .is_ok_and(|api| {
                            targets.iter().any(|other| {
                                other.key != c.key
                                    && other.matches(&api, classes.collapse_leading_slashes)
                            })
                        })
            })
            .cloned()
            .collect(),
        ..classes.clone()
    });
    let mut linked = BTreeSet::new();
    for parser in ordered {
        for example in &parser.example_urls {
            if let Some(c) = registry.class_for(example) {
                let key = hex::encode(&c.key);
                if !matches!(
                    c.url_type,
                    UrlType::Post | UrlType::Gallery | UrlType::Watchable
                ) || !linked.insert(key.clone())
                {
                    continue;
                }
                if let Some((_, old)) = classes.parser_links.iter_mut().find(|(k, _)| k == &key) {
                    if overwrite {
                        *old = Some(parser.key.clone());
                    }
                } else {
                    classes.parser_links.push((key, Some(parser.key.clone())));
                }
            }
        }
    }
}

fn semantic_eq<T: Serialize>(a: &T, b: &T) -> bool {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("reference_auxiliary");
                for value in map.values_mut() {
                    strip(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    strip(value);
                }
            }
            _ => (),
        }
    }
    let (Ok(mut a), Ok(mut b)) = (serde_json::to_value(a), serde_json::to_value(b)) else {
        return false;
    };
    strip(&mut a);
    strip(&mut b);
    a == b
}
