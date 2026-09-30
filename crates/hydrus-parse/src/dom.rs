//! HTML documents as the reference's parsers see them.
//!
//! The reference parses HTML with html5lib (the WHATWG algorithm, scripting
//! disabled) into BeautifulSoup trees, and its formulas use BeautifulSoup's
//! search, text and serialisation. html5ever builds the same trees; this
//! module keeps them in an arena and reproduces the BeautifulSoup behaviour
//! the formulas rely on:
//!
//! - attributes are held sorted by name (html5lib hands them over sorted);
//! - `class`, `rel` and a few others are lists of whitespace-separated
//!   values, matched per value or as a whole and serialised re-joined;
//! - `find_all` / sibling searches by tag name and attribute values;
//! - the serialisation `str(tag)` gives (`<br/>`, `&amp;` in text,
//!   quotes chosen around attribute values, script and style left raw).

use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData as RcData, RcDom};

/// A node's index in its document.
pub type NodeId = usize;

/// An attribute's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    Single(String),
    /// A whitespace-separated list attribute (`class`, an `<a>`'s `rel`, ...).
    List(Vec<String>),
}

impl AttrValue {
    /// The value as one string (a list re-joined with single spaces).
    pub fn joined(&self) -> String {
        match self {
            AttrValue::Single(s) => s.clone(),
            AttrValue::List(items) => items.join(" "),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeData {
    /// The document itself (BeautifulSoup calls it `[document]`).
    Document,
    /// `<!DOCTYPE ...>`, as the text BeautifulSoup keeps for it.
    Doctype(String),
    Element {
        name: String,
        /// Sorted by name.
        attrs: Vec<(String, AttrValue)>,
    },
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

/// A parsed HTML document.
#[derive(Debug, Clone)]
pub struct Html {
    nodes: Vec<Node>,
}

/// Attributes BeautifulSoup splits into lists, for any tag (`*`) or one tag.
fn is_list_attribute(tag: &str, attr: &str) -> bool {
    matches!(attr, "class" | "accesskey" | "dropzone")
        || matches!(
            (tag, attr),
            ("a" | "link", "rel" | "rev")
                | ("td" | "th", "headers")
                | ("form", "accept-charset")
                | ("object", "archive")
                | ("area", "rel")
                | ("icon", "sizes")
                | ("iframe", "sandbox")
                | ("output", "for")
        )
}

/// Python's `str.isspace` (what `\s` matches in a Python regex).
pub fn is_py_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

/// Elements BeautifulSoup writes as `<name/>` when empty.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "keygen"
            | "link"
            | "menuitem"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
            | "basefont"
            | "bgsound"
            | "command"
            | "frame"
            | "image"
            | "isindex"
            | "nextid"
            | "spacer"
    )
}

impl Html {
    /// Parse a document as html5lib would (scripting disabled).
    pub fn parse(text: &str) -> Html {
        let opts = ParseOpts {
            tree_builder: TreeBuilderOpts {
                scripting_enabled: false,
                ..TreeBuilderOpts::default()
            },
            ..ParseOpts::default()
        };
        let dom = parse_document(RcDom::default(), opts).one(text);
        let mut html = Html {
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }],
        };
        // iterative, so deeply nested pages can't overflow the stack
        let mut stack: Vec<(Handle, NodeId)> = dom
            .document
            .children
            .borrow()
            .iter()
            .rev()
            .map(|h| (h.clone(), 0))
            .collect();
        while let Some((handle, parent)) = stack.pop() {
            let data = match &handle.data {
                // (the HTML parser makes `<?...>` a comment; nothing else
                // makes processing instructions)
                RcData::Document | RcData::ProcessingInstruction { .. } => continue,
                RcData::Doctype {
                    name,
                    public_id,
                    system_id,
                } => NodeData::Doctype(doctype_text(name, public_id, system_id)),
                RcData::Text { contents } => NodeData::Text(contents.borrow().to_string()),
                RcData::Comment { contents } => NodeData::Comment(contents.to_string()),
                RcData::Element { name, attrs, .. } => {
                    let tag = name.local.to_string();
                    let mut attributes: Vec<(String, AttrValue)> = attrs
                        .borrow()
                        .iter()
                        .map(|a| {
                            let key = match &a.name.prefix {
                                Some(prefix) => format!("{prefix}:{}", a.name.local),
                                None => a.name.local.to_string(),
                            };
                            let value = if is_list_attribute(&tag, &key) {
                                AttrValue::List(
                                    a.value
                                        .split(is_py_space)
                                        .filter(|v| !v.is_empty())
                                        .map(str::to_owned)
                                        .collect(),
                                )
                            } else {
                                AttrValue::Single(a.value.to_string())
                            };
                            (key, value)
                        })
                        .collect();
                    attributes.sort_by(|a, b| a.0.cmp(&b.0));
                    NodeData::Element {
                        name: tag,
                        attrs: attributes,
                    }
                }
            };
            let id = html.nodes.len();
            html.nodes.push(Node {
                parent: Some(parent),
                children: Vec::new(),
                data,
            });
            html.nodes[parent].children.push(id);
            // a template's contents are its children, as html5lib has them
            let mut children: Vec<Handle> = handle.children.borrow().clone();
            if let RcData::Element {
                template_contents, ..
            } = &handle.data
                && let Some(contents) = template_contents.borrow().as_ref()
            {
                children.extend(contents.children.borrow().iter().cloned());
            }
            for child in children.into_iter().rev() {
                stack.push((child, id));
            }
        }
        html.merge_adjacent_text();
        html
    }

