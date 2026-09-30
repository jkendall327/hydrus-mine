//! JSON exactly as Python's `json` module reads and writes it.
//!
//! The reference stores every serialised object as `json.dumps(...)` output
//! with default settings: `", "` and `": "` separators, `ensure_ascii=True`
//! (every non-ASCII character escaped as `\uXXXX`), floats in `repr` form
//! (`1e-05`, `1e+16`, `100.0`), and the non-standard `NaN`/`Infinity`
//! literals. [`PyJson`] keeps enough information (integer vs float, object
//! key order, big integers) that [`PyJson::to_python_string`] reproduces the
//! reference's bytes exactly, which is what lets unknown objects be carried
//! across losslessly and re-emitted verbatim.
//!
//! Limitations, both of which the reference never produces in practice:
//! strings containing lone UTF-16 surrogates (`"\ud800"`) cannot be
//! represented in a Rust `String` and are rejected, and objects with
//! duplicate keys are kept as-is (Python would keep only the last).

use std::fmt::Write as _;

/// Maximum nesting depth accepted by the parser. Python's own default
/// recursion limit makes deeper documents impossible to write.
const MAX_DEPTH: usize = 1000;

/// A JSON value as the reference writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum PyJson {
    Null,
    Bool(bool),
    /// An integer that fits in an `i64`.
    Int(i64),
    /// A Python integer too big for an `i64`, kept as its decimal text.
    BigInt(String),
    Float(f64),
    Str(String),
    /// A JSON array. Python tuples are written as arrays too.
    List(Vec<PyJson>),
    /// A JSON object, keeping the stored key order.
    Object(Vec<(String, PyJson)>),
}

/// Why a document could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid json at byte {offset}: {message}")]
pub struct JsonError {
    pub offset: usize,
    pub message: String,
}

impl PyJson {
    /// Parse a document the way `json.loads` does.
    pub fn parse(text: &str) -> Result<PyJson, JsonError> {
        let mut parser = Parser {
            text: text.as_bytes(),
            pos: 0,
            depth: 0,
        };
        parser.skip_whitespace();
        let value = parser.value()?;
        parser.skip_whitespace();
        if parser.pos != parser.text.len() {
            return Err(parser.error("trailing characters after the document"));
        }
        Ok(value)
    }

    /// Serialise exactly as `json.dumps(value)` would.
    pub fn to_python_string(&self) -> String {
        let mut out = String::new();
        self.write_python(&mut out);
        out
    }

    /// Serialise as `json.dumps(value, ensure_ascii=False)` would: non-ASCII
    /// characters are written as themselves.
    pub fn to_python_string_unicode(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, false);
        out
    }

    /// Append the `json.dumps` form of this value to `out`.
    pub fn write_python(&self, out: &mut String) {
        self.write(out, true);
    }

    fn write(&self, out: &mut String, ascii: bool) {
        let string = |s: &str, out: &mut String| {
            if ascii {
                write_python_string(s, out);
            } else {
                write_python_string_unicode(s, out);
            }
        };
        match self {
            PyJson::Null => out.push_str("null"),
            PyJson::Bool(true) => out.push_str("true"),
            PyJson::Bool(false) => out.push_str("false"),
            PyJson::Int(i) => {
                let _ = write!(out, "{i}");
            }
            PyJson::BigInt(digits) => out.push_str(digits),
            PyJson::Float(f) => write_python_float(*f, out),
            PyJson::Str(s) => string(s, out),
            PyJson::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write(out, ascii);
                }
                out.push(']');
            }
            PyJson::Object(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    string(key, out);
                    out.push_str(": ");
                    value.write(out, ascii);
                }
                out.push('}');
            }
        }
    }

    /// Python's `str()` of a scalar (`True`, `None`, `1.5`, `nan`, a
    /// string as itself); `None` for a list or object.
    pub fn py_str(&self) -> Option<String> {
        Some(match self {
            PyJson::Null => "None".to_owned(),
            PyJson::Bool(true) => "True".to_owned(),
            PyJson::Bool(false) => "False".to_owned(),
            PyJson::Int(i) => i.to_string(),
            PyJson::BigInt(digits) => digits.clone(),
            PyJson::Float(f) if f.is_nan() => "nan".to_owned(),
            PyJson::Float(f) if f.is_infinite() => if *f > 0.0 { "inf" } else { "-inf" }.to_owned(),
            PyJson::Float(f) => {
                let mut out = String::new();
                write_python_float(*f, &mut out);
                out
            }
            PyJson::Str(s) => s.clone(),
            PyJson::List(_) | PyJson::Object(_) => return None,
        })
    }

    /// The value as `json.loads` gives it to Python code: a key given more
    /// than once keeps its first position and its last value.
    #[must_use]
    pub fn with_python_dict_semantics(self) -> PyJson {
        match self {
            PyJson::List(items) => PyJson::List(
                items
                    .into_iter()
                    .map(PyJson::with_python_dict_semantics)
                    .collect(),
            ),
            PyJson::Object(entries) => {
                let mut out: Vec<(String, PyJson)> = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let value = value.with_python_dict_semantics();
                    match out.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = value,
                        None => out.push((key, value)),
                    }
                }
                PyJson::Object(out)
            }
            other => other,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            PyJson::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PyJson::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Python truthiness is not used: only real booleans are booleans.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PyJson::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// A float, or an integer used where Python accepts either.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PyJson::Float(f) => Some(*f),
            PyJson::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[PyJson]> {
        match self {
            PyJson::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, PyJson::Null)
    }

    /// A short description of the value's kind, for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            PyJson::Null => "null",
            PyJson::Bool(_) => "bool",
            PyJson::Int(_) | PyJson::BigInt(_) => "int",
            PyJson::Float(_) => "float",
            PyJson::Str(_) => "string",
            PyJson::List(_) => "list",
            PyJson::Object(_) => "object",
        }
    }
}

