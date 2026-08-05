pub mod telegram;
pub mod wecom;

use crate::inlet::Message;
use anyhow::Result;
use async_trait::async_trait;

/// 出口 trait，定义如何发送消息
#[async_trait]
pub trait Outlet: Send + Sync {
    /// 发送消息到目标平台
    async fn send(&self, message: &Message) -> Result<()>;

    /// 获取出口名称
    fn name(&self) -> &str;
}
