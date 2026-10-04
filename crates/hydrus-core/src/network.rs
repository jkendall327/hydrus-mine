//! Network contexts: what a network rule (custom headers, cookies, bandwidth
//! limits) applies to. The reference's `ClientNetworkingContexts`.

use serde::{Deserialize, Serialize};

/// `CC.NETWORK_CONTEXT_GLOBAL`: everything.
pub const CONTEXT_GLOBAL: i64 = 0;
/// `CC.NETWORK_CONTEXT_HYDRUS`: a hydrus repository.
pub const CONTEXT_HYDRUS: i64 = 1;
/// `CC.NETWORK_CONTEXT_DOMAIN`: one domain (each level of a request's
/// domain is a context of its own).
pub const CONTEXT_DOMAIN: i64 = 2;
/// `CC.NETWORK_CONTEXT_DOWNLOADER`: no longer used.
pub const CONTEXT_DOWNLOADER: i64 = 3;
/// `CC.NETWORK_CONTEXT_DOWNLOADER_PAGE`: one downloader (a URL queue, a
/// gallery search).
pub const CONTEXT_DOWNLOADER_PAGE: i64 = 4;
/// `CC.NETWORK_CONTEXT_SUBSCRIPTION`: one subscription query.
pub const CONTEXT_SUBSCRIPTION: i64 = 5;
/// `CC.NETWORK_CONTEXT_WATCHER_PAGE`: one thread watcher.
pub const CONTEXT_WATCHER_PAGE: i64 = 6;

/// What a network rule applies to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NetworkContext {
    /// `CC.NETWORK_CONTEXT_*`.
    pub kind: i64,
    /// The domain for domain contexts, the subscription's name and query for
    /// subscription contexts, hex for contexts keyed by bytes; empty for the
    /// global context and for a kind's default ("every domain").
    pub data: String,
}

impl NetworkContext {
    pub fn global() -> Self {
        Self {
            kind: CONTEXT_GLOBAL,
            data: String::new(),
        }
    }

    pub fn domain(domain: impl Into<String>) -> Self {
        Self {
            kind: CONTEXT_DOMAIN,
            data: domain.into(),
        }
    }

    /// A subscription query's context: the reference keys it by
    /// `"{subscription name}: {query's display name or text}"`.
    pub fn subscription(name: &str, query: &str) -> Self {
        Self {
            kind: CONTEXT_SUBSCRIPTION,
            data: format!("{name}: {query}"),
        }
    }

    pub fn downloader_page(key: impl Into<String>) -> Self {
        Self {
            kind: CONTEXT_DOWNLOADER_PAGE,
            data: key.into(),
        }
    }

    pub fn watcher_page(key: impl Into<String>) -> Self {
        Self {
            kind: CONTEXT_WATCHER_PAGE,
            data: key.into(),
        }
    }

    /// The default of this context's kind ("every domain"), whose rules apply
    /// to contexts of the kind without rules of their own.
    pub fn default_of_kind(kind: i64) -> Self {
        Self {
            kind,
            data: String::new(),
        }
    }

    /// A kind's default (not the global context, which has no data either).
    pub fn is_default(&self) -> bool {
        self.data.is_empty() && self.kind != CONTEXT_GLOBAL
    }

    /// Downloader and watcher pages: their usage isn't kept.
    pub fn is_ephemeral(&self) -> bool {
        matches!(self.kind, CONTEXT_DOWNLOADER_PAGE | CONTEXT_WATCHER_PAGE)
    }

    /// As the reference names it to the user.
    pub fn to_human_string(&self) -> String {
        let kind = match self.kind {
            CONTEXT_GLOBAL => return "global".into(),
            CONTEXT_HYDRUS => "hydrus service",
            CONTEXT_DOMAIN => "web domain",
            CONTEXT_DOWNLOADER => "downloader",
            CONTEXT_DOWNLOADER_PAGE => "downloader page",
            CONTEXT_SUBSCRIPTION => "subscription",
            CONTEXT_WATCHER_PAGE => "watcher page",
            _ => "unknown",
        };
        if self.data.is_empty() {
            format!("{kind} default")
        } else if self.is_ephemeral() {
            format!("{kind} instance")
        } else {
            format!("{kind}: {}", self.data)
        }
    }
}
