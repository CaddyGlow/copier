use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use camino::Utf8PathBuf;
use copier::fs as copier_fs;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let mut base = env::temp_dir();
        let pid = std::process::id();
        for attempt in 0..1000 {
            base.push(format!("copier-fs-test-{}-{}", pid, attempt));
            if fs::create_dir(&base).is_ok() {
                return Self { path: base };
            }
            base.pop();
        }
        panic!("failed to create temp dir");
    }

    fn path(&self) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(self.path.clone()).expect("utf8 path")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_read_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("test.txt");
    fs::write(file_path.as_std_path(), b"hello world").unwrap();

    let result = copier_fs::read(&file_path);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), b"hello world");
}

#[test]
fn test_read_nonexistent_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("nonexistent.txt");

    let result = copier_fs::read(&file_path);
    assert!(result.is_err());
}

#[test]
fn test_read_to_string() {
    let temp = TempDir::new();
    let file_path = temp.path().join("test.txt");
    fs::write(file_path.as_std_path(), "hello world").unwrap();

    let result = copier_fs::read_to_string(&file_path);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello world");
}

#[test]
fn test_read_to_string_error_includes_path() {
    let temp = TempDir::new();
    let file_path = temp.path().join("missing.txt");

    let result = copier_fs::read_to_string(&file_path);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("missing.txt"));
}

#[test]
fn test_write_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("output.txt");

    let result = copier_fs::write(&file_path, b"test content");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "test content");
}

#[test]
fn test_write_creates_parent_directories() {
    let temp = TempDir::new();
    let file_path = temp.path().join("nested/deep/file.txt");

    let result = copier_fs::write(&file_path, b"nested content");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "nested content");
}

#[test]
fn test_write_string() {
    let temp = TempDir::new();
    let file_path = temp.path().join("string.txt");

    let result = copier_fs::write_string(&file_path, "string content");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "string content");
}

#[test]
fn test_append_to_new_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("append.txt");

    let result = copier_fs::append(&file_path, b"first");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "first");
}

#[test]
fn test_append_to_existing_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("append.txt");

    fs::write(file_path.as_std_path(), b"first\n").unwrap();

    let result = copier_fs::append(&file_path, b"second\n");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "first\nsecond\n");
}

#[test]
fn test_append_creates_parent_directories() {
    let temp = TempDir::new();
    let file_path = temp.path().join("nested/append.txt");

    let result = copier_fs::append(&file_path, b"content");
    assert!(result.is_ok());

    assert!(file_path.exists());
}

#[test]
fn test_create_dir_all() {
    let temp = TempDir::new();
    let dir_path = temp.path().join("a/b/c");

    let result = copier_fs::create_dir_all(&dir_path);
    assert!(result.is_ok());
    assert!(dir_path.exists());
}

#[test]
fn test_ensure_parent() {
    let temp = TempDir::new();
    let file_path = temp.path().join("dir/subdir/file.txt");

    let result = copier_fs::ensure_parent(&file_path);
    assert!(result.is_ok());

    let parent = file_path.parent().unwrap();
    assert!(parent.exists());
}

#[test]
fn test_ensure_parent_no_parent() {
    let result = copier_fs::ensure_parent(Utf8PathBuf::from("/").as_path());
    assert!(result.is_ok());
}

