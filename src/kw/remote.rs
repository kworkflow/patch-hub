//! Resolve the remote kw deploy target from kw's `remote.config`.
//!
//! The file is a small ssh-config-like text file written by `kw remote`
//! (`src/kw_remote.sh` at kw 0.10): an optional `#kw-default=<name>`
//! sentinel plus `Host` stanzas with `Hostname` / `Port` / `User`. This
//! parser reads that file directly rather than scraping `kw remote --list`,
//! whose output is colorized terminal prose with no machine-readable mode.
//!
//! Resolution prefers `<tree>/.kw/remote.config`, then
//! `${XDG_CONFIG_HOME:-$HOME/.config}/kw/remote.config`. A present local
//! file is authoritative even when empty or unreadable — falling through
//! to the home copy would silently deploy to a different machine than the
//! tree is configured for.
//!
//! Production callers land with the deploy start path; until then this
//! module is exercised by its tests.

#![cfg_attr(not(test), allow(dead_code))]

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

/// A Host stanza from `remote.config` that has a usable Hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KwRemote {
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub user: Option<String>,
}

impl KwRemote {
    /// The `--remote` token kw's deploy parser accepts:
    /// `[user@]host:port`.
    pub fn endpoint(&self) -> String {
        match &self.user {
            Some(user) => format!("{}@{}:{}", user, self.hostname, self.port),
            None => format!("{}:{}", self.hostname, self.port),
        }
    }
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

        let Some((keyword, rest)) = split_keyword(trimmed) else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("host") {
            if let Some(host) = current.take().and_then(HostBuilder::finish) {
                upsert_host(&mut parsed.hosts, host);
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
        } else if keyword.eq_ignore_ascii_case("user") {
            if !rest.is_empty() {
                builder.user = Some(rest.to_string());
            }
        }
    }
    if let Some(host) = current.and_then(HostBuilder::finish) {
        upsert_host(&mut parsed.hosts, host);
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

/// Resolves the remote kw deploy will be pointed at. See the module docs
/// for the file lookup order.
pub fn resolve_deploy_remote(
    fs: &dyn FileSystemTrait,
    env: &dyn EnvTrait,
    tree_path: &Path,
) -> Result<KwRemote, RemoteRefusal> {
    let local = tree_path.join(".kw").join("remote.config");
    if fs.is_file(&local) {
        return remote_from_file(fs, &local);
    }
    let Some(global) = xdg_kw_remote_config(env) else {
        return Err(RemoteRefusal::NoRemotesConfigured);
    };
    if fs.is_file(&global) {
        return remote_from_file(fs, &global);
    }
    Err(RemoteRefusal::NoRemotesConfigured)
}

fn remote_from_file(fs: &dyn FileSystemTrait, path: &Path) -> Result<KwRemote, RemoteRefusal> {
    let content = fs
        .read_to_string(path)
        .map_err(|_| RemoteRefusal::NoRemotesConfigured)?;
    select_deploy_remote(&parse_remote_config(&content))
}

/// `${XDG_CONFIG_HOME:-$HOME/.config}/kw/remote.config`. A set-but-empty
/// `XDG_CONFIG_HOME` is treated as unset, matching bash `:-` and the XDG
/// spec — otherwise the path would be relative to cwd.
fn xdg_kw_remote_config(env: &dyn EnvTrait) -> Option<PathBuf> {
    let config_home = match env.var("XDG_CONFIG_HOME") {
        Ok(xdg) if !xdg.is_empty() => xdg,
        _ => format!("{}/.config", env.var("HOME").ok()?),
    };
    Some(Path::new(&config_home).join("kw").join("remote.config"))
}

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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use crate::infrastructure::{
        env::{EnvError, MockEnvTrait},
        file_system::{FileSystemError, MockFileSystemTrait},
    };

    use super::*;

    /// The example from kw's own writer (`src/kw_remote.sh`) / the
    /// integration plan: two hosts, default on the second, fields indented.
    const PLAN_FIXTURE: &str = "\
#kw-default=arch-test
Host steamos
  Hostname steamdeck
  Port 8888
  User jozzi
Host arch-test
  Hostname arch-tm
  Port 22
  User abc
";

    /// Lab deploy-smoke file: IdentityFile must not break parsing, and
    /// User root must land on the endpoint.
    const LAB_FIXTURE: &str = "\
#kw-default=ph-dut
Host ph-dut
  Hostname lima-ph-dut.internal
  Port 22
  User root
  IdentityFile /opt/ph-lab/keys/lab_ed25519
";

    fn choose(content: &str) -> Result<KwRemote, RemoteRefusal> {
        select_deploy_remote(&parse_remote_config(content))
    }

    fn remote(name: &str, hostname: &str, port: u16, user: Option<&str>) -> KwRemote {
        KwRemote {
            name: name.to_string(),
            hostname: hostname.to_string(),
            port,
            user: user.map(str::to_string),
        }
    }

    #[test]
    fn plan_fixture_picks_the_default_host() {
        let parsed = parse_remote_config(PLAN_FIXTURE);
        assert_eq!(Some("arch-test"), parsed.default.as_deref());
        assert_eq!(
            vec![
                remote("steamos", "steamdeck", 8888, Some("jozzi")),
                remote("arch-test", "arch-tm", 22, Some("abc")),
            ],
            parsed.hosts
        );
        assert_eq!("abc@arch-tm:22", choose(PLAN_FIXTURE).unwrap().endpoint());
    }

    #[test]
    fn lab_fixture_ignores_identity_file() {
        let chosen = choose(LAB_FIXTURE).unwrap();
        assert_eq!(
            remote("ph-dut", "lima-ph-dut.internal", 22, Some("root")),
            chosen
        );
        assert_eq!("root@lima-ph-dut.internal:22", chosen.endpoint());
    }

    #[test]
    fn single_host_without_default_is_used() {
        let content = "\
Host dut
  Hostname 192.0.2.10
  User root
";
        let chosen = choose(content).unwrap();
        assert_eq!(remote("dut", "192.0.2.10", 22, Some("root")), chosen);
        assert_eq!("root@192.0.2.10:22", chosen.endpoint());
    }

    #[test]
    fn port_defaults_to_22_when_unset() {
        let content = "\
Host dut
 Hostname box
";
        let chosen = choose(content).unwrap();
        assert_eq!(22, chosen.port);
        assert_eq!(None, chosen.user);
        assert_eq!("box:22", chosen.endpoint());
    }

    #[test]
    fn multiple_hosts_without_default_are_refused() {
        let content = "\
Host a
  Hostname one
Host b
  Hostname two
";
        assert_eq!(Err(RemoteRefusal::NoDefault { count: 2 }), choose(content));
    }

    #[test]
    fn missing_default_host_is_refused() {
        let content = "\
#kw-default=missing
Host dut
  Hostname box
";
        assert_eq!(
            Err(RemoteRefusal::DefaultNotFound {
                name: "missing".to_string()
            }),
            choose(content)
        );
    }

    #[test]
    fn empty_file_is_no_remotes() {
        assert_eq!(Err(RemoteRefusal::NoRemotesConfigured), choose(""));
        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            choose("# just a comment\n")
        );
    }