    /// Adjacent text nodes (e.g. around a dropped node) are one string.
    fn merge_adjacent_text(&mut self) {
        for id in 0..self.nodes.len() {
            let children = std::mem::take(&mut self.nodes[id].children);
            let mut kept: Vec<NodeId> = Vec::with_capacity(children.len());
            for child in children {
                if let (Some(&last), NodeData::Text(text)) = (kept.last(), &self.nodes[child].data)
                    && let NodeData::Text(_) = self.nodes[last].data
                {
                    let text = text.clone();
                    if let NodeData::Text(previous) = &mut self.nodes[last].data {
                        previous.push_str(&text);
                    }
                    self.nodes[child].parent = None;
                    continue;
                }
                kept.push(child);
            }
            self.nodes[id].children = kept;
        }
    }

    /// The document node.
    pub fn root(&self) -> NodeId {
        0
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// The tag name (`[document]` for the document); `None` for text.
    pub fn name(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].data {
            NodeData::Document => Some("[document]"),
            NodeData::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Whether the node is a tag (an element or the document), which is what
    /// BeautifulSoup's searches return.
    pub fn is_tag(&self, id: NodeId) -> bool {
        matches!(
            self.nodes[id].data,
            NodeData::Element { .. } | NodeData::Document
        )
    }

    pub fn attr(&self, id: NodeId, name: &str) -> Option<&AttrValue> {
        match &self.nodes[id].data {
            NodeData::Element { attrs, .. } => {
                attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v)
            }
            _ => None,
        }
    }

    /// Every node below `id`, in document order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.nodes[id].children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    /// Whether an element matches a name and attribute values, as
    /// BeautifulSoup's `find_all(name, attrs)` tests it.
    pub fn matches(&self, id: NodeId, name: Option<&str>, attrs: &[(String, String)]) -> bool {
        let NodeData::Element {
            name: tag,
            attrs: tag_attrs,
        } = &self.nodes[id].data
        else {
            return false;
        };
        if name.is_some_and(|n| n != tag) {
            return false;
        }
        attrs.iter().all(|(key, wanted)| {
            match tag_attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
                None => false,
                Some(AttrValue::Single(v)) => v == wanted,
                Some(AttrValue::List(items)) => {
                    items.iter().any(|v| v == wanted) || items.join(" ") == *wanted
                }
            }
        })
    }

    /// `find_all`: matching elements below `id`, in document order.
    pub fn find_all(
        &self,
        id: NodeId,
        name: Option<&str>,
        attrs: &[(String, String)],
    ) -> Vec<NodeId> {
        self.descendants(id)
            .into_iter()
            .filter(|&n| self.matches(n, name, attrs))
            .collect()
    }

