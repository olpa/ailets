pub mod env_opts;
mod handlers;
pub mod matchers;
pub mod structure_builder;

use actor_io::{AReader, AWriter};
use actor_runtime::{err_to_heap_c_string, extract_errno, StdHandle};
use env_opts::EnvOpts;
use scan_json::{scan, BoxedAction, BoxedEndAction, ParentAndName, ParentParentAndName, RJiter, Trigger};
use std::cell::RefCell;
use std::ffi::c_char;
use std::io::Write;
use structure_builder::StructureBuilder;

const BUFFER_SIZE: u32 = 1024;

fn create_begin_triggers<'a, W: Write + 'a>(
) -> Vec<Trigger<'a, BoxedAction<'a, StructureBuilder<W>>>> {
    //
    // Message boilerplate
    //
    let message = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let role = Trigger::new(
        Box::new(ParentAndName::new("#top".to_string(), "role".to_string())),
        Box::new(handlers::on_role) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "#array".to_string(),
        )),
        Box::new(handlers::on_content_item_begin) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item_type = Trigger::new(
        Box::new(ParentAndName::new("#array".to_string(), "type".to_string())),
        Box::new(handlers::on_content_item_type) as BoxedAction<'_, StructureBuilder<W>>,
    );

    //
    // Content items
    //
    let content_text = Trigger::new(
        Box::new(ParentAndName::new("#array".to_string(), "text".to_string())),
        Box::new(handlers::on_content_text) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_image_url = Trigger::new(
        Box::new(ParentAndName::new(
            "#array".to_string(),
            "image_url".to_string(),
        )),
        Box::new(handlers::on_content_image_url) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_image_key = Trigger::new(
        Box::new(ParentAndName::new(
            "#array".to_string(),
            "image_key".to_string(),
        )),
        Box::new(handlers::on_content_image_key) as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item_attribute_content_type = Trigger::new(
        Box::new(ParentAndName::new(
            "#array".to_string(),
            "content_type".to_string(),
        )),
        Box::new(handlers::on_content_item_attribute_content_type)
            as BoxedAction<'_, StructureBuilder<W>>,
    );
    let content_item_attribute_detail = Trigger::new(
        Box::new(ParentAndName::new(
            "#array".to_string(),
            "detail".to_string(),
        )),
        Box::new(handlers::on_content_item_attribute_detail)
            as BoxedAction<'_, StructureBuilder<W>>,
    );

    vec![
        content_item_type,
        content_text,
        content_image_url,
        content_image_key,
        content_item,
        content_item_attribute_content_type,
        content_item_attribute_detail,
        content,
        role,
        message,
    ]
}

fn create_end_triggers<'a, W: Write + 'a>(
) -> Vec<Trigger<'a, BoxedEndAction<'a, StructureBuilder<W>>>> {
    let message = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "#object".to_string(),
        )),
        Box::new(handlers::on_message_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let content = Trigger::new(
        Box::new(ParentAndName::new(
            "#top".to_string(),
            "content".to_string(),
        )),
        Box::new(handlers::on_content_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );
    let content_item = Trigger::new(
        Box::new(ParentParentAndName::new(
            "content".to_string(),
            "#array".to_string(),
            "#array".to_string(),
        )),
        Box::new(handlers::on_content_item_end) as BoxedEndAction<'_, StructureBuilder<W>>,
    );

    vec![content_item, content, message]
}

/// # Errors
/// If anything goes wrong.
pub fn _process_query<W: Write>(
    mut reader: impl std::io::Read,
    writer: W,
    env_opts: EnvOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; BUFFER_SIZE as usize];
    let rjiter_cell = RefCell::new(RJiter::new(&mut reader, &mut buffer));
    let builder = StructureBuilder::new(writer, env_opts);
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

    if let Err(e) = _process_query(reader, writer, env_opts) {
        return err_to_heap_c_string(extract_errno(&e), &format!("Messages to query: {e}"));
    }
    std::ptr::null()
}
