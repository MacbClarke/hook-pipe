use super::Outlet;
use crate::inlet::Message;
use crate::util;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Telegram Bot 出口
pub struct TelegramOutlet {
    name: String,
    bot_token: String,
    chat_id: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct TelegramRequest<'a> {
    chat_id: &'a str,
    text: String,
    disable_web_page_preview: bool,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
    error_code: Option<i32>,
}

impl TelegramOutlet {
    pub fn new(name: String, bot_token: String, chat_id: String) -> Self {
        Self {
            name,
            bot_token,
            chat_id,
            client: Client::new(),
        }
    }

    fn endpoint(&self) -> String {
        format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token)
    }

    /// 构造发送到 Telegram 的纯文本消息
    fn format_message(message: &Message) -> String {
        format!(
            "{}\n\n{}\n\n来源: {} | 类型: {} | 时间: {}",
            message.title,
            message.content,
            message.metadata.source,
            message.metadata.message_type,
            util::format_timestamp_local(message.metadata.timestamp)
        )
    }
}

#[async_trait]
impl Outlet for TelegramOutlet {
    async fn send(&self, message: &Message) -> Result<()> {
        let payload = TelegramRequest {
            chat_id: &self.chat_id,
            text: Self::format_message(message),
            disable_web_page_preview: true,
        };

        let response = self
            .client
            .post(self.endpoint())
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("Failed to call Telegram Bot API for '{}'.", self.name))?;

        let status = response.status();
        let body: TelegramResponse = response
            .json()
            .await
            .with_context(|| format!("Failed to parse Telegram response for '{}'.", self.name))?;

        if !status.is_success() || !body.ok {
            let description = body
                .description
                .unwrap_or_else(|| "Unknown Telegram API error".to_string());
            let code_suffix = body
                .error_code
                .map(|code| format!(" (code: {})", code))
                .unwrap_or_default();
            anyhow::bail!(
                "Telegram API error for outlet '{}': {}{}",
                self.name,
                description,
                code_suffix
            );
        }

        tracing::info!(
            outlet = %self.name,
            message_title = %message.title,
            "Message sent to Telegram"
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
            "Deploy 完成".to_string(),
            "服务已成功部署".to_string(),
            "watchtower".to_string(),
            "deploy".to_string(),
        );

        let formatted = TelegramOutlet::format_message(&message);
        assert!(formatted.contains("Deploy 完成"));
        assert!(formatted.contains("服务已成功部署"));
        assert!(formatted.contains("watchtower"));
    }
}