    #[test]
    fn reordered_fields_and_comments_inside_a_stanza_are_tolerated() {
        // kw used to assume Hostname/Port/User on the three lines after
        // Host; that broke on comments and reordering. We key-match.
        let content = "\
#kw-default=origin
Host origin
# Port 33
  User root
  Port 2222
  Hostname 192.0.2.1
Host other
  Hostname 192.0.2.2
  Port 22
  User other
";
        let chosen = choose(content).unwrap();
        assert_eq!(remote("origin", "192.0.2.1", 2222, Some("root")), chosen);
    }

    #[test]
    fn keys_are_matched_case_insensitively() {
        let content = "\
Host dut
  hostname Box
  PORT 2222
  user Root
";
        assert_eq!(
            remote("dut", "Box", 2222, Some("Root")),
            choose(content).unwrap()
        );
    }

    #[test]
    fn last_kw_default_and_duplicate_host_win() {
        let content = "\
#kw-default=first
Host dut
  Hostname old
#kw-default=dut
Host dut
  Hostname new
  Port 2222
";
        let parsed = parse_remote_config(content);
        assert_eq!(Some("dut"), parsed.default.as_deref());
        assert_eq!(vec![remote("dut", "new", 2222, None)], parsed.hosts);
    }

    #[test]
    fn incomplete_hosts_are_dropped() {
        let no_hostname = "\
Host dut
  Port 22
  User root
";
        assert_eq!(Err(RemoteRefusal::NoRemotesConfigured), choose(no_hostname));

        let bad_port = "\
Host dut
  Hostname box
  Port not-a-port
";
        assert_eq!(Err(RemoteRefusal::NoRemotesConfigured), choose(bad_port));

        let zero_port = "\
Host dut
  Hostname box
  Port 0
";
        assert_eq!(Err(RemoteRefusal::NoRemotesConfigured), choose(zero_port));
    }

    #[test]
    fn default_pointing_at_an_incomplete_host_is_not_found() {
        let content = "\
#kw-default=broken
Host broken
  Port 22
Host ok
  Hostname box
";
        assert_eq!(
            Err(RemoteRefusal::DefaultNotFound {
                name: "broken".to_string()
            }),
            choose(content)
        );
    }

    #[test]
    fn last_line_without_newline_still_counts() {
        let content = "Host dut\n  Hostname box\n  User root";
        assert_eq!(
            remote("dut", "box", 22, Some("root")),
            choose(content).unwrap()
        );
    }

