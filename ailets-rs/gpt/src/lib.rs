pub mod dagops;
pub mod funcalls;
pub mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use dagops::{DagOpsTrait, DummyDagOps};
use handlers::{
    on_begin_message, on_choices, on_content, on_end_message, on_function_arguments,
    on_function_begin, on_function_id, on_function_index, on_function_name, on_role,
};
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, BoxedEndAction, ContextFrame, Name, ParentAndName, Trigger};
use std::cell::RefCell;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

type BA<'a, W> = BoxedAction<'a, StructureBuilder<W>>;

#[derive(Debug)]
struct MatchInToolCall {
    field: String,
}

impl scan_json::Matcher for MatchInToolCall {
    fn matches(&self, name: &str, context: &[ContextFrame]) -> bool {
        // Check the field name
        if name != self.field {
            return false;
        }

        // Check the "tool_calls" context
        for frame in context.iter().rev() {
            let key: &str = &frame.current_key;
            match key {
                "#object" | "#array" => continue,
                "function" => {
                    // match only top-level #object
                    if self.field == "#object" {
                        return false;
                    }
                    continue;
                }
                "tool_calls" => return true,
                _ => return false,
            }
        }
        false
    }
}

/// # Errors
/// If anything goes wrong.
pub fn _process_gpt<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
    dagops: &impl DagOpsTrait,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder = StructureBuilder::new(writer);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let begin_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_begin_message) as BA<'_, W>,
    );
    let end_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_end_message) as BoxedEndAction<StructureBuilder<W>>,
    );
    let choices = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "choices".to_string(),
        )),
        Box::new(on_choices) as BA<'_, W>,
    );
    let message_role = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "role".to_string(),
        )),
        Box::new(on_role) as BA<'_, W>,
    );
    let delta_role = Trigger::new(
        Box::new(ParentAndName::new("delta".to_string(), "role".to_string())),
        Box::new(on_role) as BA<'_, W>,
    );
    let message_content = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'_, W>,
    );
    let delta_content = Trigger::new(
        Box::new(ParentAndName::new(
            "delta".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'_, W>,
    );

    let function_begin = Trigger::new(
        Box::new(MatchInToolCall {
            field: "#object".to_string(),
        }),
        Box::new(on_function_begin) as BA<'_, W>,
    );
    let function_id = Trigger::new(
        Box::new(MatchInToolCall {
            field: "id".to_string(),
        }),
        Box::new(on_function_id) as BA<'_, W>,
    );
    let function_name = Trigger::new(
        Box::new(MatchInToolCall {
            field: "name".to_string(),
        }),
        Box::new(on_function_name) as BA<'_, W>,
    );
    let function_arguments = Trigger::new(
        Box::new(MatchInToolCall {
            field: "arguments".to_string(),
        }),
        Box::new(on_function_arguments) as BA<'_, W>,
    );
    let function_index = Trigger::new(
        Box::new(MatchInToolCall {
            field: "index".to_string(),
        }),
        Box::new(on_function_index) as BA<'_, W>,
    );

    let triggers = vec![
        begin_message,
        choices,
        message_role,
        message_content,
        delta_role,
        delta_content,
        function_begin,
        function_id,
        function_name,
        function_arguments,
        function_index,
    ];
    let triggers_end = vec![end_message];
    let sse_tokens = vec!["data:", "DONE"];

    scan(
        &triggers,
        &triggers_end,
        &sse_tokens,
        &rjiter_cell,
        &builder_cell,
    )?;
    let mut builder = builder_cell.borrow_mut();
    builder.end_message()?;

    let funcalls = builder.get_funcalls();
    dagops.inject_funcalls(funcalls)?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
#[allow(clippy::panic)]
pub extern "C" fn process_gpt() {
    let reader = AReader::new(c"").unwrap_or_else(|e| {
        panic!("Failed to create reader: {e:?}");
    });
    let writer = AWriter::new(c"").unwrap_or_else(|e| {
        panic!("Failed to create writer: {e:?}");
    });
    let dagops = DummyDagOps::new();
    _process_gpt(reader, writer, &dagops).unwrap_or_else(|e| {
        panic!("Failed to process GPT: {e:?}");
    });
}
