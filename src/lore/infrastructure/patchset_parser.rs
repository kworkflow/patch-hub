#![allow(dead_code)]

use mockall::automock;

use std::{
    io::BufRead,
    mem::swap,
    path::Path,
    sync::{Arc, LazyLock},
};

use regex::Regex;

use crate::infrastructure::{file_system::FileSystemTrait, shell::ShellCommand};

#[automock]
pub trait PatchsetParser: Send + Sync {
    fn split_patchset(&self, patchset_path: &str) -> Result<Vec<String>, String>;
}

pub struct MboxPatchsetParser {
    fs: Arc<dyn FileSystemTrait>,
}

impl MboxPatchsetParser {
    pub fn new(fs: Arc<dyn FileSystemTrait>) -> Self {
        MboxPatchsetParser { fs }
    }
}

impl PatchsetParser for MboxPatchsetParser {
    fn split_patchset(&self, patchset_path_str: &str) -> Result<Vec<String>, String> {
        let mut patches: Vec<String> = Vec::new();
        let patchset_path = Path::new(patchset_path_str);
        let cover_letter_path_str = patchset_path_str.replace(".mbx", ".cover");
        let cover_letter_path = Path::new(&cover_letter_path_str);

        if !self.fs.exists(patchset_path) {
            return Err(format!("{}: Path doesn't exist", patchset_path.display()));
        } else if !self.fs.is_file(patchset_path) {
            return Err(format!("{}: Not a file", patchset_path.display()));
        }

        if self.fs.exists(cover_letter_path) && self.fs.is_file(cover_letter_path) {
            extract_patches(&*self.fs, cover_letter_path, &mut patches);
        }

        extract_patches(&*self.fs, patchset_path, &mut patches);

        Ok(patches)
    }
}

fn extract_patches(fs: &dyn FileSystemTrait, mbox_path: &Path, patches: &mut Vec<String>) {
    let mut current_patch = String::new();
    let mut is_reading_patch = false;
    let mut is_last_line = false;

    let mbox_reader = fs.open_bufreader(mbox_path).unwrap();

    for line in mbox_reader.lines() {
        let line = line.unwrap();

        if line.starts_with("Subject: ") {
            is_reading_patch = true;
        } else if is_reading_patch && line.trim_end().eq("--") {
            is_last_line = true;
        } else if is_last_line {
            current_patch.push_str(&line);
            current_patch.push('\n');

            let mut patch_to_add = String::new();
            swap(&mut patch_to_add, &mut current_patch);
            patches.push(patch_to_add);

            is_reading_patch = false;
            is_last_line = false;
        } else if is_reading_patch && line.trim_end().eq("From git@z Thu Jan  1 00:00:00 1970") {
            let mut patch_to_add = String::new();
            swap(&mut patch_to_add, &mut current_patch);
            patches.push(patch_to_add);

            is_reading_patch = false;
        }

        if is_reading_patch {
            current_patch.push_str(&line);
            current_patch.push('\n');
        }
    }

    if !current_patch.is_empty() {
        patches.push(current_patch);
    }
}

/// Splits a raw patch string into `(cover, diff)` at the first `\n---\n` separator.
///
/// Everything before (and including) the separator line is the cover; everything
/// after is the diff. If there is no separator, the entire input is returned as the
/// cover with an empty diff slice.
pub fn split_cover(patch: &str) -> (&str, &str) {
    let mut cover: &str = patch;
    let mut diff: &str = "";

    if let Some(cover_end) = patch.find("\n---\n") {
        cover = &patch[..cover_end + 1];
        diff = &patch[cover_end + 5..];
    }

    (cover, diff)
}

/// Generates a reply template from a raw patch string, quoting the body and
/// prefixing the subject with `"Re: "`.
pub fn generate_reply_template(patch_contents: &str) -> String {
    let mut reply_template = String::new();
    let mut patch_lines_iterator = patch_contents.lines();

    for line in patch_lines_iterator.by_ref() {
        let mut line_to_push = String::new();

        if line.starts_with("Subject: ") {
            line_to_push = line.replace("Subject: ", "Subject: Re: ") + "\n";
        } else if line.starts_with("From: ")
            || line.starts_with("Date: ")
            || line.starts_with("Message-Id: ")
        {
            continue;
        } else if !line.trim().is_empty() {
            line_to_push = format!("{line}\n");
        } else if line.trim().is_empty() && !reply_template.is_empty() {
            reply_template.push('\n');
            break;
        }

        reply_template.push_str(&line_to_push);
    }

    for line in patch_lines_iterator {
        reply_template.push_str(&format!("> {line}\n"));
    }

    reply_template
}

