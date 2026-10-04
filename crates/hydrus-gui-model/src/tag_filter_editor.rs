//! The tag filter editor (the reference's `EditTagFilterPanel`): one
//! filter seen three ways. "whitelist" (allow these) and "blacklist"
//! (exclude these) show it when it is that simple, with a box each for
//! unnamespaced tags, namespaced tags and each namespace offered;
//! "advanced" lists its rules as "exclude these" and "except for these".
//! A blacklist-only editor has the blacklist alone. Under them it says
//! when an entry is already covered by a broader rule, and what the filter
//! does; a test box says whether typed tags pass.
//!
//! As in the reference, the lists are kept sorted, and "except for these"
//! keeps an entry typed twice twice (the filter has it once).

use hydrus_core::numbers::human_int;
use hydrus_core::tag::split_tag;
use hydrus_core::tag_filter::{FilterRule, NAMESPACED, TagFilter, UNNAMESPACED};

/// The editor's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Whitelist,
    Blacklist,
    Advanced,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Whitelist => "whitelist",
            Tab::Blacklist => "blacklist",
            Tab::Advanced => "advanced",
        }
    }
}

pub const TEST_DEFAULT: &str = "Enter a tag here to test if it passes the current filter:";
pub const TEST_BLACKLIST_DEFAULT: &str = "Enter a tag here to test if it passes the blacklist (siblings tested, unnamespaced rules match namespaced tags):";
pub const BLACKLIST_TEST_NOTE: &str = "This is a fixed blacklist. It will apply rules against all test tag siblings and apply unnamespaced rules to namespaced test tags.";
pub const INSTRUCTIONS: &str = "Click the \"(un)namespaced\" checkboxes to allow/disallow those tags.\nType \"namespace:\" to manually input a namespace that is not in the list.";
pub const WHITELIST_ERROR: &str =
    "The filter is currently more complicated than a simple whitelist, so it cannot be shown here.";
pub const BLACKLIST_ERROR: &str =
    "The filter is currently more complicated than a simple blacklist, so it cannot be shown here.";
pub const REMOVE_SELECTED: &str = "Remove all selected?";
pub const GLOBAL_BOXES: [&str; 2] = ["unnamespaced tags", "namespaced tags"];
pub const HELP: &str = "Here you can set rules to filter tags for one purpose or another. The default is typically to permit all tags. Check the current filter summary text at the bottom-left of the panel to ensure you have your logic correct.\n\nThe whitelist/blacklist/advanced tabs are different ways of looking at the same filter, so you can choose which works best for you. Sometimes it is more useful to think about a filter as a whitelist (where only the listed contents are kept) or a blacklist (where everything _except_ the listed contents are kept), while the advanced tab lets you do a more complicated combination of the two.\n\nAs well as selecting entire namespaces with the checkboxes, you can type or paste the individual tags directly--just hit enter to add each one. Double-click an existing entry in a list to remove it.";

/// A "tag filter" button's label, elided to 45 characters, and its tooltip,
/// the whole (`TagFilterButton._UpdateLabel`): the blacklist for a
/// blacklist-only filter, else what it permits, or with `filter_language`
/// what it adds ("all tags except goblin"), after `prefix`.
pub fn button_label(
    filter: &TagFilter,
    blacklist_only: bool,
    prefix: &str,
    filter_language: bool,
) -> (String, String) {
    let text = if blacklist_only {
        filter.to_blacklist_string()
    } else if filter_language {
        filter.to_filter_string()
    } else {
        filter.to_permitted_string()
    };
    let tooltip = format!("{prefix}{text}");
    let label = if tooltip.chars().count() > 45 {
        let mut short: String = tooltip.chars().take(44).collect();
        short.push('\u{2026}');
        short
    } else {
        tooltip.clone()
    };
    (label, tooltip)
}

/// The editor's title, from a "tag filter" button.
pub fn title(blacklist_only: bool) -> &'static str {
    if blacklist_only {
        "edit blacklist"
    } else {
        "edit tag filter"
    }
}

/// The slices of the "unnamespaced tags" and "namespaced tags" boxes.
const GLOBAL_SLICES: [&str; 2] = [UNNAMESPACED, NAMESPACED];

