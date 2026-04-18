#![allow(dead_code)]

use mockall::automock;
use thiserror::Error;

use std::{path::Path, sync::Arc};

use crate::{
    infrastructure::{
        file_system::FileSystemTrait,
        shell::{ShellCommand, ShellTrait},
    },
    lore::domain::patch::Patch,
};

#[derive(Debug, Error, PartialEq)]
pub enum PatchFetchError {
    #[error("patchset not found: {0}")]
    NotFound(String),
}

#[automock]
pub trait PatchsetFetcher: Send + Sync {
    fn download(&self, patch: &Patch) -> Result<String, PatchFetchError>;
}

pub struct B4PatchsetFetcher {
    shell: Arc<dyn ShellTrait>,
    fs: Arc<dyn FileSystemTrait>,
    cache_dir: String,
}

impl B4PatchsetFetcher {
    pub fn new(
        shell: Arc<dyn ShellTrait>,
        fs: Arc<dyn FileSystemTrait>,
        cache_dir: String,
    ) -> Self {
        B4PatchsetFetcher {
            shell,
            fs,
            cache_dir,
        }
    }
}

impl PatchsetFetcher for B4PatchsetFetcher {
    fn download(&self, patch: &Patch) -> Result<String, PatchFetchError> {
        let message_id: &str = &patch.message_id().href;
        let mbox_name = extract_mbox_name_from_message_id(message_id);
        let output_dir = &self.cache_dir;

        if !self.fs.exists(Path::new(output_dir))
            && self.fs.create_dir_all(Path::new(output_dir)).is_err()
        {
            return Err(PatchFetchError::NotFound(
                "Couldn't create patches dir.".to_string(),
            ));
        }

        let filepath = format!("{output_dir}/{mbox_name}");
        if !self.fs.exists(Path::new(&filepath)) {
            let cmd = ShellCommand::new("b4").args([
                "--quiet",
                "am",
                "--use-version",
                &format!("{}", patch.version()),
                message_id,
                "--outdir",
                output_dir,
                "--mbox-name",
                &mbox_name,
            ]);

            if self.shell.execute(&cmd).is_err() {
                return Err(PatchFetchError::NotFound(
                    "b4 couldn't fetch patchset file.".to_string(),
                ));
            }
        }

        if self.fs.exists(Path::new(&filepath)) {
            Ok(filepath)
        } else {
            Err(PatchFetchError::NotFound(
                "b4 couldn't fetch patchset file.".to_string(),
            ))
        }
    }
}

fn extract_mbox_name_from_message_id(message_id: &str) -> String {
    let mut mbox_name = message_id
        .replace("http://lore.kernel.org/", "")
        .replace("https://lore.kernel.org/", "")
        .replace('/', ".");

    if !mbox_name.ends_with('.') {
        mbox_name.push('.');
    }
    mbox_name.push_str("mbx");
    mbox_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{
        file_system::MockFileSystemTrait,
        shell::{MockShellTrait, ShellOutput},
    };

    // message_id → "linux-kernel.1234.567-1-john@johnson.com.mbx"
    // (trailing slash in href becomes the last dot, then "mbx")
    const CACHE_DIR: &str = "/cache";
    const MESSAGE_ID: &str = "https://lore.kernel.org/linux-kernel/1234.567-1-john@johnson.com/";
    const CACHED_FILE: &str = "/cache/linux-kernel.1234.567-1-john@johnson.com.mbx";

    fn make_patch_with_message_id(message_id: &str) -> Patch {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <feed xmlns="http://www.w3.org/2005/Atom">
                <entry>
                    <title>some/subsystem: Do this</title>
                    <author><name>John</name><email>j@j.com</email></author>
                    <id>{message_id}</id>
                    <updated>2024-01-01T00:00:00Z</updated>
                    <link href="{message_id}"/>
                </entry>
            </feed>"#,
        );
        crate::lore::infrastructure::parsers::parse_patch_feed(&xml)
            .unwrap()
            .patches()[0]
            .clone()
    }

    #[test]
    fn download_skips_b4_when_file_already_exists() {
        let patch = make_patch_with_message_id(MESSAGE_ID);

        let mut mock_fs = MockFileSystemTrait::new();
        // cache dir exists
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHE_DIR))
            .returning(|_| true);
        // file already cached
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHED_FILE))
            .returning(|_| true);

        let mock_shell = MockShellTrait::new(); // b4 must NOT be called

        let fetcher = B4PatchsetFetcher::new(
            Arc::new(mock_shell),
            Arc::new(mock_fs),
            CACHE_DIR.to_string(),
        );

        let result = fetcher.download(&patch);
        assert_eq!(Ok(CACHED_FILE.to_string()), result);
    }

    #[test]
    fn download_calls_b4_when_file_missing_then_returns_path() {
        let patch = make_patch_with_message_id(MESSAGE_ID);

        // Track how many times exists(CACHED_FILE) was called so we can
        // return false the first time (file missing) and true the second
        // time (after b4 fetched it).
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let mut mock_fs = MockFileSystemTrait::new();
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHE_DIR))
            .returning(|_| true);
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHED_FILE))
            .returning(move |_| {
                let n = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                n > 0 // false on first call (before b4), true on second (after b4)
            });

        let mut mock_shell = MockShellTrait::new();
        mock_shell
            .expect_execute()
            .withf(|cmd| cmd.program == "b4")
            .times(1)
            .returning(|_| {
                Ok(ShellOutput {
                    stdout: vec![],
                    stderr: vec![],
                    success: true,
                })
            });

        let fetcher = B4PatchsetFetcher::new(
            Arc::new(mock_shell),
            Arc::new(mock_fs),
            CACHE_DIR.to_string(),
        );

        let result = fetcher.download(&patch);
        assert_eq!(Ok(CACHED_FILE.to_string()), result);
    }

    #[test]
    fn download_returns_error_when_b4_fails() {
        let patch = make_patch_with_message_id(MESSAGE_ID);

        let mut mock_fs = MockFileSystemTrait::new();
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHE_DIR))
            .returning(|_| true);
        mock_fs
            .expect_exists()
            .withf(|p| p == Path::new(CACHED_FILE))
            .returning(|_| false);

        let mut mock_shell = MockShellTrait::new();
        mock_shell.expect_execute().times(1).returning(|_| {
            Err(crate::infrastructure::shell::ShellError::IoError(
                std::io::Error::other("b4 not found"),
            ))
        });

        let fetcher = B4PatchsetFetcher::new(
            Arc::new(mock_shell),
            Arc::new(mock_fs),
            CACHE_DIR.to_string(),
        );

        let result = fetcher.download(&patch);
        assert!(matches!(result, Err(PatchFetchError::NotFound(_))));
    }
}
