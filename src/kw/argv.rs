//! kw argv construction: patch-hub's base command lines plus the user
//! extra-args merge in which reserved options always win.

// Production caller is the unix-only actor.
#![cfg_attr(not(unix), allow(dead_code))]

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

/// Reserved for `kw build`: patch-hub owns the job's log file
/// (`ProcessTrait` captures kw's stdout/stderr to it) — a user-supplied
/// `--save-log-to` would split stdout/stderr away from the job log — and
/// never runs `--menu` from automation: the job's stdio is a log file, so
/// menuconfig would hang until cancelled.
///
/// `--alert` is stripped from extras (and not injected on the base argv):
/// kw beta-0.9 (still what many installs report, including this lab) treats
/// unrecognized options as hard failures (`Invalid option`), and the
/// unattended default is already `alert=n` in kw's own config.
///
/// `--clean` / `--full-cleanup` can wipe the tree, `--menu` / `--doc` /
/// `--info` hang a redirected job, and `--from-sha` mutates git. They are
/// stripped from extras and never injected.
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

/// Reserved for `kw deploy`. Patch-hub always injects a resolved
/// `--remote host:port` and the reboot/force knobs; user extras that
/// would override those, switch to local, list/uninstall kernels, run
/// interactive `--setup`, or flip `boot_into_new_kernel_once` via `-n`
/// are stripped.
///
/// Short-flag collisions with ssh-config intuition matter here: `-l` is
/// `--list`, not `--local` (`--local` has no short form); `-r` is
/// `--reboot`, not `--remote`. `--uninstall`/`-u` takes an optional
/// value in kw (`uninstall::`) but is reserved as a boolean so `-u`
/// does not eat the following token. `--alert` is not a deploy option
/// at all — injecting or forwarding it hard-fails on kw beta-0.9
/// (`Invalid option`), the version this lab still reports.
///
/// Build-only flags are stripped too: KwOps has one extras field, and
/// kw deploy's getopt (`src/deploy.sh` at 0.10: `remote:,local,reboot,
/// no-reboot,modules,list,ls-line,uninstall::,list-all,force,setup,
/// verbose,create-package,from-package:,boot-into-new-kernel-once`)
/// rejects unrecognized options with exit 22. Without this, `D` with
/// `--ccache` would build successfully then die at the deploy boundary.
/// Short spellings that collide with deploy's own flags (`-n` menu vs
/// boot-once, `-f` full-cleanup vs force, `-s` save-log-to vs ls-line)
/// stay on the deploy meaning already reserved above.
///
/// kw_parse is GNU `getopt -q`, so exact-token stripping is not enough:
/// `-rf` bundles `--reboot --force`, `-Fpkg.kw.tar` attaches a
/// from-package value, and `--boot` uniquely abbreviates
/// `--boot-into-new-kernel-once`. Those forms are stripped too.
/// `gnu_abbrev` is only set on flags deploy's getopt actually honors,
/// so `--m` still reaches `--modules` instead of matching build-only
/// `--menu`. Ambiguous prefixes (`--re` → remote and reboot) pass
/// through and fail getopt rather than eating the next token.
/// Abbreviation uniqueness is among this reserved table, not kw's full
/// option set. For deploy those coincide (the only passthrough longs are
/// `--modules`/`--verbose`). For build, `--c` is ambiguous in kw
/// (`ccache`/`cpu-scaling`/`clean`/`cflags`) but uniquely matches
/// `--clean` here, so it is stripped instead of failing getopt.
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

/// The argv for a build job: `kw build <extras>` (reserved extras stripped).
pub fn build_argv(extra_args: &[String]) -> Vec<String> {
    merge_extra_args(&["build"], BUILD_RESERVED, extra_args)
}

/// The argv for a deploy job: `kw deploy --remote <endpoint>
/// --no-reboot|--reboot [--force] <extras>`. Reserved extras are
/// stripped so the injected remote, reboot, and force flags win, and
/// so build-only extras (shared KwOps field) cannot fail kw deploy's
/// getopt. `--force` is omitted entirely when `force` is false rather
/// than passing a no-op, because kw has no `--no-force`.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn deploy_argv(
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
    merge_extra_args(&base, DEPLOY_RESERVED, extra_args)
}

