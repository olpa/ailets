mod env_opts;
mod handlers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, StdHandle};
use env_opts::EnvOpts;
use scan_json::{
    scan, BoxedAction, BoxedEndAction, ParentAndName, ParentParentAndName, RJiter, Trigger,
};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

fn create_begin_triggers<'a, W: Write + 'a>(
) -> Vec<Trigger<'a, BoxedAction<'a, StructureBuilder<W>>>> {
    let content_text = Trigger::new(
        Box::new(ParentAndName::new("#array".to_string(), "text".to_string())),
        Box::new(handlers::on_content_text) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item_begin = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_content_item_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item_type = Trigger::new(
        Box::new(ParentAndName::new("#array".to_string(), "type".to_string())),
        Box::new(handlers::on_content_item_type) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_begin_arr = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_begin_jsonl = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let role_arr = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "role".to_string(),
        )),
        Box::new(handlers::on_role) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let role_jsonl = Trigger::new(
        Box::new(ParentAndName::new("#top".to_string(), "role".to_string())),
        Box::new(handlers::on_role) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let message_begin_arr = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let message_begin_jsonl = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );

    vec![
        content_text,
        content_item_begin,
        content_item_type,
        content_begin_arr,
        content_begin_jsonl,
        role_arr,
        role_jsonl,
        message_begin_arr,
        message_begin_jsonl,
    ]
}

fn create_end_triggers<'a, W: Write + 'a>(
) -> Vec<Trigger<'a, BoxedEndAction<'a, StructureBuilder<W>>>> {
    let message_end_arr = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let message_end_jsonl = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let content_end_arr = Trigger::new(
        Box::new(ParentParentAndName::new(
            "#top".to_string(),
            "#array".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let content_end_jsonl = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let content_item_end = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_content_item_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );

    vec![
        content_item_end,
        content_end_arr,
        content_end_jsonl,
        message_end_arr,
        message_end_jsonl,
    ]
}

/// # Errors
/// If anything goes wrong.
pub fn _process_query<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));
    let builder = StructureBuilder::new(writer);
    let builder_cell = RefCell::new(builder);

    let begin_triggers = create_begin_triggers();
    let end_triggers = create_end_triggers();

    scan(
        &begin_triggers,
        &end_triggers,
        &[],
        &rjiter_cell,
        &builder_cell,
    )?;
    builder_cell.borrow_mut().end()?;
    Ok(())
}

/// # Panics
/// If anything goes wrong.
#[no_mangle]
pub extern "C" fn process_query() -> *const c_char {
    let reader = AReader::new_from_std(StdHandle::Stdin);
    let writer = AWriter::new_from_std(StdHandle::Stdout);

    let env_reader = AReader::new_from_std(StdHandle::Env);
    let env_opts = match EnvOpts::envopts_from_reader(env_reader) {
        Ok(opts) => opts,
        Err(e) => {
            return err_to_heap_c_string(
                extract_errno(&e),
                &format!("Failed to read env opts: {e}"),
            )
        }
    };

    let mut debug_print = AWriter::new_from_std(StdHandle::Log);
    if let Err(e) = debug_print.write_all(format!("Env opts: {env_opts:?}").as_bytes()) {
        let msg = format!("Failed to write debug info: {e}");
        let boxed_error: Box<dyn std::error::Error> = Box::new(e);
        return err_to_heap_c_string(extract_errno(&boxed_error), &msg);
    }
    let _ = debug_print.write_all(format!("Env opts: {env_opts:?}").as_bytes());

    if let Err(e) = _process_query(reader, writer) {
        return err_to_heap_c_string(extract_errno(&e), &format!("Messages to query: {e}"));
    }
    std::ptr::null()
}
