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
}

impl ReservedOption {
    pub const fn new(spellings: &'static [&'static str], takes_value: bool) -> Self {
        Self {
            spellings,
            takes_value,
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
const BUILD_RESERVED: &[ReservedOption] = &[
    ReservedOption::new(&["--alert"], true),
    ReservedOption::new(&["--save-log-to"], true),
    ReservedOption::new(&["--menu"], false),
];

/// The argv for a build job: `kw build <extras>` (reserved extras stripped).
pub fn build_argv(extra_args: &[String]) -> Vec<String> {
    merge_extra_args(&["build"], BUILD_RESERVED, extra_args)
}

/// Appends user-supplied extra args to `base`, stripping every token that
/// would override a reserved option: `--name=value` is stripped whole,
/// `--name value` consumes the following token too, and a boolean
/// reserved option strips only itself. All other extras pass through in
/// order.
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
                if option.takes_value && !token.contains('=') {
                    // `--name value`: the separate value token goes too.
                    extras.next();
                }
            }
            None => argv.push(token.clone()),
        }
    }
    argv
}

/// Finds the reserved option a token sets, if any: an exact spelling
/// match, or `--name=value` for a long spelling. The `=` boundary keeps
/// `--alertness` from matching `--alert`.
fn reserved_option_for<'a>(
    reserved: &'a [ReservedOption],
    token: &str,
) -> Option<&'a ReservedOption> {
    reserved.iter().find(|option| {
        option.spellings.iter().any(|spelling| {
            token == *spelling
                || spelling.starts_with("--")
                    && token
                        .strip_prefix(spelling)
                        .is_some_and(|rest| rest.starts_with('='))
        })
    })
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
}