/// A slice as a list shows it ("'creator' tags"; `ConvertTagSliceToPrettyString`).
pub fn pretty_slice(slice: &str) -> String {
    match slice {
        UNNAMESPACED => "unnamespaced tags".into(),
        NAMESPACED => "namespaced tags".into(),
        s if is_namespace_slice(s) => format!("'{}' tags", &s[..s.len() - 1]),
        s => s.into(),
    }
}

fn is_namespace_slice(slice: &str) -> bool {
    slice.len() > 1 && slice.ends_with(':') && slice.matches(':').count() == 1
}

/// What was typed, as a slice (`_CleanTagSliceInput`): lowercase and
/// trimmed, `*` for every unnamespaced tag, `*:` or `*:*` for every
/// namespaced one, `ns:*` for a namespace.
pub fn clean_slice(typed: &str) -> String {
    let mut slice = typed.to_lowercase().trim().to_owned();
    while slice.contains("**") {
        slice = slice.replace("**", "*");
    }
    match slice.as_str() {
        "*" => return UNNAMESPACED.to_owned(),
        "*:" | "*:*" => return NAMESPACED.to_owned(),
        _ => {}
    }
    if slice.contains(':') {
        let (namespace, subtag) = split_tag(&slice);
        if subtag == "*" {
            return format!("{namespace}:");
        }
    }
    slice
}

/// A simple tab as shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimpleView {
    /// Whether the filter is simple enough to show here.
    pub enabled: bool,
    /// Why it isn't (empty when it is).
    pub error: &'static str,
    pub list: Vec<String>,
    /// The "unnamespaced tags" and "namespaced tags" boxes.
    pub global: [bool; 2],
    /// A box per namespace offered.
    pub namespaces: Vec<bool>,
    pub namespaces_enabled: bool,
}

/// The editor as shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub whitelist: SimpleView,
    pub blacklist: SimpleView,
    pub advanced_blacklist: Vec<String>,
    pub advanced_whitelist: Vec<String>,
    /// "except for these" can't be added to while nothing is excluded.
    pub except_input_enabled: bool,
    /// "current filter: ..." (or the blacklist, for a blacklist-only one).
    pub current: String,
}

#[derive(Debug, Clone)]
pub struct TagFilterEditor {
    blacklist_only: bool,
    /// The namespaces offered a box each (not the empty one).
    namespaces: Vec<String>,
    /// "exclude these" and "except for these", sorted.
    black: Vec<String>,
    white: Vec<String>,
    /// That an entry was already covered, from the last change.
    redundant: Option<String>,
}

impl TagFilterEditor {
    pub fn new(filter: &TagFilter, blacklist_only: bool, namespaces: &[String]) -> Self {
        let mut editor = Self {
            blacklist_only,
            namespaces: namespaces
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect(),
            black: Vec::new(),
            white: Vec::new(),
            redundant: None,
        };
        editor.set_value(filter);
        editor
    }

    /// Load a filter (a favourite, say).
    pub fn set_value(&mut self, filter: &TagFilter) {
        self.black.clear();
        self.white.clear();
        for (slice, rule) in filter.rules() {
            match rule {
                FilterRule::Blacklist => self.black.push(slice.to_owned()),
                FilterRule::Whitelist => self.white.push(slice.to_owned()),
            }
        }
        self.black.sort();
        self.white.sort();
    }

    pub fn value(&self) -> TagFilter {
        let mut filter = TagFilter::new();
        for slice in &self.black {
            filter.set_rule(slice.clone(), FilterRule::Blacklist);
        }
        for slice in &self.white {
            filter.set_rule(slice.clone(), FilterRule::Whitelist);
        }
        filter
    }

    pub fn blacklist_only(&self) -> bool {
        self.blacklist_only
    }

    pub fn namespaces(&self) -> &[String] {
        &self.namespaces
    }

    pub fn tabs(&self) -> Vec<Tab> {
        if self.blacklist_only {
            vec![Tab::Blacklist]
        } else {
            vec![Tab::Whitelist, Tab::Blacklist, Tab::Advanced]
        }
    }

    /// The tab it opens on: the simplest that can show the filter.
    pub fn start_tab(&self) -> Tab {
        let (whitelist, blacklist) = self.possible();
        if self.blacklist_only {
            Tab::Blacklist
        } else if whitelist {
            Tab::Whitelist
        } else if blacklist {
            Tab::Blacklist
        } else {
            Tab::Advanced
        }
    }

