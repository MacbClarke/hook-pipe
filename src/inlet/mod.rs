pub mod github;
pub mod http;

use serde::{Deserialize, Serialize};

/// 统一的内部消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息标题
    pub title: String,
    /// 消息内容（支持 markdown）
    pub content: String,
    /// 消息元数据
    pub metadata: MessageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// 消息来源
    pub source: String,
    /// 消息类型
    pub message_type: String,
    /// 时间戳
    pub timestamp: i64,
}

impl Message {
    pub fn new(title: String, content: String, source: String, message_type: String) -> Self {
        Self {
            title,
            content,
            metadata: MessageMetadata {
                source,
                message_type,
                timestamp: chrono::Utc::now().timestamp(),
            },
        }
    }
}
