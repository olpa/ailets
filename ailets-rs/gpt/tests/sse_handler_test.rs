use std::cell::RefCell;
use std::io;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output, set_input};

use gpt::awriter::AWriter;
use gpt::rjiter::RJiter;
use gpt::sse_handler::{on_begin_delta, on_end_delta, on_delta_role, SSEHandler};

#[test]
fn basic_pass() {
    // Arrange
    clear_mocks();
    let input = r#""assistant" "hello""#;
    let mut rjiter = RJiter::new(io::Cursor::new(input));
    let awriter = AWriter::new("");
    let mut handler = SSEHandler::new(awriter);

    // Act
    on_begin_delta(&RefCell::new(rjiter), &RefCell::new(handler));
    on_delta_role(&RefCell::new(rjiter), &RefCell::new(handler));
    on_delta_content(&RefCell::new(rjiter), &RefCell::new(handler));
    on_end_delta(&RefCell::new(rjiter), &RefCell::new(handler));
    handler.end();

    // Assert
    assert_eq!(
        get_output(),
        r#"{"role":"assistant","content":[{"type":"text","text":"hello"}]}"#
    );
}
