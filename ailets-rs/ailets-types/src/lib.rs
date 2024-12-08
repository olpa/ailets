use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Vec<ContentItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { url: String, content_type: String },
    #[serde(rename = "function")]
    Function { function: FunctionCall },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_chat_message() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: vec![ContentItem::Text {
                text: "Hello".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.is_empty());
    }
}
