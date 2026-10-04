use std::{fs, path::Path};

use super::{FileSystemTrait, OsFileSystem};
use crate::test_support::TempDir;

#[test]
fn read_to_string_returns_file_contents() {
    let dir = TempDir::new("read_to_string");
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "hello world").expect("file writes");

    let fs = OsFileSystem;
    let contents = fs.read_to_string(&file_path).expect("file reads");
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
    fs.write(&file_path, b"test data").expect("file writes");

    let contents = fs::read_to_string(&file_path).expect("file reads");
    assert_eq!(contents, "test data");
}

#[test]
fn create_dir_all_creates_nested_directories() {
    let dir = TempDir::new("create_dir_all");
    let nested = dir.path().join("a/b/c");

    let fs = OsFileSystem;
    fs.create_dir_all(&nested).expect("dir creates");

    assert!(nested.is_dir());
}

#[test]
fn exists_returns_true_for_existing_path() {
    let dir = TempDir::new("exists");
    let file_path = dir.path().join("exists_test.txt");
    fs::write(&file_path, "").expect("file writes");

    let fs = OsFileSystem;
    assert!(fs.exists(&file_path));
    assert!(fs.exists(dir.path()));
    assert!(!fs.exists(&dir.path().join("nope")));
}

#[test]
fn is_file_distinguishes_files_from_dirs() {
    let dir = TempDir::new("is_file");
    let file_path = dir.path().join("a_file.txt");
    fs::write(&file_path, "").expect("file writes");

    let fs = OsFileSystem;
    assert!(fs.is_file(&file_path));
    assert!(!fs.is_file(dir.path()));
}

#[test]
fn is_dir_distinguishes_dirs_from_files() {
    let dir = TempDir::new("is_dir");
    let file_path = dir.path().join("a_file.txt");
    fs::write(&file_path, "").expect("file writes");

    let fs = OsFileSystem;
    assert!(fs.is_dir(dir.path()));
    assert!(!fs.is_dir(&file_path));
}

#[test]
fn read_dir_lists_immediate_children_sorted() {
    let dir = TempDir::new("read_dir");
    fs::write(dir.path().join("b.txt"), "").expect("file writes");
    fs::create_dir(dir.path().join("a_sub")).expect("dir creates");
    fs::create_dir(dir.path().join("a_sub/nested")).expect("dir creates");
    fs::write(dir.path().join("c.txt"), "").expect("file writes");

    let fs = OsFileSystem;
    let entries = fs.read_dir(dir.path()).expect("dir reads");

    assert_eq!(
        entries,
        vec![
            dir.path().join("a_sub"),
            dir.path().join("b.txt"),
            dir.path().join("c.txt"),
        ]
    );
}

#[test]
fn read_dir_returns_error_for_missing_dir() {
    let fs = OsFileSystem;
    let result = fs.read_dir(Path::new("/nonexistent/path"));
    assert!(result.is_err());
}

#[test]
fn rename_moves_file() {
    let dir = TempDir::new("rename");
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    fs::write(&src, "rename me").expect("file writes");

    let fs = OsFileSystem;
    fs.rename(&src, &dst).expect("path renames");

    assert!(!src.exists());
    assert_eq!(fs::read_to_string(&dst).expect("file reads"), "rename me");
}

#[test]
fn create_writer_writes_to_file() {
    use std::io::Write;

    let dir = TempDir::new("create_writer");
    let file_path = dir.path().join("writer_test.txt");

    let fs = OsFileSystem;
    let mut writer = fs.create_writer(&file_path).expect("writer creates");
    writer.write_all(b"via writer").expect("file writes");
    drop(writer);

    let contents = fs::read_to_string(&file_path).expect("file reads");
    assert_eq!(contents, "via writer");
}

#[test]
fn open_bufreader_reads_file() {
    use std::io::BufRead;

    let dir = TempDir::new("open_bufreader");
    let file_path = dir.path().join("bufreader_test.txt");
    fs::write(&file_path, "line1\nline2\n").expect("file writes");

    let fs = OsFileSystem;
    let reader = fs.open_bufreader(&file_path).expect("bufreader opens");

    let lines = reader
        .lines()
        .map(|l| l.expect("line reads"))
        .collect::<Vec<String>>();
    assert_eq!(lines, vec!["line1", "line2"]);
}

#[test]
fn metadata_returns_file_metadata() {
    let dir = TempDir::new("metadata");
    let file_path = dir.path().join("meta_test.txt");
    fs::write(&file_path, "some content").expect("file writes");

    let fs = OsFileSystem;
    let meta = fs.metadata(&file_path).expect("metadata reads");
    assert!(meta.is_file());
    assert_eq!(meta.len(), 12);
}

#[test]
fn metadata_returns_error_for_missing_path() {
    let fs = OsFileSystem;
    let result = fs.metadata(Path::new("/nonexistent/path"));
    assert!(result.is_err());
}

#[test]
fn read_tail_returns_the_whole_file_when_it_fits() {
    let dir = TempDir::new("read_tail_fits");
    let file_path = dir.path().join("log.txt");
    fs::write(&file_path, "line1\nline2\n").expect("file writes");

    let fs = OsFileSystem;
    assert_eq!(
        "line1\nline2\n",
        fs.read_tail_to_string(&file_path, 64)
            .expect("log tail reads")
    );
}

#[test]
fn read_tail_drops_a_partial_first_line_when_seeking() {
    let dir = TempDir::new("read_tail_partial");
    let file_path = dir.path().join("log.txt");
    // 14 bytes; an 8-byte window starts mid-"BBBB".
    fs::write(&file_path, "AAAA\nBBBB\nCCCC").expect("file writes");

    let fs = OsFileSystem;
    assert_eq!(
        "CCCC",
        fs.read_tail_to_string(&file_path, 8)
            .expect("log tail reads")
    );
}

#[test]
fn read_tail_keeps_a_window_with_no_newline() {
    let dir = TempDir::new("read_tail_nonewline");
    let file_path = dir.path().join("log.txt");
    fs::write(&file_path, "abcdefghijklmnopqrst").expect("file writes");

    let fs = OsFileSystem;
    assert_eq!(
        "mnopqrst",
        fs.read_tail_to_string(&file_path, 8)
            .expect("log tail reads")
    );
}

#[test]
fn read_tail_decodes_invalid_utf8_lossily() {
    let dir = TempDir::new("read_tail_utf8");
    let file_path = dir.path().join("log.txt");
    fs::write(&file_path, b"ok\n\xff\xfeworld").expect("file writes");

    let fs = OsFileSystem;
    let text = fs
        .read_tail_to_string(&file_path, 64)
        .expect("log tail reads");
    assert!(text.starts_with("ok\n"));
    assert!(text.contains('\u{FFFD}'));
    assert!(text.ends_with("world"));
}

#[test]
fn read_tail_errors_for_a_missing_file() {
    let fs = OsFileSystem;
    let result = fs.read_tail_to_string(Path::new("/nonexistent/path/log.txt"), 64);
    assert!(result.is_err());
}
