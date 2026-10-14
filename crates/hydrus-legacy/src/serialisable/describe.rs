//! Help > debug > "what is this object?": what the reference says of a
//! serialised object (`ReviewWhatIsThisObject._ImportPayloads` and every
//! `GetSerialisableDescription`), and of text that is not one.
//!
//! The class names are the reference's `SERIALISABLE_NAME`s, dumped from the
//! running client by `oracle/record_what_is_this_object.py` into
//! `class_names.json` (code -> name, version). Errors are worded as Python
//! words them for what a user is likely to paste: bad JSON (the scanner here
//! follows `json.decoder`'s messages), things that are not a serialised
//! tuple, unknown types and objects from the future.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use hydrus_core::numbers::{human_bytes, human_int};
use hydrus_core::pyjson::PyJson;

use super::{Body, BytesKey, BytesValue, Meta, SerialisableObject};

fn classes() -> &'static BTreeMap<u16, (String, u32)> {
    static CLASSES: OnceLock<BTreeMap<u16, (String, u32)>> = OnceLock::new();
    CLASSES.get_or_init(|| {
        let raw: BTreeMap<String, (String, u32)> =
            serde_json::from_str(include_str!("class_names.json")).unwrap_or_default();
        raw.into_iter()
            .filter_map(|(code, entry)| Some((code.parse().ok()?, entry)))
            .collect()
    })
}

/// The reference's `SERIALISABLE_NAME` of the class registered for `code`.
pub fn class_name(code: u16) -> Option<&'static str> {
    classes().get(&code).map(|(name, _)| name.as_str())
}

/// Python's class of a plain JSON value, as `str( type( x ) )` shows it.
fn python_class(value: &PyJson) -> &'static str {
    match value {
        PyJson::Null => "<class 'NoneType'>",
        PyJson::Bool(_) => "<class 'bool'>",
        PyJson::Int(_) | PyJson::BigInt(_) => "<class 'int'>",
        PyJson::Float(_) => "<class 'float'>",
        PyJson::Str(_) => "<class 'str'>",
        PyJson::List(_) => "<class 'list'>",
        PyJson::Object(_) => "<class 'dict'>",
    }
}

/// `str( x )` of a container's key or value: its own description if it is a
/// serialisable, else its class.
fn meta_text(meta: &Meta) -> String {
    match meta {
        Meta::Json(value) => python_class(value).to_owned(),
        Meta::Bytes(_) => "<class 'bytes'>".to_owned(),
        Meta::Object(object) => describe(object),
    }
}

/// `GetSerialisableDescription`: `Name (type)`, with the name of a named
/// object, and for the generic containers what they hold, counted.
pub fn describe(object: &SerialisableObject) -> String {
    let code = object.kind.code();
    let class = class_name(code).unwrap_or("unknown");
    let head = format!("{class} ({code})");
    let counted = |lines: Vec<String>| {
        let mut order: Vec<String> = Vec::new();
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for line in lines {
            if !counts.contains_key(&line) {
                order.push(line.clone());
            }
            *counts.entry(line).or_default() += 1;
        }
        let rows: Vec<String> = order
            .iter()
            .map(|line| format!("{}: {line}", human_int(counts[line])))
            .collect();
        format!("{head}\n{}", rows.join("\n"))
    };
    match &object.body {
        Body::Dictionary(pairs) => counted(
            pairs
                .iter()
                .map(|(key, value)| format!("{}: {}", meta_text(key), meta_text(value)))
                .collect(),
        ),
        Body::BytesDictionary(pairs) => counted(
            pairs
                .iter()
                .map(|(key, value)| {
                    let key = match key {
                        BytesKey::Int(_) => "<class 'int'>",
                        BytesKey::Bytes(_) => "<class 'bytes'>",
                    };
                    let value = match value {
                        BytesValue::None => "<class 'NoneType'>",
                        BytesValue::Bytes(_) => "<class 'bytes'>",
                        BytesValue::List(_) => "<class 'list'>",
                    };
                    format!("{key}: {value}")
                })
                .collect(),
        ),
        Body::List(items) => counted(items.iter().map(meta_text).collect()),
        Body::Other(_) => match &object.name {
            Some(name) => format!("{head} \"{name}\""),
            None => head,
        },
    }
}

