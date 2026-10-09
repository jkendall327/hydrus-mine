//! Staged, atomic downloader package imports and reference duplicate decisions.
//! Reference-only editor data is kept separately from executable definitions.
use crate::favourites::non_dupe_name;
use hydrus_core::{
    pages::PageKey,
    url::{AnyGug, UrlClassSettings, UrlClasses, UrlType},
};
pub use hydrus_downloader_exchange::{
    Definition, Native, decode_png, decode_text, domain_metadata::DomainMetadata, encode_png,
    encode_text,
};
use hydrus_core::bandwidth::{Rule, Rules};
use hydrus_core::network::NetworkContext;
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::network::{Approval, CustomHeader};
use hydrus_parse::Downloaders;
use hydrus_parse::login::LoginScript;
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
        Native::Login(script) => Some(format!("login:{}", script.key)),
        _ => None,
    }
}

/// A full package draft. Cancel simply drops this value.
#[derive(Debug, Clone)]
pub struct Draft {
    pub classes: UrlClassSettings,
    pub downloaders: Downloaders,
    pub auxiliary: Auxiliary,
    pub login_scripts: Vec<LoginScript>,
    /// Each domain context's custom headers, as loaded.
    pub domain_headers: BTreeMap<String, Vec<CustomHeader>>,
    /// Every context's bandwidth rules, as loaded.
    pub bandwidth: Vec<(NetworkContext, Rules)>,
    /// Imported domain metadata waiting for save, sorted by domain.
    pub domain_metadata: Vec<DomainMetadata>,
    /// Domain metadata added to the export list by the domain prompt.
    pub domain_exports: Vec<DomainMetadata>,
    original_classes: UrlClassSettings,
    original_downloaders: Downloaders,
    original_auxiliary: Auxiliary,
    original_login_scripts: Vec<LoginScript>,
    original_network: (BTreeMap<String, Vec<CustomHeader>>, Vec<(NetworkContext, Rules)>),
}
/// The review displayed before staging an import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Review {
    pub added: Vec<String>,
    pub duplicates: usize,
    /// The reference shows the first eight new domain metadata objects in
    /// detail before its final confirmation.
    pub details: Vec<String>,
}
impl Review {
    /// Concrete definitions and duplicate count for the confirmation panel.
    pub fn text(&self) -> String {
        let details = if self.details.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", self.details.join("\n\n"))
        };
        format!(
            "{} definition(s) to add; {} exact duplicate(s) skipped.\n\n{}\n\n{details}Changes are saved only when you apply the owning editor.",
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
            let mut draft = Self::new(
                settings::get(conn)?,
                settings::get(conn)?,
                settings::get(conn)?,
            );
            draft.login_scripts = hydrus_store::logins::load(conn)?.scripts;
            draft
                .original_login_scripts
                .clone_from(&draft.login_scripts);
            draft.domain_headers = domain_headers(conn)?;
            draft.bandwidth = settings::get::<BandwidthSettings>(conn)?.rules;
            draft.original_network = (draft.domain_headers.clone(), draft.bandwidth.clone());
            Ok(draft)
        })
    }
    /// Construct an isolated draft from known settings.
    pub fn new(classes: UrlClassSettings, downloaders: Downloaders, auxiliary: Auxiliary) -> Self {
        Self {
            original_classes: classes.clone(),
            original_downloaders: downloaders.clone(),
            original_auxiliary: auxiliary.clone(),
            login_scripts: Vec::new(),
            original_login_scripts: Vec::new(),
            domain_headers: BTreeMap::new(),
            bandwidth: Vec::new(),
            domain_metadata: Vec::new(),
            domain_exports: Vec::new(),
            original_network: (BTreeMap::new(), Vec::new()),
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
                Native::Class(_)
                    | Native::Gug(_)
                    | Native::Page(_)
                    | Native::Login(_)
                    | Native::Domain(_)
            )
        }) {
            return Err("Downloader bundles accept URL classes, generators, page parsers, login scripts and domain metadata. Import formulas or content nodes from their own editor.".into());
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
                Native::Login(_) => "Login Script",
                Native::Domain(_) => "Domain Metadata",
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
                Native::Login(script) => {
                    if next.login_scripts.iter().any(|old| {
                        let mut old = old.clone();
                        old.key.clone_from(&script.key);
                        old.name.clone_from(&script.name);
                        semantic_eq(&old, script)
                    }) {
                        review.duplicates += 1;
                        continue;
                    }
                    // Mixed reference imports preserve names, regenerate keys and
                    // update existing domain links with the same script name.
                    script.key = PageKey::random().to_hex();
                    next.login_scripts.push(script.clone());
                }
                Native::Domain(metadata) => {
                    // Only the headers and rules the client lacks are kept.
                    let headers = metadata
                        .headers
                        .take()
                        .filter(|h| !next.has_exactly_these_headers(&metadata.domain, h));
                    let rules = metadata
                        .rules
                        .take()
                        .filter(|r| !next.has_exactly_these_rules(&metadata.domain, r));
                    if headers.is_none() && rules.is_none() {
                        review.duplicates += 1;
                        continue;
                    }
                    metadata.headers = headers;
                    metadata.rules = rules;
                    next.domain_metadata.push(metadata.clone());
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
        next.domain_metadata.sort_by(|a, b| a.domain.cmp(&b.domain));
        review.details = next
            .domain_metadata
            .iter()
            .take(8)
            .map(DomainMetadata::detailed_summary)
            .collect();
        *self = next;
        Ok(review)
    }
    /// Save definitions, links and auxiliary data in one transaction, rejecting
    /// a stale snapshot so concurrent editors cannot lose each other's changes.
    pub fn save(&self, store: &Store) -> hydrus_store::Result<()> {
        let draft = self.clone();
        store.write_and_refresh(move |ctx| {
            let conn = ctx.conn();
            let classes: UrlClassSettings = settings::get(conn)?;
            let downloaders: Downloaders = settings::get(conn)?;
            let auxiliary: Auxiliary = settings::get(conn)?;
            let mut logins = hydrus_store::logins::load(conn)?;
            let mut bandwidth: BandwidthSettings = settings::get(conn)?;
            if classes != draft.original_classes
                || downloaders != draft.original_downloaders
                || auxiliary != draft.original_auxiliary
                || logins.scripts != draft.original_login_scripts
                || (!draft.domain_metadata.is_empty()
                    && (domain_headers(conn)? != draft.original_network.0
                        || bandwidth.rules != draft.original_network.1))
            {
                return Err(StoreError::Invalid("Downloader definitions changed in another editor. Reopen the import before applying.".into()));
            }
            if draft.login_scripts != draft.original_login_scripts {
                logins.scripts.clone_from(&draft.login_scripts);
                logins.scripts.sort_by_key(|script| script.credentials.len());
                for script in &logins.scripts {
                    for example in &script.examples {
                        if example.domain.contains('.')
                            && let Some(domain) = logins.domains.get_mut(&example.domain)
                            && domain.script_name == script.name
                        {
                            domain.script_key.clone_from(&script.key);
                            domain.access = example.access;
                            domain.description.clone_from(&example.description);
                        }
                    }
                }
                hydrus_store::logins::save(conn, &logins)?;
            }
            if !draft.domain_metadata.is_empty() {
                // The reference's bandwidth manager stops at the first package
                // without rules (`AutoAddDomainMetadatas` returns, rather than
                // continuing), so later packages' rules are not added.
                for metadata in &draft.domain_metadata {
                    let Some(rules) = &metadata.rules else {
                        break;
                    };
                    let context = NetworkContext::domain(metadata.domain.clone());
                    bandwidth.rules.retain(|(c, _)| c != &context);
                    bandwidth
                        .rules
                        .push((context, Rules::new(rules.iter().copied())));
                }
                settings::set(conn, &bandwidth)?;
                // Headers replace the domain's whole set, approved.
                for metadata in &draft.domain_metadata {
                    let Some(headers) = &metadata.headers else {
                        continue;
                    };
                    let context = NetworkContext::domain(metadata.domain.clone());
                    for old in hydrus_store::network::headers(conn, &context)? {
                        hydrus_store::network::delete_header(conn, &context, &old.name)?;
                    }
                    for (name, value, reason) in headers {
                        hydrus_store::network::set_header(
                            conn,
                            &context,
                            name,
                            Some(value),
                            Some(Approval::Approved),
                            Some(reason),
                        )?;
                    }
                }
            }
            settings::set(conn, &draft.classes)?;
            settings::set(conn, &draft.downloaders)?;
            settings::set(conn, &draft.auxiliary)
        })
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
            .chain(self.login_scripts.iter().cloned().map(Native::Login))
            .chain(self.domain_exports.iter().cloned().map(Native::Domain))
            .map(|n| self.auxiliary.definition(n))
            .collect()
    }
    /// Export chosen registered objects and their downloader dependencies.
    /// Login scripts carry their own rules and never export saved domain credentials.
    pub fn export(&self, selected: &BTreeSet<usize>) -> Vec<Definition> {
        let definitions = self.definitions();
        let mut included = selected.clone();
        let registry = UrlClasses::new(self.classes.clone());
        loop {
            let before = included.len();
            let mut needed = Vec::new();
            for &index in &included {
                let Some(definition) = definitions.get(index) else {
                    continue;
                };
                match &definition.native {
                    Native::Gug(AnyGug::Nested(nested)) => {
                        for (key, name) in &nested.gugs {
                            if let Some(gug) = self.downloaders.gugs.get(key, name) {
                                needed.push(Native::Gug(gug.clone()));
                            }
                        }
                    }
                    Native::Gug(AnyGug::Single(gug)) => {
                        if let Ok(url) = gug.example_url(hydrus_core::url::GugOptions::default())
                            && let Some(class) = registry.class_for(&url)
                        {
                            needed.push(Native::Class(Box::new(class.clone())));
                            let domain = second_level(&url);
                            needed.extend(
                                self.classes
                                    .url_classes
                                    .iter()
                                    .filter(|class| {
                                        domain.is_some()
                                            && second_level(&class.example_url) == domain
                                            && matches!(
                                                class.url_type,
                                                UrlType::Post | UrlType::File
                                            )
                                    })
                                    .cloned()
                                    .map(|class| Native::Class(Box::new(class))),
                            );
                        }
                    }
                    Native::Class(class) => {
                        let keys = registry.api_class_keys(&class.example_url);
                        needed.extend(
                            self.classes
                                .url_classes
                                .iter()
                                .filter(|class| {
                                    keys.iter().any(|(key, _)| key == &hex::encode(&class.key))
                                })
                                .cloned()
                                .map(|class| Native::Class(Box::new(class))),
                        );
                        if let Ok((_, key)) = registry.url_to_fetch_and_parser(&class.example_url)
                            && let Some(parser) = self
                                .downloaders
                                .parsers
                                .iter()
                                .find(|parser| parser.key == key)
                        {
                            needed.push(Native::Page(parser.clone()));
                        }
                    }
                    _ => (),
                }
            }
            for native in needed {
                if let Some(index) = definitions
                    .iter()
                    .position(|definition| definition.native == native)
                {
                    included.insert(index);
                }
            }
            if included.len() == before {
                break;
            }
        }
        let mut out: Vec<Definition> = definitions
            .into_iter()
            .enumerate()
            .filter_map(|(index, definition)| included.contains(&index).then_some(definition))
            .collect();
        // Adding a downloader also packages the headers and bandwidth rules of
        // its example URLs' domains (`_AddGUG`).
        let mut domains = Vec::new();
        for definition in &out {
            if let Native::Gug(AnyGug::Single(gug)) = &definition.native
                && let Ok(url) = gug.example_url(hydrus_core::url::GugOptions::default())
                && let Ok(domain) = hydrus_core::url::url_domain(&url)
            {
                domains.push(domain);
            }
        }
        let existing = out
            .iter()
            .filter_map(|d| match &d.native {
                Native::Domain(m) => Some(m.domain.clone()),
                _ => None,
            })
            .collect();
        out.extend(
            self.domain_metadata_for(&domains, &existing)
                .into_iter()
                .map(|m| Definition::new(Native::Domain(m))),
        );
        out
    }
}
fn second_level(url: &str) -> Option<String> {
    hydrus_core::url::url_domain(url)
        .ok()
        .map(|domain| hydrus_core::url::psl::second_level_domain(&domain))
}

