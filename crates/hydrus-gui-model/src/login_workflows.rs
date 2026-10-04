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
    pub argument_selection: [ListSelection<usize>; 3],
}
impl StepEditor {
    pub fn new(step: &hydrus_parse::login::LoginStep) -> Self {
        Self {
            step: step.clone(),
            selection: ListSelection::default(),
            argument_selection: std::array::from_fn(|_| ListSelection::default()),
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
    pub selection: ListSelection<usize>,
}
/// Reference domain row, with cookie-derived login state independent of activation.
pub fn domain_cells(
    domain: &str,
    login: &hydrus_parse::login::DomainLogin,
    script: Option<&LoginScript>,
    logged_in: bool,
    expires: Option<i64>,
    now: i64,
) -> Vec<String> {
    vec![
        domain.into(),
        script.map_or_else(
            || "login script not found".into(),
            |script| script.name.clone(),
        ),
        format!("{} - {}", login.access.label(), login.description),
        if login.active { "yes" } else { "no" }.into(),
        if logged_in {
            format!(
                "yes - {}",
                crate::network_sessions::expiry_text(expires, now)
            )
        } else {
            "no".into()
        },
        if login.active {
            if login.validity_error.is_empty() {
                login.validity.label().into()
            } else {
                format!("{} - {}", login.validity.label(), login.validity_error)
            }
        } else {
            String::new()
        },
        if login.no_work_until > now {
            format!(
                "{} - {}",
                crate::network_sessions::expiry_text(Some(login.no_work_until), now),
                login.delay_reason
            )
        } else {
            String::new()
        },
    ]
}
impl DomainsEditor {
    pub fn new(draft: LoginManager) -> Self {
        Self {
            original_domains: draft.domains.clone(),
            draft,
            selection: ListSelection::default(),
        }
    }
    pub fn order(&self) -> Vec<usize> {
        (0..self.draft.domains.len()).collect()
    }
    pub fn domain_at(&self, index: usize) -> Option<String> {
        self.draft.domains.keys().nth(index).cloned()
    }
    pub fn selected_domain(&self) -> Option<String> {
        self.selected_domains().into_iter().next()
    }
    pub fn selected_domains(&self) -> Vec<String> {
        self.selection
            .in_order(&self.order())
            .into_iter()
            .filter_map(|i| self.domain_at(i))
            .collect()
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

/// One of the reference's three independent request argument dictionaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    Credential,
    Static,
    Temporary,
}
impl ArgumentKind {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Credential,
            2 => Self::Temporary,
            _ => Self::Static,
        }
    }
    pub fn index(self) -> i32 {
        match self {
            Self::Credential => 0,
            Self::Static => 1,
            Self::Temporary => 2,
        }
    }
    pub fn key_name(self) -> &'static str {
        match self {
            Self::Credential => "credential name",
            Self::Static => "parameter name",
            Self::Temporary => "temp variable name",
        }
    }
}
impl StepEditor {
    pub fn arguments(&self, kind: ArgumentKind) -> &BTreeMap<String, String> {
        match kind {
            ArgumentKind::Credential => &self.step.credentials,
            ArgumentKind::Static => &self.step.static_args,
            ArgumentKind::Temporary => &self.step.temp_args,
        }
    }
    fn arguments_mut(&mut self, kind: ArgumentKind) -> &mut BTreeMap<String, String> {
        match kind {
            ArgumentKind::Credential => &mut self.step.credentials,
            ArgumentKind::Static => &mut self.step.static_args,
            ArgumentKind::Temporary => &mut self.step.temp_args,
        }
    }
    /// Blank values are accepted; duplicate keys and blank names leave the draft intact.
    pub fn set_argument(
        &mut self,
        kind: ArgumentKind,
        old: Option<&str>,
        key: String,
        value: String,
    ) -> Result<(), String> {
        if key.is_empty() {
            return Err(format!("Enter the {}.", kind.key_name()));
        }
        let values = self.arguments_mut(kind);
        if old != Some(key.as_str()) && values.contains_key(&key) {
            return Err(format!("That {} already exists!", kind.key_name()));
        }
        if let Some(old) = old {
            values.remove(old);
        }
        values.insert(key, value);
        self.argument_selection[kind.index() as usize].select_only(None);
        Ok(())
    }
    pub fn remove_argument(&mut self, kind: ArgumentKind, key: &str) {
        self.arguments_mut(kind).remove(key);
        self.argument_selection[kind.index() as usize].select_only(None);
    }
    pub fn select_arguments(&mut self, kind: ArgumentKind, index: usize, ctrl: bool, shift: bool) {
        let order = (0..self.arguments(kind).len()).collect::<Vec<_>>();
        self.argument_selection[kind.index() as usize].click(&order, index, ctrl, shift);
    }
    pub fn selected_argument(&self, kind: ArgumentKind) -> Option<(ArgumentKind, String)> {
        self.argument_selection[kind.index() as usize]
            .one()
            .and_then(|index| self.arguments(kind).keys().nth(index).cloned())
            .map(|key| (kind, key))
    }
    pub fn selected_arguments(&self, kind: ArgumentKind) -> Vec<String> {
        let order = (0..self.arguments(kind).len()).collect::<Vec<_>>();
        self.argument_selection[kind.index() as usize]
            .in_order(&order)
            .into_iter()
            .filter_map(|i| self.arguments(kind).keys().nth(i).cloned())
            .collect()
    }
    pub fn delete_arguments(&mut self, kind: ArgumentKind) {
        for key in self.selected_arguments(kind) {
            self.remove_argument(kind, &key);
        }
    }
    pub fn argument_rows(&self) -> Vec<(ArgumentKind, String, String)> {
        [
            ArgumentKind::Credential,
            ArgumentKind::Static,
            ArgumentKind::Temporary,
        ]
        .into_iter()
        .flat_map(|kind| {
            self.arguments(kind)
                .iter()
                .map(move |(key, value)| (kind, key.clone(), value.clone()))
        })
        .collect()
    }
}