/// What the window says of one payload: `Object Type: ...` and the size of
/// its text, or `Error: ...`.
pub fn what_is_this(text: &str) -> String {
    match load(text) {
        Ok(object) => format!(
            "Object Type: {}\nJSON: {}",
            describe(&object),
            human_bytes(text.chars().count() as u64)
        ),
        Err(error) => format!("Error: {error}"),
    }
}

/// `CreateFromString( text, raise_error_on_future_version = True )`, its
/// failures worded as Python's.
fn load(text: &str) -> Result<SerialisableObject, String> {
    let value = match PyJson::parse(text) {
        Ok(value) => value,
        Err(error) => return Err(python_json_error(text).unwrap_or_else(|| error.to_string())),
    };
    if let Some(error) = tuple_error(&value) {
        return Err(error);
    }
    let object = SerialisableObject::from_tuple(&value).map_err(|e| e.to_string())?;
    let code = object.kind.code();
    let Some((name, current)) = classes().get(&code) else {
        return Err(code.to_string());
    };
    if object.version > *current {
        return Err(format!(
            "Unfortunately, an object of type {name} could not be loaded because it was created in a client/server that uses an updated version of that object! We support up to version {current}, but the object was version {}.\n\nPlease update your client/server to import this object.",
            object.version
        ));
    }
    object.upgraded().map_err(|e| e.to_string())
}

/// Python's `repr` of a string.
fn repr(text: &str) -> String {
    hydrus_core::pyjson::python_str_repr(text)
}

/// The error `CreateFromSerialisableTuple` raises for something that is not
/// a three- or four-item tuple of a known type, or `None` if it may be one.
fn tuple_error(value: &PyJson) -> Option<String> {
    let unpack = |n: usize| -> Option<String> {
        match n.cmp(&4) {
            std::cmp::Ordering::Less => {
                Some(format!("not enough values to unpack (expected 4, got {n})"))
            }
            std::cmp::Ordering::Greater => Some("too many values to unpack (expected 4)".into()),
            std::cmp::Ordering::Equal => None,
        }
    };
    let key_error = |first: &PyJson| -> Option<String> {
        match first {
            PyJson::Int(n) if classes().contains_key(&u16::try_from(*n).unwrap_or(u16::MAX)) => {
                None
            }
            PyJson::Int(n) => Some(n.to_string()),
            PyJson::BigInt(digits) => Some(digits.clone()),
            PyJson::Float(f) => Some(PyJson::Float(*f).to_python_string()),
            PyJson::Str(s) => Some(repr(s)),
            PyJson::Bool(true) => Some("True".into()),
            PyJson::Bool(false) => Some("False".into()),
            PyJson::Null => Some("None".into()),
            PyJson::List(_) => Some("unhashable type: 'list'".into()),
            PyJson::Object(_) => Some("unhashable type: 'dict'".into()),
        }
    };
    match value {
        PyJson::Null => Some("object of type 'NoneType' has no len()".into()),
        PyJson::Bool(_) => Some("object of type 'bool' has no len()".into()),
        PyJson::Int(_) | PyJson::BigInt(_) => Some("object of type 'int' has no len()".into()),
        PyJson::Float(_) => Some("object of type 'float' has no len()".into()),
        PyJson::Str(s) => {
            let n = s.chars().count();
            if n == 3 {
                s.chars().next().map(|c| repr(&c.to_string()))
            } else {
                unpack(n)
            }
        }
        PyJson::Object(entries) => {
            if entries.len() == 3 {
                entries.first().map(|(k, _)| repr(k))
            } else {
                unpack(entries.len())
            }
        }
        PyJson::List(items) => match items.len() {
            3 | 4 => key_error(&items[0]),
            n => unpack(n),
        },
    }
}

