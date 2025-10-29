use super::Message;
use serde_json::Value;

/// 通用 HTTP webhook 处理
pub struct HttpInlet;

impl HttpInlet {
    /// 将通用 JSON payload 转换为消息
    pub fn to_message(payload: Value) -> Message {
        // 直接读取 title、content 和 type 字段
        let title = payload
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("HTTP Webhook")
            .to_string();

        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let source = payload
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("http")
            .to_string();

        let content_type = payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("generic")
            .to_string();

        Message::new(title, content, source, content_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_to_message_with_text() {
        let payload = json!({
            "title": "Test Title",
            "content": "This is a plain text message",
            "type": "text"
        });

        let message = HttpInlet::to_message(payload);
        assert_eq!(message.title, "Test Title");
        assert_eq!(message.content, "This is a plain text message");
    }

    #[test]
    fn test_to_message_with_markdown() {
        let payload = json!({
            "title": "Alert",
            "content": "# This is markdown\n\n**Bold text**",
            "type": "markdown"
        });

        let message = HttpInlet::to_message(payload);
        assert_eq!(message.title, "Alert");
        assert_eq!(message.content, "# This is markdown\n\n**Bold text**");
    }

    #[test]
    fn test_to_message_missing_fields() {
        let payload = json!({});

        let message = HttpInlet::to_message(payload);
        assert_eq!(message.title, "HTTP Webhook");
        assert_eq!(message.content, "");
    }

    #[test]
    fn test_to_message_default_type() {
        let payload = json!({
            "title": "No Type",
            "content": "Content without type field"
        });

        let message = HttpInlet::to_message(payload);
        assert_eq!(message.title, "No Type");
        assert_eq!(message.content, "Content without type field");
    }
}
