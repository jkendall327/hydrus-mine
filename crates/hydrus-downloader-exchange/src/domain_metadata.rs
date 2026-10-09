//! Domain metadata packages (`ClientNetworkingDomain.DomainMetadataPackage`,
//! type 71): a domain's shareable custom headers and bandwidth rules.
use crate::{Error, Result};
use hydrus_core::bandwidth::{BandwidthType, Rule};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One shared header: name, value and the reason shown to the user.
pub type Header = (String, String, String);

/// A domain's headers and/or bandwidth rules, as the reference shares them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainMetadata {
    pub domain: String,
    /// `None`: no headers. `Some(empty)` is a domain whose headers were all
    /// unapproved, which the reference still packages as "headers".
    pub headers: Option<Vec<Header>>,
    pub rules: Option<Vec<Rule>>,
}

impl DomainMetadata {
    /// `GetSafeSummary`: what it carries, and its domain.
    pub fn safe_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.rules.is_some() {
            parts.push("bandwidth rules");
        }
        if self.headers.is_some() {
            parts.push("headers");
        }
        format!("{} - {}", parts.join(" and "), self.domain)
    }

    /// `GetDetailedSafeSummary`: every rule and header.
    pub fn detailed_summary(&self) -> String {
        let mut parts = vec![format!("For domain \"{}\":", self.domain)];
        if let Some(rules) = &self.rules {
            let lines: Vec<String> = rules.iter().map(|r| rule_text(*r)).collect();
            parts.push(format!("Bandwidth rules: \n{}", lines.join("\n")));
        }
        if let Some(headers) = &self.headers {
            let lines: Vec<String> = headers
                .iter()
                .map(|(key, value, reason)| format!("{key} : {value} - {reason}"))
                .collect();
            parts.push(format!("Headers: \n{}", lines.join("\n")));
        }
        parts.join("\n\n")
    }
}

/// `HydrusNetworking.ConvertBandwidthRuleToString`.
pub fn rule_text(rule: Rule) -> String {
    if rule.max_allowed == 0 {
        return "No requests currently permitted.".into();
    }
    let amount = match rule.kind {
        BandwidthType::Data => hydrus_core::numbers::human_bytes(rule.max_allowed),
        BandwidthType::Requests => {
            format!("{} rqs", hydrus_core::numbers::human_int(rule.max_allowed))
        }
    };
    match rule.time_delta {
        None => format!("{amount} per month"),
        Some(delta) => format!(
            "{amount} per {}",
            hydrus_core::time::pretty_time_delta(i64::try_from(delta).unwrap_or(i64::MAX), false)
        ),
    }
}

/// Decode a type 71 tuple.
pub fn decode(value: &Value) -> Result<DomainMetadata> {
    let bad = |what: &str| Error::Invalid(format!("Malformed domain metadata: {what}."));
    let array = value.as_array().ok_or_else(|| bad("not a tuple"))?;
    if array.len() != 3 || array[0] != json!(71) {
        return Err(bad("not type 71"));
    }
    if array[1] != json!(1) {
        return Err(Error::Unsupported(format!(
            "domain metadata version {}",
            array[1]
        )));
    }
    let info = array[2]
        .as_array()
        .filter(|info| info.len() == 3)
        .ok_or_else(|| bad("expected domain, headers and rules"))?;
    let domain = info[0].as_str().ok_or_else(|| bad("domain"))?.to_owned();
    let headers = match &info[1] {
        Value::Null => None,
        Value::Array(rows) => Some(
            rows.iter()
                .map(|row| {
                    let text = |i: usize| row.get(i).and_then(Value::as_str).map(str::to_owned);
                    match (row.as_array().map(Vec::len), text(0), text(1), text(2)) {
                        (Some(3), Some(k), Some(v), Some(r)) => Ok((k, v, r)),
                        _ => Err(bad("header")),
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => return Err(bad("headers")),
    };
    let rules = match &info[2] {
        Value::Null => None,
        rules => Some(decode_rules(rules)?),
    };
    Ok(DomainMetadata {
        domain,
        headers,
        rules,
    })
}

fn decode_rules(value: &Value) -> Result<Vec<Rule>> {
    let bad = || Error::Invalid("Malformed bandwidth rules.".into());
    let array = value.as_array().ok_or_else(bad)?;
    if array.len() != 3 || array[0] != json!(38) || array[1] != json!(1) {
        return Err(Error::Unsupported(
            "bandwidth rules other than type 38 v1".into(),
        ));
    }
    let mut out: Vec<Rule> = Vec::new();
    for row in array[2].as_array().ok_or_else(bad)? {
        let row = row.as_array().filter(|r| r.len() == 3).ok_or_else(bad)?;
        let kind = row[0]
            .as_i64()
            .and_then(BandwidthType::from_code)
            .ok_or_else(bad)?;
        let time_delta = match &row[1] {
            Value::Null => None,
            v => Some(v.as_u64().ok_or_else(bad)?),
        };
        let max_allowed = row[2].as_u64().ok_or_else(bad)?;
        let rule = Rule::new(kind, time_delta, max_allowed);
        // The reference holds rules as a set.
        if !out.contains(&rule) {
            out.push(rule);
        }
    }
    Ok(out)
}

/// The reference's serialisable tuple.
pub fn tuple(metadata: &DomainMetadata) -> Value {
    let headers = metadata.headers.as_ref().map_or(Value::Null, |headers| {
        Value::Array(headers.iter().map(|(k, v, r)| json!([k, v, r])).collect())
    });
    let rules = metadata.rules.as_ref().map_or(Value::Null, |rules| {
        json!([
            38,
            1,
            rules
                .iter()
                .map(|r| json!([r.kind as i64, r.time_delta, r.max_allowed]))
                .collect::<Vec<_>>()
        ])
    });
    json!([71, 1, [metadata.domain, headers, rules]])
}
