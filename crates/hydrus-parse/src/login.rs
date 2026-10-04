//! Typed login definitions and pure validation; HTTP execution belongs to net.
use crate::content::{ContentKind, ContentParser};
use hydrus_core::{pages::PageKey, url::strings::StringMatch};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Whether credential input is shown normally or masked as a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialKind {
    Normal,
    Hidden,
}
impl CredentialKind {
    pub const fn code(self) -> i64 {
        match self {
            Self::Normal => 0,
            Self::Hidden => 1,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Hidden => "hidden (password)",
        }
    }
}
/// A named credential's input presentation and permitted values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialDefinition {
    pub name: String,
    pub kind: CredentialKind,
    pub string_match: StringMatch,
    /// Inert reference matcher fields retained through interchange.
    #[serde(default)]
    pub reference_auxiliary: Option<serde_json::Value>,
}
impl Default for CredentialDefinition {
    fn default() -> Self {
        Self {
            name: "username".into(),
            kind: CredentialKind::Normal,
            string_match: StringMatch::any(),
            reference_auxiliary: None,
        }
    }
}
impl CredentialDefinition {
    /// Validate non-empty or empty values as the configured string matcher does.
    pub fn test(&self, text: &str) -> Result<(), String> {
        self.string_match
            .test(text)
            .map_err(|error| format!("Could not validate \"{}\" credential: {error}", self.name))
    }
}
/// The access a domain login grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Everything,
    Nsfw,
    Special,
    UserPreferences,
}
impl Access {
    pub const fn code(self) -> i64 {
        match self {
            Self::Everything => 0,
            Self::Nsfw => 1,
            Self::Special => 2,
            Self::UserPreferences => 3,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Everything => "Everything",
            Self::Nsfw => "NSFW",
            Self::Special => "Special",
            Self::UserPreferences => "User prefs",
        }
    }
    pub const fn description(self) -> &'static str {
        match self {
            Self::Everything => "Login required to access any content.",
            Self::Nsfw => "Login required to access NSFW content.",
            Self::Special => "Login required to access special content.",
            Self::UserPreferences => "Login only required to access user preferences.",
        }
    }
    pub const fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Everything),
            1 => Some(Self::Nsfw),
            2 => Some(Self::Special),
            3 => Some(Self::UserPreferences),
            _ => None,
        }
    }
}
/// A required cookie's name and value tests, retained in execution order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CookieRequirement {
    pub name: StringMatch,
    pub value: StringMatch,
    /// Original matcher tuples, when they came from reference serialization.
    #[serde(default)]
    pub reference_auxiliary: Option<(serde_json::Value, serde_json::Value)>,
}
/// An example domain and its access description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExampleDomain {
    pub domain: String,
    pub access: Access,
    pub description: String,
}
/// One ordered request and its credential, variable, cookie and content rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginStep {
    pub name: String,
    pub scheme: String,
    pub method: String,
    pub subdomain: Option<String>,
    pub path: String,
    pub credentials: BTreeMap<String, String>,
    pub static_args: BTreeMap<String, String>,
    pub temp_args: BTreeMap<String, String>,
    pub required_cookies: Vec<CookieRequirement>,
    pub content_parsers: Vec<ContentParser>,
}
impl Default for LoginStep {
    fn default() -> Self {
        Self {
            name: "hit home page to establish session".into(),
            scheme: "https".into(),
            method: "GET".into(),
            subdomain: None,
            path: "/".into(),
            credentials: BTreeMap::new(),
            static_args: BTreeMap::new(),
            temp_args: BTreeMap::new(),
            required_cookies: Vec::new(),
            content_parsers: Vec::new(),
        }
    }
}
impl LoginStep {
    /// Clean subdomain characters and prepend a missing path slash, as Python.
    pub fn cleanse(&mut self) {
        if let Some(subdomain) = &mut self.subdomain {
            subdomain.retain(|c| c.is_ascii_lowercase() || c == '.');
        }
        if !self.path.starts_with('/') {
            self.path.insert(0, '/');
        }
    }
    /// Variables made available after this step's content parsers succeed.
    pub fn set_variables(&self) -> BTreeSet<&str> {
        self.content_parsers
            .iter()
            .filter_map(|parser| match &parser.kind {
                ContentKind::Variable { name } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}
/// A named, independently keyed login script used by domain entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoginScript {
    pub name: String,
    pub key: String,
    pub required_cookies: Vec<CookieRequirement>,
    pub credentials: Vec<CredentialDefinition>,
    pub steps: Vec<LoginStep>,
    pub examples: Vec<ExampleDomain>,
}
impl Default for LoginScript {
    fn default() -> Self {
        Self {
            name: "login script".into(),
            key: PageKey::random().to_hex(),
            required_cookies: Vec::new(),
            credentials: Vec::new(),
            steps: Vec::new(),
            examples: Vec::new(),
        }
    }
}
impl LoginScript {
    /// Ensure all required credentials are defined and variables precede use.
    pub fn check_valid(&self) -> Result<(), String> {
        let defined: BTreeSet<_> = self
            .credentials
            .iter()
            .map(|credential| credential.name.as_str())
            .collect();
        let required: BTreeSet<_> = self
            .steps
            .iter()
            .flat_map(|step| step.credentials.keys().map(String::as_str))
            .collect();
        let missing: Vec<_> = required.difference(&defined).copied().collect();
        if !missing.is_empty() {
            return Err(format!(
                "Missing required credential definitions: {}",
                missing.join(", ")
            ));
        }
        let mut variables = BTreeSet::new();
        for step in &self.steps {
            let missing: Vec<_> = step
                .temp_args
                .keys()
                .map(String::as_str)
                .filter(|name| !variables.contains(name))
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "Missing temp variables for login step \"{}\": {}",
                    step.name,
                    missing.join(", ")
                ));
            }
            variables.extend(step.set_variables());
        }
        Ok(())
    }
    /// Validate required credential presence, then each supplied defined value.
    pub fn check_credentials(&self, credentials: &BTreeMap<String, String>) -> Result<(), String> {
        self.check_valid()?;
        let missing: BTreeSet<_> = self
            .steps
            .iter()
            .flat_map(|step| step.credentials.keys())
            .filter(|name| !credentials.contains_key(*name))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Missing required credentials: {}",
                missing.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
        for (name, value) in credentials {
            if let Some(definition) = self
                .credentials
                .iter()
                .rev()
                .find(|definition| &definition.name == name)
            {
                definition.test(value)?;
            }
        }
        Ok(())
    }
}

