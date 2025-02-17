use actor_runtime_mocked::{add_file, clear_mocks};
use areader::AReader;
use std::io::Read;

#[test]
fn happy_path() {
    clear_mocks();

    add_file("test.0".to_string(), b"foo".to_vec());
    add_file("test.1".to_string(), b"bar".to_vec());
    add_file("test.2".to_string(), b"baz".to_vec());

    let mut reader = AReader::new(c"test").expect("Should create reader");
    let mut result = String::new();

    reader
        .read_to_string(&mut result)
        .expect("Should read all content");

    assert_eq!(result, "foobarbaz");
}