/// The concrete package category shown beside a registered object's name.
pub fn category(definition: &Definition) -> &'static str {
    match definition.native {
        Native::Class(_) => "URL Class",
        Native::Gug(_) => "GUG",
        Native::Page(_) => "Parser",
        Native::Login(_) => "Login Script",
        Native::Domain(_) => "Domain Metadata",
        Native::Content(_) => "Content Parser",
        Native::Formula(_) => "Formula",
        Native::Simple(_) => "Simple Formula",
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

fn domain_headers(
    conn: &rusqlite::Connection,
) -> hydrus_store::Result<BTreeMap<String, Vec<CustomHeader>>> {
    let mut out = BTreeMap::new();
    for context in hydrus_store::network::header_contexts(conn)? {
        if context.kind == hydrus_core::network::CONTEXT_DOMAIN && !context.data.is_empty() {
            let headers = hydrus_store::network::headers(conn, &context)?;
            out.insert(context.data, headers);
        }
    }
    Ok(out)
}

impl Draft {
    /// `AlreadyHaveExactlyTheseHeaders`: the same names and values (reasons
    /// and approval aside). A domain without headers has exactly none.
    fn has_exactly_these_headers(&self, domain: &str, headers: &[(String, String, String)]) -> bool {
        let existing = self.domain_headers.get(domain).map_or(&[][..], Vec::as_slice);
        existing.len() == headers.len()
            && headers.iter().all(|(name, value, _)| {
                existing
                    .iter()
                    .any(|h| &h.name == name && &h.value == value)
            })
    }

    /// `AlreadyHaveExactlyTheseBandwidthRules`: the domain has its own rules,
    /// and they are these.
    fn has_exactly_these_rules(&self, domain: &str, rules: &[Rule]) -> bool {
        let context = NetworkContext::domain(domain);
        self.bandwidth
            .iter()
            .find(|(c, _)| c == &context)
            .is_some_and(|(_, existing)| {
                existing.rules().len() == rules.len()
                    && rules.iter().all(|r| existing.rules().contains(r))
            })
    }

    /// The reference's "add headers/bandwidth rules" button: package a typed
    /// domain (and its parents). Returns the new entries' detail text, or the
    /// reference's notice when there is nothing to share.
    pub fn add_domain_exports(&mut self, domain: &str) -> Result<Vec<String>, String> {
        let existing = self
            .domain_exports
            .iter()
            .map(|m| m.domain.clone())
            .collect();
        let found = self.domain_metadata_for(&[domain.to_owned()], &existing);
        if found.is_empty() {
            return Err("No headers/bandwidth rules found!".into());
        }
        let details = found.iter().map(DomainMetadata::detailed_summary).collect();
        self.domain_exports.extend(found);
        Ok(details)
    }

    /// The domain metadata the reference's "add headers/bandwidth rules"
    /// prompt (and an added downloader's example domains) packages: for
    /// each of the domains and their parents down to the registrable
    /// domain, not already in `existing`, its approved headers and its own
    /// bandwidth rules, if it has either.
    pub fn domain_metadata_for(
        &self,
        domains: &[String],
        existing: &BTreeSet<String>,
    ) -> Vec<DomainMetadata> {
        let mut all: BTreeSet<String> = domains
            .iter()
            .flat_map(|d| hydrus_core::url::psl::all_applicable_domains(d))
            .collect();
        all.retain(|d| !existing.contains(d));
        all.into_iter()
            .filter_map(|domain| {
                let headers = self
                    .domain_headers
                    .get(&domain)
                    .filter(|h| !h.is_empty())
                    .map(|h| {
                        h.iter()
                            .filter(|h| h.approval == Approval::Approved)
                            .map(|h| (h.name.clone(), h.value.clone(), h.reason.clone()))
                            .collect()
                    });
                let context = NetworkContext::domain(domain.clone());
                let rules = self
                    .bandwidth
                    .iter()
                    .find(|(c, _)| c == &context)
                    .map(|(_, r)| r.rules().to_vec());
                (headers.is_some() || rules.is_some()).then_some(DomainMetadata {
                    domain,
                    headers,
                    rules,
                })
            })
            .collect()
    }
}
