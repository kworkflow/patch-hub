//! kw argv construction: patch-hub's base command lines plus the user
//! extra-args merge in which reserved options always win.

use std::ptr;

/// A CLI option patch-hub controls: user-supplied extra args that set it
/// are stripped, so the occurrence on patch-hub's own base argv wins.
pub struct ReservedOption {
    /// Every spelling of the option, e.g. `&["--force", "-f"]`.
    spellings: &'static [&'static str],
    /// Whether the option takes a value (`--name=value`, or `--name
    /// value` as a separate token).
    takes_value: bool,
    /// When true, GNU getopt unique prefixes of this option's long
    /// spellings are stripped too (`--boot` → `--boot-into-new-kernel-once`).
    /// Only for flags the target kw command actually honors, so a
    /// build-only `--menu` on the deploy table cannot steal `--m` from
    /// `--modules`.
    match_abbrev: bool,
}

impl ReservedOption {
    pub const fn new(spellings: &'static [&'static str], takes_value: bool) -> Self {
        Self {
            spellings,
            takes_value,
            match_abbrev: false,
        }
    }

    /// Like [`Self::new`], and also strip unambiguous GNU getopt long
    /// abbreviations of this option.
    pub const fn gnu_abbrev(spellings: &'static [&'static str], takes_value: bool) -> Self {
        Self {
            spellings,
            takes_value,
            match_abbrev: true,
        }
    }
}

/// Reserved for `kw build`. Patch-hub owns the job log, so `--save-log-to`
/// is stripped. `--menu` is stripped and never injected: stdio is a log, so
/// menuconfig would hang. `--alert` is stripped, not injected: unknown
/// options fail hard, and the unattended default is already `alert=n`.
/// `--clean`, `--full-cleanup`, `--doc`, `--info`, and `--from-sha` too.
const BUILD_RESERVED: &[ReservedOption] = &[
    ReservedOption::gnu_abbrev(&["--help", "-h"], false),
    ReservedOption::gnu_abbrev(&["--alert"], true),
    ReservedOption::gnu_abbrev(&["--save-log-to"], true),
    ReservedOption::gnu_abbrev(&["--menu"], false),
    ReservedOption::gnu_abbrev(&["--clean"], false),
    ReservedOption::gnu_abbrev(&["--full-cleanup"], false),
    ReservedOption::gnu_abbrev(&["--doc"], false),
    ReservedOption::gnu_abbrev(&["--info"], false),
    ReservedOption::gnu_abbrev(&["--from-sha"], true),
];

/// Reserved for `kw deploy`. Injected `--remote`, reboot, and force win.
/// Extras that override those, go local, list/uninstall, `--setup`, or `-n`
/// are stripped, plus build-only extras (unknown flags exit 22) and GNU
/// getopt (`-rf`, `-Fpkg`, unique `--boot`). `-l` is `--list` not `--local`;
/// `-r` is `--reboot` not `--remote`; `-u` takes no value; `--alert` is not.
const DEPLOY_RESERVED: &[ReservedOption] = &[
    ReservedOption::gnu_abbrev(&["--remote"], true),
    ReservedOption::gnu_abbrev(&["--local"], false),
    ReservedOption::gnu_abbrev(&["--reboot", "-r"], false),
    ReservedOption::gnu_abbrev(&["--no-reboot"], false),
    ReservedOption::gnu_abbrev(&["--force", "-f"], false),
    ReservedOption::gnu_abbrev(&["--list", "-l"], false),
    ReservedOption::gnu_abbrev(&["--ls-line", "-s"], false),
    ReservedOption::gnu_abbrev(&["--list-all", "-a"], false),
    ReservedOption::gnu_abbrev(&["--setup"], false),
    ReservedOption::gnu_abbrev(&["--uninstall", "-u"], false),
    ReservedOption::gnu_abbrev(&["--from-package", "-F"], true),
    ReservedOption::gnu_abbrev(&["--create-package", "-p"], false),
    ReservedOption::gnu_abbrev(&["--boot-into-new-kernel-once", "-n"], false),
    ReservedOption::new(&["--help", "-h"], false),
    ReservedOption::new(&["--save-log-to"], true),
    ReservedOption::new(&["--alert"], true),
    ReservedOption::new(&["--ccache"], false),
    ReservedOption::new(&["--llvm"], false),
    ReservedOption::new(&["--cpu-scaling", "-S"], true),
    // kw declares `warnings::` (optional value): GNU getopt only consumes
    // an attached `--warnings=1` / `-w1`. Treating this as required would
    // eat the following token (`--warnings --verbose`).
    ReservedOption::new(&["--warnings", "-w"], false),
    ReservedOption::new(&["--cflags"], true),
    ReservedOption::new(&["--menu"], false),
    ReservedOption::new(&["--doc", "-d"], false),
    ReservedOption::new(&["--info", "-i"], false),
    ReservedOption::new(&["--clean", "-c"], false),
    ReservedOption::new(&["--full-cleanup"], false),
    ReservedOption::new(&["--from-sha"], true),
];

