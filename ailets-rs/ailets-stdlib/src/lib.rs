use wasm_bindgen::prelude::*;
use ailets_types::ChatMessage;

#[wasm_bindgen(js_name = messages_to_markdown)]
pub fn messages_to_markdown(messages: &str) -> Result<String, JsValue> {
    // Parse the input JSON string to ChatMessage
    let messages: Vec<ChatMessage> = serde_json::from_str(messages)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse messages: {}", e)))?;

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
    
    Ok(markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_messages_to_markdown() {
        let input = r#"[
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "Hello"
                    }
                ]
            }
        ]"#;
        
        let result = messages_to_markdown(input).unwrap();
        assert!(result.contains("**user**"));
        assert!(result.contains("Hello"));
    }
}
