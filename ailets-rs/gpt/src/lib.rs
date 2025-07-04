pub mod dagops;
pub mod funcalls;
pub mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, DagOps, StdHandle};
use dagops::{InjectDagOps, InjectDagOpsTrait};
use handlers::{
    on_begin_message, on_choices, on_content, on_end_message, on_function_arguments,
    on_function_begin, on_function_id, on_function_index, on_function_name, on_role,
};
use scan_json::RJiter;
use scan_json::{scan, BoxedAction, BoxedEndAction, ContextFrame, Name, ParentAndName, Trigger};
use std::cell::RefCell;
use std::ffi::c_char;
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

fn make_triggers<'a, W: Write + 'a>() -> Vec<Trigger<'a, BA<'a, W>>> {
    let begin_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_begin_message) as BA<'a, W>,
    );
    let choices = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "choices".to_string(),
        )),
        Box::new(on_choices) as BA<'a, W>,
    );
    let message_role = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "role".to_string(),
        )),
        Box::new(on_role) as BA<'a, W>,
    );
    let delta_role = Trigger::new(
        Box::new(ParentAndName::new("delta".to_string(), "role".to_string())),
        Box::new(on_role) as BA<'a, W>,
    );
    let message_content = Trigger::new(
        Box::new(ParentAndName::new(
            "message".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'a, W>,
    );
    let delta_content = Trigger::new(
        Box::new(ParentAndName::new(
            "delta".to_string(),
            "content".to_string(),
        )),
        Box::new(on_content) as BA<'a, W>,
    );

    let function_begin = Trigger::new(
        Box::new(MatchInToolCall {
            field: "#object".to_string(),
        }),
        Box::new(on_function_begin) as BA<'a, W>,
    );
    let function_id = Trigger::new(
        Box::new(MatchInToolCall {
            field: "id".to_string(),
        }),
        Box::new(on_function_id) as BA<'a, W>,
    );
    let function_name = Trigger::new(
        Box::new(MatchInToolCall {
            field: "name".to_string(),
        }),
        Box::new(on_function_name) as BA<'a, W>,
    );
    let function_arguments = Trigger::new(
        Box::new(MatchInToolCall {
            field: "arguments".to_string(),
        }),
        Box::new(on_function_arguments) as BA<'a, W>,
    );
    let function_index = Trigger::new(
        Box::new(MatchInToolCall {
            field: "index".to_string(),
        }),
        Box::new(on_function_index) as BA<'a, W>,
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

    triggers
}

/// # Errors
/// If anything goes wrong.
pub fn _process_gpt<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
    dagops: &mut impl InjectDagOpsTrait,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder = StructureBuilder::new(writer);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];

    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    let end_message = Trigger::new(
        Box::new(Name::new("message".to_string())),
        Box::new(on_end_message) as BoxedEndAction<StructureBuilder<W>>,
    );
    let triggers = make_triggers::<W>();
    let triggers_end = vec![end_message];
    let sse_tokens = vec![String::from("data:"), String::from("DONE")];

    scan(
        &triggers,
        &triggers_end,
        &rjiter_cell,
        &builder_cell,
        &scan_json::Options {
            sse_tokens,
            stop_early: false,
        },
    )?;
    let mut builder = builder_cell.borrow_mut();
    builder.end_message()?;

    let funcalls = builder.get_funcalls();
    dagops.inject_tool_calls(funcalls.get_tool_calls())?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn process_gpt() -> *const c_char {
    let reader = AReader::new_from_std(StdHandle::Stdin);
    let writer = AWriter::new_from_std(StdHandle::Stdout);

    let mut dagops = InjectDagOps::new(DagOps {});
    if let Err(e) = _process_gpt(reader, writer, &mut dagops) {
        return err_to_heap_c_string(extract_errno(&e), &format!("Failed to process GPT: {e}"));
    }
    std::ptr::null()
}