pub struct KwArgvService;

impl KwArgvService {
    /// The argv for a build job: `kw build <extras>` (reserved extras stripped).
    pub fn build_argv(extra_args: &[String]) -> Vec<String> {
        Self::merge_extra_args(&["build"], BUILD_RESERVED, extra_args)
    }

    /// Deploy argv: `kw deploy --remote <endpoint> --no-reboot|--reboot
    /// [--force] <extras>`. Reserved extras are stripped so the injected
    /// remote, reboot, and force flags win, and build-only extras cannot
    /// fail deploy's getopt. `--force` is omitted when `force` is false:
    /// kw has no `--no-force`.
    pub fn build_deploy_argv(
        endpoint: &str,
        reboot: bool,
        force: bool,
        extra_args: &[String],
    ) -> Vec<String> {
        let mut base = vec![
            "deploy",
            "--remote",
            endpoint,
            if reboot { "--reboot" } else { "--no-reboot" },
        ];
        if force {
            base.push("--force");
        }
        Self::merge_extra_args(&base, DEPLOY_RESERVED, extra_args)
    }

    /// Appends extra args to `base` in order, stripping tokens that would
    /// override a reserved option. GNU getopt forms are stripped too:
    /// `--name=value`, `--name value`, unique long abbreviations, bundled
    /// shorts (`-rf`), and attached values (`-Fpkg`). A mixed short cluster
    /// containing any reserved flag is dropped whole, not rewritten.
    pub fn merge_extra_args(
        base: &[&str],
        reserved: &[ReservedOption],
        extra_args: &[String],
    ) -> Vec<String> {
        let mut argv = base
            .iter()
            .map(|arg| arg.to_string())
            .collect::<Vec<String>>();
        let mut extras = extra_args.iter();
        while let Some(token) = extras.next() {
            match Self::find_reserved_option(reserved, token) {
                Some(option) => {
                    if option.takes_value && !Self::is_value_attached(token, option) {
                        // `--name value` / `-F value`: the separate value token goes too.
                        extras.next();
                    }
                }
                None => argv.push(token.clone()),
            }
        }
        argv
    }
}

