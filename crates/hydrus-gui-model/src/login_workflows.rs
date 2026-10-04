//! Login drafts: credential validation/confirmation and isolated script edits.
use crate::{favourites::non_dupe_name, list_selection::ListSelection};
use hydrus_core::pages::PageKey;
use hydrus_parse::login::{CredentialDefinition, CredentialKind, LoginManager, LoginScript};
use std::collections::BTreeMap;

/// One credential entry and its live validity label.
#[derive(Debug, Clone, PartialEq)]
pub struct CredentialRow {
    pub definition: CredentialDefinition,
    pub value: String,
    pub label: String,
    pub valid: bool,
}
/// A credentials draft in normal-then-hidden stable display order.
#[derive(Debug, Clone)]
pub struct CredentialsEditor {
    rows: Vec<(CredentialDefinition, String)>,
}
impl CredentialsEditor {
    pub fn new(
        definitions: &[CredentialDefinition],
        credentials: &BTreeMap<String, String>,
    ) -> Self {
        let mut definitions = definitions.to_vec();
        definitions.sort_by_key(|definition| definition.kind == CredentialKind::Hidden);
        Self {
            rows: definitions
                .into_iter()
                .map(|definition| {
                    let value = credentials
                        .get(&definition.name)
                        .cloned()
                        .unwrap_or_default();
                    (definition, value)
                })
                .collect(),
        }
    }
    pub fn set(&mut self, index: usize, value: String) {
        if let Some(row) = self.rows.get_mut(index) {
            row.1 = value;
        }
    }
    pub fn rows(&self) -> Vec<CredentialRow> {
        self.rows
            .iter()
            .map(|(definition, value)| {
                let result = definition.test(value);
                let label = if value.is_empty() {
                    definition.string_match.describe(false, false)
                } else {
                    result
                        .clone()
                        .map_or_else(|error| error, |()| "looks good ✓".into())
                };
                CredentialRow {
                    definition: definition.clone(),
                    value: value.clone(),
                    label,
                    valid: !value.is_empty() && result.is_ok(),
                }
            })
            .collect()
    }
    /// Confirmation text; invalid or blank input remains explicitly acceptable.
    pub fn warning(&self) -> Option<String> {
        let errors = self
            .rows
            .iter()
            .filter_map(|(definition, value)| {
                if value.is_empty() {
                    Some(format!("Value for {} is blank!", definition.name))
                } else {
                    definition
                        .test(value)
                        .err()
                        .map(|error| format!("For {}: {error}", definition.name))
                }
            })
            .collect::<Vec<_>>();
        (!errors.is_empty()).then(|| {
            format!(
                "These values are invalid--are you sure this is ok?\n\n{}",
                errors.join("\n")
            )
        })
    }
    pub fn value(&self) -> BTreeMap<String, String> {
        self.rows
            .iter()
            .map(|(definition, value)| (definition.name.clone(), value.clone()))
            .collect()
    }
}
/// A sorted login scripts draft; adding/importing regenerates stable keys.
#[derive(Debug, Clone)]
pub struct ScriptsEditor {
    pub draft: LoginManager,
    selection: ListSelection<usize>,
    original_scripts: Vec<LoginScript>,
}
impl ScriptsEditor {
    pub fn new(draft: LoginManager) -> Self {
        Self {
            original_scripts: draft.scripts.clone(),
            draft,
            selection: ListSelection::default(),
        }
    }
    pub fn order(&self) -> Vec<usize> {
        let mut rows: Vec<_> = (0..self.draft.scripts.len()).collect();
        rows.sort_by(|&a, &b| self.draft.scripts[a].name.cmp(&self.draft.scripts[b].name));
        rows
    }
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        self.selection.click(&self.order(), row, ctrl, shift);
    }
    pub fn selected(&self) -> Vec<usize> {
        self.selection.in_order(&self.order())
    }
    pub fn editing(&self) -> Option<usize> {
        self.selection.one()
    }
    pub fn put(&mut self, index: Option<usize>, mut script: LoginScript) {
        script.name = non_dupe_name(&script.name, &|name| {
            self.draft
                .scripts
                .iter()
                .enumerate()
                .any(|(i, old)| Some(i) != index && old.name == name)
        });
        let index = if let Some(index) = index.filter(|&i| i < self.draft.scripts.len()) {
            script.key.clone_from(&self.draft.scripts[index].key);
            self.draft.scripts[index] = script;
            index
        } else {
            script.key = PageKey::random().to_hex();
            self.draft.scripts.push(script);
            self.draft.scripts.len() - 1
        };
        self.selection.select_only(Some(index));
    }
    pub fn delete(&mut self) {
        let mut selected = self.selected();
        selected.sort_unstable();
        for index in selected.into_iter().rev() {
            self.draft.scripts.remove(index);
        }
        self.selection.select_only(None);
    }
    /// Apply only this script list, preserving concurrently edited domain credentials.
    pub fn save(&self, store: &hydrus_store::Store) -> hydrus_store::Result<()> {
        let scripts = self.draft.scripts.clone();
        let original = self.original_scripts.clone();
        store.write_and_refresh(move |ctx| {
            let mut current = hydrus_store::logins::load(ctx.conn())?;
            if current.scripts != original {
                return Err(hydrus_store::StoreError::Invalid(
                    "Login scripts changed in another editor. Reopen this dialog before applying."
                        .into(),
                ));
            }
            current.scripts = scripts;
            hydrus_store::logins::save(ctx.conn(), &current)
        })
    }
    /// Import every supported script atomically, with reference name/key rules.
    pub fn import(&mut self, text: &str) -> Result<(), String> {
        let scripts = hydrus_downloader_exchange::logins::decode_text(text)
            .map_err(|error| error.to_string())?;
        for script in scripts {
            self.put(None, script);
        }
        Ok(())
    }
    pub fn export(&self) -> Vec<LoginScript> {
        self.selected()
            .into_iter()
            .map(|i| self.draft.scripts[i].clone())
            .collect()
    }
}
/// The reference's advisory invalid-script confirmation.
pub fn script_warning(script: &LoginScript) -> Option<String> {
    script.check_valid().err().map(|error| format!("There is a problem with this script. The reason is:\n\n{error}\n\nDo you want to proceed with this invalid script, or go back and fix it?"))
}