/// The last known validation outcome for a domain's login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Validity {
    Valid,
    Untested,
    Invalid,
}
impl Validity {
    pub const fn code(self) -> i64 {
        match self {
            Self::Valid => 0,
            Self::Untested => 1,
            Self::Invalid => 2,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Untested => "untested",
            Self::Invalid => "invalid",
        }
    }
    pub const fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Valid),
            1 => Some(Self::Untested),
            2 => Some(Self::Invalid),
            _ => None,
        }
    }
}
/// The script selection, credentials, activation and delay for a login domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainLogin {
    pub script_key: String,
    pub script_name: String,
    pub credentials: BTreeMap<String, String>,
    pub access: Access,
    pub description: String,
    pub active: bool,
    pub validity: Validity,
    pub validity_error: String,
    pub no_work_until: i64,
    pub delay_reason: String,
}
/// Login preferences preserve scripts and domain credentials together.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LoginManager {
    pub scripts: Vec<LoginScript>,
    pub domains: BTreeMap<String, DomainLogin>,
}
impl LoginManager {
    /// Resolve a domain's script by stable key, falling back to its saved name.
    pub fn script(&self, login: &DomainLogin) -> Option<&LoginScript> {
        self.scripts
            .iter()
            .find(|script| script.key == login.script_key)
            .or_else(|| {
                self.scripts
                    .iter()
                    .find(|script| script.name == login.script_name)
            })
    }
}
