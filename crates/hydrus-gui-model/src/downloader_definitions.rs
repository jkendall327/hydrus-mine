//! Downloader definition lists and editors. Drafts own a copy of native
//! settings; opening or cancelling an editor never writes to the store.

use hydrus_core::pages::PageKey;
use hydrus_core::url::class::{GalleryIndex, GalleryIndexPosition, ReferralMode};
use hydrus_core::url::{
    AnyGug, DomainMask, Gug, GugOptions, NestedGug, UrlClass, UrlClassSettings, UrlClasses, UrlType,
};
use hydrus_parse::downloaders::Downloaders;
use hydrus_store::{Store, settings};

use crate::favourites::non_dupe_name;
use crate::list_selection::ListSelection;
use crate::string_editors::{CHARACTER_SETS, MatchEditor};

/// Reference list headings for URL classes.
pub const CLASS_COLUMNS: [&str; 3] = ["name", "type", "example url"];
/// Reference list headings for single generators.
pub const GUG_COLUMNS: [&str; 3] = ["name", "example url", "gallery url class"];
/// Reference list headings for nested generators.
pub const NESTED_COLUMNS: [&str; 3] = ["name", "gugs", "missing gugs?"];
/// The question before removing definitions.
pub const REMOVE: &str = "Remove all selected?";
/// The question before discarding changed URL classes.
pub const CANCEL: &str = "You have made changes. Sure you are ok to cancel?";
/// The prompt for the URL class list's test field.
pub const CHECKER_HINT: &str = "<-- Enter a URL here to see which url class it currently matches!";

/// Which list is being managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Classes,
    Generators,
    Nested,
}

/// A definition's stable draft identity is its position, unaffected by sorting.
#[derive(Debug, Clone)]
pub struct Draft {
    pub classes: UrlClassSettings,
    pub downloaders: Downloaders,
    /// Global encoding options used by the actual gallery downloader.
    pub gug_options: GugOptions,
    pub selection: ListSelection<usize>,
    pub kind: Kind,
    pub sort_column: usize,
    pub ascending: bool,
    original_classes: UrlClassSettings,
    original_downloaders: Downloaders,
}

impl Draft {
    /// Open native settings as an isolated draft.
    pub fn load(store: &Store, kind: Kind) -> hydrus_store::Result<Self> {
        let (classes, downloaders, network) = store.read(|conn| {
            Ok((
                settings::get(conn)?,
                settings::get(conn)?,
                settings::get::<hydrus_store::network::NetworkSettings>(conn)?,
            ))
        })?;
        let mut draft = Self::new(classes, downloaders, kind);
        draft.gug_options.percent_twenty_is_space = network.gug_percent_twenty_is_space;
        Ok(draft)
    }

    /// Build a draft from settings (also used for fixture replay).
    pub fn new(classes: UrlClassSettings, downloaders: Downloaders, kind: Kind) -> Self {
        Self {
            original_classes: classes.clone(),
            original_downloaders: downloaders.clone(),
            gug_options: GugOptions {
                collapse_leading_slashes: classes.collapse_leading_slashes,
                ..GugOptions::default()
            },
            classes,
            downloaders,
            selection: ListSelection::default(),
            kind,
            sort_column: 0,
            ascending: true,
        }
    }

    /// Whether applying would change this list's settings.
    pub fn changed(&self) -> bool {
        match self.kind {
            Kind::Classes => self.classes != self.original_classes,
            Kind::Generators | Kind::Nested => {
                self.downloaders.gugs != self.original_downloaders.gugs
            }
        }
    }