    fn siblings(&self, id: NodeId) -> (&[NodeId], usize) {
        let Some(parent) = self.nodes[id].parent else {
            return (&[], 0);
        };
        let siblings = &self.nodes[parent].children;
        let at = siblings.iter().position(|&s| s == id).unwrap_or(0);
        (siblings, at)
    }

    /// `find_next_siblings`: matching later siblings, nearest first.
    pub fn next_siblings(
        &self,
        id: NodeId,
        name: Option<&str>,
        attrs: &[(String, String)],
    ) -> Vec<NodeId> {
        let (siblings, at) = self.siblings(id);
        siblings
            .iter()
            .skip(at + 1)
            .copied()
            .filter(|&n| self.matches(n, name, attrs))
            .collect()
    }

    /// `find_previous_siblings`: matching earlier siblings, nearest first.
    pub fn previous_siblings(
        &self,
        id: NodeId,
        name: Option<&str>,
        attrs: &[(String, String)],
    ) -> Vec<NodeId> {
        let (siblings, at) = self.siblings(id);
        siblings[..at.min(siblings.len())]
            .iter()
            .rev()
            .copied()
            .filter(|&n| self.matches(n, name, attrs))
            .collect()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    /// The reference's `GetHTMLTagString`: every string below the node
    /// (comments included), with a newline for each `<br>` and at the start
    /// of each `<p>`.
    pub fn text(&self, id: NodeId) -> String {
        let mut out = String::new();
        for n in self.descendants(id) {
            match &self.nodes[n].data {
                NodeData::Element { name, .. } if name == "br" || name == "p" => out.push('\n'),
                NodeData::Text(s) | NodeData::Comment(s) | NodeData::Doctype(s) => out.push_str(s),
                _ => {}
            }
        }
        out
    }

    /// `str(tag)`: the node serialised as BeautifulSoup writes it.
    pub fn outer_html(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.write(id, &mut out);
        out
    }

    fn write(&self, id: NodeId, out: &mut String) {
        match &self.nodes[id].data {
            NodeData::Document => {
                for &child in &self.nodes[id].children {
                    self.write(child, out);
                }
            }
            NodeData::Doctype(text) => {
                out.push_str("<!DOCTYPE ");
                out.push_str(text);
                out.push_str(">\n");
            }
            NodeData::Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
            NodeData::Text(text) => {
                let raw = self.nodes[id]
                    .parent
                    .and_then(|p| self.name(p))
                    .is_some_and(|p| p == "script" || p == "style");
                if raw {
                    out.push_str(text);
                } else {
                    escape_xml(text, out);
                }
            }
            NodeData::Element { name, attrs } => {
                out.push('<');
                out.push_str(name);
                for (key, value) in attrs {
                    out.push(' ');
                    out.push_str(key);
                    out.push('=');
                    quoted_attribute(&value.joined(), out);
                }
                let children = &self.nodes[id].children;
                if children.is_empty() && is_void(name) {
                    out.push_str("/>");
                    return;
                }
                out.push('>');
                for &child in children {
                    self.write(child, out);
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
    }
}

/// BeautifulSoup's text for a doctype.
fn doctype_text(name: &str, public_id: &str, system_id: &str) -> String {
    let mut text = name.to_owned();
    if !public_id.is_empty() {
        text.push_str(&format!(" PUBLIC \"{public_id}\""));
        if !system_id.is_empty() {
            text.push_str(&format!(" \"{system_id}\""));
        }
    } else if !system_id.is_empty() {
        text.push_str(&format!(" SYSTEM \"{system_id}\""));
    }
    text
}

/// BeautifulSoup's "minimal" formatter: `&`, `<` and `>` as entities.
fn escape_xml(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// An attribute value in quotes: double, or single if it contains a double
/// quote, or double with `&quot;` if it contains both.
fn quoted_attribute(value: &str, out: &mut String) {
    let mut escaped = String::with_capacity(value.len());
    escape_xml(value, &mut escaped);
    if escaped.contains('"') {
        if escaped.contains('\'') {
            out.push('"');
            out.push_str(&escaped.replace('"', "&quot;"));
            out.push('"');
        } else {
            out.push('\'');
            out.push_str(&escaped);
            out.push('\'');
        }
    } else {
        out.push('"');
        out.push_str(&escaped);
        out.push('"');
    }
}
