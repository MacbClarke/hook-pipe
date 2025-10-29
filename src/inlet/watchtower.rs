use super::Message;
use serde_json::Value;

/// Watchtower webhook inlet
pub struct WatchtowerInlet;

impl WatchtowerInlet {
    /// 将 Watchtower webhook payload 转换为统一的消息格式
    ///
    /// Watchtower 使用 shoutrrr generic webhook，默认格式为:
    /// ```json
    /// {
    ///   "title": "...",
    ///   "message": "..."
    /// }
    /// ```
    ///
    /// 但用户可以通过配置自定义字段名和添加额外字段
    pub fn to_message(payload: Value) -> Message {
        // 尝试提取 title，支持多种可能的字段名
        let title = Self::extract_string_field(
            &payload,
            &["title", "Title", "subject", "Subject"],
        )
        .unwrap_or_else(|| "Watchtower Notification".to_string());

        // 尝试提取 message/content
        let message_text = Self::extract_string_field(
            &payload,
            &["message", "Message", "text", "Text", "body", "Body", "content", "Content"],
        )
        .unwrap_or_else(|| "No message provided".to_string());

        // 构建内容，将 message 作为主要内容
        let mut content = message_text.clone();

        // 如果 payload 中有其他字段，将它们也包含进来
        if let Some(obj) = payload.as_object() {
            let known_fields = [
                "title", "Title", "message", "Message", "text", "Text",
                "body", "Body", "content", "Content", "subject", "Subject",
            ];

            let extra_fields: Vec<_> = obj
                .iter()
                .filter(|(k, _)| !known_fields.contains(&k.as_str()))
                .collect();

            if !extra_fields.is_empty() {
                content.push_str("\n\n**Additional Information:**\n");
                for (key, value) in extra_fields {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => serde_json::to_string_pretty(value).unwrap_or_default(),
                    };
                    content.push_str(&format!("- **{}**: {}\n", key, value_str));
                }
            }
        }

        Message::new(
            title,
            content,
            "watchtower".to_string(),
            "notification".to_string(),
        )
    }

    /// 从 JSON payload 中提取字符串字段，尝试多个可能的字段名
    fn extract_string_field(payload: &Value, field_names: &[&str]) -> Option<String> {
        for field_name in field_names {
            if let Some(value) = payload.get(field_name) {
                if let Some(s) = value.as_str() {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchtower_default_format() {
        let payload = serde_json::json!({
            "title": "Container updated",
            "message": "my-app has been updated to latest version"
        });

        let message = WatchtowerInlet::to_message(payload);

        assert_eq!(message.metadata.source, "watchtower");
        assert_eq!(message.title, "Container updated");
        assert!(message.content.contains("my-app has been updated"));
    }

    #[test]
    fn test_watchtower_with_extra_fields() {
        let payload = serde_json::json!({
            "title": "Update notification",
            "message": "Containers have been updated",
            "hostname": "server-01",
            "count": 3
        });

        let message = WatchtowerInlet::to_message(payload);

        assert_eq!(message.title, "Update notification");
        assert!(message.content.contains("Containers have been updated"));
        assert!(message.content.contains("hostname"));
        assert!(message.content.contains("server-01"));
        assert!(message.content.contains("count"));
    }

    #[test]
    fn test_watchtower_minimal() {
        let payload = serde_json::json!({});

        let message = WatchtowerInlet::to_message(payload);

        assert_eq!(message.metadata.source, "watchtower");
        assert_eq!(message.title, "Watchtower Notification");
        assert_eq!(message.content, "No message provided");
    }

    #[test]
    fn test_watchtower_custom_field_names() {
        let payload = serde_json::json!({
            "subject": "System alert",
            "body": "This is the notification body"
        });

        let message = WatchtowerInlet::to_message(payload);

        assert_eq!(message.title, "System alert");
        assert!(message.content.contains("This is the notification body"));
    }
}