/// Write a float as Python's `float.__repr__` does, with `json`'s spellings
/// of the non-finite values.
///
/// Python prints the shortest digit string that round-trips (as Rust does),
/// then uses positional notation when the decimal exponent is in
/// `-4 < decpt <= 16` and scientific notation otherwise.
pub fn write_python_float(f: f64, out: &mut String) {
    if f.is_nan() {
        out.push_str("NaN");
        return;
    }
    if f.is_infinite() {
        out.push_str(if f > 0.0 { "Infinity" } else { "-Infinity" });
        return;
    }
    // Rust's `{:e}` gives the shortest round-tripping digits, e.g. "-1.25e-7".
    let sci = format!("{f:e}");
    let (mantissa, exponent) = sci
        .split_once('e')
        .expect("LowerExp output always has an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("LowerExp exponent is always an integer");
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    // value = 0.DIGITS * 10^decpt
    let decpt = exponent + 1;
    if negative {
        out.push('-');
    }
    if decpt <= -4 || decpt > 16 {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        let _ = write!(out, "e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    } else if decpt <= 0 {
        out.push_str("0.");
        for _ in 0..(-decpt) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        let decpt = decpt as usize;
        if decpt >= digits.len() {
            out.push_str(&digits);
            for _ in digits.len()..decpt {
                out.push('0');
            }
            out.push_str(".0");
        } else {
            out.push_str(&digits[..decpt]);
            out.push('.');
            out.push_str(&digits[decpt..]);
        }
    }
}

/// Write a string as `json.dumps` does with `ensure_ascii=True`.
pub fn write_python_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Write a string as `json.dumps` does with `ensure_ascii=False`.
pub fn write_python_string_unicode(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    text: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> JsonError {
        JsonError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    fn eat_literal(&mut self, literal: &str) -> bool {
        if self.text[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<PyJson, JsonError> {
        match self.peek() {
            None => Err(self.error("unexpected end of document")),
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::list),
            Some(b'"') => self.string().map(PyJson::Str),
            Some(b'-' | b'0'..=b'9') => {
                if self.eat_literal("-Infinity") {
                    Ok(PyJson::Float(f64::NEG_INFINITY))
                } else {
                    self.number()
                }
            }
            Some(_) => {
                for (literal, value) in [
                    ("null", PyJson::Null),
                    ("true", PyJson::Bool(true)),
                    ("false", PyJson::Bool(false)),
                    ("NaN", PyJson::Float(f64::NAN)),
                    ("Infinity", PyJson::Float(f64::INFINITY)),
                ] {
                    if self.eat_literal(literal) {
                        return Ok(value);
                    }
                }
                Err(self.error("expected a value"))
            }
        }
    }

    fn nested(
        &mut self,
        parse: fn(&mut Self) -> Result<PyJson, JsonError>,
    ) -> Result<PyJson, JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("document is nested too deeply"));
        }
        let result = parse(self);
        self.depth -= 1;
        result
    }

    fn list(&mut self) -> Result<PyJson, JsonError> {
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(PyJson::List(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(PyJson::List(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<PyJson, JsonError> {
        self.pos += 1; // '{'
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(PyJson::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a string key"));
            }
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(self.error("expected ':'"));
            }
            self.pos += 1;
            self.skip_whitespace();
            let value = self.value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(PyJson::Object(entries));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn number(&mut self) -> Result<PyJson, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.skip_digits(),
            _ => return Err(self.error("expected a digit")),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') && matches!(self.text.get(self.pos + 1), Some(b'0'..=b'9')) {
            is_float = true;
            self.pos += 1;
            self.skip_digits();
        }
        if let Some(b'e' | b'E') = self.peek() {
            let save = self.pos;
            self.pos += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                is_float = true;
                self.skip_digits();
            } else {
                // Python's scanner stops the number before a dangling 'e'.
                self.pos = save;
            }
        }
        let text =
            std::str::from_utf8(&self.text[start..self.pos]).expect("number characters are ascii");
        if is_float {
            text.parse::<f64>()
                .map(PyJson::Float)
                .map_err(|e| self.error(format!("bad float {text:?}: {e}")))
        } else {
            Ok(text
                .parse::<i64>()
                .map_or_else(|_| PyJson::BigInt(text.to_owned()), PyJson::Int))
        }
    }

    fn skip_digits(&mut self) {
        while let Some(b'0'..=b'9') = self.peek() {
            self.pos += 1;
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let run_start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.text[run_start..self.pos])
                    .map_err(|_| self.error("string is not valid utf-8"))?,
            );
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                Some(_) => return Err(self.error("control character in string")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), JsonError> {
        let Some(c) = self.peek() else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        match c {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let first = self.hex4()?;
                let code = if (0xD800..0xDC00).contains(&first) {
                    if !self.eat_literal("\\u") {
                        return Err(self.error("lone surrogate in string"));
                    }
                    let second = self.hex4()?;
                    if !(0xDC00..0xE000).contains(&second) {
                        return Err(self.error("lone surrogate in string"));
                    }
                    0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                } else {
                    first
                };
                let c =
                    char::from_u32(code).ok_or_else(|| self.error("lone surrogate in string"))?;
                out.push(c);
            }
            _ => return Err(self.error("invalid escape")),
        }
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let digits = self
            .text
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("truncated \\u escape"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("bad \\u escape"))?;
        let value = u32::from_str_radix(text, 16).map_err(|_| self.error("bad \\u escape"))?;
        self.pos += 4;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) {
        let value = PyJson::parse(text).unwrap();
        assert_eq!(value.to_python_string(), text);
    }

    #[test]
    fn python_float_repr() {
        for (f, expected) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (1.5, "1.5"),
            (0.5, "0.5"),
            (100.0, "100.0"),
            (0.01, "0.01"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.234_567_890_123_456_8e17, "1.2345678901234568e+17"),
            (5e-324, "5e-324"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::NAN, "NaN"),
        ] {
            let mut out = String::new();
            write_python_float(f, &mut out);
            assert_eq!(out, expected, "{f:?}");
        }
    }

    #[test]
    fn python_string_escapes() {
        let mut out = String::new();
        write_python_string("a\"b\\c\n\t\u{7f}é初🙂\u{1}", &mut out);
        assert_eq!(out, r#""a\"b\\c\n\t\u007f\u00e9\u521d\ud83d\ude42\u0001""#);
    }

    #[test]
    fn round_trips_reference_style_documents() {
        round_trip(r#"[21, 2, [[[0, "port"], [0, 45901]], [[0, "x"], [0, null]]]]"#);
        round_trip(r"[[26, 3, []], {}]");
        round_trip(r#"{"a": [1.5, -2, true, false, null], "b": {}}"#);
        round_trip(r"[NaN, Infinity, -Infinity, 1e-05, 123456789012345678901234567890]");
        round_trip(r#""\u521d\ud83d\ude42 \"quoted\"""#);
    }

    #[test]
    fn parses_what_python_accepts() {
        assert_eq!(
            PyJson::parse(" [ 1 ,2.50,\"\\/\" ] ").unwrap(),
            PyJson::List(vec![
                PyJson::Int(1),
                PyJson::Float(2.5),
                PyJson::Str("/".into())
            ])
        );
        assert_eq!(
            PyJson::parse("-9223372036854775809").unwrap(),
            PyJson::BigInt("-9223372036854775809".into())
        );
    }

    #[test]
    fn rejects_what_python_rejects() {
        for bad in [
            "",
            "[1,]",
            "{1: 2}",
            "\"\u{1}\"",
            "[1] x",
            "01",
            "\"\\ud800\"",
        ] {
            assert!(PyJson::parse(bad).is_err(), "{bad:?}");
        }
    }

    proptest::proptest! {
        #[test]
        fn floats_round_trip_through_python_repr(f in proptest::num::f64::ANY) {
            let mut out = String::new();
            write_python_float(f, &mut out);
            let back = PyJson::parse(&out).unwrap();
            match back {
                PyJson::Float(g) => proptest::prop_assert!(g.to_bits() == f.to_bits() || (g.is_nan() && f.is_nan())),
                other => proptest::prop_assert!(false, "{other:?}"),
            }
        }

        #[test]
        fn strings_round_trip(s in "\\PC*") {
            let mut out = String::new();
            write_python_string(&s, &mut out);
            proptest::prop_assert!(out.is_ascii());
            proptest::prop_assert_eq!(PyJson::parse(&out).unwrap(), PyJson::Str(s));
        }
    }
}
