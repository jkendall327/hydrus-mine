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
/// The local browser URL (`_OpenBaseURL`): `https` when the service uses it.
pub fn base_url(config: &hydrus_store::services::ServerConfig) -> Result<String, String> {
    let scheme = if config.use_https { "https" } else { "http" };
    let port = config
        .port
        .ok_or("The service is not running, so you cannot view it in a web browser!")?;
    Ok(format!("{scheme}://127.0.0.1:{port}/"))
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
        let scheme = if config.use_https { "https" } else { "http" };
        return Ok(format!("{scheme}://{address}/"));
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
    pub use_https: bool,
    pub normie_eris: bool,
    /// `NoneableTextCtrl` values: `None` is the "none" box; text is kept as typed.
    pub external_scheme: Option<String>,
    pub external_host: Option<String>,
    pub external_port: Option<String>,
}
/// Apply the service editor's fields, as the reference's `GetValue` reads them.
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
    config.use_https = edit.use_https;
    config.use_normie_eris = edit.normie_eris;
    config
        .external_scheme_override
        .clone_from(&edit.external_scheme);
    config
        .external_host_override
        .clone_from(&edit.external_host);
    config
        .external_port_override
        .clone_from(&edit.external_port);
    Ok(config)
}

/// The service editor's tooltips for the Client API rows, as the reference words them.
pub mod tooltips {
    pub const NON_LOCAL: &str = "Allow other computers on the network to talk to use service. If unchecked, only localhost can talk to it. On Windows, the first time you start a local service that allows non-local connections, you will get the Windows firewall popup dialog when you ok the main services dialog.";
    pub const HTTPS: &str = "Host the server using https instead of http. This uses a self-signed certificate, stored in your db folder, which is imperfect but better than straight http. Your software (e.g. web browser testing the Client API welcome page) may need to go through a manual 'approve this ssl certificate' process before it can work. If you host your client on a real DNS domain and acquire your own signed certificate, you can replace the cert+key file pair with that.";
    pub const CORS: &str = "Have this server support Cross-Origin Resource Sharing, which allows web browsers to access it off other domains. Turn this on if you want to access this service through a web-based wrapper (e.g. a booru wrapper) hosted on another domain.";
    pub const LOGS: &str = "Hydrus server services will write a brief anonymous line to the log for every request made, but for the client services this tends to be a bit spammy. You probably want this off unless you are testing something.";
    pub const NORMIE: &str = "Use alternate ASCII art on the root page of the server.";
    pub const EXTERNAL_PORT: &str =
        "Setting this to a non-none empty string will forego the ':' in the URL.";
}
