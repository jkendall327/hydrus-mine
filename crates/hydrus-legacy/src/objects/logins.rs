//! The login manager (`ClientNetworkingLogin.NetworkLoginManager`, type
//! 48): which domains log in with which login script, and whether that
//! login is switched on, plus full scripts, steps and credential definitions.

use super::domain::expect;
use super::util::{DecodeResult, boolean, malformed, string, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const MANAGER: SerialisableType = SerialisableType(48);

/// A domain's login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyLogin {
    pub domain: String,
    /// The login script's name.
    pub script: String,
    /// Whether hydrus logs in for this domain.
    pub active: bool,
}

/// Decode the login manager's per-domain logins.
pub fn logins(object: &SerialisableObject) -> DecodeResult<Vec<LegacyLogin>> {
    let k = MANAGER;
    expect(object, k, &[1])?;
    let info = object.info();
    let [_scripts, domains] = tuple::<2>(k, &info, "login manager")?;
    let PyJson::Object(domains) = domains else {
        return Err(malformed(k, "domains to login info is not a dictionary"));
    };
    let mut out = Vec::new();
    for (domain, login) in domains {
        let PyJson::List(fields) = login else {
            return Err(malformed(k, format!("{domain}: login info is not a tuple")));
        };
        // (script key and name, credentials, access type, access text,
        // active, validity, ...)
        let (Some(script), Some(active)) = (fields.first(), fields.get(4)) else {
            return Err(malformed(k, format!("{domain}: login info is too short")));
        };
        let [_key, name] = tuple::<2>(k, script, "login script key and name")?;
        out.push(LegacyLogin {
            domain: domain.clone(),
            script: string(k, name, "login script name")?,
            active: boolean(k, active, "active")?,
        });
    }
    Ok(out)
}

/// Decode a named credential definition with its presentation and input test.
pub fn credential_definition(
    object: &SerialisableObject,
) -> DecodeResult<hydrus_parse::login::CredentialDefinition> {
    use hydrus_parse::login::{CredentialDefinition, CredentialKind};
    let k = SerialisableType(72);
    expect(object, k, &[1])?;
    let info = object.info();
    let [kind, matcher] = tuple::<2>(k, &info, "credential definition")?;
    let kind = match super::util::int(k, kind, "credential type")? {
        0 => CredentialKind::Normal,
        1 => CredentialKind::Hidden,
        code => return Err(malformed(k, format!("unknown credential type {code}"))),
    };
    Ok(CredentialDefinition {
        name: object
            .name
            .clone()
            .ok_or_else(|| malformed(k, "credential has no name"))?,
        kind,
        reference_auxiliary: Some(
            serde_json::from_str(&object.to_tuple().to_python_string())
                .map_err(|error| malformed(k, error.to_string()))?,
        ),
        string_match: super::domain::string_match(&SerialisableObject::from_tuple(matcher)?)?,
    })
}

