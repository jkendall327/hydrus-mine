//! How tags are shown to the user (`ClientTags.RenderTag` with
//! `render_for_user`, and the tag lists' sort): namespaces shown or hidden,
//! the connector between namespace and subtag, underscores and emojis
//! replaced, and the user's namespace order and default tag sorts.

use serde::{Deserialize, Serialize};

use crate::tag::split_tag;
use crate::tag_sort::{TagSort, default_user_namespaces};

/// The four display modes offered to newly opened media tag lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagDisplayType {
    Storage,
    Display,
    SingleMedia,
    SelectionList,
}
impl TagDisplayType {
    pub const CHOICES: [Self; 4] = [
        Self::SelectionList,
        Self::SingleMedia,
        Self::Display,
        Self::Storage,
    ];
    pub const LABELS: [&str; 4] = [
        "multiple media view tags",
        "single media view tags",
        "display tags",
        "stored tags",
    ];
    pub fn code(self) -> i64 {
        match self {
            Self::Storage => 0,
            Self::Display => 1,
            Self::SingleMedia => 2,
            Self::SelectionList => 3,
        }
    }
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Storage),
            1 => Some(Self::Display),
            2 => Some(Self::SingleMedia),
            3 => Some(Self::SelectionList),
            _ => None,
        }
    }
    pub fn choice(self) -> usize {
        match self {
            Self::SelectionList => 0,
            Self::SingleMedia => 1,
            Self::Display => 2,
            Self::Storage => 3,
        }
    }
    pub fn from_choice(choice: usize) -> Option<Self> {
        Self::CHOICES.get(choice).copied()
    }
}

/// The user's tag presentation options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagPresentation {
    /// `show_namespaces`.
    pub show_namespaces: bool,
    /// Show the namespace when it is a number (`show_number_namespaces`).
    pub show_number_namespaces: bool,
    /// Show the namespace when the subtag is a number
    /// (`show_subtag_number_namespaces`).
    pub show_subtag_number_namespaces: bool,
    /// Between namespace and subtag (`namespace_connector`).
    pub namespace_connector: String,
    /// Between a raw alias and its ideal sibling (`sibling_connector`).
    pub sibling_connector: String,
    /// Cap sorted thumbnail items counted when none are selected; `None` has no limit.
    pub unselected_tag_limit: Option<u32>,
    /// Opening default for page sidebar tag lists; existing lists retain their mode.
    pub sidebar_display_type: TagDisplayType,
    /// Opening default for media viewer tag lists.
    pub viewer_display_type: TagDisplayType,
    /// `replace_tag_underscores_with_spaces`.
    pub replace_underscores: bool,
    /// `replace_tag_emojis_with_boxes`.
    pub replace_emojis: bool,
    /// The order of namespaces when grouping by them
    /// (`user_namespace_group_by_sort`; `""` is unnamespaced, `":"` any
    /// other namespace).
    pub user_namespaces: Vec<String>,
    /// The search page's tag list sort (`default_tag_sorts`).
    pub search_page_sort: TagSort,
    /// The media viewer's.
    pub media_viewer_sort: TagSort,
}

impl Default for TagPresentation {
    fn default() -> Self {
        Self {
            show_namespaces: true,
            show_number_namespaces: true,
            show_subtag_number_namespaces: true,
            namespace_connector: ":".into(),
            sibling_connector: " → ".into(),
            unselected_tag_limit: Some(4096),
            sidebar_display_type: TagDisplayType::SelectionList,
            viewer_display_type: TagDisplayType::SingleMedia,
            replace_underscores: false,
            replace_emojis: false,
            user_namespaces: default_user_namespaces(),
            search_page_sort: TagSort::DEFAULT,
            media_viewer_sort: TagSort::DEFAULT,
        }
    }
}

