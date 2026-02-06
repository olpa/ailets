//! Integration tests for MemKV module

use ailetos::{KVBuffers, KVError, MemKV, OpenMode};

#[tokio::test]
async fn test_open_write_then_read() {
    let kv = MemKV::new();

    // Write creates a new buffer
    let buffer = kv.open("test/path", OpenMode::Write).await.unwrap();
    buffer.append(b"hello world").unwrap();

    // Read returns the same buffer
    let buffer2 = kv.open("test/path", OpenMode::Read).await.unwrap();
    let guard = buffer2.lock();
    assert_eq!(&*guard, b"hello world");
}

#[tokio::test]
async fn test_open_append() {
    let kv = MemKV::new();

    // Append creates new buffer if not exists
    let buffer = kv.open("test/path", OpenMode::Append).await.unwrap();
    buffer.append(b"first").unwrap();

    // Append returns existing buffer
    let buffer2 = kv.open("test/path", OpenMode::Append).await.unwrap();
    buffer2.append(b" second").unwrap();

    // Verify both writes are in the buffer
    let guard = buffer.lock();
    assert_eq!(&*guard, b"first second");
}

#[tokio::test]
async fn test_open_read_not_found() {
    let kv = MemKV::new();

    let result = kv.open("nonexistent", OpenMode::Read).await;
    assert!(result.is_err());
    match result {
        Err(KVError::NotFound(path)) => assert_eq!(path, "nonexistent"),
        _ => panic!("Expected NotFound error"),
    }
}

#[tokio::test]
async fn test_open_write_overwrites() {
    let kv = MemKV::new();

    // Write initial data
    let buffer = kv.open("test/path", OpenMode::Write).await.unwrap();
    buffer.append(b"initial data").unwrap();

    // Write again overwrites
    let buffer2 = kv.open("test/path", OpenMode::Write).await.unwrap();
    let guard = buffer2.lock();
    assert!(guard.is_empty(), "Write mode should create empty buffer");
}

#[tokio::test]
async fn test_listdir() {
    let kv = MemKV::new();

    // Create some paths
    kv.open("dir1/file1", OpenMode::Write).await.unwrap();
    kv.open("dir1/file2", OpenMode::Write).await.unwrap();
    kv.open("dir2/file1", OpenMode::Write).await.unwrap();

    let paths = kv.listdir("dir1/").await.unwrap();
    assert_eq!(paths, vec!["dir1/file1", "dir1/file2"]);
}

#[tokio::test]
async fn test_listdir_adds_slash() {
    let kv = MemKV::new();

    // Create some paths
    kv.open("dir1/file1", OpenMode::Write).await.unwrap();
    kv.open("dir1/file2", OpenMode::Write).await.unwrap();
    kv.open("dir11/file1", OpenMode::Write).await.unwrap();

    // listdir without trailing slash should still work correctly
    let paths = kv.listdir("dir1").await.unwrap();
    assert_eq!(paths, vec!["dir1/file1", "dir1/file2"]);

    // dir11 should not be included
    assert!(!paths.contains(&"dir11/file1".to_string()));
}

#[tokio::test]
async fn test_destroy() {
    let kv = MemKV::new();

    // Create some paths
    kv.open("path1", OpenMode::Write).await.unwrap();
    kv.open("path2", OpenMode::Write).await.unwrap();

    // Destroy clears all
    kv.destroy().await.unwrap();

    // Verify paths are gone
    assert!(kv.open("path1", OpenMode::Read).await.is_err());
    assert!(kv.open("path2", OpenMode::Read).await.is_err());
}