#[test]
fn test_canonicalize() {
    let temp = TempDir::new();
    let file_path = temp.path().join("test.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    let result = copier_fs::canonicalize(&file_path);
    assert!(result.is_ok());

    let canonical = result.unwrap();
    assert!(canonical.is_absolute());
}

#[test]
fn test_metadata() {
    let temp = TempDir::new();
    let file_path = temp.path().join("test.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    let result = copier_fs::metadata(&file_path);
    assert!(result.is_ok());

    let metadata = result.unwrap();
    assert!(metadata.is_file());
}

#[test]
fn test_file_exists_true() {
    let temp = TempDir::new();
    let file_path = temp.path().join("exists.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    assert!(copier_fs::file_exists(&file_path));
}

#[test]
fn test_file_exists_false() {
    let temp = TempDir::new();
    let file_path = temp.path().join("missing.txt");

    assert!(!copier_fs::file_exists(&file_path));
}

#[test]
fn test_open_read() {
    let temp = TempDir::new();
    let file_path = temp.path().join("read.txt");
    fs::write(file_path.as_std_path(), "content").unwrap();

    let result = copier_fs::open_read(&file_path);
    assert!(result.is_ok());
}

#[test]
fn test_open_write() {
    let temp = TempDir::new();
    let file_path = temp.path().join("write.txt");

    let result = copier_fs::open_write(&file_path);
    assert!(result.is_ok());

    // File should be created
    assert!(file_path.exists());
}

#[test]
fn test_open_write_creates_parent_directories() {
    let temp = TempDir::new();
    let file_path = temp.path().join("nested/write.txt");

    let result = copier_fs::open_write(&file_path);
    assert!(result.is_ok());

    assert!(file_path.exists());
}

#[test]
fn test_copy_permissions() {
    let temp = TempDir::new();
    let source_path = temp.path().join("source.txt");
    let dest_path = temp.path().join("dest.txt");

    fs::write(source_path.as_std_path(), "source").unwrap();
    fs::write(dest_path.as_std_path(), "dest").unwrap();

    let result = copier_fs::copy_permissions(&source_path, &dest_path);
    assert!(result.is_ok());
}

#[test]
fn test_read_permissions() {
    let temp = TempDir::new();
    let file_path = temp.path().join("perms.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    let result = copier_fs::read_permissions(&file_path);
    assert!(result.is_ok());
}

#[test]
fn test_set_permissions() {
    let temp = TempDir::new();
    let file_path = temp.path().join("perms.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    let perms = fs::metadata(file_path.as_std_path()).unwrap().permissions();
    let result = copier_fs::set_permissions(&file_path, perms);
    assert!(result.is_ok());
}

#[test]
fn test_remove_file() {
    let temp = TempDir::new();
    let file_path = temp.path().join("remove.txt");
    fs::write(file_path.as_std_path(), "test").unwrap();

    assert!(file_path.exists());

    let result = copier_fs::remove_file(&file_path);
    assert!(result.is_ok());
    assert!(!file_path.exists());
}

#[test]
fn test_remove_dir_all() {
    let temp = TempDir::new();
    let dir_path = temp.path().join("remove_dir");
    fs::create_dir(dir_path.as_std_path()).unwrap();
    fs::write(dir_path.join("file.txt").as_std_path(), "test").unwrap();

    assert!(dir_path.exists());

    let result = copier_fs::remove_dir_all(&dir_path);
    assert!(result.is_ok());
    assert!(!dir_path.exists());
}

#[test]
fn test_read_to_end() {
    let temp = TempDir::new();
    let file_path = temp.path().join("read_end.txt");
    fs::write(file_path.as_std_path(), b"binary data").unwrap();

    let mut file = fs::File::open(file_path.as_std_path()).unwrap();
    let result = copier_fs::read_to_end(&mut file);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), b"binary data");
}

#[test]
fn test_write_all() {
    let temp = TempDir::new();
    let file_path = temp.path().join("write_all.txt");

    let mut file = fs::File::create(file_path.as_std_path()).unwrap();
    let result = copier_fs::write_all(&mut file, b"data");
    assert!(result.is_ok());

    let content = fs::read_to_string(file_path.as_std_path()).unwrap();
    assert_eq!(content, "data");
}

#[test]
fn test_flush() {
    let temp = TempDir::new();
    let file_path = temp.path().join("flush.txt");

    let mut file = fs::File::create(file_path.as_std_path()).unwrap();
    file.write_all(b"data").unwrap();

    let result = copier_fs::flush(&mut file);
    assert!(result.is_ok());
}

#[test]
fn test_rename() {
    let temp = TempDir::new();
    let old_path = temp.path().join("old.txt");
    let new_path = temp.path().join("new.txt");

    fs::write(old_path.as_std_path(), "content").unwrap();

    let result = copier_fs::rename(&old_path, &new_path);
    assert!(result.is_ok());

    assert!(!old_path.exists());
    assert!(new_path.exists());

    let content = fs::read_to_string(new_path.as_std_path()).unwrap();
    assert_eq!(content, "content");
}

#[test]
fn test_rename_creates_parent_directories() {
    let temp = TempDir::new();
    let old_path = temp.path().join("old.txt");
    let new_path = temp.path().join("nested/new.txt");

    fs::write(old_path.as_std_path(), "content").unwrap();

    let result = copier_fs::rename(&old_path, &new_path);
    assert!(result.is_ok());

    assert!(new_path.exists());
}

#[test]
fn test_read_dir() {
    let temp = TempDir::new();
    fs::write(temp.path().join("file1.txt").as_std_path(), "test").unwrap();
    fs::write(temp.path().join("file2.txt").as_std_path(), "test").unwrap();

    let result = copier_fs::read_dir(&temp.path());
    assert!(result.is_ok());

    let entries: Vec<_> = result.unwrap().collect();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_read_dir_empty() {
    let temp = TempDir::new();

    let result = copier_fs::read_dir(&temp.path());
    assert!(result.is_ok());

    let entries: Vec<_> = result.unwrap().collect();
    assert_eq!(entries.len(), 0);
}
