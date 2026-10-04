use thiserror::Error;

/// A Host stanza from `remote.config` that has a usable Hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KwRemote {
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub user: Option<String>,
}

/// Why a deploy remote could not be resolved. Each variant's message is
/// the actionable explanation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteRefusal {
    #[error(
        "no remotes configured; configure a remote with `kw remote --set-default` \
         or edit `.kw/remote.config`"
    )]
    NoRemotesConfigured,
    #[error(
        "{count} remotes configured with no default; set one with \
         `kw remote --set-default` or edit `.kw/remote.config`"
    )]
    NoDefault { count: usize },
    #[error(
        "default remote '{name}' is not a Host in remote.config; set a valid \
         default with `kw remote --set-default`"
    )]
    DefaultNotFound { name: String },
}

/// Parsed contents of a `remote.config` file. Incomplete Host stanzas
/// (no Hostname, or an unparseable Port) are dropped rather than guessed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedRemoteConfig {
    pub default: Option<String>,
    pub hosts: Vec<KwRemote>,
}
