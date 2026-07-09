use std::{collections::HashSet, path::PathBuf, str};

use color_eyre::{eyre::eyre, Result};

use crate::{
    infrastructure::shell::{ShellCommand, ShellTrait},
    lore::application::handle::LoreApiHandle,
};

pub(crate) struct ReviewedReplyRequest {
    pub raw_patches: Vec<String>,
    pub patches_to_reply: Vec<bool>,
    pub successful_indexes: HashSet<usize>,
    pub git_send_email_options: String,
}

pub(crate) enum ReviewedReplyResult {
    NoAction { successful_indexes: HashSet<usize> },
    MissingGitIdentity { successful_indexes: HashSet<usize> },
    Completed { successful_indexes: HashSet<usize> },
}

impl ReviewedReplyResult {
    pub(crate) fn into_successful_indexes(self) -> HashSet<usize> {
        match self {
            ReviewedReplyResult::NoAction { successful_indexes }
            | ReviewedReplyResult::MissingGitIdentity { successful_indexes }
            | ReviewedReplyResult::Completed { successful_indexes } => successful_indexes,
        }
    }
}

pub(crate) async fn execute_reviewed_reply(
    request: ReviewedReplyRequest,
    lore_api: &LoreApiHandle,
    shell: &dyn ShellTrait,
) -> Result<ReviewedReplyResult> {
    if !request.patches_to_reply.contains(&true) {
        return Ok(ReviewedReplyResult::NoAction {
            successful_indexes: request.successful_indexes,
        });
    }

    let (git_user_name, git_user_email) = lore_api
        .get_git_signature(String::new())
        .await
        .map_err(|e| eyre!("{e:#?}"))?;

    let mut successful_indexes = request.successful_indexes;
    let Some(git_signature) = git_signature(git_user_name, git_user_email) else {
        println!("`git config user.name` or `git config user.email` not set\nAborting...");
        return Ok(ReviewedReplyResult::MissingGitIdentity { successful_indexes });
    };

    let mktemp_cmd = ShellCommand::new("mktemp").arg("--directory");
    let tmp_out = shell
        .execute(&mktemp_cmd)
        .map_err(|e| eyre!("failed to create temp directory: {}", e))?;
    let tmp_dir_str = str::from_utf8(&tmp_out.stdout)
        .map_err(|e| eyre!("invalid utf-8 in temp dir path: {}", e))?
        .trim()
        .to_string();
    let tmp_dir = PathBuf::from(tmp_dir_str);

    let git_reply_commands = lore_api
        .prepare_reply_commands(
            tmp_dir,
            "all".to_string(),
            request.raw_patches,
            request.patches_to_reply.clone(),
            git_signature,
            request.git_send_email_options,
        )
        .await
        .map_err(|e| eyre!("{e:#?}"))?;

    record_successful_reply_indexes(
        shell,
        &mut successful_indexes,
        &request.patches_to_reply,
        git_reply_commands,
    );

    Ok(ReviewedReplyResult::Completed { successful_indexes })
}

fn record_successful_reply_indexes(
    shell: &dyn ShellTrait,
    successful_indexes: &mut HashSet<usize>,
    patches_to_reply: &[bool],
    git_reply_commands: Vec<ShellCommand>,
) {
    let reply_indexes: Vec<usize> = selected_reply_indexes(patches_to_reply);
    for (i, command) in git_reply_commands.into_iter().enumerate() {
        let success = shell.spawn_interactive(&command).unwrap_or(false);
        if success {
            successful_indexes.insert(reply_indexes[i]);
        }
    }
}

fn selected_reply_indexes(patches_to_reply: &[bool]) -> Vec<usize> {
    patches_to_reply
        .iter()
        .enumerate()
        .filter_map(|(i, &val)| if val { Some(i) } else { None })
        .collect()
}

fn git_signature(git_user_name: String, git_user_email: String) -> Option<String> {
    if git_user_name.is_empty() || git_user_email.is_empty() {
        None
    } else {
        Some(format!("{git_user_name} <{git_user_email}>"))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, io};

    use crate::infrastructure::shell::{MockShellTrait, ShellError};

    use super::*;

    fn command(name: &str) -> ShellCommand {
        ShellCommand::new("git").arg("send-email").arg(name)
    }

    #[test]
    fn selected_reply_indexes_preserves_original_patch_indexes() {
        let indexes = selected_reply_indexes(&[false, true, false, true]);

        assert_eq!(vec![1, 3], indexes);
    }

    #[test]
    fn git_signature_requires_name_and_email() {
        assert_eq!(
            Some("User <user@example.com>".to_string()),
            git_signature("User".to_string(), "user@example.com".to_string())
        );
        assert_eq!(
            None,
            git_signature(String::new(), "user@example.com".to_string())
        );
        assert_eq!(None, git_signature("User".to_string(), String::new()));
    }

    #[test]
    fn record_successful_reply_indexes_records_only_successful_commands() {
        let mut shell = MockShellTrait::new();
        shell
            .expect_spawn_interactive()
            .times(2)
            .returning(|cmd| Ok(cmd.args.last().is_some_and(|arg| arg == "first")));
        let mut successful_indexes = HashSet::from([0]);

        record_successful_reply_indexes(
            &shell,
            &mut successful_indexes,
            &[false, true, false, true],
            vec![command("first"), command("second")],
        );

        assert_eq!(HashSet::from([0, 1]), successful_indexes);
    }

    #[test]
    fn record_successful_reply_indexes_treats_shell_error_as_failure() {
        let mut shell = MockShellTrait::new();
        shell.expect_spawn_interactive().times(1).returning(|_| {
            Err(ShellError::IoError(io::Error::new(
                io::ErrorKind::Other,
                "failed",
            )))
        });
        let mut successful_indexes = HashSet::new();

        record_successful_reply_indexes(
            &shell,
            &mut successful_indexes,
            &[true],
            vec![command("first")],
        );

        assert!(successful_indexes.is_empty());
    }

    #[test]
    fn reviewed_reply_result_returns_successful_indexes_for_each_status() {
        let no_action = ReviewedReplyResult::NoAction {
            successful_indexes: HashSet::from([1]),
        };
        let missing_identity = ReviewedReplyResult::MissingGitIdentity {
            successful_indexes: HashSet::from([2]),
        };
        let completed = ReviewedReplyResult::Completed {
            successful_indexes: HashSet::from([3]),
        };

        assert_eq!(HashSet::from([1]), no_action.into_successful_indexes());
        assert_eq!(
            HashSet::from([2]),
            missing_identity.into_successful_indexes()
        );
        assert_eq!(HashSet::from([3]), completed.into_successful_indexes());
    }
}