    /// Write only the settings this editor owns, preserving current parser and
    /// link data even if another editor saved while this draft was open.
    pub fn save(&self, store: &Store) -> hydrus_store::Result<()> {
        match self.kind {
            Kind::Classes => {
                let mut classes = self.classes.url_classes.clone();
                classes.sort_by(|a, b| a.name.cmp(&b.name));
                store.write_and_refresh(move |ctx| {
                    let conn = ctx.conn();
                    let mut current: UrlClassSettings = settings::get(conn)?;
                    current.url_classes = classes;
                    settings::set(conn, &current)
                })
            }
            Kind::Generators | Kind::Nested => {
                let mut gugs = self.downloaders.gugs.clone();
                let available = gugs.clone();
                for gug in &mut gugs.gugs {
                    if let AnyGug::Nested(n) = gug {
                        n.gugs = n
                            .gugs
                            .iter()
                            .filter_map(|(key, name)| match available.get(key, name) {
                                Some(AnyGug::Single(g)) => Some((g.key.clone(), g.name.clone())),
                                _ => None,
                            })
                            .collect();
                    }
                }
                store.write(move |ctx| {
                    let conn = ctx.conn();
                    let mut current: Downloaders = settings::get(conn)?;
                    current.gugs = gugs;
                    settings::set(conn, &current)
                })
            }
        }
    }

    /// The row as the reference displays it.
    pub fn row(&self, index: usize) -> Vec<String> {
        match self.kind {
            Kind::Classes => class_row(
                &self.classes.url_classes[index],
                self.classes.collapse_leading_slashes,
            ),
            Kind::Generators | Kind::Nested => gug_row(
                &self.downloaders.gugs.gugs[index],
                &self.downloaders,
                &self.classes,
                self.gug_options,
            ),
        }
    }

