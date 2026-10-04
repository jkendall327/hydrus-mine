//! The reference GUG key/name button and two-stage gallery selection.

use hydrus_core::url::{AnyGug, GugOptions, Gugs, UrlClassSettings, UrlClasses, UrlType};
use hydrus_parse::Downloaders;

pub type KeyAndName = (String, String);

pub const OTHER: &str = "--other galleries…";
pub const NON_FUNCTIONAL: &str = "--non-functional galleries…";
pub const NOTHING_TO_SELECT: &str = "Hey, you do not have any downloaders set up in this client, so there is nothing to select!\n\nCheck the _network->downloaders_ menu to find downloaders made by users.";
pub const NO_SUBSCRIPTION: &str = "Hey, you do not have any downloaders set up in this client, so you cannot create a new subscription yet!\n\nCheck the _network->downloaders_ menu to find downloaders made by users.";

/// Existing keys win over names; name fallback refreshes both saved fields.
/// A missing entry stays available as a missing caption rather than disappearing.
pub fn resolve(gugs: &Gugs, current: Option<KeyAndName>) -> Option<KeyAndName> {
    current.and_then(|(key, name)| {
        if name.is_empty() {
            None
        } else {
            Some(gugs.get(&key, &name).map_or((key, name), key_and_name))
        }
    })
}

fn key_and_name(gug: &AnyGug) -> KeyAndName {
    (gug.key().to_owned(), gug.name().to_owned())
}

pub fn caption(gugs: &Gugs, current: Option<&KeyAndName>) -> String {
    match current {
        None => "no downloader set".into(),
        Some((_, name)) if name.is_empty() => "no downloader set".into(),
        Some((key, name)) if gugs.get(key, name).is_none() => format!("not found: {name}"),
        Some((_, name)) => name.clone(),
    }
}

/// Check actual URL matching, API conversion and installed parser definitions.
/// Nested missing members are skipped, as the reference does, including an empty
/// nested GUG. Cycles, which the reference editor cannot create, are rejected.
pub fn check_functional(
    gug: &AnyGug,
    downloaders: &Downloaders,
    classes: &UrlClassSettings,
    options: GugOptions,
) -> Result<(), String> {
    let mut classes = classes.clone();
    classes.parser_keys = downloaders.parsers.iter().map(|p| p.key.clone()).collect();
    let registry = UrlClasses::new(classes);
    check(gug, &downloaders.gugs, &registry, options, &mut Vec::new())
}

fn check(
    gug: &AnyGug,
    gugs: &Gugs,
    registry: &UrlClasses,
    options: GugOptions,
    visiting: &mut Vec<String>,
) -> Result<(), String> {
    if visiting.iter().any(|key| key == gug.key()) {
        return Err("Unusual error: cyclic nested gallery".into());
    }
    visiting.push(gug.key().to_owned());
    let result = match gug {
        AnyGug::Single(single) => {
            let url = single
                .example_url(options)
                .map_err(|e| format!("Unusual error: {e}"))?;
            let capability = registry.parse_capability(&url);
            if capability.url_type == UrlType::Unknown {
                Err("No URL Class for example URL!".into())
            } else {
                capability
                    .parser
                    .map_err(|reason| format!("Cannot parse {}: {reason}", capability.match_name))
            }
        }
        AnyGug::Nested(nested) => nested
            .gugs
            .iter()
            .filter_map(|(key, name)| gugs.get(key, name))
            .try_for_each(|member| check(member, gugs, registry, options, visiting)),
    };
    visiting.pop();
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Gallery(KeyAndName),
    Other,
    NonFunctional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub choice: Choice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Main,
    Other,
    NonFunctional,
}

#[derive(Debug, Clone)]
pub struct Selector {
    pub current: Option<KeyAndName>,
    pub group: Group,
    pub selected: Option<usize>,
    main: Vec<Entry>,
    other: Vec<Entry>,
    non_functional: Vec<Entry>,
    pub warning: Option<&'static str>,
}

impl Selector {
    pub fn new(
        downloaders: &Downloaders,
        classes: &UrlClassSettings,
        options: GugOptions,
        current: Option<KeyAndName>,
        for_subscription: bool,
    ) -> Self {
        let current = resolve(&downloaders.gugs, current);
        let mut out = Self {
            current,
            group: Group::Main,
            selected: None,
            main: Vec::new(),
            other: Vec::new(),
            non_functional: Vec::new(),
            warning: downloaders
                .gugs
                .gugs
                .is_empty()
                .then_some(if for_subscription {
                    NO_SUBSCRIPTION
                } else {
                    NOTHING_TO_SELECT
                }),
        };
        let mut gugs: Vec<_> = downloaders.gugs.gugs.iter().collect();
        gugs.sort_by(|a, b| a.name().cmp(b.name()));
        for gug in gugs {
            let functional = check_functional(gug, downloaders, classes, options);
            let entry = Entry {
                label: functional.as_ref().map_or_else(
                    |error| format!("{} ({error})", gug.name()),
                    |()| gug.name().to_owned(),
                ),
                choice: Choice::Gallery(key_and_name(gug)),
            };
            if functional.is_err() {
                out.non_functional.push(entry);
            } else if downloaders
                .gugs
                .keys_to_display
                .iter()
                .any(|key| key == gug.key())
            {
                out.main.push(entry);
            } else {
                out.other.push(entry);
            }
        }
        if !out.other.is_empty() {
            out.main.push(Entry {
                label: OTHER.into(),
                choice: Choice::Other,
            });
        }
        if !out.non_functional.is_empty() {
            out.main.push(Entry {
                label: NON_FUNCTIONAL.into(),
                choice: Choice::NonFunctional,
            });
        }
        out.select_current();
        out
    }

    pub fn entries(&self) -> &[Entry] {
        match self.group {
            Group::Main => &self.main,
            Group::Other => &self.other,
            Group::NonFunctional => &self.non_functional,
        }
    }

    fn select_current(&mut self) {
        self.selected = self.entries().iter().position(|entry| {
            matches!(&entry.choice, Choice::Gallery(value) if Some(value) == self.current.as_ref())
        }).or_else(|| (!self.entries().is_empty()).then_some(0));
    }

    /// A category opens its second chooser; a gallery accepts its exact pair.
    pub fn accept(&mut self) -> Option<KeyAndName> {
        match self
            .selected
            .and_then(|index| self.entries().get(index))
            .map(|e| e.choice.clone())?
        {
            Choice::Gallery(value) => Some(value),
            Choice::Other => {
                self.group = Group::Other;
                self.select_current();
                None
            }
            Choice::NonFunctional => {
                self.group = Group::NonFunctional;
                self.select_current();
                None
            }
        }
    }
}
