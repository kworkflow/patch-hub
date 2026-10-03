//! Scans a finished job's log for what the exit code alone does not say:
//! the first real error of a failed job, and the known non-fatal failures
//! `kw deploy` exits 0 through.
//!
//! Pure functions over the log text; the actor reads the file.

/// Longer lines (deep include paths, long make targets) are cut so one
/// finding stays a single KwOps row.
const MAX_LINE_CHARS: usize = 200;
const MAX_DEPLOY_WARNINGS: usize = 5;

/// Lines `kw deploy` prints for a failure it does not turn into a non-zero
/// exit: initramfs generation (Debian `update-initramfs`, Arch
/// `mkinitcpio`) and kw's own bootloader/boot-once setup.
const DEPLOY_WARNING_MARKERS: [&str; 4] = [
    "update-initramfs: failed",
    "==> ERROR:",
    "kw was unable to set up the first boot",
    "kw did not find grub.cfg",
];

/// The first line of a failed job's log that names what went wrong: a
/// compiler/linker `error:`, a modpost `ERROR:`, a kbuild `*** ...`
/// banner (e.g. "The source tree is not clean"), or a git/kw `error:`.
/// Falls back to the last `make: *** ... Error N` line, which only names
/// the failing target. Kbuild prints the real cause first and a cascade
/// of make errors after it, so the first match is the useful one.
pub fn first_error(log: &str) -> Option<String> {
    log.lines()
        .map(str::trim)
        .find(|line| is_error_line(line))
        .or_else(|| log.lines().map(str::trim).rfind(|line| is_make_error(line)))
        .map(cap_line)
}

/// Known failures in a deploy log that `kw deploy` exits 0 through, at
/// most [`MAX_DEPLOY_WARNINGS`]. With `kernelrelease`, a GRUB update that
/// did not list that kernel is reported too: kw installs arm64 kernels as
/// `Image-<release>`, which Debian's `10_linux` does not pick up, so the
/// deployed kernel is never bootable from the menu.
pub fn deploy_warnings(log: &str, kernelrelease: Option<&str>) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    for line in log.lines().map(str::trim) {
        if DEPLOY_WARNING_MARKERS
            .iter()
            .any(|marker| line.contains(marker))
        {
            let line = cap_line(line);
            if !warnings.contains(&line) {
                warnings.push(line);
            }
        }
    }
    let unlisted = kernelrelease.filter(|release| grub_missed_release(log, release));
    warnings.truncate(MAX_DEPLOY_WARNINGS - usize::from(unlisted.is_some()));
    if let Some(release) = unlisted {
        warnings.push(format!("GRUB did not list kernel {release}"));
    }
    warnings
}

fn is_error_line(line: &str) -> bool {
    line.contains("fatal error:")
        || line.contains(" error:")
        || line.starts_with("error:")
        || line.starts_with("ERROR:")
        || line
            .strip_prefix("*** ")
            .is_some_and(|banner| !banner.trim().is_empty())
}

/// `make[N]: *** [target] Error N`, the summary make prints per failing
/// level.
fn is_make_error(line: &str) -> bool {
    line.starts_with("make")
        && line.contains(": *** ")
        && line
            .rsplit_once("Error ")
            .is_some_and(|(_, code)| !code.is_empty() && code.chars().all(|c| c.is_ascii_digit()))
}

/// GRUB ran (`Generating grub configuration file`) but no `Found linux
/// image:` line names an image for `release`. Matched on the `-<release>`
/// suffix so `7.2.0-rc6` is not satisfied by `Image-7.2.0-rc6-phboot`.
fn grub_missed_release(log: &str, release: &str) -> bool {
    if !log.contains("Generating grub configuration file") {
        return false;
    }
    let suffix = format!("-{release}");
    !log.lines().map(str::trim).any(|line| {
        line.strip_prefix("Found linux image:")
            .is_some_and(|image| image.trim().ends_with(&suffix))
    })
}