fn cookies(
    kind: SerialisableType,
    value: &PyJson,
    old: bool,
) -> DecodeResult<Vec<hydrus_parse::login::CookieRequirement>> {
    use crate::serialisable::Meta;
    let dictionary = SerialisableObject::from_tuple(value)?;
    super::util::dictionary_pairs(&dictionary)?
        .iter()
        .map(|(raw_name, value)| {
            let (name, original_name) = if old {
                let name = raw_name
                    .as_str()
                    .ok_or_else(|| malformed(kind, "cookie name is not text"))?;
                (
                    hydrus_core::url::strings::StringMatch::fixed(name),
                    serde_json::json!([51, 1, [0, name, null, null, name]]),
                )
            } else {
                let Meta::Object(name) = raw_name else {
                    return Err(malformed(kind, "cookie name is not a string matcher"));
                };
                let original = serde_json::from_str(&name.to_tuple().to_python_string())
                    .map_err(|error| malformed(kind, error.to_string()))?;
                (super::domain::string_match(name)?, original)
            };
            let Meta::Object(value) = value else {
                return Err(malformed(kind, "cookie value is not a string matcher"));
            };
            let original_value = serde_json::from_str(&value.to_tuple().to_python_string())
                .map_err(|error| malformed(kind, error.to_string()))?;
            Ok(hydrus_parse::login::CookieRequirement {
                name,
                value: super::domain::string_match(value)?,
                reference_auxiliary: Some((original_name, original_value)),
            })
        })
        .collect()
}
fn text_map(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<std::collections::BTreeMap<String, String>> {
    let PyJson::Object(values) = value else {
        return Err(malformed(kind, format!("{what} is not a dictionary")));
    };
    values
        .iter()
        .map(|(name, value)| Ok((name.clone(), string(kind, value, what)?)))
        .collect()
}

/// Decode an ordered step, upgrading v1 cookie names to fixed matchers.
pub fn login_step(object: &SerialisableObject) -> DecodeResult<hydrus_parse::login::LoginStep> {
    let k = SerialisableType(74);
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let [
        scheme,
        method,
        subdomain,
        path,
        credentials,
        static_args,
        temp_args,
        required_cookies,
        parsers,
    ] = tuple::<9>(k, &info, "login step")?;
    let scheme = string(k, scheme, "scheme")?;
    let method = string(k, method, "method")?;
    if !matches!(scheme.as_str(), "http" | "https") || !matches!(method.as_str(), "GET" | "POST") {
        return Err(malformed(k, "login steps support http/https and GET/POST"));
    }
    let content_parsers = super::domain::nested_list(k, parsers, "content parsers")?
        .iter()
        .map(|object| {
            let mut parser = super::parsers::content_parser(object)?;
            let content_info = object.info();
            let [_name, _kind, formula, _extra] =
                tuple::<4>(SerialisableType(30), &content_info, "content parser")?;
            parser.formula.reference_auxiliary = Some(
                serde_json::from_str(&formula.to_python_string())
                    .map_err(|error| malformed(k, error.to_string()))?,
            );
            Ok(parser)
        })
        .collect::<DecodeResult<_>>()?;
    let mut step = hydrus_parse::login::LoginStep {
        name: object
            .name
            .clone()
            .ok_or_else(|| malformed(k, "step has no name"))?,
        scheme,
        method,
        subdomain: super::util::opt_string(k, subdomain, "subdomain")?,
        path: string(k, path, "path")?,
        credentials: text_map(k, credentials, "credentials")?,
        static_args: text_map(k, static_args, "static arguments")?,
        temp_args: text_map(k, temp_args, "temporary arguments")?,
        required_cookies: cookies(k, required_cookies, object.version == 1)?,
        content_parsers,
    };
    step.cleanse();
    Ok(step)
}

/// Decode a named script, retaining order, key, credentials and parser content.
pub fn login_script(object: &SerialisableObject) -> DecodeResult<hydrus_parse::login::LoginScript> {
    use hydrus_parse::login::{Access, ExampleDomain, LoginScript};
    let k = SerialisableType(73);
    expect(object, k, &[1, 2])?;
    let info = object.info();
    let [key, required_cookies, credentials, steps, examples] =
        tuple::<5>(k, &info, "login script")?;
    let examples = super::util::list(k, examples, "example domains")?
        .iter()
        .map(|value| {
            let [domain, access, description] = tuple::<3>(k, value, "example domain")?;
            let access = Access::from_code(super::util::int(k, access, "access type")?)
                .ok_or_else(|| malformed(k, "unknown login access type"))?;
            Ok(ExampleDomain {
                domain: string(k, domain, "domain")?,
                access,
                description: string(k, description, "access description")?,
            })
        })
        .collect::<DecodeResult<_>>()?;
    Ok(LoginScript {
        name: object
            .name
            .clone()
            .ok_or_else(|| malformed(k, "script has no name"))?,
        key: hex::encode(super::util::hex_bytes(k, key, "login script key")?),
        required_cookies: cookies(k, required_cookies, object.version == 1)?,
        credentials: super::domain::nested_list(k, credentials, "credential definitions")?
            .iter()
            .map(credential_definition)
            .collect::<DecodeResult<_>>()?,
        steps: super::domain::nested_list(k, steps, "login steps")?
            .iter()
            .map(login_step)
            .collect::<DecodeResult<_>>()?,
        examples,
    })
}

/// Decode preserved scripts and domain credentials without discarding fields.
pub fn manager(object: &SerialisableObject) -> DecodeResult<hydrus_parse::login::LoginManager> {
    use hydrus_parse::login::{Access, DomainLogin, LoginManager, Validity};
    let k = MANAGER;
    expect(object, k, &[1])?;
    let info = object.info();
    let [scripts, domains] = tuple::<2>(k, &info, "login manager")?;
    let PyJson::Object(domains) = domains else {
        return Err(malformed(k, "login domains are not a dictionary"));
    };
    let domains = domains
        .iter()
        .map(|(domain, value)| {
            let [
                script,
                credentials,
                access,
                description,
                active,
                validity,
                error,
                until,
                reason,
            ] = tuple::<9>(k, value, "domain login")?;
            let [key, name] = tuple::<2>(k, script, "login script key and name")?;
            let access = Access::from_code(super::util::int(k, access, "access")?)
                .ok_or_else(|| malformed(k, "unknown access type"))?;
            let validity = Validity::from_code(super::util::int(k, validity, "validity")?)
                .ok_or_else(|| malformed(k, "unknown validity"))?;
            Ok((
                domain.clone(),
                DomainLogin {
                    script_key: hex::encode(super::util::hex_bytes(k, key, "script key")?),
                    script_name: string(k, name, "script name")?,
                    credentials: text_map(k, credentials, "credentials")?,
                    access,
                    description: string(k, description, "access description")?,
                    active: boolean(k, active, "active")?,
                    validity,
                    validity_error: string(k, error, "validity error")?,
                    no_work_until: super::util::int(k, until, "delay time")?,
                    delay_reason: string(k, reason, "delay reason")?,
                },
            ))
        })
        .collect::<DecodeResult<_>>()?;
    Ok(LoginManager {
        scripts: super::domain::nested_list(k, scripts, "login scripts")?
            .iter()
            .map(login_script)
            .collect::<DecodeResult<_>>()?,
        domains,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_domains_log_in() {
        let stored = r#"[48, 1, [[24, 1, []], {"example.com": [["00ff", "example login"], {"username": "someone", "password": "secret"}, 0, "", true, 1, "", 0, ""], "other.example": [["01", "other"], {}, 0, "", false, 0, "", 0, ""]}]]"#;
        let object = SerialisableObject::from_tuple_str(stored).unwrap();
        let logins = logins(&object).unwrap();
        assert_eq!(
            logins,
            [
                LegacyLogin {
                    domain: "example.com".into(),
                    script: "example login".into(),
                    active: true,
                },
                LegacyLogin {
                    domain: "other.example".into(),
                    script: "other".into(),
                    active: false,
                },
            ]
        );
    }
}
