//! Options' namespace colour draft, including protected default rows and tag-list selection.
use crate::write_autocomplete::Selection;

/// Namespace RGB entries; `None` is the namespaced fallback and `Some("")` unnamespaced.
pub type Colours = Vec<(Option<String>, [u8; 3])>;

/// A painted namespace entry in the reference's lexical display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub namespace: Option<String>,
    pub label: String,
    pub rgb: [u8; 3],
    pub selected: bool,
}

/// A detached namespace list draft; mutations never write settings themselves.
#[derive(Debug)]
pub struct Editor {
    colours: Colours,
    selection: Selection,
}
impl Editor {
    /// Start a private list from saved or already staged namespace entries.
    pub fn new(colours: Colours) -> Self {
        let mut editor = Self {
            colours,
            selection: Selection::default(),
        };
        editor.sort();
        editor
    }
    fn label(namespace: Option<&str>) -> String {
        match namespace {
            None => "namespaced tags".into(),
            Some("") => "unnamespaced tags".into(),
            // Qt prettifies only slices containing exactly one colon. A nested
            // namespace plus its final colon remains a literal tag slice.
            Some(namespace) if namespace.contains(':') => format!("{namespace}:"),
            Some(namespace) => format!("'{namespace}' tags"),
        }
    }
    fn sort(&mut self) {
        self.colours
            .sort_by_key(|(namespace, _)| Self::label(namespace.as_deref()));
    }
    /// Current rows with a stable semantic namespace identity.
    pub fn rows(&self) -> Vec<Row> {
        self.colours
            .iter()
            .enumerate()
            .map(|(index, (namespace, rgb))| Row {
                namespace: namespace.clone(),
                label: Self::label(namespace.as_deref()),
                rgb: *rgb,
                selected: self.selection.selected.contains(&index),
            })
            .collect()
    }
    /// The staged RGB mapping, excluding selection.
    pub fn values(&self) -> Colours {
        self.colours.clone()
    }
    /// Real tag-list Ctrl/Shift selection; out-of-range callbacks are harmless.
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        if index < self.colours.len() {
            self.selection.click(index, ctrl, shift);
        }
    }
    /// Add after the exact namespace normalization/warnings. RGB comes from the owner.
    pub fn add(&mut self, raw: &str, rgb: [u8; 3]) -> Result<(), &'static str> {
        let namespace = raw
            .to_lowercase()
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
            .to_owned();
        if namespace.is_empty() || namespace == ":" {
            return Err(
                "Sorry, that namespace means unnamespaced/default namespaced, which are already listed.",
            );
        }
        let mut namespace = namespace.trim_end_matches(':').to_owned();
        if namespace != "system" {
            namespace = hydrus_core::tag::strip_tag_text_of_gumpf(&namespace);
        }
        if self
            .colours
            .iter()
            .any(|(existing, _)| existing.as_deref() == Some(namespace.as_str()))
        {
            return Err("Sorry, that namespace is already listed!");
        }
        let old: Vec<_> = self
            .colours
            .iter()
            .map(|(namespace, _)| namespace.clone())
            .collect();
        self.colours.push((Some(namespace), rgb));
        self.sort();
        let new: Vec<_> = self
            .colours
            .iter()
            .map(|(namespace, _)| namespace.clone())
            .collect();
        // Qt keeps positional last-hit/anchor values after sorting. Only its
        // selected term set follows namespace identities; range bookkeeping is numeric.
        self.selection.remap_selected(|index| {
            old.get(index)
                .and_then(|namespace| new.iter().position(|entry| entry == namespace))
        });
        Ok(())
    }
    /// Add with fresh random RGB channels, as the visible Add control does.
    pub fn add_random(&mut self, raw: &str) -> Result<(), &'static str> {
        self.add(raw, rand::random())
    }
    /// Protected default rows alone offer no destructive question.
    pub fn removal_question(&self) -> Option<&'static str> {
        self.colours
            .iter()
            .enumerate()
            .any(|(index, (namespace, _))| {
                self.selection.selected.contains(&index)
                    && namespace.as_deref().is_some_and(|value| !value.is_empty())
            })
            .then_some("Delete all selected colours?")
    }
    /// Accept deletion of selected ordinary namespaces, keeping both default rows.
    pub fn remove_selected(&mut self) {
        let kept: Vec<_> = self
            .rows()
            .into_iter()
            .filter(|row| !row.selected || row.namespace.as_deref().is_none_or(str::is_empty))
            .collect();
        self.colours = kept
            .iter()
            .map(|row| (row.namespace.clone(), row.rgb))
            .collect();
        self.selection = Selection::default();
        for (index, row) in kept.iter().enumerate() {
            if row.selected {
                self.selection.selected.insert(index);
            }
        }
    }
}