fn cap_line(line: &str) -> String {
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((index, _)) => format!("{}…", &line[..index]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kw build` under an env `O=` with in-tree leftovers in the source
    /// tree (lab/logs/20260920-122005).
    const UNCLEAN_TREE_BUILD_LOG: &str = "\
/boot/config-6.8.0-138-generic:883:warning: symbol value '0' invalid for BASE_SMALL
  SYNC    include/config/auto.conf.cmd
***
*** The source tree is not clean, please run 'make ARCH=arm64 mrproper'
*** in /opt/ph-lab/work/linux
***
make[3]: *** [/opt/ph-lab/work/linux/Makefile:709: outputmakefile] Error 1
make[2]: *** [/opt/ph-lab/work/linux/Makefile:906: include/config/auto.conf.cmd] Error 2
make[2]: *** [include/config/auto.conf.cmd] Deleting file 'include/generated/rustc_cfg'
make[2]: *** [include/config/auto.conf.cmd] Deleting file 'include/generated/autoconf.h'
make[1]: *** [/opt/ph-lab/work/linux/Makefile:248: __sub-make] Error 2
make[1]: Leaving directory '/opt/ph-lab/work/out'
";

    /// The M4 fixture's compile break (lab/logs/20260920-151635/M18).
    const COMPILE_ERROR_BUILD_LOG: &str = "\
  CC      mm/filemap.o
  AS      arch/arm64/kernel/entry.o
/opt/ph-lab/work/linux/init/main.c:1691:2: error: #error M4 broken-build fixture
 1691 | #error M4 broken-build fixture
      |  ^~~~~
make[4]: *** [/opt/ph-lab/work/linux/scripts/Makefile.build:289: init/main.o] Error 1
make[3]: *** [/opt/ph-lab/work/linux/scripts/Makefile.build:549: init] Error 2
make[3]: *** Waiting for unfinished jobs....
  CC      kernel/exec_domain.o
make: *** [Makefile:248: __sub-make] Error 2
";

    /// tinyconfig deploy to the Ubuntu DUT: kw exits 0 although
    /// initramfs generation failed and GRUB never listed the kernel
    /// (lab/logs/20261003-111949).
    const TINYCONFIG_DEPLOY_LOG: &str = "\
cp /tmp/kw/kw_pkg/Image-7.2.0-rc6+ /boot/
generate_debian_temporary_root_file_system VERBOSE 7.2.0-rc6+ remote GRUB
update-initramfs -c -k 7.2.0-rc6+
update-initramfs: Generating /boot/initrd.img-7.2.0-rc6+
W: zstd compression (CONFIG_RD_ZSTD) not supported by kernel, using gzip
E: gzip compression (CONFIG_RD_GZIP) not supported by kernel
update-initramfs: failed for /boot/initrd.img-7.2.0-rc6+ with 1.
update-grub
Sourcing file `/etc/default/grub'
Generating grub configuration file ...
Found linux image: /boot/vmlinuz-6.8.0-137-generic
Found initrd image: /boot/initrd.img-6.8.0-137-generic
Found linux image: /boot/vmlinuz-6.8.0-136-generic
Found initrd image: /boot/initrd.img-6.8.0-136-generic
Adding boot menu entry for UEFI Firmware Settings ...
done
";

    /// The real-boot deploy of the tipc patch: everything worked
    /// (lab/logs/20261003-154745-real-boot-tui-tipc).
    const CLEAN_DEPLOY_LOG: &str = "\
* Preparing modules
cp: cannot stat '/home/lima.guest/.cache/kw/envs/L29wdC9waC1sYWIvd29yay9saW51eA==/ph-boot/arch/arm64/boot/dts/*.dtb': No such file or directory
* Sending kernel package (7.2.0-rc6-phboot-g2a475abe5df2.kw.tar) to the remote
update-initramfs: Generating /boot/initrd.img-7.2.0-rc6-phboot-g2a475abe5df2
Ignoring old or unknown version 7.2.0-rc6-phboot-g2a475abe5df2 (latest is 6.8.0-137-generic)
Generating grub configuration file ...
Found linux image: /boot/vmlinuz-6.8.0-137-generic
Found initrd image: /boot/initrd.img-6.8.0-137-generic
Found linux image: /boot/Image-7.2.0-rc6-phboot-g2a475abe5df2
Found initrd image: /boot/initrd.img-7.2.0-rc6-phboot-g2a475abe5df2
Found linux image: /boot/Image-7.2.0-rc6-phboot+
Found linux image: /boot/Image-7.2.0-rc6+
done
";

    #[test]
    fn first_error_names_the_kbuild_banner_not_the_make_cascade() {
        assert_eq!(
            Some(
                "*** The source tree is not clean, please run 'make ARCH=arm64 mrproper'"
                    .to_string()
            ),
            first_error(UNCLEAN_TREE_BUILD_LOG)
        );
    }

    #[test]
    fn first_error_names_the_compiler_error() {
        assert_eq!(
            Some(
                "/opt/ph-lab/work/linux/init/main.c:1691:2: error: #error M4 broken-build fixture"
                    .to_string()
            ),
            first_error(COMPILE_ERROR_BUILD_LOG)
        );
    }

    #[test]
    fn first_error_recognizes_modpost_and_git_errors() {
        assert_eq!(
            Some("ERROR: modpost: \"foo\" [drivers/bar.ko] undefined!".to_string()),
            first_error(
                "  MODPOST Module.symvers\nERROR: modpost: \"foo\" [drivers/bar.ko] undefined!\n"
            )
        );
        assert_eq!(
            Some("error: pathspec 'nope' did not match any file(s) known to git".to_string()),
            first_error("error: pathspec 'nope' did not match any file(s) known to git\n")
        );
        assert_eq!(
            Some("drivers/foo.c:1:10: fatal error: bar.h: No such file or directory".to_string()),
            first_error("drivers/foo.c:1:10: fatal error: bar.h: No such file or directory\n")
        );
    }

    #[test]
    fn first_error_falls_back_to_the_last_make_error() {
        let log = "\
  CC      init/main.o
make[2]: *** [scripts/Makefile.build:549: certs] Error 2
make[2]: *** Waiting for unfinished jobs....
make: *** [Makefile:248: __sub-make] Error 2
";
        assert_eq!(
            Some("make: *** [Makefile:248: __sub-make] Error 2".to_string()),
            first_error(log)
        );
    }

    #[test]
    fn first_error_is_none_without_any_error_line() {
        assert_eq!(None, first_error(""));
        assert_eq!(None, first_error("  CC      init/main.o\n***\n"));
    }

    #[test]
    fn first_error_caps_long_lines() {
        let line = format!("drivers/x.c:1:1: error: {}", "y".repeat(400));
        let found = first_error(&line).unwrap();
        assert_eq!(MAX_LINE_CHARS + 1, found.chars().count());
        assert!(found.ends_with('…'));
    }

    #[test]
    fn deploy_warnings_catch_initramfs_failure_and_unlisted_kernel() {
        assert_eq!(
            vec![
                "update-initramfs: failed for /boot/initrd.img-7.2.0-rc6+ with 1.".to_string(),
                "GRUB did not list kernel 7.2.0-rc6+".to_string(),
            ],
            deploy_warnings(TINYCONFIG_DEPLOY_LOG, Some("7.2.0-rc6+"))
        );
    }

    #[test]
    fn deploy_warnings_skip_the_grub_check_without_a_release() {
        assert_eq!(
            vec!["update-initramfs: failed for /boot/initrd.img-7.2.0-rc6+ with 1.".to_string()],
            deploy_warnings(TINYCONFIG_DEPLOY_LOG, None)
        );
    }

    #[test]
    fn clean_deploy_has_no_warnings() {
        assert!(
            deploy_warnings(CLEAN_DEPLOY_LOG, Some("7.2.0-rc6-phboot-g2a475abe5df2")).is_empty()
        );
    }

    #[test]
    fn grub_check_needs_an_exact_release_suffix() {
        // `Image-7.2.0-rc6-phboot+` must not satisfy release `7.2.0-rc6`.
        let log = "\
Generating grub configuration file ...
Found linux image: /boot/Image-7.2.0-rc6-phboot+
done
";
        assert_eq!(
            vec!["GRUB did not list kernel 7.2.0-rc6".to_string()],
            deploy_warnings(log, Some("7.2.0-rc6"))
        );
    }

    #[test]
    fn grub_check_is_silent_when_grub_did_not_run() {
        // systemd-boot / non-GRUB DUTs never print the GRUB banner.
        assert!(deploy_warnings("==> Starting build: '7.2.0'\n", Some("7.2.0")).is_empty());
    }

    #[test]
    fn deploy_warnings_dedup_and_cap() {
        let log = "\
==> ERROR: module not found: 'a'
==> ERROR: module not found: 'a'
==> ERROR: module not found: 'b'
==> ERROR: module not found: 'c'
==> ERROR: module not found: 'd'
==> ERROR: module not found: 'e'
==> ERROR: module not found: 'f'
kw was unable to set up the first boot
";
        let warnings = deploy_warnings(log, None);
        assert_eq!(MAX_DEPLOY_WARNINGS, warnings.len());
        assert_eq!("==> ERROR: module not found: 'a'", warnings[0]);
        assert_eq!("==> ERROR: module not found: 'e'", warnings[4]);

        // The unlisted-kernel warning survives the cap.
        let log = format!("{log}Generating grub configuration file ...\n");
        let warnings = deploy_warnings(&log, Some("7.2.0"));
        assert_eq!(MAX_DEPLOY_WARNINGS, warnings.len());
        assert_eq!("GRUB did not list kernel 7.2.0", warnings[4]);
    }
}