/// Appends user-supplied extra args to `base`, stripping every token that
/// would override a reserved option. GNU getopt forms are stripped too:
/// `--name=value`, `--name value`, unique long abbreviations, bundled
/// shorts (`-rf`), and attached short values (`-Fpkg.kw.tar`). A mixed
/// short cluster that contains any reserved flag is dropped whole, so
/// `-uVALUE` cannot be rewritten into a leftover `-VALUE`. All other
/// extras pass through in order.
pub fn merge_extra_args(
    base: &[&str],
    reserved: &[ReservedOption],
    extra_args: &[String],
) -> Vec<String> {
    let mut argv: Vec<String> = base.iter().map(|arg| arg.to_string()).collect();
    let mut extras = extra_args.iter();
    while let Some(token) = extras.next() {
        match reserved_option_for(reserved, token) {
            Some(option) => {
                if option.takes_value && !value_is_attached(token, option) {
                    // `--name value` / `-F value`: the separate value token goes too.
                    extras.next();
                }
            }
            None => argv.push(token.clone()),
        }
    }
    argv
}

/// Finds the reserved option a token sets, if any: an exact spelling,
/// `--name=value` for a long spelling, a unique GNU getopt abbreviation
/// of a `match_abbrev` long, a bundled reserved short, or an attached
/// short value. The `=` boundary keeps `--alertness` from matching
/// `--alert`; an abbreviation must be a prefix of the reserved spelling,
/// not the other way around.
fn reserved_option_for<'a>(
    reserved: &'a [ReservedOption],
    token: &str,
) -> Option<&'a ReservedOption> {
    if token.starts_with("--") {
        return reserved_long_option(reserved, token);
    }
    reserved_short_cluster(reserved, token)
}

fn reserved_long_option<'a>(
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
        .or_else(|| unique_long_abbrev(reserved, name))
}

/// GNU getopt unique-prefix match among options that opted into
/// `match_abbrev`. Uniqueness is against this reserved table, not kw's
/// full option list — a prefix kw would reject as ambiguous can still
/// strip here when only one reserved long matches. Ambiguous prefixes
/// among reserved longs are left alone so a following value token is
/// not eaten.
fn unique_long_abbrev<'a>(
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
            Some(previous) if std::ptr::eq(previous, option) => {}
            Some(_) => return None,
        }
    }
    found
}

/// A short token that contains any reserved flag: `-f`, `-rf`, `-Fpkg`.
/// Walking leftover letters would turn `-ukernel` into `-kernel`, so a
/// hit drops the whole token.
fn reserved_short_cluster<'a>(
    reserved: &'a [ReservedOption],
    token: &str,
) -> Option<&'a ReservedOption> {
    let body = token.strip_prefix('-')?;
    if body.is_empty() || body.starts_with('-') {
        return None;
    }
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    let mut hit = None;
    while i < chars.len() {
        let spelling = format!("-{}", chars[i]);
        match exact_short(reserved, &spelling) {
            Some(option) => {
                hit = Some(option);
                if option.takes_value {
                    // Remainder is the attached value; stop so
                    // `value_is_attached` can see those extra chars.
                    break;
                }
                i += 1;
            }
            None => i += 1,
        }
    }
    hit
}

fn exact_short<'a>(reserved: &'a [ReservedOption], spelling: &str) -> Option<&'a ReservedOption> {
    reserved
        .iter()
        .find(|option| option.spellings.iter().any(|s| *s == spelling))
}

