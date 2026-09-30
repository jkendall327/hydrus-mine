//! Registrable domains, from the public suffix list.
//!
//! The reference asks `tldextract` for a domain's "top domain under the public
//! suffix" (`forums.bbc.co.uk` -> `bbc.co.uk`) using the ICANN section of
//! `static/public_suffix_list.dat`. It decides which network session a
//! cookie belongs to and which domains a URL's rules apply to. This is a port
//! of that lookup: a trie of the list's rules by reversed labels, with
//! wildcard (`*.kawasaki.jp`) and exception (`!city.kawasaki.jp`) rules, and
//! labels matched case-insensitively and punycode-decoded. Checked against
//! the reference on `oracle/fixtures/domains.json`.

use std::collections::HashMap;
use std::sync::OnceLock;

const LIST: &str = include_str!("../../../../static/public_suffix_list.dat");

#[derive(Default)]
struct Node {
    children: HashMap<String, Node>,
    /// A rule ends here.
    end: bool,
}

/// The ICANN rules (the reference leaves out the private section).
fn rules() -> &'static Node {
    static RULES: OnceLock<Node> = OnceLock::new();
    RULES.get_or_init(|| {
        let icann = LIST
            .split("// ===BEGIN PRIVATE DOMAINS===")
            .next()
            .unwrap_or_default();
        let mut root = Node::default();
        for line in icann.lines() {
            let Some(rule) = line.split_whitespace().next() else {
                continue;
            };
            // a rule is optional '.', '*' or '!' then a word character
            let rest = rule.trim_start_matches(['.', '*', '!']);
            if !rest
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let mut node = &mut root;
            for label in rule.rsplit('.') {
                node = node.children.entry(label.to_owned()).or_default();
            }
            node.end = true;
        }
        root
    })
}

/// A label as the list spells it: lowercase, and unicode if punycode.
fn match_form(label: &str) -> String {
    let lower = label.to_lowercase();
    lower
        .strip_prefix("xn--")
        .and_then(punycode_decode)
        .unwrap_or(lower)
}

/// Index of the first label of the public suffix, if the domain has one.
fn suffix_start(labels: &[&str]) -> Option<usize> {
    let mut node = rules();
    let mut suffix = labels.len();
    let mut index = labels.len();
    for label in labels.iter().rev() {
        let label = match_form(label);
        if let Some(child) = node.children.get(&label) {
            index -= 1;
            node = child;
            if node.end {
                suffix = index;
            }
            continue;
        }
        if node.children.contains_key("*") {
            let exception = node.children.contains_key(&format!("!{label}"));
            return Some(if exception { index } else { index - 1 });
        }
        break;
    }
    (suffix < labels.len()).then_some(suffix)
}

/// The host part of a domain-ish string, as `tldextract` takes it: no
/// port, no trailing dots, ideographic full stops as dots.
fn host(domain: &str) -> String {
    let after_user = domain
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = after_user.split(':').next().unwrap_or_default().trim();
    host.trim_end_matches(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}'])
        .replace(['\u{3002}', '\u{ff0e}', '\u{ff61}'], ".")
}

/// The registrable domain (`a.b.example.co.uk` -> `example.co.uk`), or
/// empty for a domain without one (`co.uk`, `localhost`, an IP, an unknown
/// top-level domain). Case is kept.
pub fn second_level_domain(domain: &str) -> String {
    let host = host(domain);
    if host.starts_with('[') {
        return String::new();
    }
    let labels: Vec<&str> = host.split('.').collect();
    match suffix_start(&labels) {
        Some(start) if start > 0 && !labels[start - 1].is_empty() => labels[start - 1..].join("."),
        _ => String::new(),
    }
}

/// The domain one level up (`maps.google.com` -> `google.com`).
pub fn next_level_domain(domain: &str) -> &str {
    domain.split_once('.').map_or("", |(_, rest)| rest)
}

/// `www.example.com` -> `example.com`; other domains unchanged.
pub fn remove_www(domain: &str) -> &str {
    if domain.starts_with("www")
        && domain.matches('.').count() > 1
        && domain != second_level_domain(domain)
    {
        next_level_domain(domain)
    } else {
        domain
    }
}

