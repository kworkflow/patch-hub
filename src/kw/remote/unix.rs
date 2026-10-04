use std::path::{Path, PathBuf};

use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

use super::{KwRemote, ParsedRemoteConfig, RemoteRefusal};

struct HostBuilder {
    name: String,
    hostname: Option<String>,
    port: Option<String>,
    user: Option<String>,
}

impl HostBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hostname: None,
            port: None,
            user: None,
        }
    }

    fn finish(self) -> Option<KwRemote> {
        let hostname = self.hostname.filter(|value| !value.is_empty())?;
        let port = match self.port.as_deref() {
            None | Some("") => 22,
            Some(value) => value.parse().ok().filter(|port| *port != 0)?,
        };
        let user = self.user.filter(|value| !value.is_empty());
        Some(KwRemote {
            name: self.name,
            hostname,
            port,
            user,
        })
    }
}

pub struct RemoteConfigService;

impl RemoteConfigService {
    /// Parses kw's ssh-config-like `remote.config`. Blank lines and comments
    /// are skipped; `#kw-default=<name>` (anywhere) names the default Host,
    /// last occurrence winning; `Host <name>` starts a stanza; `Hostname`,
    /// `Port`, and `User` are read case-insensitively; `IdentityFile` and
    /// other keys are tolerated and ignored. Port defaults to 22 when unset.
    /// Duplicate Host names keep the last complete stanza.
    pub fn parse_remote_config(content: &str) -> ParsedRemoteConfig {
        let mut parsed = ParsedRemoteConfig::default();
        let mut current: Option<HostBuilder> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix("#kw-default=") {
                let name = name.trim();
                if !name.is_empty() {
                    parsed.default = Some(name.to_string());
                }
                continue;
            }
            if trimmed.starts_with('#') {
                continue;
            }

            let Some((keyword, rest)) = Self::split_keyword(trimmed) else {
                continue;
            };
            if keyword.eq_ignore_ascii_case("host") {
                if let Some(host) = current.take().and_then(HostBuilder::finish) {
                    Self::upsert_host(&mut parsed.hosts, host);
                }
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    current = Some(HostBuilder::new(name));
                }
                continue;
            }
            let Some(builder) = current.as_mut() else {
                continue;
            };
            if keyword.eq_ignore_ascii_case("hostname") {
                if !rest.is_empty() {
                    builder.hostname = Some(rest.to_string());
                }
            } else if keyword.eq_ignore_ascii_case("port") {
                if !rest.is_empty() {
                    builder.port = Some(rest.to_string());
                }
            } else if keyword.eq_ignore_ascii_case("user") && !rest.is_empty() {
                builder.user = Some(rest.to_string());
            }
        }
        if let Some(host) = current.and_then(HostBuilder::finish) {
            Self::upsert_host(&mut parsed.hosts, host);
        }
        parsed
    }

    /// Picks the deploy remote from a parsed file: the `#kw-default=` Host
    /// when set, otherwise the only Host. Multiple hosts with no default, or
    /// a default that names no complete Host, are refusals.
    pub fn select_deploy_remote(parsed: &ParsedRemoteConfig) -> Result<KwRemote, RemoteRefusal> {
        if let Some(name) = parsed.default.as_deref() {
            return parsed
                .hosts
                .iter()
                .find(|host| host.name == name)
                .cloned()
                .ok_or_else(|| RemoteRefusal::DefaultNotFound {
                    name: name.to_string(),
                });
        }
        match parsed.hosts.as_slice() {
            [] => Err(RemoteRefusal::NoRemotesConfigured),
            [only] => Ok(only.clone()),
            hosts => Err(RemoteRefusal::NoDefault { count: hosts.len() }),
        }
    }

    /// Resolves the remote kw deploy will be pointed at. See the [`remote`](super)
    /// module docs for the file lookup order.
    pub fn resolve_deploy_remote(
        fs: &dyn FileSystemTrait,
        env: &dyn EnvTrait,
        tree_path: &Path,
    ) -> Result<KwRemote, RemoteRefusal> {
        let local = tree_path.join(".kw").join("remote.config");
        if fs.is_file(&local) {
            return Self::read_remote_from_file(fs, &local);
        }
        let Some(global) = Self::resolve_xdg_remote_config(env) else {
            return Err(RemoteRefusal::NoRemotesConfigured);
        };
        if fs.is_file(&global) {
            return Self::read_remote_from_file(fs, &global);
        }
        Err(RemoteRefusal::NoRemotesConfigured)
    }
}

impl RemoteConfigService {
    fn read_remote_from_file(
        fs: &dyn FileSystemTrait,
        path: &Path,
    ) -> Result<KwRemote, RemoteRefusal> {
        let content = fs
            .read_to_string(path)
            .map_err(|_| RemoteRefusal::NoRemotesConfigured)?;
        Self::select_deploy_remote(&Self::parse_remote_config(&content))
    }

    /// `${XDG_CONFIG_HOME:-$HOME/.config}/kw/remote.config`. A set-but-empty
    /// `XDG_CONFIG_HOME` is treated as unset, matching bash `:-` and the XDG
    /// spec — otherwise the path would be relative to cwd.
    fn resolve_xdg_remote_config(env: &dyn EnvTrait) -> Option<PathBuf> {
        let config_home = match env.var("XDG_CONFIG_HOME") {
            Ok(xdg) if !xdg.is_empty() => xdg,
            _ => format!("{}/.config", env.var("HOME").ok()?),
        };
        Some(Path::new(&config_home).join("kw").join("remote.config"))
    }

    fn split_keyword(line: &str) -> Option<(&str, &str)> {
        let keyword_end = line.find(|c: char| c.is_whitespace()).unwrap_or(line.len());
        let keyword = &line[..keyword_end];
        if keyword.is_empty() {
            return None;
        }
        Some((keyword, line[keyword_end..].trim()))
    }

    fn upsert_host(hosts: &mut Vec<KwRemote>, host: KwRemote) {
        if let Some(existing) = hosts.iter_mut().find(|entry| entry.name == host.name) {
            *existing = host;
        } else {
            hosts.push(host);
        }
    }
}