impl TagPresentation {
    /// A tag as the user sees it (`RenderTag(tag, render_for_user = True)`).
    pub fn render(&self, tag: &str) -> String {
        let replaced;
        let tag = if self.replace_underscores {
            replaced = tag.replace('_', " ");
            replaced.as_str()
        } else {
            tag
        };
        let (namespace, subtag) = split_tag(tag);
        let result = if namespace.is_empty() {
            subtag.to_owned()
        } else if self.show_namespaces
            || (self.show_number_namespaces && is_decimal(namespace))
            || (self.show_subtag_number_namespaces && is_decimal(subtag))
        {
            format!("{namespace}{}{subtag}", self.namespace_connector)
        } else {
            return subtag.to_owned();
        };
        if self.replace_emojis {
            replace_emojis(&result)
        } else {
            result
        }
    }
}

/// The first of each block of ten Unicode decimal digits (category Nd), as
/// Python 3.11's `str.isdecimal` knows them (Unicode 14).
const DECIMAL_ZEROS: [u32; 66] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650,
    0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x16A60, 0x16AC0, 0x16B50,
    0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E950, 0x1FBF0,
];

/// Python's `str.isdecimal`: not empty, and every character a decimal digit.
pub fn is_decimal(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            let c = u32::from(c);
            DECIMAL_ZEROS
                .iter()
                .any(|&zero| (zero..zero + 10).contains(&c))
        })
}

/// The colours tags and predicates are listed in, by namespace (the old
/// `namespace_colours` option, and `or_connector_custom_namespace_colour`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamespaceColours {
    /// RGB by namespace: `Some("")` is unnamespaced tags', `None` that of
    /// any namespace without its own.
    pub colours: Vec<(Option<String>, [u8; 3])>,
    /// The namespace whose colour an OR predicate's row takes (`None`: the
    /// default).
    pub or_connector: Option<String>,
}

impl Default for NamespaceColours {
    /// The reference's (`ClientDefaults`).
    fn default() -> Self {
        let colour = |namespace: Option<&str>, rgb| (namespace.map(str::to_owned), rgb);
        Self {
            colours: vec![
                colour(Some("system"), [153, 101, 21]),
                colour(Some("meta"), [0, 0, 0]),
                colour(Some("creator"), [170, 0, 0]),
                colour(Some("studio"), [128, 0, 0]),
                colour(Some("character"), [0, 170, 0]),
                colour(Some("person"), [0, 128, 0]),
                colour(Some("series"), [170, 0, 170]),
                colour(None, [114, 160, 193]),
                colour(Some(""), [0, 111, 250]),
            ],
            or_connector: None,
        }
    }
}

impl NamespaceColours {
    /// A namespace's colour, else the default's
    /// (`_GetRowsOfTextsAndColours`).
    pub fn colour(&self, namespace: Option<&str>) -> [u8; 3] {
        let find = |key: Option<&str>| {
            self.colours
                .iter()
                .find(|(n, _)| n.as_deref() == key)
                .map(|(_, rgb)| *rgb)
        };
        find(namespace)
            .or_else(|| find(None))
            .unwrap_or([114, 160, 193])
    }

    /// A tag's colour: its namespace's.
    pub fn tag(&self, tag: &str) -> [u8; 3] {
        self.colour(Some(split_tag(tag).0))
    }

    /// A predicate's colour (`Predicate.GetNamespace`): a system
    /// predicate's is `system`'s, a namespace or tag predicate's its
    /// namespace's (`*` for a wildcard in the namespace), an OR's the OR
    /// connector's.
    pub fn predicate(&self, predicate: &crate::search::predicate::Predicate) -> [u8; 3] {
        use crate::search::predicate::Predicate;
        match predicate {
            Predicate::System(_) => self.colour(Some("system")),
            Predicate::Namespace { namespace, .. } => self.colour(Some(namespace)),
            Predicate::Tag { tag, .. } => self.tag(tag.as_ref()),
            Predicate::Wildcard { pattern, .. } => self.wildcard(pattern.as_str()),
            Predicate::Or(_) => self.colour(self.or_connector.as_deref()),
        }
    }

    /// The colour of a predicate as typed (`-` for exclusion), as the
    /// autocomplete offers it: a tag, a wildcard or a `system:` predicate.
    pub fn predicate_text(&self, text: &str) -> [u8; 3] {
        let text = text.strip_prefix('-').unwrap_or(text);
        if text.starts_with("system:") {
            self.colour(Some("system"))
        } else {
            self.wildcard(text)
        }
    }

