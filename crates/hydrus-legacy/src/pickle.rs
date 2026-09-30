//! Reading Python pickles, as data.
//!
//! The reference pickles a few things into its database (a network session's
//! cookie jar, notably). This reads pickle protocols 2 to 5 into a
//! [`Pickled`] tree without running anything: a class instance becomes an
//! [`Pickled::Object`] naming its class, with the arguments it was created
//! with and the state `__setstate__` would have received.
//!
//! Containers are shared by the pickle's memo (a dict can be memoised when
//! empty and filled afterwards), so they live in an arena while reading and
//! are resolved into a tree at the end.

use std::fmt;

/// A value read from a pickle.
#[derive(Debug, Clone, PartialEq)]
pub enum Pickled {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<Pickled>),
    List(Vec<Pickled>),
    /// In insertion order.
    Dict(Vec<(Pickled, Pickled)>),
    Set(Vec<Pickled>),
    /// A reference to a class or function (`module.name`).
    Global {
        module: String,
        name: String,
    },
    /// An instance: its class, constructor arguments and state.
    Object {
        module: String,
        name: String,
        args: Vec<Pickled>,
        state: Option<Box<Pickled>>,
    },
}

impl Pickled {
    /// A dict's value for a string key.
    pub fn get(&self, key: &str) -> Option<&Pickled> {
        match self {
            Pickled::Dict(pairs) => pairs
                .iter()
                .find(|(k, _)| matches!(k, Pickled::Str(s) if s == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// An object's state (the dict it would restore), or the value itself
    /// for a plain dict.
    pub fn state(&self) -> Option<&Pickled> {
        match self {
            Pickled::Object { state, .. } => state.as_deref(),
            Pickled::Dict(_) => Some(self),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Pickled::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Pickled::Int(i) => Some(*i),
            Pickled::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Pickled::Bool(b) => Some(*b),
            Pickled::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Pickled::None)
    }
}

/// Why a pickle could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickleError {
    pub offset: usize,
    pub detail: String,
}

impl fmt::Display for PickleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad pickle at byte {}: {}", self.offset, self.detail)
    }
}

impl std::error::Error for PickleError {}

/// A value on the stack: plain, or a container in the arena.
#[derive(Debug, Clone)]
enum Value {
    Plain(Pickled),
    Tuple(Vec<Value>),
    Node(usize),
    Mark,
}

#[derive(Debug)]
enum Node {
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Set(Vec<Value>),
    Object {
        module: String,
        name: String,
        args: Vec<Value>,
        state: Option<Value>,
    },
}

/// Read a pickle.
pub fn read(data: &[u8]) -> Result<Pickled, PickleError> {
    let mut vm = Machine {
        data,
        pos: 0,
        stack: Vec::new(),
        memo: Vec::new(),
        arena: Vec::new(),
    };
    let top = vm.run()?;
    vm.resolve(&top, 0)
}

struct Machine<'a> {
    data: &'a [u8],
    pos: usize,
    stack: Vec<Value>,
    memo: Vec<Option<Value>>,
    arena: Vec<Node>,
}