impl KwArgvService {
    /// The reserved option a token sets, if any: an exact spelling,
    /// `--name=value`, a unique GNU getopt abbreviation of a `match_abbrev`
    /// long, a bundled reserved short, or an attached short value. The `=`
    /// boundary keeps `--alertness` from matching `--alert`. An abbreviation
    /// must be a prefix of the reserved spelling, not the reverse.
    fn find_reserved_option<'a>(
        reserved: &'a [ReservedOption],
        token: &str,
    ) -> Option<&'a ReservedOption> {
        if token.starts_with("--") {
            return Self::find_reserved_long_option(reserved, token);
        }
        Self::find_reserved_short_cluster(reserved, token)
    }

    fn find_reserved_long_option<'a>(
        reserved: &'a [ReservedOption],
        token: &str,
    ) -> Option<&'a ReservedOption> {
        let name = token.split_once('=').map_or(token, |(name, _)| name);
        reserved
            .iter()
            .find(|option| {
                option
                    .spellings
                    .iter()
                    .any(|spelling| spelling.starts_with("--") && name == *spelling)
            })
            .or_else(|| Self::find_unique_long_abbrev(reserved, name))
    }

    /// GNU getopt unique-prefix match among options with `match_abbrev`.
    /// Uniqueness is against this reserved table, not kw's full option list:
    /// a prefix kw would call ambiguous can still strip here when only one
    /// reserved long matches. Ambiguous prefixes among reserved longs are
    /// left alone so a following value token is not eaten.
    fn find_unique_long_abbrev<'a>(
        reserved: &'a [ReservedOption],
        name: &str,
    ) -> Option<&'a ReservedOption> {
        if name.len() < 3 || !name.starts_with("--") {
            return None;
        }
        let mut found: Option<&ReservedOption> = None;
        for option in reserved {
            if !option.match_abbrev {
                continue;
            }
            let matches = option
                .spellings
                .iter()
                .any(|spelling| spelling.starts_with("--") && spelling.starts_with(name));
            if !matches {
                continue;
            }
            match found {
                None => found = Some(option),
                Some(previous) if ptr::eq(previous, option) => {}
                Some(_) => return None,
            }
        }
        found
    }

    /// A short token that contains any reserved flag: `-f`, `-rf`, `-Fpkg`.
    /// Walking leftover letters would turn `-ukernel` into `-kernel`, so a
    /// hit drops the whole token.
    fn find_reserved_short_cluster<'a>(
        reserved: &'a [ReservedOption],
        token: &str,
    ) -> Option<&'a ReservedOption> {
        let body = token.strip_prefix('-')?;
        if body.is_empty() || body.starts_with('-') {
            return None;
        }
        let chars = body.chars().collect::<Vec<char>>();
        let mut i = 0;
        let mut hit = None;
        while i < chars.len() {
            let spelling = format!("-{}", chars[i]);
            match Self::find_exact_short(reserved, &spelling) {
                Some(option) => {
                    hit = Some(option);
                    if option.takes_value {
                        // Remainder is the attached value; stop so
                        // `is_value_attached` can see those extra chars.
                        break;
                    }
                    i += 1;
                }
                None => i += 1,
            }
        }
        hit
    }

    fn find_exact_short<'a>(
        reserved: &'a [ReservedOption],
        spelling: &str,
    ) -> Option<&'a ReservedOption> {
        reserved
            .iter()
            .find(|option| option.spellings.contains(&spelling))
    }

    fn is_value_attached(token: &str, option: &ReservedOption) -> bool {
        if token.contains('=') {
            return true;
        }
        if !option.takes_value {
            return false;
        }
        let Some(body) = token
            .strip_prefix('-')
            .filter(|body| !body.starts_with('-'))
        else {
            return false;
        };
        let mut chars = body.chars();
        while let Some(ch) = chars.next() {
            let spelling = format!("-{ch}");
            if option.spellings.iter().any(|s| *s == spelling) {
                return chars.next().is_some();
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {

    mod helpers {
        use super::super::*;

        pub(super) fn extras(tokens: &[&str]) -> Vec<String> {
            tokens.iter().map(|token| token.to_string()).collect()
        }

        pub(super) const ENDPOINT: &str = "root@lima-ph-dut.internal:22";

        pub(super) fn deploy(extra: &[&str]) -> Vec<String> {
            KwArgvService::build_deploy_argv(ENDPOINT, false, true, &extras(extra))
        }
    }
    use super::*;
    use helpers::*;

    #[test]
    fn build_argv_without_extras_is_the_base_command() {
        assert_eq!(vec!["build"], KwArgvService::build_argv(&[]));
    }

    #[test]
    fn extras_are_appended_in_order() {
        assert_eq!(
            vec!["build", "--verbose", "--ccache", "-j8"],
            KwArgvService::build_argv(&extras(&["--verbose", "--ccache", "-j8"]))
        );
    }

    #[test]
    fn reserved_alert_is_stripped_from_extras() {
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&["--alert=vv", "--verbose"]))
        );
        // Separate-token value form: the value token is consumed too.
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&["--alert", "v", "--verbose"]))
        );
    }

    #[test]
    fn save_log_to_is_reserved_in_both_value_forms() {
        assert_eq!(
            vec!["build"],
            KwArgvService::build_argv(&extras(&["--save-log-to=/tmp/x.log"]))
        );
        assert_eq!(
            vec!["build"],
            KwArgvService::build_argv(&extras(&["--save-log-to", "/tmp/x.log"]))
        );
    }

    #[test]
    fn similar_prefix_is_not_reserved() {
        // `--alertness` only shares a prefix with `--alert`.
        assert_eq!(
            vec!["build", "--alertness"],
            KwArgvService::build_argv(&extras(&["--alertness"]))
        );
    }

    #[test]
    fn trailing_reserved_option_without_value_is_stripped() {
        assert_eq!(
            vec!["build"],
            KwArgvService::build_argv(&extras(&["--alert"]))
        );
    }

    #[test]
    fn menu_is_stripped_without_eating_the_next_token() {
        // kw build --menu would open menuconfig with the job's stdio
        // redirected to a log file — a hang, not a build.
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&["--menu", "--verbose"]))
        );
    }

    #[test]
    fn tree_mutating_and_hanging_build_flags_are_stripped() {
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&[
                "--clean",
                "--full-cleanup",
                "--doc",
                "--info",
                "--from-sha",
                "abc123",
                "--verbose",
            ]))
        );
        assert_eq!(
            vec!["build"],
            KwArgvService::build_argv(&extras(&["--from-sha=abc123"]))
        );
    }

    #[test]
    fn boolean_reserved_strips_only_itself_in_all_spellings() {
        // A boolean reserved option must not eat the following token.
        let reserved = [ReservedOption::new(&["--force", "-f"], false)];
        assert_eq!(
            vec!["deploy", "extra"],
            KwArgvService::merge_extra_args(&["deploy"], &reserved, &extras(&["--force", "extra"]))
        );
        assert_eq!(
            vec!["deploy", "extra"],
            KwArgvService::merge_extra_args(&["deploy"], &reserved, &extras(&["-f", "extra"]))
        );
    }

    #[test]
    fn tokens_after_a_consumed_value_keep_flowing() {
        let reserved = [ReservedOption::new(&["--remote"], true)];
        assert_eq!(
            vec!["deploy", "--no-reboot"],
            KwArgvService::merge_extra_args(
                &["deploy"],
                &reserved,
                &extras(&["--remote", "host:22", "--no-reboot"])
            )
        );
    }

    #[test]
    fn deploy_argv_without_extras_is_remote_no_reboot_force() {
        assert_eq!(
            vec!["deploy", "--remote", ENDPOINT, "--no-reboot", "--force",],
            KwArgvService::build_deploy_argv(ENDPOINT, false, true, &[])
        );
    }

    #[test]
    fn deploy_argv_reboot_and_unforced_swap_the_injected_flags() {
        assert_eq!(
            vec!["deploy", "--remote", ENDPOINT, "--reboot"],
            KwArgvService::build_deploy_argv(ENDPOINT, true, false, &[])
        );
    }

    #[test]
    fn deploy_remote_always_wins_over_user_remote_and_local() {
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&[
                "--remote",
                "other:22",
                "--local",
                "--remote=evil:1",
                "--verbose",
            ])
        );
    }

    #[test]
    fn deploy_strips_query_modes_setup_and_package_flags() {
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&[
                "--list",
                "-l",
                "--ls-line",
                "-s",
                "--list-all",
                "-a",
                "--setup",
                "--from-package",
                "kernel.kw.tar",
                "--from-package=other.kw.tar",
                "-F",
                "also.kw.tar",
                "--create-package",
                "-p",
                "--verbose",
            ])
        );
    }

    #[test]
    fn deploy_strips_boot_once_alert_save_log_and_reboot_overrides() {
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--modules",
            ],
            deploy(&[
                "--boot-into-new-kernel-once",
                "-n",
                "--alert=n",
                "--alert",
                "v",
                "--save-log-to",
                "/tmp/x.log",
                "--reboot",
                "-r",
                "--no-reboot",
                "--force",
                "-f",
                "--modules",
            ])
        );
    }

    #[test]
    fn uninstall_short_flag_does_not_eat_the_next_token() {
        // kw's `-u` takes an optional value (`uninstall::`). Treating it as
        // a boolean reserved option keeps a following extra from vanishing.
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["-u", "--verbose"])
        );
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["--uninstall", "--verbose"])
        );
        assert_eq!(
            vec!["deploy", "--remote", ENDPOINT, "--no-reboot", "--force",],
            deploy(&["--uninstall=old-kernel"])
        );
    }

    #[test]
    fn deploy_strips_build_only_flags_that_kw_deploy_would_reject() {
        // Shared KwOps extras: --verbose is a real deploy option; the rest
        // are kw build-only (`src/build.sh` 0.10) and would exit 22.
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&[
                "--ccache",
                "--llvm",
                "--cpu-scaling",
                "50",
                "--warnings=1",
                "--cflags",
                "-O2",
                "--menu",
                "--doc",
                "--info",
                "--clean",
                "--full-cleanup",
                "--from-sha",
                "abc123",
                "--verbose",
            ])
        );
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&[
                "--cpu-scaling=50",
                "--warnings=1",
                "--cflags=-O2",
                "--from-sha=abc123",
                "-S",
                "25",
                "-w2",
                "-d",
                "-i",
                "-c",
                "--verbose",
            ])
        );
    }

    #[test]
    fn deploy_strips_bundled_reserved_shorts_and_attached_values() {
        // GNU getopt: `-rf` is `--reboot --force`; `-Fpkg` is from-package.
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["-rf", "-Fpkg.kw.tar", "-S25", "-w12", "--verbose"])
        );
        // Mixed cluster containing a reserved flag is dropped whole so
        // `-ukernel` cannot be rewritten into leftover `-kernel`.
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["-mr", "-ukernel", "--verbose"])
        );
    }

    #[test]
    fn deploy_strips_unique_gnu_getopt_long_abbreviations() {
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&[
                "--rem",
                "evil:1",
                "--rem=other:1",
                "--reb",
                "--no-r",
                "--boot",
                "--verbose",
            ])
        );
    }

    #[test]
    fn deploy_keeps_modules_abbrev_and_ambiguous_long_prefixes() {
        // `--m` uniquely matches `--modules` on deploy; `--menu` is
        // build-only and must not steal it. `--re` is ambiguous between
        // `--remote` and `--reboot`, so it is left for getopt to reject
        // rather than eating the next token.
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--m",
                "--re",
                "--verbose",
            ],
            deploy(&["--m", "--re", "--verbose"])
        );
    }

    #[test]
    fn build_strips_unique_long_abbreviations_of_reserved_flags() {
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&["--men", "--from", "abc123", "--verbose"]))
        );
    }

    #[test]
    fn help_is_stripped_from_build_and_deploy_extras() {
        // `kw build --help` exits 0 without compiling, so a D job would
        // otherwise chain into `kw deploy --help` (exit 22).
        assert_eq!(
            vec!["build", "--verbose"],
            KwArgvService::build_argv(&extras(&["--help", "-h", "--verbose"]))
        );
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["--help", "-h", "--verbose"])
        );
    }

    #[test]
    fn warnings_optional_value_does_not_eat_the_next_token() {
        assert_eq!(
            vec![
                "deploy",
                "--remote",
                ENDPOINT,
                "--no-reboot",
                "--force",
                "--verbose",
            ],
            deploy(&["--warnings", "--verbose"])
        );
    }
}