/// Detached cookie requirement list; matcher objects may have identical descriptions.
#[derive(Debug, Clone)]
pub struct CookiesEditor {
    pub rows: Vec<hydrus_parse::login::CookieRequirement>,
    pub selection: ListSelection<usize>,
}
impl CookiesEditor {
    pub fn new(rows: &[hydrus_parse::login::CookieRequirement]) -> Self {
        Self {
            rows: rows.to_vec(),
            selection: ListSelection::default(),
        }
    }
    pub fn order(&self) -> Vec<usize> {
        let mut order = (0..self.rows.len()).collect::<Vec<_>>();
        order.sort_by_cached_key(|&i| {
            (
                self.rows[i].name.describe(false, false),
                self.rows[i].value.describe(false, false),
            )
        });
        order
    }
    pub fn value(&self) -> Vec<hydrus_parse::login::CookieRequirement> {
        self.order()
            .into_iter()
            .map(|i| self.rows[i].clone())
            .collect()
    }
    pub fn put(&mut self, index: Option<usize>, value: hydrus_parse::login::CookieRequirement) {
        let index = if let Some(index) = index.filter(|&i| i < self.rows.len()) {
            self.rows[index] = value;
            index
        } else {
            self.rows.push(value);
            self.rows.len() - 1
        };
        self.selection.select_only(Some(index));
    }
    pub fn delete(&mut self) {
        let selected = self.selection.in_order(&self.order());
        self.rows = std::mem::take(&mut self.rows)
            .into_iter()
            .enumerate()
            .filter_map(|(i, row)| (!selected.contains(&i)).then_some(row))
            .collect();
        self.selection.select_only(None);
    }
}

/// Three-stage reference example-domain entry, including final-description Cancel.
#[derive(Debug, Clone)]
pub struct ExampleDraft {
    pub domain: String,
    pub access: hydrus_parse::login::Access,
    pub description: String,
    original_access: hydrus_parse::login::Access,
    original_description: String,
}
impl ExampleDraft {
    pub fn new(row: Option<&hydrus_parse::login::ExampleDomain>) -> Self {
        let access = row.map_or(hydrus_parse::login::Access::Nsfw, |row| row.access);
        let description = row.map_or_else(
            || access.description().to_owned(),
            |row| row.description.clone(),
        );
        Self {
            domain: row.map_or_else(|| "example.com".to_owned(), |row| row.domain.clone()),
            access,
            original_access: access,
            original_description: description.clone(),
            description,
        }
    }
    pub fn validate_domain(
        &self,
        rows: &[hydrus_parse::login::ExampleDomain],
        index: Option<usize>,
    ) -> Result<(), String> {
        if self.domain.is_empty() {
            return Err("Enter the domain.".into());
        }
        if index
            .and_then(|i| rows.get(i))
            .is_some_and(|row| row.domain == self.domain)
        {
            return Ok(());
        }
        if rows.iter().any(|row| row.domain == self.domain) {
            return Err("That domain already exists!".into());
        }
        Ok(())
    }
    pub fn select_access(&mut self, access: hydrus_parse::login::Access) {
        self.access = access;
        self.description = if access == self.original_access {
            self.original_description.clone()
        } else {
            access.description().to_owned()
        };
    }
    pub fn value(
        &self,
        description: Option<&str>,
    ) -> Result<hydrus_parse::login::ExampleDomain, String> {
        if description == Some("") {
            return Err("Enter the access description.".into());
        }
        Ok(hydrus_parse::login::ExampleDomain {
            domain: self.domain.clone(),
            access: self.access,
            description: description.unwrap_or(&self.description).to_owned(),
        })
    }
}