impl Machine<'_> {
    fn error(&self, detail: impl Into<String>) -> PickleError {
        PickleError {
            offset: self.pos,
            detail: detail.into(),
        }
    }

    fn take(&mut self, n: usize) -> Result<&[u8], PickleError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| self.error("truncated"))?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, PickleError> {
        Ok(self.take(1)?[0])
    }

    fn le<const N: usize>(&mut self) -> Result<[u8; N], PickleError> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }

    fn len32(&mut self) -> Result<usize, PickleError> {
        Ok(u32::from_le_bytes(self.le()?) as usize)
    }

    fn len64(&mut self) -> Result<usize, PickleError> {
        usize::try_from(u64::from_le_bytes(self.le()?)).map_err(|_| self.error("too long"))
    }

    fn line(&mut self) -> Result<String, PickleError> {
        let rest = &self.data[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| self.error("unterminated line"))?;
        let text = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.pos += end + 1;
        Ok(text)
    }

    fn string(&mut self, n: usize) -> Result<Value, PickleError> {
        let bytes = self.take(n)?.to_vec();
        let text = String::from_utf8(bytes).map_err(|_| self.error("invalid utf-8"))?;
        Ok(Value::Plain(Pickled::Str(text)))
    }

    fn bytes(&mut self, n: usize) -> Result<Value, PickleError> {
        Ok(Value::Plain(Pickled::Bytes(self.take(n)?.to_vec())))
    }

    fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    fn pop(&mut self) -> Result<Value, PickleError> {
        match self.stack.pop() {
            Some(Value::Mark) | None => Err(self.error("stack underflow")),
            Some(value) => Ok(value),
        }
    }

    fn top(&self) -> Result<&Value, PickleError> {
        self.stack.last().ok_or_else(|| self.error("empty stack"))
    }

    /// Everything above the topmost mark, removing the mark.
    fn pop_mark(&mut self) -> Result<Vec<Value>, PickleError> {
        let mark = self
            .stack
            .iter()
            .rposition(|v| matches!(v, Value::Mark))
            .ok_or_else(|| self.error("no mark"))?;
        let items = self.stack.split_off(mark + 1);
        self.stack.pop();
        Ok(items)
    }

    fn new_node(&mut self, node: Node) -> Value {
        self.arena.push(node);
        Value::Node(self.arena.len() - 1)
    }

    fn node_mut(&mut self, value: &Value) -> Result<&mut Node, PickleError> {
        match value {
            Value::Node(i) => Ok(&mut self.arena[*i]),
            _ => Err(self.error("expected a container")),
        }
    }

    fn memo_put(&mut self, index: usize) -> Result<(), PickleError> {
        let value = self.top()?.clone();
        if self.memo.len() <= index {
            self.memo.resize(index + 1, None);
        }
        self.memo[index] = Some(value);
        Ok(())
    }

    fn memo_get(&mut self, index: usize) -> Result<(), PickleError> {
        let value = self
            .memo
            .get(index)
            .cloned()
            .flatten()
            .ok_or_else(|| self.error(format!("no memo entry {index}")))?;
        self.push(value);
        Ok(())
    }

    fn global(&mut self, module: String, name: String) {
        self.push(Value::Plain(Pickled::Global { module, name }));
    }

    /// An instance of `class` made with `args`.
    fn instantiate(&mut self, class: Value, args: Vec<Value>) -> Result<Value, PickleError> {
        let Value::Plain(Pickled::Global { module, name }) = class else {
            return Err(self.error("can only instantiate a global"));
        };
        // dict and list subclasses are filled by SETITEMS/APPENDS afterwards
        match (module.as_str(), name.as_str()) {
            ("collections", "OrderedDict" | "defaultdict") | ("builtins", "dict") => {
                return Ok(self.new_node(Node::Dict(Vec::new())));
            }
            ("builtins", "list") => return Ok(self.new_node(Node::List(Vec::new()))),
            _ => {}
        }
        // protocol 0/1 style `copyreg._reconstructor(cls, base, state)`
        if module == "copyreg"
            && name == "_reconstructor"
            && let Some(Value::Plain(Pickled::Global { module, name })) = args.first()
        {
            return Ok(self.new_node(Node::Object {
                module: module.clone(),
                name: name.clone(),
                args: Vec::new(),
                state: None,
            }));
        }
        Ok(self.new_node(Node::Object {
            module,
            name,
            args,
            state: None,
        }))
    }

    fn tuple_items(&self, value: Value) -> Result<Vec<Value>, PickleError> {
        match value {
            Value::Tuple(items) => Ok(items),
            _ => Err(self.error("expected a tuple")),
        }
    }

    fn run(&mut self) -> Result<Value, PickleError> {
        loop {
            let op = self.u8()?;
            match op {
                0x80 => {
                    // PROTO
                    let proto = self.u8()?;
                    if proto > 5 {
                        return Err(self.error(format!("unsupported protocol {proto}")));
                    }
                }
                0x95 => {
                    // FRAME: a length prefix only
                    self.take(8)?;
                }
                b'.' => return self.pop(),
                b'(' => self.push(Value::Mark),
                b'0' => {
                    self.pop()?;
                }
                b'1' => {
                    self.pop_mark()?;
                }
                b'2' => {
                    let top = self.top()?.clone();
                    self.push(top);
                }
                b'N' => self.push(Value::Plain(Pickled::None)),
                0x88 => self.push(Value::Plain(Pickled::Bool(true))),
                0x89 => self.push(Value::Plain(Pickled::Bool(false))),
                b'J' => {
                    let v = i32::from_le_bytes(self.le()?);
                    self.push(Value::Plain(Pickled::Int(i64::from(v))));
                }
                b'K' => {
                    let v = self.u8()?;
                    self.push(Value::Plain(Pickled::Int(i64::from(v))));
                }
                b'M' => {
                    let v = u16::from_le_bytes(self.le()?);
                    self.push(Value::Plain(Pickled::Int(i64::from(v))));
                }
                0x8a => {
                    // LONG1: little-endian two's complement
                    let n = self.u8()? as usize;
                    let bytes = self.take(n)?.to_vec();
                    if n > 8 {
                        return Err(self.error("integer too large"));
                    }
                    let mut buf = if bytes.last().is_some_and(|b| b & 0x80 != 0) {
                        [0xff; 8]
                    } else {
                        [0; 8]
                    };
                    buf[..n].copy_from_slice(&bytes);
                    self.push(Value::Plain(Pickled::Int(i64::from_le_bytes(buf))));
                }
                b'I' => {
                    // INT (text); "01"/"00" are booleans
                    let line = self.line()?;
                    let value = match line.as_str() {
                        "01" => Pickled::Bool(true),
                        "00" => Pickled::Bool(false),
                        text => Pickled::Int(text.parse().map_err(|_| self.error("bad integer"))?),
                    };
                    self.push(Value::Plain(value));
                }
                b'L' => {
                    let line = self.line()?;
                    let v = line
                        .trim_end_matches('L')
                        .parse()
                        .map_err(|_| self.error("bad long"))?;
                    self.push(Value::Plain(Pickled::Int(v)));
                }
                b'G' => {
                    let v = f64::from_be_bytes(self.le()?);
                    self.push(Value::Plain(Pickled::Float(v)));
                }
                b'F' => {
                    let line = self.line()?;
                    let v = line.parse().map_err(|_| self.error("bad float"))?;
                    self.push(Value::Plain(Pickled::Float(v)));
                }
                0x8c => {
                    let n = self.u8()? as usize;
                    let v = self.string(n)?;
                    self.push(v);
                }
                b'X' => {
                    let n = self.len32()?;
                    let v = self.string(n)?;
                    self.push(v);
                }
                0x8d => {
                    let n = self.len64()?;
                    let v = self.string(n)?;
                    self.push(v);
                }
                b'C' => {
                    let n = self.u8()? as usize;
                    let v = self.bytes(n)?;
                    self.push(v);
                }
                b'B' => {
                    let n = self.len32()?;
                    let v = self.bytes(n)?;
                    self.push(v);
                }
                0x8e => {
                    let n = self.len64()?;
                    let v = self.bytes(n)?;
                    self.push(v);
                }
                b'U' => {
                    // SHORT_BINSTRING (a Python 2 str): bytes, read as latin-1 text
                    let n = self.u8()? as usize;
                    let text: String = self.take(n)?.iter().map(|&b| b as char).collect();
                    self.push(Value::Plain(Pickled::Str(text)));
                }
                b'T' => {
                    let n = self.len32()?;
                    let text: String = self.take(n)?.iter().map(|&b| b as char).collect();
                    self.push(Value::Plain(Pickled::Str(text)));
                }
                b')' => self.push(Value::Tuple(Vec::new())),
                b't' => {
                    let items = self.pop_mark()?;
                    self.push(Value::Tuple(items));
                }
                0x85..=0x87 => {
                    let n = (op - 0x84) as usize;
                    if self.stack.len() < n {
                        return Err(self.error("stack underflow"));
                    }
                    let items = self.stack.split_off(self.stack.len() - n);
                    self.push(Value::Tuple(items));
                }
                b']' => {
                    let v = self.new_node(Node::List(Vec::new()));
                    self.push(v);
                }
                b'l' => {
                    let items = self.pop_mark()?;
                    let v = self.new_node(Node::List(items));
                    self.push(v);
                }
                b'a' => {
                    let item = self.pop()?;
                    let list = self.top()?.clone();
                    match self.node_mut(&list)? {
                        Node::List(items) => items.push(item),
                        _ => return Err(self.error("APPEND to a non-list")),
                    }
                }
                b'e' => {
                    let new = self.pop_mark()?;
                    let list = self.top()?.clone();
                    match self.node_mut(&list)? {
                        Node::List(items) => items.extend(new),
                        _ => return Err(self.error("APPENDS to a non-list")),
                    }
                }
                b'}' => {
                    let v = self.new_node(Node::Dict(Vec::new()));
                    self.push(v);
                }
                b'd' => {
                    let items = self.pop_mark()?;
                    let pairs = pairs(items).ok_or_else(|| self.error("odd DICT"))?;
                    let v = self.new_node(Node::Dict(pairs));
                    self.push(v);
                }
                b's' => {
                    let value = self.pop()?;
                    let key = self.pop()?;
                    let dict = self.top()?.clone();
                    self.set_items(&dict, vec![(key, value)])?;
                }
                b'u' => {
                    let items = self.pop_mark()?;
                    let pairs = pairs(items).ok_or_else(|| self.error("odd SETITEMS"))?;
                    let dict = self.top()?.clone();
                    self.set_items(&dict, pairs)?;
                }
                0x8f => {
                    let v = self.new_node(Node::Set(Vec::new()));
                    self.push(v);
                }
                0x90 => {
                    let new = self.pop_mark()?;
                    let set = self.top()?.clone();
                    match self.node_mut(&set)? {
                        Node::Set(items) => items.extend(new),
                        _ => return Err(self.error("ADDITEMS to a non-set")),
                    }
                }
                0x91 => {
                    let items = self.pop_mark()?;
                    let v = self.new_node(Node::Set(items));
                    self.push(v);
                }
                b'c' => {
                    let module = self.line()?;
                    let name = self.line()?;
                    self.global(module, name);
                }
                0x93 => {
                    let name = self.pop()?;
                    let module = self.pop()?;
                    match (module, name) {
                        (Value::Plain(Pickled::Str(module)), Value::Plain(Pickled::Str(name))) => {
                            self.global(module, name);
                        }
                        _ => return Err(self.error("STACK_GLOBAL needs two strings")),
                    }
                }
                0x81 => {
                    // NEWOBJ
                    let args = self.pop()?;
                    let args = self.tuple_items(args)?;
                    let class = self.pop()?;
                    let v = self.instantiate(class, args)?;
                    self.push(v);
                }
                0x92 => {
                    // NEWOBJ_EX (keyword arguments are dropped)
                    self.pop()?;
                    let args = self.pop()?;
                    let args = self.tuple_items(args)?;
                    let class = self.pop()?;
                    let v = self.instantiate(class, args)?;
                    self.push(v);
                }
                b'R' => {
                    let args = self.pop()?;
                    let args = self.tuple_items(args)?;
                    let callable = self.pop()?;
                    let v = self.instantiate(callable, args)?;
                    self.push(v);
                }
                b'b' => {
                    let state = self.pop()?;
                    let object = self.top()?.clone();
                    match self.node_mut(&object)? {
                        Node::Object { state: slot, .. } => *slot = Some(state),
                        _ => return Err(self.error("BUILD on a non-object")),
                    }
                }
                0x94 => {
                    let index = self.memo.len();
                    self.memo_put(index)?;
                }
                b'p' => {
                    let index = self.line()?.parse().map_err(|_| self.error("bad PUT"))?;
                    self.memo_put(index)?;
                }
                b'q' => {
                    let index = self.u8()? as usize;
                    self.memo_put(index)?;
                }
                b'r' => {
                    let index = self.len32()?;
                    self.memo_put(index)?;
                }
                b'g' => {
                    let index = self.line()?.parse().map_err(|_| self.error("bad GET"))?;
                    self.memo_get(index)?;
                }
                b'h' => {
                    let index = self.u8()? as usize;
                    self.memo_get(index)?;
                }
                b'j' => {
                    let index = self.len32()?;
                    self.memo_get(index)?;
                }
                other => {
                    self.pos -= 1;
                    return Err(self.error(format!("unsupported opcode 0x{other:02x}")));
                }
            }
        }
    }

    fn set_items(&mut self, dict: &Value, new: Vec<(Value, Value)>) -> Result<(), PickleError> {
        // keys are compared once resolved; containers can't be dict keys
        let resolved: Vec<Pickled> = new
            .iter()
            .map(|(k, _)| self.resolve(k, 0))
            .collect::<Result<_, _>>()?;
        let existing_keys: Vec<Value> = match self.node_mut(dict)? {
            Node::Dict(pairs) => pairs.iter().map(|(k, _)| k.clone()).collect(),
            _ => return Err(self.error("SETITEM on a non-dict")),
        };
        let existing: Vec<Pickled> = existing_keys
            .iter()
            .map(|k| self.resolve(k, 0))
            .collect::<Result<_, _>>()?;
        let Node::Dict(pairs) = self.node_mut(dict)? else {
            unreachable!("checked above");
        };
        let mut keys = existing;
        for ((key, value), resolved) in new.into_iter().zip(resolved) {
            if let Some(i) = keys.iter().position(|k| *k == resolved) {
                pairs[i].1 = value;
            } else {
                pairs.push((key, value));
                keys.push(resolved);
            }
        }
        Ok(())
    }

    fn resolve(&self, value: &Value, depth: usize) -> Result<Pickled, PickleError> {
        if depth > 200 {
            return Err(self.error("nested too deeply (or recursive)"));
        }
        let all = |items: &[Value]| -> Result<Vec<Pickled>, PickleError> {
            items.iter().map(|v| self.resolve(v, depth + 1)).collect()
        };
        Ok(match value {
            Value::Plain(p) => p.clone(),
            Value::Mark => return Err(self.error("unexpected mark")),
            Value::Tuple(items) => Pickled::Tuple(all(items)?),
            Value::Node(i) => match &self.arena[*i] {
                Node::List(items) => Pickled::List(all(items)?),
                Node::Set(items) => Pickled::Set(all(items)?),
                Node::Dict(pairs) => Pickled::Dict(
                    pairs
                        .iter()
                        .map(|(k, v)| {
                            Ok((self.resolve(k, depth + 1)?, self.resolve(v, depth + 1)?))
                        })
                        .collect::<Result<_, PickleError>>()?,
                ),
                Node::Object {
                    module,
                    name,
                    args,
                    state,
                } => Pickled::Object {
                    module: module.clone(),
                    name: name.clone(),
                    args: all(args)?,
                    state: state
                        .as_ref()
                        .map(|s| self.resolve(s, depth + 1).map(Box::new))
                        .transpose()?,
                },
            },
        })
    }
}

fn pairs(items: Vec<Value>) -> Option<Vec<(Value, Value)>> {
    if !items.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(items.len() / 2);
    let mut it = items.into_iter();
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        out.push((k, v));
    }
    Some(out)
}
