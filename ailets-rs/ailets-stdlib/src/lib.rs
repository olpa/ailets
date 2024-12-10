use ailets_types::ChatMessage;

#[no_mangle]
pub extern "C" fn messages_to_markdown(ptr: *const u8, len: usize) -> *const u8 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let input = std::str::from_utf8(slice).unwrap();
    
    // Parse the input JSON string to ChatMessage
    let messages: Vec<ChatMessage> = serde_json::from_str(input).unwrap();
    
    let mut markdown = String::new();
    
    for message in messages {
        markdown.push_str(&format!("**{}**:\n", message.role));
        
        for content in message.content {
            match content {
                ailets_types::ContentItem::Text { text } => {
                    markdown.push_str(&text);
                    markdown.push_str("\n\n");
                },
                ailets_types::ContentItem::Image { url, .. } => {
                    markdown.push_str(&format!("![image]({})\n\n", url));
                },
                ailets_types::ContentItem::Function { function } => {
                    markdown.push_str("```json\n");
                    markdown.push_str(&format!("{}: {}\n", 
                        function.name, 
                        function.arguments));
                    markdown.push_str("```\n\n");
                }
            }
        }
    }
    
    let result = markdown.into_bytes();
    let ptr = result.as_ptr();
    std::mem::forget(result); // Prevent deallocation
    ptr
}

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}