/// Extracts the `git send-email` command from a lore patch HTML page and returns
/// a [`ShellCommand`] ready to invoke, with `reply_path` appended as the last
/// argument.
pub fn extract_git_reply_command(
    patch_html: &str,
    git_send_email_options: &str,
    reply_path: &str,
) -> ShellCommand {
    let mut args: Vec<String> = vec!["send-email".to_string()];

    for option in git_send_email_options.split_whitespace() {
        args.push(option.to_string());
    }

    static RE_FULL_GIT_COMMAND: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)git-send-email\(1\):(.*?)/path/to/YOUR_REPLY"#).unwrap()
    });

    static RE_LONG_OPTIONS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"--[^\s=]+=[^\s]+").unwrap());

    if let Some(capture) = RE_FULL_GIT_COMMAND.captures(patch_html) {
        if let Some(full_git_command_match) = capture.get(1) {
            for long_option_match in RE_LONG_OPTIONS.find_iter(full_git_command_match.as_str()) {
                args.push(long_option_match.as_str().to_string());
            }
        }
    }

    args.push(reply_path.to_string());

    ShellCommand {
        program: "git".to_string(),
        args,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use crate::infrastructure::file_system::OsFileSystem;

    use super::*;

    fn parser() -> MboxPatchsetParser {
        MboxPatchsetParser::new(Arc::new(OsFileSystem))
    }

    fn commands_eq(cmd1: &ShellCommand, cmd2: &ShellCommand) -> bool {
        cmd1.program == cmd2.program && cmd1.args == cmd2.args
    }

    #[test]
    fn split_patchset_returns_error_on_missing_path() {
        let ret = parser().split_patchset("invalid/path");
        assert_eq!(Err("invalid/path: Path doesn't exist".to_string()), ret);
    }

    #[test]
    fn split_patchset_returns_error_on_directory() {
        let ret = parser().split_patchset("test_samples/lore_session/split_patchset/not_a_file");
        assert_eq!(
            Err("test_samples/lore_session/split_patchset/not_a_file: Not a file".to_string()),
            ret
        );
    }

    #[test]
    fn split_patchset_without_cover_letter() {
        let patches = parser()
            .split_patchset(
                "test_samples/lore_session/split_patchset/patchset_sample_without_cover_letter.mbx",
            )
            .expect("Should return a Vec<String>");

        assert_eq!(3, patches.len(), "Wrong number of patches");
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_1.mbx")
                .unwrap(),
            patches[0]
        );
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_2.mbx")
                .unwrap(),
            patches[1]
        );
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_3.mbx")
                .unwrap(),
            patches[2]
        );
    }

    #[test]
    fn split_patchset_complete_with_cover_letter() {
        let patches = parser()
            .split_patchset("test_samples/lore_session/split_patchset/patchset_sample_complete.mbx")
            .expect("Should return a Vec<String>");

        assert_eq!(4, patches.len(), "Wrong number of patches");
        assert_eq!(
            fs::read_to_string(
                "test_samples/lore_session/split_patchset/expected_cover_letter.cover"
            )
            .unwrap(),
            patches[0],
            "Wrong cover letter"
        );
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_1.mbx")
                .unwrap(),
            patches[1]
        );
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_2.mbx")
                .unwrap(),
            patches[2]
        );
        assert_eq!(
            fs::read_to_string("test_samples/lore_session/split_patchset/expected_patch_3.mbx")
                .unwrap(),
            patches[3]
        );
    }

    #[test]
    fn split_cover_finds_separator() {
        let raw = "Subject: test\nTo: list\n\nBody text.\n---\ndiff --git a/foo b/foo";
        let (cover, diff) = split_cover(raw);
        assert!(cover.contains("Body text."));
        assert!(diff.contains("diff --git"));
    }

    #[test]
    fn split_cover_returns_full_patch_when_no_separator() {
        let raw = "Subject: no diff here\nBody only.";
        let (cover, diff) = split_cover(raw);
        assert_eq!(raw, cover);
        assert!(diff.is_empty());
    }

    #[test]
    fn generate_reply_template_produces_expected_output() {
        let patch_sample = fs::read_to_string(
            "test_samples/lore_session/generate_patch_reply_template/patch_sample.mbx",
        )
        .unwrap();
        let expected = fs::read_to_string(
            "test_samples/lore_session/generate_patch_reply_template/expected_reply_template.mbx",
        )
        .unwrap();

        assert_eq!(expected, generate_reply_template(&patch_sample));
    }

    #[test]
    fn extract_git_reply_command_parses_lore_html() {
        let patch_html = fs::read_to_string(
            "test_samples/lore_session/extract_git_reply_command/patch_lore_sample.html",
        )
        .unwrap();
        let reply_path = "/tmp/some-reply.mbx";
        let expected = ShellCommand {
            program: "git".to_string(),
            args: vec![
                "send-email".to_string(),
                "--dry-run".to_string(),
                "--suppress-cc=all".to_string(),
                "--in-reply-to=1234.567-3-john@johnson.com".to_string(),
                "--to=foo@bar.com".to_string(),
                "--cc=bar@foo.com".to_string(),
                "--cc=foo@list.org".to_string(),
                "--cc=bar@list.org".to_string(),
                reply_path.to_string(),
            ],
        };

        let result =
            extract_git_reply_command(&patch_html, "--dry-run --suppress-cc=all", reply_path);

        assert!(
            commands_eq(&expected, &result),
            "Wrong git reply command\nExpected:{expected:?}\n  Actual:{result:?}"
        );
    }
}