/// Current prompt in the reference's Add/change-login-script chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainEntryStage {
    Script,
    Example,
    Domain,
    Access,
    Description,
    Credentials,
    Activate,
    Done,
    Canceled,
}
/// One reference choice; the separator deliberately carries no script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainChoice {
    pub label: String,
    pub value: Option<usize>,
}
/// Isolated domain entry: no manager mutation until the entire prompt chain accepts.
#[derive(Debug, Clone)]
pub struct DomainEntry {
    manager: LoginManager,
    editing: Option<String>,
    script_index: Option<usize>,
    pub stage: DomainEntryStage,
    pub domain: String,
    pub access: hydrus_parse::login::Access,
    pub description: String,
    pub credentials: BTreeMap<String, String>,
    active: bool,
    validity: hydrus_parse::login::Validity,
    validity_error: String,
}
impl DomainEntry {
    pub fn new(manager: &LoginManager, editing: Option<&str>) -> Result<Self, String> {
        if manager.scripts.is_empty() && editing.is_none() {
            return Err("You have no login scripts, so you cannot add a new login!".into());
        }
        let old = editing.and_then(|domain| manager.domains.get(domain));
        Ok(Self {
            manager: manager.clone(),
            editing: editing.map(str::to_owned),
            script_index: None,
            stage: DomainEntryStage::Script,
            domain: editing.unwrap_or_default().into(),
            access: hydrus_parse::login::Access::Everything,
            description: String::new(),
            credentials: old.map_or_else(BTreeMap::new, |old| old.credentials.clone()),
            active: old.is_some_and(|old| old.active),
            validity: hydrus_parse::login::Validity::Untested,
            validity_error: String::new(),
        })
    }
    pub fn script(&self) -> Option<&LoginScript> {
        self.script_index.and_then(|i| self.manager.scripts.get(i))
    }
    pub fn choices(&self) -> Vec<DomainChoice> {
        match self.stage {
            DomainEntryStage::Script => {
                let mut indices = (0..self.manager.scripts.len()).collect::<Vec<_>>();
                indices.sort_by(|&a, &b| {
                    self.manager.scripts[a]
                        .name
                        .cmp(&self.manager.scripts[b].name)
                });
                let choice = |i: usize| DomainChoice {
                    label: self.manager.scripts[i].name.clone(),
                    value: Some(i),
                };
                if self.editing.is_none() {
                    return indices.into_iter().map(choice).collect();
                }
                let matches = |i: &usize| {
                    self.manager.scripts[*i]
                        .examples
                        .iter()
                        .any(|example| example.domain == self.domain)
                };
                let mut rows = indices
                    .iter()
                    .copied()
                    .filter(matches)
                    .map(choice)
                    .collect::<Vec<_>>();
                let other = indices
                    .into_iter()
                    .filter(|i| !matches(i))
                    .map(choice)
                    .collect::<Vec<_>>();
                if !rows.is_empty() && !other.is_empty() {
                    rows.push(DomainChoice {
                        label: "------".into(),
                        value: None,
                    });
                }
                rows.extend(other);
                rows
            }
            DomainEntryStage::Example => {
                let mut domains = self
                    .script()
                    .into_iter()
                    .flat_map(|script| &script.examples)
                    .filter(|example| !self.manager.domains.contains_key(&example.domain))
                    .map(|example| example.domain.clone())
                    .collect::<Vec<_>>();
                domains.sort();
                domains.dedup();
                let mut rows = domains
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| DomainChoice {
                        label,
                        value: Some(i),
                    })
                    .collect::<Vec<_>>();
                rows.push(DomainChoice {
                    label: "use other domain".into(),
                    value: None,
                });
                rows
            }
            DomainEntryStage::Access => (0..4)
                .map(|i| DomainChoice {
                    label: hydrus_parse::login::Access::from_code(i as i64)
                        .expect("access code")
                        .label()
                        .into(),
                    value: Some(i),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    pub fn selected_choice(&self) -> Option<usize> {
        if self.stage == DomainEntryStage::Script
            && let Some(domain) = &self.editing
        {
            let old = self
                .manager
                .domains
                .get(domain)
                .and_then(|login| self.manager.script(login));
            return self.choices().iter().position(|row| {
                row.value
                    .and_then(|i| self.manager.scripts.get(i))
                    .zip(old)
                    .is_some_and(|(a, b)| a.key == b.key)
            });
        }
        (!self.choices().is_empty()).then_some(0)
    }
    pub fn choose(&mut self, row: usize) {
        let Some(choice) = self.choices().get(row).cloned() else {
            return;
        };
        match self.stage {
            DomainEntryStage::Script => {
                let Some(index) = choice.value else {
                    self.stage = DomainEntryStage::Canceled;
                    return;
                };
                self.script_index = Some(index);
                if let Some(domain) = &self.editing {
                    let old = self
                        .manager
                        .domains
                        .get(domain)
                        .and_then(|login| self.manager.script(login));
                    if old.is_some_and(|old| old.key == self.manager.scripts[index].key) {
                        self.stage = DomainEntryStage::Canceled;
                        return;
                    }
                    if self.use_example() {
                        self.finish_credentials();
                    } else {
                        self.stage = DomainEntryStage::Access;
                    }
                } else {
                    self.stage = DomainEntryStage::Example;
                    if self.choices().len() == 1 {
                        self.stage = DomainEntryStage::Domain;
                    }
                }
            }
            DomainEntryStage::Example => {
                if choice.value.is_some() {
                    self.domain = choice.label;
                    self.use_example();
                    self.after_access();
                } else {
                    self.stage = DomainEntryStage::Domain;
                }
            }
            DomainEntryStage::Access => {
                self.access =
                    hydrus_parse::login::Access::from_code(choice.value.unwrap_or_default() as i64)
                        .expect("access choice");
                self.description = self.access.description().into();
                self.stage = DomainEntryStage::Description;
            }
            _ => {}
        }
    }
    fn use_example(&mut self) -> bool {
        let example = self
            .script()
            .and_then(|script| {
                script
                    .examples
                    .iter()
                    .find(|example| example.domain == self.domain)
            })
            .cloned();
        if let Some(example) = example {
            self.access = example.access;
            self.description = example.description;
            true
        } else {
            false
        }
    }
    pub fn enter_text(&mut self, text: &str) -> Result<(), String> {
        if text.is_empty() {
            return Ok(());
        }
        match self.stage {
            DomainEntryStage::Domain => {
                if self.manager.domains.contains_key(text) {
                    self.stage = DomainEntryStage::Canceled;
                    return Err("That domain is already in use!".into());
                }
                self.domain = text.into();
                self.stage = DomainEntryStage::Access;
            }
            DomainEntryStage::Description => {
                self.description = text.into();
                self.after_access();
            }
            _ => {}
        }
        Ok(())
    }
    /// Only the final description prompt's cancellation accepts its default.
    pub fn cancel(&mut self) {
        if self.stage == DomainEntryStage::Description {
            self.after_access();
        } else {
            self.stage = DomainEntryStage::Canceled;
        }
    }
    fn after_access(&mut self) {
        if self.editing.is_none()
            && self
                .script()
                .is_some_and(|script| !script.credentials.is_empty())
        {
            self.stage = DomainEntryStage::Credentials;
        } else {
            self.finish_credentials();
        }
    }
    pub fn set_credentials(&mut self, values: BTreeMap<String, String>) {
        if self.stage == DomainEntryStage::Credentials {
            self.credentials = values;
            self.finish_credentials();
        }
    }
    fn finish_credentials(&mut self) {
        let result = self
            .script()
            .expect("chosen script")
            .check_credentials_for_entry(&self.credentials);
        let good = result.is_ok()
            && (self.editing.is_some()
                || self.credentials.is_empty()
                || self.credentials.values().any(|value| !value.is_empty()));
        match result {
            Ok(()) => {
                self.validity = hydrus_parse::login::Validity::Untested;
                self.validity_error.clear();
            }
            Err(error) => {
                self.validity = hydrus_parse::login::Validity::Invalid;
                self.validity_error = error;
            }
        }
        if !good {
            self.active = false;
        }
        self.stage = if self.editing.is_none() && good {
            DomainEntryStage::Activate
        } else {
            DomainEntryStage::Done
        };
    }
    pub fn activate(&mut self, active: bool) {
        if self.stage == DomainEntryStage::Activate {
            self.active = active;
            self.stage = DomainEntryStage::Done;
        }
    }
    pub fn value(&self) -> Option<(String, hydrus_parse::login::DomainLogin)> {
        if self.stage != DomainEntryStage::Done {
            return None;
        }
        let script = self.script()?;
        Some((
            self.domain.clone(),
            hydrus_parse::login::DomainLogin {
                script_key: script.key.clone(),
                script_name: script.name.clone(),
                credentials: self.credentials.clone(),
                access: self.access,
                description: self.description.clone(),
                active: self.active,
                validity: self.validity,
                validity_error: self.validity_error.clone(),
                no_work_until: 0,
                delay_reason: String::new(),
            },
        ))
    }
}
impl DomainsEditor {
    pub fn put(&mut self, domain: String, login: hydrus_parse::login::DomainLogin) {
        self.draft.domains.insert(domain.clone(), login);
        self.selection
            .select_only(self.draft.domains.keys().position(|key| key == &domain));
    }
    pub fn delete(&mut self) {
        for domain in self.selected_domains() {
            self.draft.domains.remove(&domain);
        }
        self.selection.select_only(None);
    }
}
