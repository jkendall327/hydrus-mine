//! Detached Client API access-key administration shared by list and permission windows.
use crate::list_selection::ListSelection;
use hydrus_core::casefold::casefold;
use hydrus_store::{
    Store,
    api_permissions::{self, AccessPermissions, Permission},
};

/// Reference review-list headings.
pub const COLUMNS: [&str; 3] = ["name", "basic permissions", "advanced permissions"];
/// Confirmation for removing access keys.
pub const DELETE_QUESTION: &str = "Remove all selected?";
/// Warning shown before changing an existing key.
pub const CHANGE_KEY_WARNING: &str = "Enter your new Access Key here. It must be 64 characters of hex. The button below has a valid random key.\n\nOBVIOUSLY ANYTHING THAT USES THIS CURRENT ACCESS KEY WILL LOSE ACCESS ONCE YOU CHANGE IT";
/// Reference collision refusal.
pub const COLLISION: &str = "You appear to have edited the API Access Key to something that already exists. Either something has gone technically wrong, or there has been a misunderstanding. I will not make any changes.";
/// One stable list row.
#[derive(Debug, Clone)]
pub struct Row {
    pub token: usize,
    pub permissions: AccessPermissions,
}
impl Row {
    /// Reference display tuple, with tag restrictions only for a search permission.
    pub fn cells(&self) -> [String; 3] {
        [
            self.permissions.name.clone(),
            self.permissions.basic_string(),
            if !self.permissions.permits_everything && self.permissions.has(Permission::SearchFiles)
            {
                format!(
                    "Can search: {}",
                    self.permissions.search_filter.to_permitted_string()
                )
            } else {
                String::new()
            },
        ]
    }
}
/// Staged access keys. No write occurs until Apply.
#[derive(Debug, Clone)]
pub struct Editor {
    pub rows: Vec<Row>,
    pub selection: ListSelection<usize>,
    pub sort_column: usize,
    pub ascending: bool,
    original: Vec<AccessPermissions>,
    next_token: usize,
}
impl Editor {
    /// Read native keys into detached editor state.
    pub fn new(store: &Store) -> hydrus_store::Result<Self> {
        let original = store.read(api_permissions::stored_keys)?;
        let rows = original
            .iter()
            .enumerate()
            .map(|(token, permissions)| Row {
                token,
                permissions: permissions.clone(),
            })
            .collect();
        let mut editor = Self {
            rows,
            selection: ListSelection::default(),
            sort_column: 0,
            ascending: true,
            next_token: original.len(),
            original,
        };
        editor.sort(0, true);
        Ok(editor)
    }
    /// Stable tokens in visible order.
    pub fn order(&self) -> Vec<usize> {
        self.rows.iter().map(|r| r.token).collect()
    }
    /// Exactly one selected row can be edited/copied.
    pub fn selected(&self) -> Option<&Row> {
        let mut rows = self
            .rows
            .iter()
            .filter(|r| self.selection.is_selected(r.token));
        let row = rows.next()?;
        if rows.next().is_some() {
            None
        } else {
            Some(row)
        }
    }
    /// Sort columns, retaining row identity.
    pub fn sort(&mut self, column: usize, ascending: bool) {
        self.sort_column = column.min(2);
        self.ascending = ascending;
        self.rows.sort_by(|a, b| {
            let order = a.cells()[self.sort_column]
                .cmp(&b.cells()[self.sort_column])
                .then_with(|| a.permissions.name.cmp(&b.permissions.name));
            if ascending { order } else { order.reverse() }
        });
    }
    /// Add or replace a staged row after a child editor is accepted.
    pub fn edit(
        &mut self,
        token: Option<usize>,
        permissions: AccessPermissions,
    ) -> Result<(), String> {
        if permissions.access_key.len() != 32 {
            return Err("Access keys need 64 hexadecimal characters.".into());
        }
        if self
            .rows
            .iter()
            .any(|r| Some(r.token) != token && r.permissions.access_key == permissions.access_key)
        {
            return Err(COLLISION.into());
        }
        if let Some(token) = token {
            self.rows
                .iter_mut()
                .find(|r| r.token == token)
                .ok_or("Access key no longer exists.")?
                .permissions = permissions;
        } else {
            let token = self.next_token;
            self.next_token += 1;
            self.rows.push(Row { token, permissions });
            self.selection.select_only(Some(token));
        }
        self.sort(self.sort_column, self.ascending);
        Ok(())
    }
    /// Duplicate selected keys with fresh caller-supplied keys and casefolded names.
    pub fn duplicate(&mut self, mut generate: impl FnMut() -> Vec<u8>) -> Result<(), String> {
        let selected = self
            .rows
            .iter()
            .filter(|r| self.selection.is_selected(r.token))
            .map(|r| r.permissions.clone())
            .collect::<Vec<_>>();
        for mut permissions in selected {
            permissions.access_key = generate();
            permissions.name = crate::favourites::non_dupe_name(&permissions.name, &|candidate| {
                self.rows
                    .iter()
                    .any(|r| casefold(&r.permissions.name) == casefold(candidate))
            });
            self.edit(None, permissions)?;
        }
        Ok(())
    }
    /// Remove the confirmed selection from detached state.
    pub fn delete_selected(&mut self) {
        self.rows.retain(|r| !self.selection.is_selected(r.token));
        self.selection = ListSelection::default();
    }
    /// Persist only after Apply, rejecting a stale editor atomically.
    pub fn apply(&self, store: &Store) -> hydrus_store::Result<()> {
        api_permissions::apply(
            store,
            self.original.clone(),
            self.rows.iter().map(|r| r.permissions.clone()).collect(),
        )
    }
}
/// Parse the explicit key-change input; whitespace around a key is ignored.
pub fn parse_key(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.len() != 64 {
        return Err(format!(
            "Sorry, the key you submitted was {} characters long. It needs to be 64.",
            text.len()
        ));
    }
    hex::decode(text)
        .map_err(|_| "Sorry, the key you submitted did not seem to be hexadecimal.".into())
}
/// Build the local browser URL, refusing disabled/unsupported HTTPS configurations.
pub fn base_url(config: &hydrus_store::services::ServerConfig) -> Result<String, String> {
    if config.use_https {
        return Err("HTTPS is not supported by this Client API server. Disable HTTPS in the imported configuration before starting it.".into());
    }
    let port = config
        .port
        .ok_or("The service is not running, so you cannot view it in a web browser!")?;
    Ok(format!("http://127.0.0.1:{port}/"))
}

