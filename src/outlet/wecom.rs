use super::Outlet;
use crate::inlet::Message;
use crate::util;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// 企业微信群机器人出口
pub struct WecomOutlet {
    name: String,
    webhook_url: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct WecomRequest {
    msgtype: String,
    markdown_v2: WecomMarkdown,
}

#[derive(Debug, Serialize)]
struct WecomMarkdown {
    content: String,
}

#[derive(Debug, Deserialize)]
struct WecomResponse {
    errcode: i32,
    errmsg: String,
}

impl WecomOutlet {
    pub fn new(name: String, webhook_url: String) -> Self {
        Self {
            name,
            webhook_url,
            client: Client::new(),
        }
    }

    /// 将消息转换为企业微信 markdown 格式
    fn format_message(message: &Message) -> String {
        format!(
            "## {}\n\n{}\n\n---\n*Source: {} | Type: {} | Time: {}*",
            message.title,
            message.content,
            message.metadata.source,
            message.metadata.message_type,
            util::format_timestamp_local(message.metadata.timestamp)
        )
    }
}

#[async_trait]
impl Outlet for WecomOutlet {
    async fn send(&self, message: &Message) -> Result<()> {
        let content = Self::format_message(message);

        let request = WecomRequest {
            msgtype: "markdown_v2".to_string(),
            markdown_v2: WecomMarkdown { content },
        };

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to send message to WeCom webhook: {}", self.name))?;

        let status = response.status();
        let body: WecomResponse = response
            .json()
            .await
            .with_context(|| format!("Failed to parse WeCom response for outlet: {}", self.name))?;

        if body.errcode != 0 {
            anyhow::bail!(
                "WeCom API error for outlet '{}': {} (code: {})",
                self.name,
                body.errmsg,
                body.errcode
            );
        }

        if !status.is_success() {
            anyhow::bail!(
                "WeCom webhook returned error status {} for outlet '{}'",
                status,
                self.name
            );
        }

        tracing::info!(
            outlet = %self.name,
            message_title = %message.title,
            "Message sent successfully"
        );

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_message() {
        let message = Message::new(
            "Test Title".to_string(),
            "Test content".to_string(),
            "github".to_string(),
            "push".to_string(),
        );

        let formatted = WecomOutlet::format_message(&message);
        assert!(formatted.contains("Test Title"));
        assert!(formatted.contains("Test content"));
        assert!(formatted.contains("github"));
    }
}
