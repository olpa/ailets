use messages_to_query::structure_builder::StructureBuilder;
use std::io::Cursor;

#[test]
fn can_create_structure_builder() {
    let writer = Cursor::new(Vec::new());
    let _builder = StructureBuilder::new(writer);
}
