use actor_io::{error_kind_to_str, AReader};
use actor_runtime_mocked::VfsActorRuntime;
use embedded_io::Read;

/// Helper function to read all content from a reader into a Vec<u8>
fn read_to_end(reader: &mut AReader) -> Result<Vec<u8>, embedded_io::ErrorKind> {
    let mut result = Vec::new();
    let mut buf = [0u8; 1024];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        result.extend_from_slice(&buf[..n]);
    }

    Ok(result)
}

#[test]
fn happy_path() {
    let runtime = VfsActorRuntime::new();
    runtime.add_file("test".to_string(), b"foo".to_vec());

    let mut reader = AReader::new(&runtime, "test").expect("Should create reader");
    let result = read_to_end(&mut reader).expect("Should read all content");

    assert_eq!(result, b"foo");
}

#[test]
fn read_in_chunks() {
    let runtime = VfsActorRuntime::new();
    runtime.add_file(
        "chunks".to_string(),
        b"first\nchunk\nthird\nfourth\nfifth".to_vec(),
    );

    let mut reader = AReader::new(&runtime, "chunks").expect("Should create reader");
    let mut buf = [0u8; 10];

    // Read first chunk manually
    let n = reader.read(&mut buf).expect("Should read first chunk");
    assert_eq!(&buf[..n], b"first\n");

    // Read second chunk manually
    let n = reader.read(&mut buf).expect("Should read second chunk");
    assert_eq!(&buf[..n], b"chunk\n");

    // Read the rest
    let result = read_to_end(&mut reader).expect("Should read remaining content");

    assert_eq!(result, b"third\nfourth\nfifth");
}

#[test]
fn cant_open_nonexistent_file() {
    let runtime = VfsActorRuntime::new();
    let err = AReader::new(&runtime, "no-such-file").expect_err("Should fail to create reader");

    assert_eq!(
        err,
        embedded_io::ErrorKind::NotFound,
        "Error should be NotFound, got: {}",
        error_kind_to_str(err)
    );
}

#[test]
fn read_error() {
    let runtime = VfsActorRuntime::new();
    runtime.add_file(
        "fname-read-error".to_string(),
        vec![actor_runtime_mocked::WANT_ERROR as u8],
    );

    let mut reader = AReader::new(&runtime, "fname-read-error").expect("Should create reader");
    let mut buf = [0u8; 10];

    reader.read(&mut buf).expect_err("Should fail to read");
}