/// Browser URL for the daemon's actual listener, including CLI port/bind overrides.
/// Wildcard interfaces use the corresponding loopback address on this computer.
pub fn reported_base_url(
    config: &hydrus_store::services::ServerConfig,
    state: &hydrus_store::settings::ClientApiState,
) -> Result<String, String> {
    if let hydrus_store::settings::ClientApiState::Listening(address) = state {
        let mut address = address.parse::<std::net::SocketAddr>().map_err(|_| {
            "The daemon reported an invalid Client API listening address.".to_owned()
        })?;
        if address.ip().is_unspecified() {
            address.set_ip(match address.ip() {
                std::net::IpAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
                std::net::IpAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
            });
        }
        return Ok(format!("http://{address}/"));
    }
    base_url(config)
}

/// Which interfaces the local API listener accepts connections on.
#[derive(Debug, Clone, Copy)]
pub enum Binding {
    LocalOnly,
    Network,
}
/// Editable listener fields, independent of preserved imported settings.
#[derive(Debug, Clone)]
pub struct ListenerEdit {
    pub port: Option<i32>,
    pub binding: Binding,
    pub cors: bool,
    pub logs: bool,
    pub disable_https: bool,
}
/// Edit supported listener fields while preserving all unrelated imported flags.
pub fn server_config(
    original: &hydrus_store::services::ServerConfig,
    edit: &ListenerEdit,
) -> Result<hydrus_store::services::ServerConfig, String> {
    let port = edit
        .port
        .map(|port| {
            u16::try_from(port)
                .ok()
                .filter(|p| *p > 0)
                .ok_or("Client API ports must be between 1 and 65535.")
        })
        .transpose()?;
    let mut config = original.clone();
    config.port = port;
    config.allow_non_local_connections = matches!(edit.binding, Binding::Network);
    config.support_cors = edit.cors;
    config.log_requests = edit.logs;
    if edit.disable_https {
        config.use_https = false;
    }
    Ok(config)
}
