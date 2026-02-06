//! Integration tests for Buffer module

use ailetos::io::Buffer;

#[test]
fn test_new_buffer_is_empty() {
    let buffer = Buffer::new();
    assert!(buffer.is_empty());
    assert_eq!(buffer.len(), 0);
}

#[test]
fn test_append_and_read() {
    let buffer = Buffer::new();
    buffer.append(b"hello").unwrap();
    buffer.append(b" world").unwrap();

    let guard = buffer.lock();
    assert_eq!(&*guard, b"hello world");
}

#[test]
fn test_clone_shares_data() {
    let buffer1 = Buffer::new();
    let buffer2 = buffer1.clone();

    buffer1.append(b"from buffer1").unwrap();

    let guard = buffer2.lock();
    assert_eq!(&*guard, b"from buffer1");
}

#[test]
fn test_read_guard_deref() {
    let buffer = Buffer::new();
    buffer.append(b"test data").unwrap();

    let guard = buffer.lock();
    // Test Deref
    assert_eq!(guard.len(), 9);
    assert_eq!(&guard[0..4], b"test");
}

#[test]
fn test_read_guard_as_ref() {
    let buffer = Buffer::new();
    buffer.append(b"test").unwrap();

    let guard = buffer.lock();
    let slice: &[u8] = guard.as_ref();
    assert_eq!(slice, b"test");
}
