pub mod dagops;
pub mod fcw_chat;
pub mod fcw_tools;
pub mod fcw_trait;
pub mod funcalls_builder;
pub mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, StdHandle};
use dagops::{DagOps, DagOpsTrait};

use handlers::{
    on_begin_message, on_content, on_end_message, on_function_arguments, on_function_end,
    on_function_id, on_function_index, on_function_name, on_role,
};
use scan_json::RJiter;
use scan_json::{
    scan, iter_match, BoxedAction, BoxedEndAction, Options,
};
use scan_json::matcher::StructuralPseudoname;
use scan_json::stack::ContextIter;
use u8pool::U8Pool;
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

type BA<W, D> = BoxedAction<StructureBuilder<W, D>>;
type EA<W, D> = BoxedEndAction<StructureBuilder<W, D>>;

fn find_action<W: Write + 'static, D: DagOpsTrait + 'static>(
    structural_pseudoname: StructuralPseudoname,
    context: ContextIter,
) -> Option<BA<W, D>> {
    // Begin message
    if iter_match(|| ["message".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_begin_message));
    }

    // Role handlers (message.role or delta.role)
    if iter_match(|| ["role".as_bytes(), "message".as_bytes()], structural_pseudoname, context.clone()) ||
       iter_match(|| ["role".as_bytes(), "delta".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_role));
    }

    // Content handlers (message.content or delta.content)
    if iter_match(|| ["content".as_bytes(), "message".as_bytes()], structural_pseudoname, context.clone()) ||
       iter_match(|| ["content".as_bytes(), "delta".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_content));
    }

    // Tool call handlers
    if iter_match(|| ["id".as_bytes(), "function".as_bytes(), "#array".as_bytes(), "tool_calls".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_function_id));
    }
    if iter_match(|| ["name".as_bytes(), "function".as_bytes(), "#array".as_bytes(), "tool_calls".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_function_name));
    }
    if iter_match(|| ["arguments".as_bytes(), "function".as_bytes(), "#array".as_bytes(), "tool_calls".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_function_arguments));
    }
    if iter_match(|| ["index".as_bytes(), "#array".as_bytes(), "tool_calls".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_function_index));
    }

    None
}

fn find_end_action<W: Write + 'static, D: DagOpsTrait + 'static>(
    structural_pseudoname: StructuralPseudoname,
    context: ContextIter,
) -> Option<EA<W, D>> {
    // End message
    if iter_match(|| ["message".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_end_message));
    }

    // End tool call (tool_calls[].#object pattern)
    if iter_match(|| ["#object".as_bytes(), "#array".as_bytes(), "tool_calls".as_bytes()], structural_pseudoname, context.clone()) {
        return Some(Box::new(on_function_end));
    }

    None
}

/// # Errors
/// If anything goes wrong.
#[allow(clippy::used_underscore_items)]
pub fn _process_gpt<W: Write + 'static, D: DagOpsTrait + 'static>(
    mut reader: impl std::io::Read,
    stdout_writer: W,
    dagops: D,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder = StructureBuilder::new(stdout_writer, dagops);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));

    // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
    // Based on estimation: 16 bytes per JSON key, plus 8 bytes per frame for state tracking
    let mut working_buffer = [0u8; 512];
    let mut context = U8Pool::new(&mut working_buffer, 20)?;

    let sse_tokens: &[&[u8]] = &[b"data:", b"DONE"];

    scan(
        find_action::<W, D>,
        find_end_action::<W, D>,
        &rjiter_cell,
        &builder_cell,
        &mut context,
        &Options::with_sse_tokens(sse_tokens),
    )?;

    let mut builder = builder_cell.borrow_mut();
    builder.end_message()?;
    builder.end()?;

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
        return err_to_heap_c_string(extract_errno(&e), &format!("Failed to process GPT: {e}"));
    }
    std::ptr::null()
}
