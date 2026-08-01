use std::path::{Path, PathBuf};

use super::{FileSystemTrait, OsFileSystem};

struct TempDir(PathBuf);

impl TempDir {
    fn new(test_name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "patch_hub_fs_test_{}_{test_name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn read_to_string_returns_file_contents() {
    let dir = TempDir::new("read_to_string");
    let file_path = dir.path().join("test.txt");
    std::fs::write(&file_path, "hello world").unwrap();

    let fs = OsFileSystem;
    let contents = fs.read_to_string(&file_path).unwrap();
    assert_eq!(contents, "hello world");
}

#[test]
fn read_to_string_returns_error_for_missing_file() {
    let fs = OsFileSystem;
    let result = fs.read_to_string(Path::new("/nonexistent/path/file.txt"));
    assert!(result.is_err());
}

#[test]
fn write_creates_file_with_contents() {
    let dir = TempDir::new("write");
    let file_path = dir.path().join("write_test.txt");

    let fs = OsFileSystem;
    fs.write(&file_path, b"test data").unwrap();

    let contents = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "test data");
}

#[test]
fn create_dir_all_creates_nested_directories() {
    let dir = TempDir::new("create_dir_all");
    let nested = dir.path().join("a/b/c");

    let fs = OsFileSystem;
    fs.create_dir_all(&nested).unwrap();

    assert!(nested.is_dir());
}

#[test]
fn exists_returns_true_for_existing_path() {
    let dir = TempDir::new("exists");
    let file_path = dir.path().join("exists_test.txt");
    std::fs::write(&file_path, "").unwrap();

    let fs = OsFileSystem;
    assert!(fs.exists(&file_path));
    assert!(fs.exists(dir.path()));
    assert!(!fs.exists(&dir.path().join("nope")));
}

#[test]
fn is_file_distinguishes_files_from_dirs() {
    let dir = TempDir::new("is_file");
    let file_path = dir.path().join("a_file.txt");
    std::fs::write(&file_path, "").unwrap();

    let fs = OsFileSystem;
    assert!(fs.is_file(&file_path));
    assert!(!fs.is_file(dir.path()));
}

#[test]
fn is_dir_distinguishes_dirs_from_files() {
    let dir = TempDir::new("is_dir");
    let file_path = dir.path().join("a_file.txt");
    std::fs::write(&file_path, "").unwrap();

    let fs = OsFileSystem;
    assert!(fs.is_dir(dir.path()));
    assert!(!fs.is_dir(&file_path));
}

#[test]
fn rename_moves_file() {
    let dir = TempDir::new("rename");
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    std::fs::write(&src, "rename me").unwrap();

    let fs = OsFileSystem;
    fs.rename(&src, &dst).unwrap();

    assert!(!src.exists());
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "rename me");
}

#[test]
fn create_writer_writes_to_file() {
    use std::io::Write;

    let dir = TempDir::new("create_writer");
    let file_path = dir.path().join("writer_test.txt");

    let fs = OsFileSystem;
    let mut writer = fs.create_writer(&file_path).unwrap();
    writer.write_all(b"via writer").unwrap();
    drop(writer);

    let contents = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "via writer");
}

#[test]
fn open_bufreader_reads_file() {
    use std::io::BufRead;

    let dir = TempDir::new("open_bufreader");
    let file_path = dir.path().join("bufreader_test.txt");
    std::fs::write(&file_path, "line1\nline2\n").unwrap();

    let fs = OsFileSystem;
    let reader = fs.open_bufreader(&file_path).unwrap();

    let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();
    assert_eq!(lines, vec!["line1", "line2"]);
}

#[test]
fn metadata_returns_file_metadata() {
    let dir = TempDir::new("metadata");
    let file_path = dir.path().join("meta_test.txt");
    std::fs::write(&file_path, "some content").unwrap();

    let fs = OsFileSystem;
    let meta = fs.metadata(&file_path).unwrap();
    assert!(meta.is_file());
    assert_eq!(meta.len(), 12);
}

#[test]
fn metadata_returns_error_for_missing_path() {
    let fs = OsFileSystem;
    let result = fs.metadata(Path::new("/nonexistent/path"));
    assert!(result.is_err());
}
