use mockall::automock;

use std::{
    collections::{HashMap, HashSet},
    io,
    path::Path,
    sync::Arc,
};

use crate::{
    infrastructure::file_system::{FileSystemError, FileSystemTrait},
    lore::domain::{mailing_list::MailingList, patch::Patch},
};

#[automock]
pub trait LorePersistence: Send + Sync {
    fn load_available_lists(&self) -> Result<Vec<MailingList>, FileSystemError>;
    fn save_available_lists(&self, lists: &[MailingList]) -> Result<(), FileSystemError>;

    fn load_bookmarked_patchsets(&self) -> Result<Vec<Patch>, FileSystemError>;
    fn save_bookmarked_patchsets(&self, patchsets: &[Patch]) -> Result<(), FileSystemError>;

    fn load_reviewed_patchsets(&self) -> Result<HashMap<String, HashSet<usize>>, FileSystemError>;
    fn save_reviewed_patchsets(
        &self,
        reviewed: &HashMap<String, HashSet<usize>>,
    ) -> Result<(), FileSystemError>;
}

pub struct FileLorePersistence {
    fs: Arc<dyn FileSystemTrait>,
    mailing_lists_path: String,
    bookmarked_path: String,
    reviewed_path: String,
}

impl FileLorePersistence {
    pub fn new(
        fs: Arc<dyn FileSystemTrait>,
        mailing_lists_path: String,
        bookmarked_path: String,
        reviewed_path: String,
    ) -> Self {
        FileLorePersistence {
            fs,
            mailing_lists_path,
            bookmarked_path,
            reviewed_path,
        }
    }

    fn atomic_write_json<T: serde::Serialize + ?Sized>(
        &self,
        value: &T,
        path: &str,
    ) -> Result<(), FileSystemError> {
        if let Some(parent) = Path::new(path).parent() {
            self.fs.create_dir_all(parent)?;
        }

        let tmp_path = format!("{path}.tmp");
        {
            let writer = self.fs.create_writer(Path::new(&tmp_path))?;
            serde_json::to_writer(writer, value).map_err(io::Error::from)?;
        }
        self.fs.rename(Path::new(&tmp_path), Path::new(path))?;
        Ok(())
    }

    fn read_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, FileSystemError> {
        let reader = self.fs.open_bufreader(Path::new(path))?;
        serde_json::from_reader(reader)
            .map_err(io::Error::from)
            .map_err(FileSystemError::from)
    }
}

impl LorePersistence for FileLorePersistence {
    fn load_available_lists(&self) -> Result<Vec<MailingList>, FileSystemError> {
        self.read_json(&self.mailing_lists_path)
    }

    fn save_available_lists(&self, lists: &[MailingList]) -> Result<(), FileSystemError> {
        self.atomic_write_json(lists, &self.mailing_lists_path)
    }

    fn load_bookmarked_patchsets(&self) -> Result<Vec<Patch>, FileSystemError> {
        self.read_json(&self.bookmarked_path)
    }

    fn save_bookmarked_patchsets(&self, patchsets: &[Patch]) -> Result<(), FileSystemError> {
        self.atomic_write_json(patchsets, &self.bookmarked_path)
    }

    fn load_reviewed_patchsets(&self) -> Result<HashMap<String, HashSet<usize>>, FileSystemError> {
        self.read_json(&self.reviewed_path)
    }

    fn save_reviewed_patchsets(
        &self,
        reviewed: &HashMap<String, HashSet<usize>>,
    ) -> Result<(), FileSystemError> {
        self.atomic_write_json(reviewed, &self.reviewed_path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::env;
    use std::fs;
    use std::sync::Arc;

    use crate::infrastructure::file_system::OsFileSystem;
    use crate::lore::domain::mailing_list::MailingList;

    use super::*;

    fn tmp_dir(test_name: &str) -> std::path::PathBuf {
        let dir = env::temp_dir().join(format!(
            "patch-hub-persistence-{}-{}",
            test_name,
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_persistence(dir: &std::path::Path) -> FileLorePersistence {
        FileLorePersistence::new(
            Arc::new(OsFileSystem),
            dir.join("lists.json").to_str().unwrap().to_string(),
            dir.join("bookmarked.json").to_str().unwrap().to_string(),
            dir.join("reviewed.json").to_str().unwrap().to_string(),
        )
    }

    #[test]
    fn available_lists_round_trip() {
        let dir = tmp_dir("lists");
        let p = make_persistence(&dir);

        let lists = vec![
            MailingList::new("linux-mm", "Linux-mm Archive"),
            MailingList::new("linux-kernel", "LKML"),
        ];

        p.save_available_lists(&lists).unwrap();
        let loaded = p.load_available_lists().unwrap();

        assert_eq!(2, loaded.len());
        assert_eq!("linux-mm", loaded[0].name());
        assert_eq!("linux-kernel", loaded[1].name());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reviewed_patchsets_round_trip() {
        let dir = tmp_dir("reviewed");
        let p = make_persistence(&dir);

        let mut reviewed: HashMap<String, HashSet<usize>> = HashMap::new();
        reviewed.entry("some-id".to_string()).or_default().insert(1);
        reviewed.entry("some-id".to_string()).or_default().insert(2);

        p.save_reviewed_patchsets(&reviewed).unwrap();
        let loaded = p.load_reviewed_patchsets().unwrap();

        assert!(loaded.contains_key("some-id"));
        assert!(loaded["some-id"].contains(&1));
        assert!(loaded["some-id"].contains(&2));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_returns_error_when_file_missing() {
        let dir = tmp_dir("missing");
        let p = make_persistence(&dir);

        assert!(p.load_available_lists().is_err());
        assert!(p.load_bookmarked_patchsets().is_err());
        assert!(p.load_reviewed_patchsets().is_err());

        fs::remove_dir_all(&dir).unwrap();
    }
}
