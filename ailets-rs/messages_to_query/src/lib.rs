pub mod action_error;
pub mod env_opts;
mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, StdHandle};
use env_opts::EnvOpts;
use scan_json::matcher::StructuralPseudoname;
use scan_json::stack::ContextIter;
use scan_json::{iter_match, scan, Action, EndAction, Options, RJiter};
use std::cell::RefCell;
use std::ffi::c_char;
use structure_builder::StructureBuilder;
use u8pool::U8Pool;

const BUFFER_SIZE: u32 = 1024;

/// # Errors
/// If anything goes wrong.
#[allow(clippy::used_underscore_items)]
#[allow(clippy::too_many_lines)]
pub fn _process_messages<W: embedded_io::Write>(
    mut reader: impl embedded_io::Read,
    writer: W,
    env_opts: EnvOpts,
) -> Result<(), String> {
    let builder = StructureBuilder::new(writer, env_opts);
    let builder_cell = RefCell::new(builder);

    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let mut rjiter = RJiter::new(&mut reader, &mut buffer);

    let find_action = |structural_pseudoname: StructuralPseudoname,
                       context: ContextIter,
                       _baton: &RefCell<StructureBuilder<W>>|
     -> Option<Action<&RefCell<StructureBuilder<W>>, _>> {
        // Message boilerplate
        if iter_match(
            || ["role".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_role);
        }
        if iter_match(
            || ["#array".as_bytes(), "#top".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_item_begin);
        }
        if iter_match(
            || ["type".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_item_type);
        }

        // Content items
        if iter_match(
            || ["text".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_text);
        }
        if iter_match(
            || ["image_url".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_image_url);
        }
        if iter_match(
            || ["image_key".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_image_key);
        }
        if iter_match(
            || ["content_type".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_image_content_type);
        }
        if iter_match(
            || ["detail".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_image_detail);
        }
        if iter_match(
            || ["id".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_func_id);
        }
        if iter_match(
            || ["name".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_func_name);
        }
        if iter_match(
            || ["arguments".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_func_arguments);
        }
        if iter_match(
            || ["toolspec".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_toolspec);
        }
        if iter_match(
            || ["toolspec_key".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_toolspec_key);
        }
        if iter_match(
            || ["tool_call_id".as_bytes(), "#array".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_tool_call_id);
        }

        None
    };

    let find_end_action = |structural_pseudoname: StructuralPseudoname,
                           context: ContextIter,
                           _baton: &RefCell<StructureBuilder<W>>|
     -> Option<EndAction<&RefCell<StructureBuilder<W>>>> {
        if iter_match(
            || ["#array".as_bytes(), "#top".as_bytes()],
            structural_pseudoname,
            context.clone(),
        ) {
            return Some(handlers::on_item_end);
        }

        None
    };

    // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
    let mut working_buffer = [0u8; 512];
    let mut context = U8Pool::new(&mut working_buffer, 20)
        .map_err(|e| format!("Failed to create context pool: {e:?}"))?;

    let scan_result = scan(
        find_action,
        find_end_action,
        &mut rjiter,
        &builder_cell,
        &mut context,
        &Options::new(),
    );

    // Check if there's a detailed error in the baton before returning scan error
    if let Err(e) = scan_result {
        let mut builder = builder_cell.borrow_mut();
        if let Some(detailed_error) = builder.take_error() {
            return Err(detailed_error.to_string());
        }
        return Err(format!("Scan error: {e:?}"));
    }

    builder_cell.borrow_mut().end()?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn process_messages() -> *const c_char {
    let reader = AReader::new_from_std(StdHandle::Stdin);
    let writer = AWriter::new_from_std(StdHandle::Stdout);

    let env_reader = AReader::new_from_std(StdHandle::Env);
    let env_opts = match EnvOpts::envopts_from_reader(env_reader) {
        Ok(opts) => opts,
        Err(e) => return err_to_heap_c_string(1, &e),
    };

    #[allow(clippy::used_underscore_items)]
    if let Err(e) = _process_messages(reader, writer, env_opts) {
        return err_to_heap_c_string(1, &format!("Messages to query: {e}"));
    }
    std::ptr::null()
}