    /// Whether the filter can be shown as a simple whitelist, and as a
    /// simple blacklist (`_GetWhiteBlacklistsPossible`).
    fn possible(&self) -> (bool, bool) {
        let whitelist = self
            .black
            .iter()
            .all(|s| s == UNNAMESPACED || s == NAMESPACED);
        (whitelist, self.white.is_empty())
    }

    /// Whether a rule in "exclude these" blocks `slice` (`_CurrentlyBlocked`).
    fn blocked(&self, slice: &str) -> bool {
        let tests: Vec<String> = if slice == UNNAMESPACED || slice == NAMESPACED {
            vec![slice.to_owned()]
        } else if is_namespace_slice(slice) {
            vec![NAMESPACED.to_owned(), slice.to_owned()]
        } else if slice.contains(':') {
            let (namespace, _) = split_tag(slice);
            vec![
                NAMESPACED.to_owned(),
                format!("{namespace}:"),
                slice.to_owned(),
            ]
        } else {
            vec![UNNAMESPACED.to_owned(), slice.to_owned()]
        };
        self.black.iter().any(|b| tests.contains(b))
    }

    /// Take the message that an entry was already covered, if the last
    /// change made one.
    pub fn take_redundant(&mut self) -> Option<String> {
        self.redundant.take()
    }

    fn add(list: &mut Vec<String>, slices: impl IntoIterator<Item = String>) {
        list.extend(slices);
        list.sort();
    }

    fn remove(list: &mut Vec<String>, slices: &[String]) {
        for slice in slices {
            if let Some(at) = list.iter().position(|s| s == slice) {
                list.remove(at);
            }
        }
    }

    /// The message for entries already covered by a broader rule.
    fn covered(slices: &[String], first: &str, what: &str) -> String {
        if slices.len() == 1 {
            format!(
                "{} is already {what} by a broader rule!",
                pretty_slice(first)
            )
        } else {
            let separator = if slices.len() < 5 { "\n" } else { ", " };
            let list: Vec<String> = slices.iter().map(|s| pretty_slice(s)).collect();
            format!(
                "The tags\n\n{}\n\nare already {what} by a broader rule!",
                list.join(separator)
            )
        }
    }

    /// Typed into "exclude these" (`_AdvancedEnterBlacklistMultiple`): each
    /// excluded already comes out, unless only adding; the rest go in.
    fn enter_blacklist(&mut self, typed: &[String], only_add: bool) {
        let mut slices: Vec<String> = Vec::new();
        for slice in typed.iter().map(|t| clean_slice(t)) {
            if !slices.contains(&slice) {
                slices.push(slice);
            }
        }
        let to_remove: Vec<String> = slices
            .iter()
            .filter(|s| self.black.contains(s))
            .cloned()
            .collect();
        let to_add: Vec<String> = slices
            .into_iter()
            .filter(|s| !to_remove.contains(s))
            .collect();
        if !to_remove.is_empty() && !only_add {
            Self::remove(&mut self.black, &to_remove);
        }
        if !to_add.is_empty() {
            Self::remove(&mut self.white, &to_add);
            let already: Vec<String> = to_add.iter().filter(|s| self.blocked(s)).cloned().collect();
            if !already.is_empty() {
                self.redundant = Some(Self::covered(&already, &already[0], "blocked"));
            }
            Self::add(&mut self.black, to_add);
        }
    }

    /// Typed into "except for these" (`_AdvancedEnterWhitelistMultiple`).
    fn enter_whitelist(&mut self, typed: &[String], only_add: bool) {
        let slices: Vec<String> = typed.iter().map(|t| clean_slice(t)).collect();
        let to_remove: Vec<String> = slices
            .iter()
            .filter(|s| self.white.contains(s))
            .cloned()
            .collect();
        if !to_remove.is_empty() && !only_add {
            let mut once = to_remove.clone();
            once.dedup();
            Self::remove(&mut self.white, &once);
        }
        let to_add: Vec<String> = slices
            .iter()
            .filter(|s| !to_remove.contains(s))
            .cloned()
            .collect();
        if !to_add.is_empty() {
            Self::remove(&mut self.black, &to_add);
            let already: Vec<String> = to_add
                .iter()
                .filter(|s| s.as_str() != UNNAMESPACED && s.as_str() != NAMESPACED)
                .filter(|s| !self.blocked(s))
                .cloned()
                .collect();
            if !already.is_empty() {
                // (the reference names the first typed, for one)
                self.redundant = Some(Self::covered(&already, &to_add[0], "permitted"));
            }
            // (all typed, as the reference adds them; not the (un)namespaced)
            Self::add(
                &mut self.white,
                slices
                    .into_iter()
                    .filter(|s| s.as_str() != UNNAMESPACED && s.as_str() != NAMESPACED),
            );
        }
    }

