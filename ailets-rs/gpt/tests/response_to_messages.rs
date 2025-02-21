use gpt::_process_gpt;
use std::io::Cursor;
use std::io::Result;
use std::rc::Rc;
use std::io::Write;
use std::cell::RefCell;

fn get_expected_basic_message() -> String {
    "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\
    \"Hello! How can I assist you today?\"}]}\n"
        .to_string()
}

#[derive(Clone)]
struct RcWriter {
    inner: Rc<RefCell<Vec<u8>>>,
}

impl Write for RcWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.inner.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.borrow_mut().flush()
    }
}

impl RcWriter {
    fn new() -> Self {
        RcWriter {
            inner: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.inner.borrow()).to_string()
    }
}


#[test]
fn test_basic_processing() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_response.txt")
        .expect("Failed to read fixture file 'basic_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone());

    let result = writer.get_output();
    assert_eq!(result, get_expected_basic_message());
}

#[test]
fn test_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone());

    let result = writer.get_output();
    assert_eq!(result, get_expected_basic_message());
}