    /// Visible rows sorted by their selected column; matching precedence uses
    /// specificity and name, as the reference domain manager does.
    pub fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = match self.kind {
            Kind::Classes => (0..self.classes.url_classes.len()).collect(),
            kind => self
                .downloaders
                .gugs
                .gugs
                .iter()
                .enumerate()
                .filter_map(|(i, g)| {
                    (matches!(
                        (kind, g),
                        (Kind::Generators, AnyGug::Single(_)) | (Kind::Nested, AnyGug::Nested(_))
                    ))
                    .then_some(i)
                })
                .collect(),
        };
        if self.kind == Kind::Nested && self.sort_column == 1 {
            order.sort_by_key(|&i| match &self.downloaders.gugs.gugs[i] {
                AnyGug::Nested(n) => n.gugs.len(),
                AnyGug::Single(_) => 0,
            });
        } else {
            order.sort_by_cached_key(|&i| {
                self.row(i)
                    .get(self.sort_column)
                    .cloned()
                    .unwrap_or_default()
            });
        }
        if !self.ascending {
            order.reverse();
        }
        order
    }

    /// Test against the draft, selecting the matching class as the reference does.
    pub fn check_url(&mut self, url: &str) -> String {
        if url.is_empty() {
            return CHECKER_HINT.into();
        }
        if let Err(e) = hydrus_core::url::functions::check_full_url(url) {
            return e.to_string();
        }
        let mut settings = self.classes.clone();
        settings.url_classes.sort_by(|a, b| a.name.cmp(&b.name));
        let registry = UrlClasses::new(settings);
        let Some(class) = registry.class_for(url) else {
            return "No match!".into();
        };
        let name = class.name.clone();
        self.selection.select_only(
            self.classes
                .url_classes
                .iter()
                .position(|c| c.key == class.key),
        );
        format!("Matches \"{name}\"")
    }

    /// Add or replace a validated class; edits keep the key and copies receive
    /// a fresh key. Names are made unique using the reference's suffixes.
    pub fn put_class(
        &mut self,
        mut class: UrlClass,
        replacing: Option<usize>,
    ) -> Result<(), String> {
        validate_class(&class, self.classes.collapse_leading_slashes)?;
        class.name = non_dupe_name(&class.name, &|name| {
            self.classes
                .url_classes
                .iter()
                .enumerate()
                .any(|(i, c)| Some(i) != replacing && c.name == name)
        });
        let index = if let Some(i) = replacing {
            if i >= self.classes.url_classes.len() {
                return Err("The URL class no longer exists.".into());
            }
            class.key.clone_from(&self.classes.url_classes[i].key);
            self.classes.url_classes[i] = class;
            i
        } else {
            class.key = PageKey::random().0.to_vec();
            self.classes.url_classes.push(class);
            self.classes.url_classes.len() - 1
        };
        self.selection.select_only(Some(index));
        Ok(())
    }

    /// Add or replace a generator, preserving edited keys and repairing nested
    /// members by key/name as the reference does.
    pub fn put_gug(&mut self, mut gug: AnyGug, replacing: Option<usize>) -> Result<(), String> {
        if let AnyGug::Single(g) = &gug {
            validate_gug(g)?;
        }
        let name = non_dupe_name(gug.name(), &|name| {
            self.downloaders
                .gugs
                .gugs
                .iter()
                .enumerate()
                .any(|(i, g)| Some(i) != replacing && g.name() == name)
        });
        let key = replacing
            .and_then(|i| self.downloaders.gugs.gugs.get(i))
            .map_or_else(|| PageKey::random().to_hex(), |g| g.key().to_owned());
        match &mut gug {
            AnyGug::Single(g) => {
                g.name = name;
                g.key = key;
            }
            AnyGug::Nested(g) => {
                g.name = name;
                g.key = key;
                g.gugs = g
                    .gugs
                    .iter()
                    .filter_map(|(k, n)| match self.downloaders.gugs.get(k, n) {
                        Some(AnyGug::Single(g)) => Some((g.key.clone(), g.name.clone())),
                        _ => None,
                    })
                    .collect();
            }
        }
        let index = if let Some(i) = replacing {
            let Some(old) = self.downloaders.gugs.gugs.get_mut(i) else {
                return Err("The generator no longer exists.".into());
            };
            *old = gug;
            i
        } else {
            self.downloaders.gugs.keys_to_display.push(key_of(&gug));
            self.downloaders.gugs.gugs.push(gug);
            self.downloaders.gugs.gugs.len() - 1
        };
        self.selection.select_only(Some(index));
        Ok(())
    }

    /// The reference warns when a single generator is used by a nested one.
    pub fn delete_question(&self) -> String {
        let selected = self.selection.in_order(&self.order());
        let mut message = REMOVE.to_owned();
        if self.kind == Kind::Generators {
            for i in selected {
                let g = &self.downloaders.gugs.gugs[i];
                let mut affected: Vec<&str> = self
                    .downloaders
                    .gugs
                    .gugs
                    .iter()
                    .filter_map(|n| match n {
                        AnyGug::Nested(n) if n.gugs.iter().any(|(key, _)| key == g.key()) => {
                            Some(n.name.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                affected.sort_unstable();
                if !affected.is_empty() {
                    message.push_str(&format!("\n\nThe GUG \"{}\" is in the NGUGs:\n\n{}\n\nDeleting this GUG will ultimately remove it from those NGUGs--are you sure that is ok?", g.name(), affected.join("\n")));
                }
            }
        }
        message
    }

    /// Remove the selected items after confirmation; nested dangling references
    /// remain visible until repaired by their editor, as in the reference list.
    pub fn delete_selected(&mut self) {
        let mut indices = self.selection.in_order(&self.order());
        indices.sort_unstable();
        for i in indices.into_iter().rev() {
            match self.kind {
                Kind::Classes => {
                    self.classes.url_classes.remove(i);
                }
                Kind::Generators | Kind::Nested => {
                    let g = self.downloaders.gugs.gugs.remove(i);
                    self.downloaders
                        .gugs
                        .keys_to_display
                        .retain(|key| key != g.key());
                }
            }
        }
        self.selection = ListSelection::default();
    }
}

fn key_of(gug: &AnyGug) -> String {
    gug.key().to_owned()
}

/// A new single generator with editable, working example values.
pub fn new_gug() -> Gug {
    Gug {
        name: "new gallery url generator".into(),
        key: String::new(),
        url_template: "https://example.com/search?q=%tags%&index=0".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "search tags".into(),
        example_search_text: "blue_eyes blonde_hair".into(),
    }
}

/// The reference's new-class example, with editable post path and query rules.
pub fn new_class() -> UrlClass {
    use hydrus_core::url::class::Referral;
    use hydrus_core::url::strings::{FlexibleMatch, MatchKind};
    use hydrus_core::url::{
        DomainMask, StringConverter, StringMatch, StringProcessor, UrlParameter,
    };
    let example = "https://hostname.com/post/page.php?id=123456&s=view";
    let parameter = |name: &str, value| UrlParameter {
        name: name.into(),
        value,
        ephemeral: false,
        default: None,
        default_processor: StringProcessor::default(),
    };
    let converter = StringConverter {
        example: example.into(),
        ..StringConverter::default()
    };
    UrlClass {
        domain_mask: DomainMask::new(vec!["hostname.com".into()], Vec::new(), false, false),
        path_components: vec![
            (StringMatch::fixed("post"), None),
            (StringMatch::fixed("page.php"), None),
        ],
        parameters: vec![
            parameter("s", StringMatch::fixed("view")),
            parameter(
                "id",
                StringMatch {
                    kind: MatchKind::Flexible(FlexibleMatch::Numeric),
                    example: "123456".into(),
                    ..StringMatch::any()
                },
            ),
        ],
        example_url: example.into(),
        api_lookup_converter: converter.clone(),
        referral: Referral {
            converter,
            ..Referral::default()
        },
        ..UrlClass::default()
    }
}

/// A new nested generator.
pub fn new_nested() -> NestedGug {
    NestedGug {
        name: "new nested gallery url generator".into(),
        key: String::new(),
        initial_search_text: "search tags".into(),
        gugs: Vec::new(),
    }
}

/// Validate a class's own example before accepting its editor.
pub fn validate_class(class: &UrlClass, collapse: bool) -> Result<(), String> {
    class
        .test(&class.example_url, collapse)
        .map_err(|_| "Please enter an example url that matches the given rules!".to_owned())?;
    if class.uses_api_url() {
        class
            .api_url(&class.example_url, collapse)
            .map_err(|_| "Problem making API/Redirect URL!".to_owned())?;
    }
    Ok(())
}

/// Validate the generator template before accepting its editor.
pub fn validate_gug(gug: &Gug) -> Result<(), String> {
    gug.example_url(GugOptions::default())
        .map(|_| ())
        .map_err(|_| "Please ensure your generator can make an example url!".into())
}

/// Query parameters must have distinct, nonempty encoded names.
pub fn validate_parameter_name(name: &str, other_names: &[String]) -> Result<(), String> {
    if name.is_empty() {
        Err("Sorry, you have to set a key/name!".into())
    } else if other_names.iter().any(|existing| existing == name) {
        Err("Sorry, your key/name already exists, pick something else!".into())
    } else {
        Ok(())
    }
}

/// Explain an unusual file association choice using the reference's notice.
pub fn association_notice(
    kind: hydrus_core::url::UrlType,
    associate: bool,
) -> Option<&'static str> {
    use hydrus_core::url::UrlType;
    match (kind, associate) {
        (UrlType::Gallery | UrlType::Watchable, true) => Some(
            "Please note that it is only appropriate to associate a Gallery or Watchable URL with a file if that URL is non-ephemeral. It is only appropriate if the exact same URL will definitely give the same files in six months' time (like a fixed doujin chapter gallery).\n\nIf you are not sure what this means, turn this back off.",
        ),
        (UrlType::File | UrlType::Post, false) => Some(
            "Hydrus uses these file associations to make sure not to re-download the same file when it comes across the same URL in future. It is only appropriate to not associate a file or post url with a file if that url is particularly ephemeral, such as if the URL includes a non-removable random key that becomes invalid after a few minutes.\n\nIf you are not sure what this means, turn this back on.",
        ),
        _ => None,
    }
}

/// A class's row, including its invalid-example warning.
pub fn class_row(class: &UrlClass, collapse: bool) -> Vec<String> {
    let example = class
        .normalise(&class.example_url, false, collapse)
        .unwrap_or_else(|_| format!("DOES NOT MATCH OWN EXAMPLE URL!! {}", class.example_url));
    vec![
        class.name.clone(),
        class.url_type.name().unwrap_or("source url").into(),
        example,
    ]
}

/// Generated raw URL, matched class description and request URL.
pub fn gug_preview(
    gug: &Gug,
    classes: &UrlClassSettings,
    options: GugOptions,
) -> (String, String, String) {
    let registry = UrlClasses::new(classes.clone());
    let generated = gug
        .example_url(options)
        .map_err(|e| e.to_string())
        .and_then(|url| {
            hydrus_core::url::functions::check_full_url(&url).map_err(|e| e.to_string())?;
            Ok(url)
        })
        .and_then(|url| {
            registry
                .normalise(&url, true)
                .map(|normalised| (url, normalised))
                .map_err(|e| e.to_string())
        });
    match generated {
        Ok((url, normalised)) => {
            let matched = registry.class_for(&url).map_or_else(
                || "Did not match a known url class.".into(),
                |c| format!("Matched {} url class.", c.name),
            );
            (url, matched, normalised)
        }
        Err(e) => (
            format!("Could not generate - {e}"),
            String::new(),
            String::new(),
        ),
    }
}

/// A generator's row, including missing nested members.
pub fn gug_row(
    gug: &AnyGug,
    downloaders: &Downloaders,
    classes: &UrlClassSettings,
    options: GugOptions,
) -> Vec<String> {
    match gug {
        AnyGug::Single(g) => {
            let (_, _, normalised) = gug_preview(g, classes, options);
            let registry = UrlClasses::new(classes.clone());
            let class = registry
                .class_for(&normalised)
                .map_or_else(String::new, |c| c.name.clone());
            vec![
                g.name.clone(),
                if normalised.is_empty() {
                    "unable to parse example url".into()
                } else {
                    normalised
                },
                class,
            ]
        }
        AnyGug::Nested(n) => {
            let missing = n.gugs.iter().any(|(_, name)| {
                !downloaders
                    .gugs
                    .gugs
                    .iter()
                    .any(|g| matches!(g, AnyGug::Single(_)) && g.name() == name)
            });
            vec![
                n.name.clone(),
                n.gugs
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                if missing { "yes" } else { "" }.into(),
            ]
        }
    }
}

/// Values shown below the class editor's example URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPreview {
    pub status: String,
    pub normalised: String,
    pub request: String,
    pub api: String,
    pub referral: String,
    pub next: String,
}

/// Test the example and evaluate normalization, API, referral and pagination
/// using the actual typed URL class implementation.
pub fn class_preview(class: &UrlClass, collapse: bool) -> ClassPreview {
    let mut preview = ClassPreview {
        status: String::new(),
        normalised: String::new(),
        request: String::new(),
        api: String::new(),
        referral: String::new(),
        next: String::new(),
    };
    if let Err(e) = class.test(&class.example_url, collapse) {
        preview.status = format!("Example does not match - {e}");
        return preview;
    }
    preview.status = "Example matches ok!".into();
    preview.normalised = class
        .normalise(&class.example_url, false, collapse)
        .unwrap_or_else(|e| e.to_string());
    preview.request = class
        .normalise(&class.example_url, true, collapse)
        .unwrap_or_else(|e| e.to_string());
    preview.api = if class.uses_api_url() {
        match class.api_url(&class.example_url, collapse) {
            Ok(url) => {
                if class.matches(&url, collapse) {
                    preview.status = "Matches own API/Redirect URL!".into();
                }
                preview.request.clone_from(&url);
                url
            }
            Err(e) => {
                preview.status = "API/Redirect URL Problem!".into();
                format!("Could not convert - {e}")
            }
        }
    } else {
        "none set".into()
    };
    preview.referral = if class.uses_api_url() {
        "Not used, as API converter will redirect.".into()
    } else {
        use hydrus_core::url::class::ReferralMode;
        if class.referral.mode == ReferralMode::ConverterIfNoneProvided {
            format!(
                "normal referral url -or- {}",
                class
                    .referral_url(&preview.normalised, None, collapse)
                    .unwrap_or_else(|| "None".into())
            )
        } else {
            class
                .referral_url(&preview.normalised, Some("normal referral url"), collapse)
                .unwrap_or_else(|| "None".into())
        }
    };
    preview.next = if class.can_generate_next_gallery_page() {
        class
            .next_gallery_page(&preview.normalised, collapse)
            .unwrap_or_else(|e| format!("Could not convert - {e}"))
    } else {
        "none set".into()
    };
    preview
}

/// A path or query value's match and default, edited in isolation.
#[derive(Debug, Clone)]
pub struct RuleEdit {
    pub matcher: MatchEditor,
    pub name: String,
    pub default_enabled: bool,
    pub default: String,
    pub ephemeral: bool,
    /// Processor for generated ephemeral defaults, preserved and editable.
    pub default_processor: hydrus_core::url::StringProcessor,
}

impl RuleEdit {
    /// A query rule's value. Unlike path defaults, the reference permits
    /// query defaults that do not match the value test (ephemeral processors
    /// can supply a different value at request time).
    pub fn parameter_value(
        &self,
    ) -> Result<(hydrus_core::url::StringMatch, Option<String>), String> {
        Ok((
            self.matcher.value()?,
            self.default_enabled.then(|| self.default.clone()),
        ))
    }

    /// Validate both the match example and optional default.
    pub fn value(&self) -> Result<(hydrus_core::url::StringMatch, Option<String>), String> {
        let matcher = self.matcher.value()?;
        let default = self.default_enabled.then(|| self.default.clone());
        if let Some(d) = &default {
            matcher
                .test(d)
                .map_err(|_| "That default value does not match the rule!".to_owned())?;
        }
        Ok((matcher, default))
    }
}

/// A typed definition or component edited in its own child draft.
#[derive(Debug, Clone)]
pub enum EditValue {
    Class(Box<UrlClass>),
    Gug(AnyGug),
    Rule(RuleEdit, bool),
    Header(String, String),
}

/// Definition editor values, preview inputs and the current rule selection.
#[derive(Debug, Clone)]
pub struct DefinitionEditor {
    pub value: EditValue,
    pub tab: usize,
    pub rule_tab: usize,
    pub selected_rule: Option<usize>,
    pub classes: hydrus_core::url::UrlClassSettings,
    pub downloaders: Downloaders,
    pub gug_options: hydrus_core::url::GugOptions,
    pub gallery_position: usize,
    pub gallery_identifier: String,
    pub gallery_delta: String,
}

impl DefinitionEditor {
    /// Start an isolated editor from the current definition settings.
    pub fn new(value: EditValue, draft: &Draft) -> Self {
        let (gallery_position, gallery_identifier, gallery_delta) = match &value {
            EditValue::Class(c) => {
                c.gallery_index
                    .as_ref()
                    .map_or((0, String::new(), "1".into()), |g| match &g.position {
                        GalleryIndexPosition::PathComponent(i) => {
                            (1, i.to_string(), g.delta.to_string())
                        }
                        GalleryIndexPosition::Parameter(p) => (2, p.clone(), g.delta.to_string()),
                    })
            }
            _ => (0, String::new(), "1".into()),
        };
        Self {
            value,
            tab: 0,
            rule_tab: 0,
            selected_rule: None,
            classes: draft.classes.clone(),
            downloaders: draft.downloaders.clone(),
            gug_options: draft.gug_options,
            gallery_position,
            gallery_identifier,
            gallery_delta,
        }
    }

    /// Apply a text control edit to the typed draft.
    pub fn text(&mut self, id: i32, text: String) {
        match &mut self.value {
            EditValue::Class(c) => match id {
                0 => c.name = text,
                3 | 4 => {
                    let lines = text
                        .lines()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect();
                    let mask = &c.domain_mask;
                    c.domain_mask = if id == 3 {
                        DomainMask::new(
                            lines,
                            mask.domain_regexes.clone(),
                            mask.match_subdomains,
                            mask.keep_matched_subdomains,
                        )
                    } else {
                        DomainMask::new(
                            mask.raw_domains.clone(),
                            lines,
                            mask.match_subdomains,
                            mask.keep_matched_subdomains,
                        )
                    };
                }
                21 => self.gallery_identifier = text,
                22 => self.gallery_delta = text,
                40 => c.example_url = text,
                _ => (),
            },
            EditValue::Gug(AnyGug::Single(g)) => match id {
                0 => g.name = text,
                1 => g.url_template = text,
                2 => g.replacement_phrase = text,
                3 => g.separator = text,
                4 => g.initial_search_text = text,
                5 => g.example_search_text = text,
                _ => (),
            },
            EditValue::Gug(AnyGug::Nested(g)) => match id {
                0 => g.name = text,
                4 => g.initial_search_text = text,
                _ => (),
            },
            EditValue::Rule(r, _) => match id {
                0 => r.name = text,
                1 => r.default = text,
                101 => {
                    if r.matcher.match_type == 1 {
                        r.matcher.fixed = text;
                    } else {
                        r.matcher.regex = text;
                    }
                }
                103 => r.matcher.min_chars = text.parse().ok(),
                104 => r.matcher.max_chars = text.parse().ok(),
                105 => r.matcher.example = text,
                _ => (),
            },
            EditValue::Header(name, value) => match id {
                0 => *name = text,
                1 => *value = text,
                _ => (),
            },
        }
        self.update_gallery();
    }

    /// Apply a choice control, including URL-type defaults and pagination.
    pub fn choose(&mut self, id: i32, index: usize) {
        match &mut self.value {
            EditValue::Class(c) => match id {
                1 => {
                    c.url_type = [
                        UrlType::Post,
                        UrlType::Gallery,
                        UrlType::Watchable,
                        UrlType::File,
                    ]
                    .get(index)
                    .copied()
                    .unwrap_or(UrlType::Post);
                    c.should_be_associated_with_files =
                        matches!(c.url_type, UrlType::Post | UrlType::File);
                }
                2 => c.preferred_scheme = if index == 0 { "http" } else { "https" }.into(),
                15 => {
                    c.referral.mode = ReferralMode::from_code(i64::try_from(index).unwrap_or(0))
                        .unwrap_or_default();
                }
                20 => self.gallery_position = index,
                _ => (),
            },
            EditValue::Rule(r, _) => match id {
                100 => r.matcher.set_type(index),
                102 => {
                    if let Some((kind, _)) = CHARACTER_SETS.get(index) {
                        r.matcher.flexible = *kind;
                    }
                }
                _ => (),
            },
            _ => (),
        }
        self.update_gallery();
    }

    /// Apply a boolean control to the typed draft.
    pub fn toggle(&mut self, id: i32, on: bool) {
        match &mut self.value {
            EditValue::Class(c) => match id {
                5 | 6 => {
                    let m = &c.domain_mask;
                    c.domain_mask = DomainMask::new(
                        m.raw_domains.clone(),
                        m.domain_regexes.clone(),
                        if id == 5 { on } else { m.match_subdomains },
                        if id == 6 {
                            on
                        } else {
                            m.keep_matched_subdomains
                        },
                    );
                }
                7 => c.no_more_path_components_than_this = on,
                8 => c.no_more_parameters_than_this = on,
                9 => c.has_single_value_parameters = on,
                10 => c.alphabetise_get_parameters = on,
                11 => c.keep_extra_parameters_for_server = on,
                12 => c.can_produce_multiple_files = on,
                13 => c.should_be_associated_with_files = on,
                14 => c.keep_fragment = on,
                _ => (),
            },
            EditValue::Rule(r, _) => match id {
                2 => r.default_enabled = on,
                3 => r.ephemeral = on,
                _ => (),
            },
            _ => (),
        }
    }

    fn update_gallery(&mut self) {
        if let EditValue::Class(c) = &mut self.value {
            c.gallery_index = match self.gallery_position {
                1 => self
                    .gallery_identifier
                    .parse()
                    .ok()
                    .map(GalleryIndexPosition::PathComponent),
                2 => Some(GalleryIndexPosition::Parameter(
                    self.gallery_identifier.clone(),
                )),
                _ => None,
            }
            .map(|position| GalleryIndex {
                position,
                delta: self.gallery_delta.parse().unwrap_or(1),
            });
        }
    }

    /// Evaluate the current example with the actual native rules.
    pub fn preview(&self) -> String {
        match &self.value {
            EditValue::Class(c) => {
                let p = class_preview(c, self.classes.collapse_leading_slashes);
                format!(
                    "{}\nnormalised url: {}\nrequest url: {}\napi url: {}\nreferral url: {}\nnext gallery page: {}",
                    p.status, p.normalised, p.request, p.api, p.referral, p.next
                )
            }
            EditValue::Gug(AnyGug::Single(g)) => {
                let (raw, matched, normalised) = gug_preview(g, &self.classes, self.gug_options);
                format!("raw url: {raw}\nmatches as a: {matched}\nnormalised url: {normalised}")
            }
            EditValue::Gug(AnyGug::Nested(g)) => self
                .downloaders
                .gugs
                .gallery_urls(
                    &AnyGug::Nested(g.clone()),
                    g.initial_search_text.as_str(),
                    self.gug_options,
                )
                .map_or_else(
                    |e| e.to_string(),
                    |urls| format!("example URLs:\n{}", urls.join("\n")),
                ),
            EditValue::Rule(r, _) => r.matcher.test_result().map_or_else(String::new, |p| p.0),
            EditValue::Header(..) => String::new(),
        }
    }

    /// The current path, query, header or single-value rule rows.
    pub fn rules(&self) -> Vec<String> {
        let EditValue::Class(c) = &self.value else {
            return Vec::new();
        };
        match self.rule_tab {
            0 => c
                .path_components
                .iter()
                .enumerate()
                .map(|(i, (m, d))| {
                    format!(
                        "{}: {}{}",
                        i,
                        m.describe(false, true),
                        default_summary(d.as_deref())
                    )
                })
                .collect(),
            1 => c
                .parameters
                .iter()
                .map(|p| {
                    format!(
                        "{}: {}{}{}",
                        p.name,
                        p.value.describe(false, true),
                        default_summary(p.default.as_deref()),
                        if p.ephemeral { " (ephemeral)" } else { "" }
                    )
                })
                .collect(),
            2 => c
                .header_overrides
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect(),
            _ => vec![c.single_value_parameters_match.describe(false, true)],
        }
    }

    /// Check examples, defaults, parameter names and gallery index inputs.
    pub fn validate(&self) -> Result<(), String> {
        match &self.value {
            EditValue::Class(c) => {
                validate_class(c, self.classes.collapse_leading_slashes)?;
                if self.gallery_position > 0 {
                    if c.gallery_index.is_none() {
                        return Err("Please enter a path component index.".into());
                    }
                    let delta = self
                        .gallery_delta
                        .parse::<i64>()
                        .map_err(|_| "Please enter a page delta from 1 to 65536.".to_owned())?;
                    if !(1..=65536).contains(&delta) {
                        return Err("Please enter a page delta from 1 to 65536.".into());
                    }
                }
                Ok(())
            }
            EditValue::Gug(AnyGug::Single(g)) => validate_gug(g),
            EditValue::Gug(AnyGug::Nested(_)) => Ok(()),
            EditValue::Rule(r, parameter) => {
                if *parameter {
                    r.parameter_value()?;
                } else {
                    r.value()?;
                }
                if *parameter {
                    validate_parameter_name(&r.name, &[])?;
                }
                Ok(())
            }
            EditValue::Header(name, _) => {
                if name.is_empty() {
                    Err("Please enter a header name.".into())
                } else {
                    Ok(())
                }
            }
        }
    }
}

fn default_summary(default: Option<&str>) -> String {
    default.map_or_else(String::new, |d| format!(" (default: {d})"))
}