    /// Typed into the advanced tab's "exclude these" input.
    pub fn add_advanced_blacklist(&mut self, typed: &[String]) {
        self.enter_blacklist(typed, true);
    }

    /// Typed into the advanced tab's "except for these" input.
    pub fn add_advanced_whitelist(&mut self, typed: &[String]) {
        self.enter_whitelist(typed, true);
    }

    /// Typed into the whitelist tab's input.
    pub fn add_simple_whitelist(&mut self, typed: &[String]) {
        let mut set: Vec<String> = Vec::new();
        for t in typed {
            if !set.contains(t) {
                set.push(t.clone());
            }
        }
        self.enter_whitelist(&set, true);
    }

    /// Typed into the blacklist tab's input.
    pub fn add_simple_blacklist(&mut self, typed: &[String]) {
        self.enter_blacklist(typed, true);
    }

    /// "block everything": exclude every tag, keeping the exceptions.
    pub fn block_everything(&mut self) {
        let simples = [UNNAMESPACED.to_owned(), NAMESPACED.to_owned()];
        self.black.clear();
        Self::remove(&mut self.white, &simples);
        Self::add(&mut self.black, simples);
    }

    /// "delete" under "exclude these", the selected removed (after asking).
    pub fn delete_advanced_blacklist(&mut self, selected: &[String]) {
        Self::remove(&mut self.black, selected);
    }

    /// "delete" under "except for these".
    pub fn delete_advanced_whitelist(&mut self, selected: &[String]) {
        Self::remove(&mut self.white, selected);
    }

    /// Entries taken out of the whitelist tab's list (double-clicked, or
    /// "remove"): the (un)namespaced are excluded, the rest no longer
    /// excepted.
    pub fn remove_simple_whitelist(&mut self, slices: &[String]) {
        let mut rest: Vec<String> = Vec::new();
        for slice in slices {
            if !rest.contains(slice) {
                rest.push(slice.clone());
            }
        }
        for simple in GLOBAL_SLICES {
            if let Some(at) = rest.iter().position(|s| s == simple) {
                rest.remove(at);
                self.enter_blacklist(&[simple.to_owned()], false);
            }
        }
        self.enter_whitelist(&rest, false);
    }

    /// Entries taken out of the blacklist tab's list: no longer excluded.
    pub fn remove_simple_blacklist(&mut self, slices: &[String]) {
        self.enter_blacklist(slices, false);
    }

    /// The whitelist tab's "unnamespaced tags" (0) or "namespaced tags" (1)
    /// box clicked.
    pub fn whitelist_global(&mut self, index: usize) {
        let Some(slice) = GLOBAL_SLICES.get(index) else {
            return;
        };
        let slice = (*slice).to_owned();
        if self.view().whitelist.list.contains(&slice) {
            self.enter_blacklist(&[slice], false);
        } else {
            self.enter_whitelist(&[slice], false);
        }
    }

    /// A namespace's box clicked on the whitelist tab.
    pub fn whitelist_namespace(&mut self, index: usize) {
        if let Some(namespace) = self.namespaces.get(index) {
            let slice = format!("{namespace}:");
            self.enter_whitelist(&[slice], false);
        }
    }

    /// The blacklist tab's "unnamespaced tags" or "namespaced tags" box.
    pub fn blacklist_global(&mut self, index: usize) {
        if let Some(slice) = GLOBAL_SLICES.get(index) {
            self.enter_blacklist(&[(*slice).to_owned()], false);
        }
    }

    /// A namespace's box clicked on the blacklist tab.
    pub fn blacklist_namespace(&mut self, index: usize) {
        if let Some(namespace) = self.namespaces.get(index) {
            let slice = format!("{namespace}:");
            self.enter_blacklist(&[slice], false);
        }
    }