/// One login step draft with sorted, named VARIABLE/VETO response parsers.
#[derive(Debug, Clone)]
pub struct StepEditor {
    pub step: hydrus_parse::login::LoginStep,
    pub selection: ListSelection<usize>,
}
impl StepEditor {
    pub fn new(step: &hydrus_parse::login::LoginStep) -> Self {
        Self {
            step: step.clone(),
            selection: ListSelection::default(),
        }
    }
    pub fn order(&self) -> Vec<usize> {
        let mut order = (0..self.step.content_parsers.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| {
            self.step.content_parsers[a]
                .name
                .cmp(&self.step.content_parsers[b].name)
        });
        order
    }
    pub fn put(
        &mut self,
        index: Option<usize>,
        mut parser: hydrus_parse::content::ContentParser,
    ) -> Result<(), String> {
        if !matches!(
            parser.kind,
            hydrus_parse::content::ContentKind::Variable { .. }
                | hydrus_parse::content::ContentKind::Veto { .. }
        ) {
            return Err("Login response parsers must produce a temporary variable or veto.".into());
        }
        parser.name = non_dupe_name(&parser.name, &|name| {
            self.step
                .content_parsers
                .iter()
                .enumerate()
                .any(|(i, old)| Some(i) != index && old.name == name)
        });
        let i = if let Some(i) = index.filter(|&i| i < self.step.content_parsers.len()) {
            self.step.content_parsers[i] = parser;
            i
        } else {
            self.step.content_parsers.push(parser);
            self.step.content_parsers.len() - 1
        };
        self.selection.select_only(Some(i));
        Ok(())
    }
    pub fn import(
        &mut self,
        definitions: Vec<hydrus_downloader_exchange::Definition>,
    ) -> Result<(), String> {
        use hydrus_downloader_exchange::Native;
        let parsers = definitions
            .into_iter()
            .map(|definition| match definition.native {
                Native::Content(parser)
                    if matches!(
                        parser.kind,
                        hydrus_parse::content::ContentKind::Variable { .. }
                            | hydrus_parse::content::ContentKind::Veto { .. }
                    ) =>
                {
                    Ok(parser)
                }
                _ => Err("Import only temporary-variable or veto content parsers.".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        for parser in parsers {
            self.put(None, parser)?;
        }
        Ok(())
    }
    pub fn delete(&mut self) {
        let mut selected = self.selection.in_order(&self.order());
        selected.sort_unstable();
        for i in selected.into_iter().rev() {
            self.step.content_parsers.remove(i);
        }
        self.selection.select_only(None);
    }
    pub fn value(&self) -> hydrus_parse::login::LoginStep {
        let mut step = self.step.clone();
        step.content_parsers = self
            .order()
            .into_iter()
            .map(|i| step.content_parsers[i].clone())
            .collect();
        step.cleanse();
        step
    }
}

/// Domain credential edits preserve scripts and all unrelated domain fields.
#[derive(Debug, Clone)]
pub struct DomainsEditor {
    pub draft: LoginManager,
    original_domains: BTreeMap<String, hydrus_parse::login::DomainLogin>,
    pub selection: ListSelection<String>,
}
impl DomainsEditor {
    pub fn new(draft: LoginManager) -> Self {
        Self {
            original_domains: draft.domains.clone(),
            draft,
            selection: ListSelection::default(),
        }
    }
    pub fn order(&self) -> Vec<String> {
        self.draft.domains.keys().cloned().collect()
    }
    pub fn replace_credentials(
        &mut self,
        domain: &str,
        credentials: BTreeMap<String, String>,
    ) -> Result<bool, String> {
        use hydrus_parse::login::Validity;
        let old = self
            .draft
            .domains
            .get(domain)
            .ok_or("The login domain no longer exists.")?;
        let script=self.draft.script(old).ok_or_else(||format!("Could not find a login script for \"{domain}\"! Please re-add the login script in the other dialog or update the entry here to a new one!"))?;
        let result = script.check_credentials_for_entry(&credentials);
        let good = result.is_ok()
            && (credentials.is_empty() || credentials.values().any(|v| !v.is_empty()));
        let login = self.draft.domains.get_mut(domain).expect("existing domain");
        login.credentials = credentials;
        match result {
            Ok(()) => {
                login.validity = Validity::Untested;
                login.validity_error.clear();
            }
            Err(error) => {
                login.validity = Validity::Invalid;
                login.validity_error = error;
            }
        }
        if !good {
            login.active = false;
        }
        login.no_work_until = 0;
        login.delay_reason.clear();
        Ok(good && !login.active)
    }
    pub fn save(&self, store: &hydrus_store::Store) -> hydrus_store::Result<()> {
        let domains = self.draft.domains.clone();
        let original = self.original_domains.clone();
        store.write_and_refresh(move |ctx| {
            let mut current = hydrus_store::logins::load(ctx.conn())?;
            if current.domains != original {
                return Err(hydrus_store::StoreError::Invalid(
                    "Domain logins changed in another editor. Reopen this dialog before applying."
                        .into(),
                ));
            }
            current.domains = domains;
            hydrus_store::logins::save(ctx.conn(), &current)
        })
    }
}