fn value_is_attached(token: &str, option: &ReservedOption) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extras(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| token.to_string()).collect()
    }

    #[test]
    fn build_argv_without_extras_is_the_base_command() {
        assert_eq!(vec!["build"], build_argv(&[]));
    }

    #[test]
    fn extras_are_appended_in_order() {
        assert_eq!(
            vec!["build", "--verbose", "--ccache", "-j8"],
            build_argv(&extras(&["--verbose", "--ccache", "-j8"]))
        );
    }

    #[test]
    fn reserved_alert_is_stripped_from_extras() {
        assert_eq!(
            vec!["build", "--verbose"],
            build_argv(&extras(&["--alert=vv", "--verbose"]))
        );
        // Separate-token value form: the value token is consumed too.
        assert_eq!(
            vec!["build", "--verbose"],
            build_argv(&extras(&["--alert", "v", "--verbose"]))
        );
    }

    #[test]
    fn save_log_to_is_reserved_in_both_value_forms() {
        assert_eq!(
            vec!["build"],
            build_argv(&extras(&["--save-log-to=/tmp/x.log"]))
        );
        assert_eq!(
            vec!["build"],
            build_argv(&extras(&["--save-log-to", "/tmp/x.log"]))
        );
    }

    #[test]
    fn similar_prefix_is_not_reserved() {
        // `--alertness` only shares a prefix with `--alert`.
        assert_eq!(
            vec!["build", "--alertness"],
            build_argv(&extras(&["--alertness"]))
        );
    }

    #[test]
    fn trailing_reserved_option_without_value_is_stripped() {
        assert_eq!(vec!["build"], build_argv(&extras(&["--alert"])));
    }

    #[test]
    fn menu_is_stripped_without_eating_the_next_token() {
        // kw build --menu would open menuconfig with the job's stdio
        // redirected to a log file — a hang, not a build.
        assert_eq!(
            vec!["build", "--verbose"],
            build_argv(&extras(&["--menu", "--verbose"]))
        );
    }

    #[test]
    fn tree_mutating_and_hanging_build_flags_are_stripped() {
        assert_eq!(
            vec!["build", "--verbose"],
            build_argv(&extras(&[
                "--clean",
                "--full-cleanup",
                "--doc",
                "--info",
                "--from-sha",
                "abc123",
                "--verbose",
            ]))
        );
        assert_eq!(vec!["build"], build_argv(&extras(&["--from-sha=abc123"])));
    }

    #[test]
    fn boolean_reserved_strips_only_itself_in_all_spellings() {
        // A boolean reserved option must not eat the following token.
        let reserved = [ReservedOption::new(&["--force", "-f"], false)];
        assert_eq!(
            vec!["deploy", "extra"],
            merge_extra_args(&["deploy"], &reserved, &extras(&["--force", "extra"]))
        );
        assert_eq!(
            vec!["deploy", "extra"],
            merge_extra_args(&["deploy"], &reserved, &extras(&["-f", "extra"]))
        );
    }

    #[test]
    fn tokens_after_a_consumed_value_keep_flowing() {
        let reserved = [ReservedOption::new(&["--remote"], true)];
        assert_eq!(
            vec!["deploy", "--no-reboot"],
            merge_extra_args(
                &["deploy"],
                &reserved,
                &extras(&["--remote", "host:22", "--no-reboot"])
            )
        );
    }

    const ENDPOINT: &str = "root@lima-ph-dut.internal:22";

    fn deploy(extra: &[&str]) -> Vec<String> {
        deploy_argv(ENDPOINT, false, true, &extras(extra))
    }

    #[test]
    fn deploy_argv_without_extras_is_remote_no_reboot_force() {
        assert_eq!(
            vec!["deploy", "--remote", ENDPOINT, "--no-reboot", "--force",],
            deploy_argv(ENDPOINT, false, true, &[])
        );
    }

    #[test]
    fn deploy_argv_reboot_and_unforced_swap_the_injected_flags() {
        assert_eq!(
            vec!["deploy", "--remote", ENDPOINT, "--reboot"],
            deploy_argv(ENDPOINT, true, false, &[])
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
            build_argv(&extras(&["--men", "--from", "abc123", "--verbose"]))
        );
    }

    #[test]
    fn help_is_stripped_from_build_and_deploy_extras() {
        // `kw build --help` exits 0 without compiling, so a D job would
        // otherwise chain into `kw deploy --help` (exit 22).
        assert_eq!(
            vec!["build", "--verbose"],
            build_argv(&extras(&["--help", "-h", "--verbose"]))
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