    fn wildcard(&self, pattern: &str) -> [u8; 3] {
        let namespace = split_tag(pattern).0;
        self.colour(Some(if namespace.contains('*') {
            "*"
        } else {
            namespace
        }))
    }
}

/// The ranges of the reference's `emoji_pattern`.
const EMOJI_RANGES: [(u32, u32); 12] = [
    (0x1F600, 0x1F64F),
    (0x1F300, 0x1F5FF),
    (0x1F680, 0x1F6FF),
    (0x1F700, 0x1F77F),
    (0x1F780, 0x1F7FF),
    (0x1F800, 0x1F8FF),
    (0x1F900, 0x1F9FF),
    (0x1FA00, 0x1FA6F),
    (0x1FA70, 0x1FAFF),
    (0x2600, 0x26FF),
    (0x2702, 0x27B0),
    (0x3000, 0x303F),
];

fn is_emoji(c: char) -> bool {
    let c = u32::from(c);
    EMOJI_RANGES.iter().any(|&(a, b)| (a..=b).contains(&c))
}

/// `emoji_pattern.sub('□', text)`: each run of emojis, with a variation
/// selector after it, becomes one box.
fn replace_emojis(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if is_emoji(c) {
            while chars.next_if(|&c| is_emoji(c)).is_some() {}
            chars.next_if_eq(&'\u{FE0F}');
            out.push('□');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_take_their_namespace_s_colour() {
        use crate::search::predicate::{Predicate, SystemPredicate, Wildcard};
        let colours = NamespaceColours::default();
        let character = [0, 170, 0];
        let unnamespaced = [0, 111, 250];
        let other = [114, 160, 193];
        let system = [153, 101, 21];
        assert_eq!(colours.tag("character:samus aran"), character);
        assert_eq!(colours.tag("blue eyes"), unnamespaced);
        assert_eq!(colours.tag("title:a test"), other);
        // (an unnamespaced tag with a colon is stored with a leading one)
        assert_eq!(colours.tag("::)"), unnamespaced);
        let tag = |t: &str, inclusive| Predicate::Tag {
            tag: crate::Tag::new(t).unwrap(),
            inclusive,
        };
        assert_eq!(colours.predicate(&tag("character:link", false)), character);
        assert_eq!(
            colours.predicate(&Predicate::System(SystemPredicate::Inbox)),
            system
        );
        assert_eq!(
            colours.predicate(&Predicate::Namespace {
                namespace: "series".into(),
                inclusive: true
            }),
            [170, 0, 170]
        );
        let wildcard = |p: &str| Predicate::Wildcard {
            pattern: Wildcard::from_clean(p),
            inclusive: true,
        };
        assert_eq!(colours.predicate(&wildcard("character:sam*")), character);
        assert_eq!(colours.predicate(&wildcard("char*:sam")), other);
        let or = Predicate::Or(vec![tag("character:link", true), tag("blue eyes", true)]);
        assert_eq!(colours.predicate(&or), other);
        assert_eq!(colours.predicate_text("-character:link"), character);
        assert_eq!(colours.predicate_text("system:archive"), system);
        assert_eq!(colours.predicate_text("blue*"), unnamespaced);
        // the user's, an OR connector's namespace, and a missing default
        let custom = NamespaceColours {
            colours: vec![(Some("meta".into()), [1, 2, 3])],
            or_connector: Some("meta".into()),
        };
        assert_eq!(custom.predicate(&or), [1, 2, 3]);
        assert_eq!(custom.tag("character:link"), other);
    }

    #[test]
    fn decimals_are_python_s() {
        assert!(is_decimal("2024"));
        assert!(is_decimal("١٢")); // Arabic-Indic
        assert!(is_decimal("１２")); // full-width
        assert!(!is_decimal(""));
        assert!(!is_decimal("²")); // a digit, but not a decimal
        assert!(!is_decimal("½"));
        assert!(!is_decimal("12a"));
    }
}