/// `json.loads`' error for `text`, worded and positioned as Python's, if it
/// is not JSON (`json.decoder.JSONDecodeError`).
pub fn python_json_error(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut scanner = Scanner {
        chars: &chars,
        at: 0,
    };
    scanner.skip_ws();
    let outcome = scanner.value().and_then(|()| {
        scanner.skip_ws();
        if scanner.at < chars.len() {
            Err(("Extra data", scanner.at))
        } else {
            Ok(())
        }
    });
    let (message, at) = outcome.err()?;
    let line = chars[..at].iter().filter(|c| **c == '\n').count() + 1;
    let column = at
        - chars[..at]
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |i| i + 1)
        + 1;
    Some(format!(
        "{message}: line {line} column {column} (char {at})"
    ))
}

struct Scanner<'a> {
    chars: &'a [char],
    at: usize,
}

type Scan = Result<(), (&'static str, usize)>;

impl Scanner<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Scan {
        match self.peek() {
            Some('"') => self.string(),
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some(_) => self.atom(),
            None => Err(("Expecting value", self.at)),
        }
    }

    fn atom(&mut self) -> Scan {
        let rest: String = self.chars[self.at..].iter().take(9).collect();
        for literal in ["null", "true", "false", "NaN", "Infinity", "-Infinity"] {
            if rest.starts_with(literal) {
                self.at += literal.chars().count();
                return Ok(());
            }
        }
        // a number: -?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?
        let start = self.at;
        if self.peek() == Some('-') {
            self.at += 1;
        }
        match self.peek() {
            Some('0') => self.at += 1,
            Some('1'..='9') => {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.at += 1;
                }
            }
            _ => {
                self.at = start;
                return Err(("Expecting value", start));
            }
        }
        if self.peek() == Some('.')
            && self
                .chars
                .get(self.at + 1)
                .is_some_and(char::is_ascii_digit)
        {
            self.at += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let mut look = self.at + 1;
            if matches!(self.chars.get(look), Some('+' | '-')) {
                look += 1;
            }
            if self.chars.get(look).is_some_and(char::is_ascii_digit) {
                self.at = look;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.at += 1;
                }
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Scan {
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(("Unterminated string starting at", start)),
                Some('"') => {
                    self.at += 1;
                    return Ok(());
                }
                Some('\\') => match self.chars.get(self.at + 1) {
                    None => return Err(("Unterminated string starting at", start)),
                    Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') => self.at += 2,
                    Some('u') => {
                        let hex = self.chars.get(self.at + 2..self.at + 6);
                        if hex.is_some_and(|h| h.iter().all(char::is_ascii_hexdigit)) {
                            self.at += 6;
                        } else {
                            return Err(("Invalid \\uXXXX escape", self.at + 1));
                        }
                    }
                    Some(_) => return Err(("Invalid \\escape", self.at)),
                },
                Some(c) if (c as u32) < 0x20 => {
                    return Err(("Invalid control character at", self.at));
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn array(&mut self) -> Scan {
        self.at += 1;
        self.skip_ws();
        if self.peek() == Some(']') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.value()?;
            self.skip_ws();
            match self.peek() {
                Some(']') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(',') => {
                    self.at += 1;
                    self.skip_ws();
                }
                _ => return Err(("Expecting ',' delimiter", self.at)),
            }
        }
    }

    fn object(&mut self) -> Scan {
        self.at += 1;
        self.skip_ws();
        if self.peek() == Some('}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            if self.peek() != Some('"') {
                return Err(("Expecting property name enclosed in double quotes", self.at));
            }
            self.string()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(("Expecting ':' delimiter", self.at));
            }
            self.at += 1;
            self.skip_ws();
            self.value()?;
            self.skip_ws();
            match self.peek() {
                Some('}') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(',') => {
                    self.at += 1;
                    self.skip_ws();
                }
                _ => return Err(("Expecting ',' delimiter", self.at)),
            }
        }
    }
}
