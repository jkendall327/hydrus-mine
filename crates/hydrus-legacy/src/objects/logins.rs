//! The login manager (`ClientNetworkingLogin.NetworkLoginManager`, type
//! 48): which domains log in with which login script, and whether that
//! login is switched on. The credentials stored alongside are not read.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_domains_log_in() {
        let stored = r#"[48, 1, [[24, 1, []], {"example.com": [["00ff", "example login"], {"username": "someone", "password": "secret"}, 0, "", true, 1, "", 0, ""], "other.net": [["01", "other"], {}, 0, "", false, 0, "", 0, ""]}]]"#;
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
                    domain: "other.net".into(),
                    script: "other".into(),
                    active: false,
                },
            ]
        );
    }
}
