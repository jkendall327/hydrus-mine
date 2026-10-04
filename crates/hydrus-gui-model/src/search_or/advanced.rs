//! The reference's Boolean input syntax, converted to AND-of-OR predicates.
use hydrus_search::{Predicate, TextContext, parse_api_search, predicate_text};
use std::{collections::BTreeSet, rc::Rc};

#[derive(Clone, Debug)]
enum Token {
    Term(String),
    Operator(&'static str),
    Open,
    Close,
}
#[derive(Clone, Debug)]
enum Expr {
    Term(String),
    Not(Rc<Self>),
    And(Rc<Self>, Rc<Self>),
    Or(Rc<Self>, Rc<Self>),
}
const WORDS: [&str; 8] = ["not", "and", "or", "implies", "xor", "nor", "nand", "xnor"];
fn operator(input: &str) -> Option<(&'static str, usize)> {
    for (symbol, op) in [
        ("<=>", "iff"),
        ("&&", "and"),
        ("||", "or"),
        ("=>", "implies"),
        ("!", "not"),
        ("-", "not"),
    ] {
        if input.starts_with(symbol) {
            return Some((op, symbol.len()));
        }
    }
    for word in WORDS.into_iter().chain(["iff"]) {
        if let Some(rest) = input.strip_prefix(word)
            && rest.starts_with(|c: char| c.is_whitespace() || c == '(')
        {
            return Some((if word == "xnor" { "iff" } else { word }, word.len()));
        }
    }
    None
}
fn term_end(input: &str) -> bool {
    input.starts_with(['(', ')'])
        || ["&&", "||", "=>", "<=>"]
            .iter()
            .any(|op| input.starts_with(op))
        || input.starts_with(char::is_whitespace)
            && WORDS
                .into_iter()
                .skip(1)
                .chain(["iff"])
                .any(|op| input.trim_start().starts_with(op))
}
fn tokens(input: &str) -> Result<Vec<Token>, String> {
    let lower = input.to_lowercase();
    let mut remaining = lower.as_str();
    let mut out = Vec::new();
    while !remaining.trim().is_empty() {
        remaining = remaining.trim();
        if let Some((op, size)) = operator(remaining) {
            out.push(Token::Operator(op));
            remaining = &remaining[size..];
            continue;
        }
        if remaining.starts_with('(') {
            out.push(Token::Open);
            remaining = &remaining[1..];
            continue;
        }
        if remaining.starts_with(')') {
            out.push(Token::Close);
            remaining = &remaining[1..];
            continue;
        }
        let mut term = String::new();
        // Escaping the initial character suppresses its operator/parenthesis meaning.
        if remaining.starts_with('\\') && remaining.len() > 1 {
            remaining = &remaining[1..];
            let c = remaining.chars().next().unwrap();
            term.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
        while !remaining.is_empty() && !term_end(remaining) {
            if remaining.starts_with('\\') && remaining.len() > 1 {
                remaining = &remaining[1..];
            }
            let c = remaining.chars().next().unwrap();
            term.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
        let term = term.trim().to_owned();
        if term.is_empty() {
            return Err("Syntax error: empty search term".into());
        }
        out.push(Token::Term(term));
    }
    Ok(out)
}
fn precedence(op: &str) -> u8 {
    match op {
        "not" => 10,
        "and" => 9,
        "or" => 8,
        "nor" | "nand" => 7,
        "xor" => 6,
        "implies" => 5,
        _ => 4,
    }
}
fn not(expr: Expr) -> Expr {
    Expr::Not(Rc::new(expr))
}
fn and(a: Expr, b: Expr) -> Expr {
    Expr::And(Rc::new(a), Rc::new(b))
}
fn or(a: Expr, b: Expr) -> Expr {
    Expr::Or(Rc::new(a), Rc::new(b))
}
fn binary(op: &str, a: Expr, b: Expr) -> Expr {
    match op {
        "and" => and(a, b),
        "or" => or(a, b),
        "implies" => or(not(a), b),
        "nor" => not(or(a, b)),
        "nand" => not(and(a, b)),
        "xor" => or(and(a.clone(), not(b.clone())), and(not(a), b)),
        _ => or(and(a.clone(), b.clone()), and(not(a), not(b))),
    }
}
fn parse(input: &str) -> Result<Expr, String> {
    let mut output = Vec::new();
    let mut operators = Vec::new();
    let mut previous_value = false;
    for token in tokens(input)? {
        match token {
            Token::Term(_) => {
                output.push(token);
                previous_value = true;
            }
            Token::Open => {
                operators.push(token);
                previous_value = false;
            }
            Token::Close => {
                loop {
                    match operators.pop() {
                        Some(Token::Open) => break,
                        Some(token) => output.push(token),
                        None => return Err("Syntax error: mismatched parentheses".into()),
                    }
                }
                previous_value = true;
            }
            Token::Operator(op) => {
                if op == "not" && previous_value {
                    return Err("Syntax error: invalid negation".into());
                }
                while let Some(Token::Operator(top)) = operators.last() {
                    if precedence(top) > precedence(op)
                        || precedence(top) == precedence(op) && *top != "not"
                    {
                        output.push(operators.pop().unwrap());
                    } else {
                        break;
                    }
                }
                operators.push(token);
                previous_value = false;
            }
        }
    }
    while let Some(token) = operators.pop() {
        if matches!(token, Token::Open | Token::Close) {
            return Err("Syntax error: mismatched parentheses".into());
        }
        output.push(token);
    }
    if output.is_empty() {
        return Err("Empty input!".into());
    }
    let mut stack = Vec::new();
    for token in output {
        match token {
            Token::Term(term) => stack.push(Expr::Term(term)),
            Token::Operator(op) => {
                let b = stack
                    .pop()
                    .ok_or("Syntax error: wrong number of arguments")?;
                let result = if op == "not" {
                    not(b)
                } else {
                    let a = stack
                        .pop()
                        .ok_or("Syntax error: wrong number of arguments")?;
                    binary(op, a, b)
                };
                stack.push(result);
            }
            _ => unreachable!(),
        }
    }
    if stack.len() != 1 {
        return Err("Parser error: unused values left in stack".into());
    }
    Ok(stack.pop().unwrap())
}
// A clause is a disjunction; the outer vector is a conjunction.
fn cnf(expr: &Expr, negate: bool) -> Result<Vec<BTreeSet<String>>, String> {
    match expr {
        Expr::Term(term) => Ok(vec![BTreeSet::from([if negate {
            format!("-{term}")
        } else {
            term.clone()
        }])]),
        Expr::Not(expr) => cnf(expr, !negate),
        Expr::And(a, b) | Expr::Or(a, b) => {
            let left = cnf(a, negate)?;
            let right = cnf(b, negate)?;
            if matches!(expr, Expr::And(..)) == negate {
                if left.len().saturating_mul(right.len()) > 4096 {
                    return Err("This expression expands to too many search rules.".into());
                }
                Ok(left
                    .iter()
                    .flat_map(|a| right.iter().map(move |b| a.union(b).cloned().collect()))
                    .collect())
            } else {
                if left.len() + right.len() > 4096 {
                    return Err("This expression expands to too many search rules.".into());
                }
                Ok(left.into_iter().chain(right).collect())
            }
        }
    }
}
/// Parse exactly the panel's grammar, including its unsupported negated system terms.
pub fn predicates(input: &str, text: &TextContext) -> Result<Vec<Predicate>, String> {
    let clauses = cnf(&parse(input)?, false)?;
    let mut filtered = Vec::new();
    let mut tautology = None;
    for clause in clauses {
        if clause
            .iter()
            .any(|term| clause.contains(&format!("-{term}")))
        {
            tautology = Some(clause);
        } else {
            // The reference retains the last occurrence of a duplicate clause.
            filtered.retain(|previous| previous != &clause);
            filtered.push(clause);
        }
    }
    if filtered.is_empty()
        && let Some(clause) = tautology
    {
        filtered.push(clause);
    }
    let mut out = Vec::new();
    for clause in filtered {
        if clause.iter().any(|term| term.starts_with("-system:")) {
            return Err("Sorry, that would make negated system tags, which are not supported yet! Try to rephrase or negate the system tag yourself.".into());
        }
        let mut row = Vec::new();
        for term in clause {
            row.extend(
                parse_api_search(&serde_json::json!([term])).map_err(|error| error.to_string())?,
            );
        }
        row.sort_by_cached_key(|predicate| {
            hydrus_core::sort::human_sort_key(&predicate_text(predicate, text))
        });
        match row.len() {
            0 => {}
            1 => out.extend(row),
            _ => out.push(Predicate::Or(row)),
        }
    }
    Ok(out)
}
/// Preview and validity as the real advanced editor displays them.
pub fn preview(input: &str, text: &TextContext) -> (String, bool) {
    if input.is_empty() {
        return (String::new(), false);
    }
    match predicates(input, text) {
        Ok(predicates) => (
            predicates
                .iter()
                .map(|p| predicate_text(p, text))
                .collect::<Vec<_>>()
                .join("\n"),
            true,
        ),
        Err(error) => (format!("Could not parse! {error}"), false),
    }
}