/// The domain and each parent down to its registrable domain, most specific
/// first, without a leading `www`. IPs and dotless hosts are just
/// themselves.
pub fn all_applicable_domains(domain: &str) -> Vec<String> {
    let is_address = !domain.is_empty()
        && domain
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ':');
    if !domain.contains('.') || is_address {
        return vec![domain.to_owned()];
    }
    let mut domain = remove_www(domain);
    let registrable = second_level_domain(domain);
    let mut out = Vec::new();
    while domain.contains('.') {
        out.push(domain.to_owned());
        if domain == registrable {
            break;
        }
        domain = next_level_domain(domain);
    }
    out
}

/// Decode a punycode label body (RFC 3492), `None` if malformed. (Names
/// follow the RFC.)
#[allow(clippy::many_single_char_names)]
fn punycode_decode(input: &str) -> Option<String> {
    const BASE: u32 = 36;
    const T_MIN: u32 = 1;
    const T_MAX: u32 = 26;
    const SKEW: u32 = 38;
    const DAMP: u32 = 700;

    fn adapt(delta: u32, points: u32, first: bool) -> u32 {
        let mut delta = if first { delta / DAMP } else { delta / 2 };
        delta += delta / points;
        let mut k = 0;
        while delta > ((BASE - T_MIN) * T_MAX) / 2 {
            delta /= BASE - T_MIN;
            k += BASE;
        }
        k + (BASE - T_MIN + 1) * delta / (delta + SKEW)
    }

    let (basic, extended) = match input.rfind('-') {
        Some(i) => (&input[..i], &input[i + 1..]),
        None => ("", input),
    };
    if !basic.is_ascii() || extended.is_empty() {
        return None;
    }
    let mut output: Vec<char> = basic.chars().collect();
    let (mut n, mut i, mut bias) = (128u32, 0u32, 72u32);
    let mut digits = extended.chars().peekable();
    while digits.peek().is_some() {
        let old_i = i;
        let mut weight = 1u32;
        let mut k = BASE;
        loop {
            let c = digits.next()?;
            let digit = match c {
                'a'..='z' => c as u32 - 'a' as u32,
                'A'..='Z' => c as u32 - 'A' as u32,
                '0'..='9' => c as u32 - '0' as u32 + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(weight)?)?;
            let t = if k <= bias {
                T_MIN
            } else if k >= bias + T_MAX {
                T_MAX
            } else {
                k - bias
            };
            if digit < t {
                break;
            }
            weight = weight.checked_mul(BASE - t)?;
            k += BASE;
        }
        let points = output.len() as u32 + 1;
        bias = adapt(i - old_i, points, old_i == 0);
        n = n.checked_add(i / points)?;
        i %= points;
        output.insert(i as usize, char::from_u32(n)?);
        i += 1;
    }
    Some(output.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punycode_decodes() {
        assert_eq!(punycode_decode("p1ai").as_deref(), Some("рф"));
        assert_eq!(punycode_decode("e1afmkfd").as_deref(), Some("пример"));
        assert_eq!(punycode_decode("mnchen-3ya").as_deref(), Some("münchen"));
        assert_eq!(punycode_decode("!!"), None);
    }

    #[test]
    fn matches_the_reference() {
        let recorded: serde_json::Value =
            serde_json::from_str(include_str!("../../../../oracle/fixtures/domains.json")).unwrap();
        let mut failures = Vec::new();
        for case in recorded["cases"].as_array().unwrap() {
            let domain = case[0].as_str().unwrap();
            let got = serde_json::json!([
                domain,
                second_level_domain(domain),
                all_applicable_domains(domain),
                remove_www(domain),
            ]);
            if &got != case {
                failures.push(format!("expected {case}\n     got {got}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} differ:\n{}",
            failures.len(),
            recorded["cases"].as_array().unwrap().len(),
            failures[..failures.len().min(20)].join("\n")
        );
    }
}