    fn ticks(&self, list: &[String]) -> ([bool; 2], Vec<bool>) {
        (
            GLOBAL_SLICES.map(|s| list.iter().any(|l| l == s)),
            self.namespaces
                .iter()
                .map(|n| list.contains(&format!("{n}:")))
                .collect(),
        )
    }

    /// The editor as it shows the filter (`_UpdateStatus`).
    pub fn view(&self) -> View {
        let (whitelist_possible, blacklist_possible) = self.possible();
        let off = |error| SimpleView {
            enabled: false,
            error,
            list: Vec::new(),
            global: [false; 2],
            namespaces: vec![false; self.namespaces.len()],
            namespaces_enabled: false,
        };
        let whitelist = if whitelist_possible {
            let mut list: Vec<String> = self.white.clone();
            for simple in GLOBAL_SLICES {
                if !self.blocked(simple) {
                    list.push(simple.to_owned());
                }
            }
            list.sort();
            list.dedup();
            let (global, namespaces) = self.ticks(&list);
            SimpleView {
                enabled: true,
                error: "",
                namespaces_enabled: self.blocked(NAMESPACED),
                list,
                global,
                namespaces,
            }
        } else {
            off(WHITELIST_ERROR)
        };
        let blacklist = if blacklist_possible {
            let (global, namespaces) = self.ticks(&self.black);
            SimpleView {
                enabled: true,
                error: "",
                namespaces_enabled: !self.blocked(NAMESPACED),
                list: self.black.clone(),
                global,
                namespaces,
            }
        } else {
            off(BLACKLIST_ERROR)
        };
        let filter = self.value();
        View {
            whitelist,
            blacklist,
            advanced_blacklist: self.black.clone(),
            advanced_whitelist: self.white.clone(),
            except_input_enabled: !self.black.is_empty(),
            current: if self.blacklist_only {
                filter.to_blacklist_string()
            } else {
                format!("current filter: {}", filter.to_permitted_string())
            },
        }
    }

    /// What the test box says of `typed` (a tag a line), and whether they
    /// all pass (`None` while nothing is typed). A blacklist-only editor
    /// tests each tag with its siblings (`siblings`: each tag's, itself
    /// included), its unnamespaced rules matching namespaced tags.
    pub fn test(
        &self,
        typed: &str,
        siblings: &dyn Fn(&[String]) -> Vec<Vec<String>>,
    ) -> (String, Option<bool>) {
        let mut tags: Vec<String> = Vec::new();
        for line in typed.lines() {
            let tag = hydrus_core::tag::clean_tag(line);
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        if typed.is_empty() {
            let text = if self.blacklist_only {
                TEST_BLACKLIST_DEFAULT
            } else {
                TEST_DEFAULT
            };
            return (text.to_owned(), None);
        }
        let filter = self.value();
        let results: Vec<bool> = if self.blacklist_only {
            siblings(&tags)
                .iter()
                .map(|chain| chain.iter().all(|t| filter.tag_ok(t, true)))
                .collect()
        } else {
            tags.iter().map(|t| filter.tag_ok(t, false)).collect()
        };
        let passed = results.iter().filter(|r| **r).count();
        let blocked = results.len() - passed;
        let all_good = blocked == 0;
        let text = if results.len() == 1 {
            if all_good {
                "tag passes!"
            } else {
                "tag blocked!"
            }
            .to_owned()
        } else if all_good {
            "all pass!".to_owned()
        } else if passed == 0 {
            "all blocked!".to_owned()
        } else {
            format!(
                "{} pass, {} blocked!",
                human_int(passed as u64),
                human_int(blocked as u64)
            )
        };
        (text, Some(all_good))
    }
}

/// The reference's prompt before saving or importing a favourite.
pub const FAVOURITE_NAME: &str = "Enter a name for the favourite.";
/// The empty favourite menu's label.
pub const NO_FAVOURITES: &str = "no favourites set!";
/// Ask before replacing a saved name (case-sensitive, as the reference).
pub fn overwrite_favourite(name: &str) -> String {
    format!("\"{name}\" already exists! Overwrite?")
}
/// Ask before deleting a saved filter.
pub fn delete_favourite(name: &str) -> String {
    format!("Delete \"{name}\"?")
}

/// Shared favourites in insertion order, independent of any editor's draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FavouriteTagFilters(pub Vec<(String, TagFilter)>);
impl hydrus_store::settings::Setting for FavouriteTagFilters {
    const KEY: &'static str = "favourite_tag_filters";
}
impl FavouriteTagFilters {
    /// Read current favourites; failed reads never become an empty write.
    pub fn load(store: &hydrus_store::Store) -> hydrus_store::Result<Self> {
        store.read(hydrus_store::settings::get::<Self>)
    }
    /// An owned saved value for a detached editor draft.
    pub fn get(&self, name: &str) -> Option<TagFilter> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| f.clone())
    }
    /// Save only this name, preserving changes from other open editors.
    /// Returns false when replacement still requires the user's answer.
    pub fn save(
        store: &hydrus_store::Store,
        name: String,
        filter: TagFilter,
        overwrite: bool,
    ) -> hydrus_store::Result<bool> {
        store.write(move |ctx| {
            let mut favourites: Self = hydrus_store::settings::get(ctx.conn())?;
            if let Some((_, old)) = favourites.0.iter_mut().find(|(n, _)| *n == name) {
                if !overwrite {
                    return Ok(false);
                }
                *old = filter;
            } else {
                favourites.0.push((name, filter));
            }
            hydrus_store::settings::set(ctx.conn(), &favourites)?;
            Ok(true)
        })
    }
    /// Delete only the confirmed name; the caller's filter stays intact.
    pub fn delete(store: &hydrus_store::Store, name: String) -> hydrus_store::Result<()> {
        store.write(move |ctx| {
            let mut favourites: Self = hydrus_store::settings::get(ctx.conn())?;
            favourites.0.retain(|(n, _)| *n != name);
            hydrus_store::settings::set(ctx.conn(), &favourites)
        })
    }
}

