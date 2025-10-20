pub mod action_error;
pub mod dagops;
pub mod fcw_chat;
pub mod fcw_tools;
pub mod fcw_trait;
pub mod funcalls_builder;
pub mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, StdHandle};
use dagops::{DagOps, DagOpsTrait};

use handlers::{
    on_begin_message, on_content, on_end_message, on_function_arguments, on_function_end,
    on_function_id, on_function_index, on_function_name, on_role,
};
use scan_json::matcher::StructuralPseudoname;
use scan_json::stack::ContextIter;
use scan_json::RJiter;
use scan_json::{iter_match, scan, Action, EndAction, Options};
use std::cell::RefCell;
use std::ffi::c_char;
use structure_builder::StructureBuilder;
use u8pool::U8Pool;

const BUFFER_SIZE: u32 = 1024;

/// # Errors
/// If anything goes wrong.
#[allow(clippy::used_underscore_items)]
#[allow(clippy::too_many_lines)]
pub fn _process_gpt<W: embedded_io::Write + 'static, D: DagOpsTrait + 'static>(
    mut reader: impl embedded_io::Read,
    stdout_writer: W,
    dagops: D,
) -> Result<(), String> {
    let builder = StructureBuilder::new(stdout_writer, dagops);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let find_action = |structural_pseudoname: StructuralPseudoname,
                       context: ContextIter,
                       _baton: &RefCell<StructureBuilder<W, D>>|
     -> Option<Action<&RefCell<StructureBuilder<W, D>>, _>> {
        // Begin message
        if iter_match(
            || ["message".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_begin_message);
        }

        // Role handlers (message.role or delta.role)
        if iter_match(
            || ["role".as_bytes(), "message".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) || iter_match(
            || ["role".as_bytes(), "delta".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_role);
        }

        // Content handlers (message.content or delta.content)
        if iter_match(
            || ["content".as_bytes(), "message".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) || iter_match(
            || ["content".as_bytes(), "delta".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_content);
        }

        // Tool call handlers
        if iter_match(
            || {
                [
                    "id".as_bytes(),
                    "#array".as_bytes(),
                    "tool_calls".as_bytes(),
                ]
            },
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_function_id);
        }
        if iter_match(
            || {
                [
                    "name".as_bytes(),
                    "function".as_bytes(),
                    "#array".as_bytes(),
                    "tool_calls".as_bytes(),
                ]
            },
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_function_name);
        }
        if iter_match(
            || {
                [
                    "arguments".as_bytes(),
                    "function".as_bytes(),
                    "#array".as_bytes(),
                    "tool_calls".as_bytes(),
                ]
            },
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_function_arguments);
        }
        if iter_match(
            || {
                [
                    "index".as_bytes(),
                    "#array".as_bytes(),
                    "tool_calls".as_bytes(),
                ]
            },
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_function_index);
        }

        None
    };

    let find_end_action = |structural_pseudoname: StructuralPseudoname,
                           context: ContextIter,
                           _baton: &RefCell<StructureBuilder<W, D>>|
     -> Option<EndAction<&RefCell<StructureBuilder<W, D>>>> {
        // End message
        if iter_match(
            || ["message".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_end_message);
        }

        // End tool call (tool_calls[].#object pattern)
        if iter_match(
            || {
                [
                    "#object".as_bytes(),
                    "#array".as_bytes(),
                    "tool_calls".as_bytes(),
                ]
            },
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(on_function_end);
        }

        None
    };

    // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
    // Based on estimation: 16 bytes per JSON key, plus 8 bytes per frame for state tracking
    let mut working_buffer = [0u8; 512];
    let mut context = U8Pool::new(&mut working_buffer, 20)
        .map_err(|e| format!("Failed to create context pool: {e:?}"))?;

    let sse_tokens: &[&[u8]] = &[b"data:", b"DONE"];

    let scan_result = scan(
        find_action,
        find_end_action,
        &mut rjiter,
        &builder_cell,
        &mut context,
        &Options::with_sse_tokens(sse_tokens),
    );

    // Check if there's a detailed error in the baton before returning scan error
    if let Err(e) = scan_result {
        let mut builder = builder_cell.borrow_mut();
        if let Some(detailed_error) = builder.take_error() {
            return Err(detailed_error.to_string());
        }
        return Err(format!("Scan error: {e:?}"));
    }

    let mut builder = builder_cell.borrow_mut();
    builder.end_message().map_err(|e| format!("{e:?}"))?;
    builder.end().map_err(|e| e.to_string())?;

    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn process_gpt() -> *const c_char {
    let reader = AReader::new_from_std(StdHandle::Stdin);
    let writer = AWriter::new_from_std(StdHandle::Stdout);

    let dagops = DagOps::new();
    #[allow(clippy::used_underscore_items)]
    if let Err(e) = _process_gpt(reader, writer, dagops) {
        return err_to_heap_c_string(1, &format!("Failed to process GPT: {e}"));
    }
    std::ptr::null()
}