    #[test]
    fn host_name_is_the_first_token_like_kw() {
        // kw's `cut -d ' ' -f2` keeps only the first word after Host.
        let content = "\
Host dut extra
  Hostname box
";
        assert_eq!("dut", choose(content).unwrap().name);
    }

    #[test]
    fn refusal_messages_are_actionable() {
        assert!(RemoteRefusal::NoRemotesConfigured
            .to_string()
            .contains("kw remote --set-default"));
        assert!(RemoteRefusal::NoDefault { count: 2 }
            .to_string()
            .contains("2 remotes"));
        assert!(RemoteRefusal::DefaultNotFound {
            name: "gone".to_string()
        }
        .to_string()
        .contains("gone"));
    }

    fn fs_with_files(files: &[(&str, &str)]) -> MockFileSystemTrait {
        let map: HashMap<PathBuf, String> = files
            .iter()
            .map(|(path, content)| (PathBuf::from(path), content.to_string()))
            .collect();
        let is_file = Arc::new(map.clone());
        let read = Arc::new(map);
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .returning(move |path| is_file.contains_key(path));
        fs.expect_read_to_string().returning(move |path| {
            read.get(path).cloned().ok_or_else(|| {
                FileSystemError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "missing",
                ))
            })
        });
        fs
    }

    fn env_with(xdg: Option<&str>, home: Option<&str>) -> MockEnvTrait {
        let xdg = xdg.map(str::to_string);
        let home = home.map(str::to_string);
        let mut env = MockEnvTrait::new();
        env.expect_var().returning(move |key| match key {
            "XDG_CONFIG_HOME" => xdg.clone().ok_or_else(|| missing_var()),
            "HOME" => home.clone().ok_or_else(|| missing_var()),
            _ => Err(missing_var()),
        });
        env
    }

    fn missing_var() -> EnvError {
        std::env::VarError::NotPresent.into()
    }

    #[test]
    fn resolve_prefers_the_tree_remote_config() {
        let fs = fs_with_files(&[
            ("/home/user/linux/.kw/remote.config", LAB_FIXTURE),
            ("/xdg/kw/remote.config", "Host other\n Hostname elsewhere\n"),
        ]);
        let env = env_with(Some("/xdg"), Some("/home/user"));

        let chosen = resolve_deploy_remote(&fs, &env, Path::new("/home/user/linux")).unwrap();
        assert_eq!("ph-dut", chosen.name);
    }

    #[test]
    fn resolve_falls_back_to_xdg_config_when_the_tree_has_none() {
        let fs = fs_with_files(&[(
            "/xdg/kw/remote.config",
            "Host dut\n Hostname box\n User root\n",
        )]);
        let env = env_with(Some("/xdg"), Some("/home/user"));

        let chosen = resolve_deploy_remote(&fs, &env, Path::new("/home/user/linux")).unwrap();
        assert_eq!("root@box:22", chosen.endpoint());
    }

    #[test]
    fn resolve_treats_empty_xdg_config_home_as_unset() {
        let fs = fs_with_files(&[(
            "/home/user/.config/kw/remote.config",
            "Host dut\n Hostname box\n",
        )]);
        let env = env_with(Some(""), Some("/home/user"));

        let chosen = resolve_deploy_remote(&fs, &env, Path::new("/kernel")).unwrap();
        assert_eq!("box:22", chosen.endpoint());
    }

    #[test]
    fn resolve_with_no_files_is_no_remotes() {
        let fs = fs_with_files(&[]);
        let env = env_with(Some("/xdg"), Some("/home/user"));

        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            resolve_deploy_remote(&fs, &env, Path::new("/kernel"))
        );
    }

    #[test]
    fn local_file_does_not_fall_through_when_empty() {
        // A present local file is the tree's config even if it names no
        // hosts: guessing the home copy would deploy somewhere else.
        let fs = fs_with_files(&[
            ("/kernel/.kw/remote.config", "# nothing yet\n"),
            ("/xdg/kw/remote.config", "Host dut\n Hostname box\n"),
        ]);
        let env = env_with(Some("/xdg"), Some("/home/user"));

        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            resolve_deploy_remote(&fs, &env, Path::new("/kernel"))
        );
    }

    #[test]
    fn unreadable_local_file_does_not_fall_through() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .returning(|path| path == Path::new("/kernel/.kw/remote.config"));
        fs.expect_read_to_string().returning(|_| {
            Err(FileSystemError::IoError(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied",
            )))
        });
        let env = env_with(Some("/xdg"), Some("/home/user"));

        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            resolve_deploy_remote(&fs, &env, Path::new("/kernel"))
        );
    }

    #[test]
    fn resolve_without_home_or_xdg_and_no_local_file_is_no_remotes() {
        let fs = fs_with_files(&[]);
        let env = env_with(None, None);

        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            resolve_deploy_remote(&fs, &env, Path::new("/kernel"))
        );
    }
}