/// Encode the reference's JSON-serialised Tag Filter object for clipboard/files.
pub fn export_favourite(filter: &TagFilter) -> String {
    let rules: Vec<_> = filter
        .rules()
        .map(|(slice, rule)| {
            serde_json::json!([slice, if rule == FilterRule::Blacklist { 1 } else { 0 }])
        })
        .collect();
    hydrus_core::pyjson::PyJson::parse(&serde_json::json!([44, 1, rules]).to_string())
        .expect("a tag filter tuple is valid JSON")
        .to_python_string()
}

/// Decode and clean a single reference Tag Filter, before any draft/settings change.
pub fn import_favourite(text: &str) -> Result<TagFilter, String> {
    if text.len() > 16 * 1024 * 1024 {
        return Err("Tag filter data exceeds the 16 MiB limit.".into());
    }
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("Could not parse JSON-serialised Tag Filter object: {e}"))?;
    let tuple = value
        .as_array()
        .filter(|v| v.len() == 3)
        .ok_or_else(|| "That object was not a Tag Filter!".to_owned())?;
    if tuple[0] != 44 || tuple[1] != 1 {
        return Err("That object was not a supported Tag Filter!".into());
    }
    let rules = tuple[2]
        .as_array()
        .ok_or_else(|| "Invalid Tag Filter rules.".to_owned())?;
    let mut filter = TagFilter::new();
    for rule in rules {
        let pair = rule
            .as_array()
            .filter(|p| p.len() == 2)
            .ok_or_else(|| "Invalid Tag Filter rule.".to_owned())?;
        let slice = pair[0]
            .as_str()
            .ok_or_else(|| "Invalid tag slice.".to_owned())?;
        let rule = match pair[1].as_i64() {
            Some(0) => FilterRule::Whitelist,
            Some(1) => FilterRule::Blacklist,
            _ => return Err("Invalid Tag Filter rule type.".into()),
        };
        // CleanRules preserves the global slices and cleans namespaces using
        // an example tag. It does not interpret '*' as an editor shortcut.
        let cleaned = if slice == UNNAMESPACED || slice == NAMESPACED {
            slice.to_owned()
        } else if is_namespace_slice(slice) {
            let example = hydrus_core::tag::clean_tag(&format!("{slice}example"));
            example
                .strip_suffix("example")
                .unwrap_or_default()
                .to_owned()
        } else {
            hydrus_core::tag::clean_tag(slice)
        };
        if !cleaned.is_empty() || slice == UNNAMESPACED {
            filter.set_rule(cleaned, rule);
        }
    }
    Ok(filter)
}
