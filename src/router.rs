use crate::config::{Config, OutletConfig, OutletType};
use crate::inlet::Message;
use crate::outlet::Outlet;
use crate::outlet::telegram::TelegramOutlet;
use crate::outlet::wecom::WecomOutlet;
use crate::retry::RetryPolicy;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// 消息路由器
pub struct Router {
    outlets: HashMap<String, Arc<dyn Outlet>>,
    routes: HashMap<String, Vec<String>>,
    retry_policy: RetryPolicy,
}

impl Router {
    /// 从配置创建路由器
    pub fn from_config(config: &Config) -> Result<Self> {
        let mut outlets: HashMap<String, Arc<dyn Outlet>> = HashMap::new();

        // 创建所有出口
        for outlet_config in &config.outlets {
            let outlet: Arc<dyn Outlet> = Self::create_outlet(outlet_config)?;
            outlets.insert(outlet_config.name.clone(), outlet);
        }

        let retry_policy =
            RetryPolicy::new(config.retry.max_attempts, config.retry.initial_delay_ms);

        Ok(Self {
            outlets,
            routes: config.routes.clone(),
            retry_policy,
        })
    }

    /// 创建出口实例
    fn create_outlet(config: &OutletConfig) -> Result<Arc<dyn Outlet>> {
        match config.outlet_type {
            OutletType::Wecom => {
                let webhook_url = config
                    .webhook_url
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!("Outlet '{}' missing webhook_url for WeCom", config.name)
                    })?
                    .clone();
                let outlet = WecomOutlet::new(config.name.clone(), webhook_url);
                Ok(Arc::new(outlet))
            }
            OutletType::Telegram => {
                let bot_token = config
                    .bot_token
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!("Outlet '{}' missing bot_token for Telegram", config.name)
                    })?
                    .clone();

                let chat_id = config
                    .chat_id
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!("Outlet '{}' missing chat_id for Telegram", config.name)
                    })?
                    .clone();

                let outlet = TelegramOutlet::new(config.name.clone(), bot_token, chat_id);
                Ok(Arc::new(outlet))
            }
        }
    }

    /// 路由消息到对应的出口
    pub async fn route(&self, inlet_name: &str, message: Message) -> Result<()> {
        // 查找该入口对应的出口列表
        let outlet_names = self
            .routes
            .get(inlet_name)
            .ok_or_else(|| anyhow::anyhow!("No route configured for inlet: {}", inlet_name))?;

        if outlet_names.is_empty() {
            tracing::warn!(
                inlet = %inlet_name,
                "No outlets configured for this inlet"
            );
            return Ok(());
        }

        tracing::info!(
            inlet = %inlet_name,
            outlets = ?outlet_names,
            message_title = %message.title,
            "Routing message"
        );

        // 并发发送到所有出口
        let mut tasks = Vec::new();

        for outlet_name in outlet_names {
            let outlet = self
                .outlets
                .get(outlet_name)
                .ok_or_else(|| anyhow::anyhow!("Outlet '{}' not found", outlet_name))?;

            let outlet = Arc::clone(outlet);
            let message = message.clone();
            let retry_policy = self.retry_policy.clone();

            // 为每个出口创建一个异步任务
            let task = tokio::spawn(async move {
                retry_policy
                    .execute(|| async { outlet.send(&message).await })
                    .await
            });

            tasks.push((outlet_name.clone(), task));
        }

        // 等待所有任务完成并收集结果
        let mut errors = Vec::new();
        for (outlet_name, task) in tasks {
            match task.await {
                Ok(Ok(())) => {
                    tracing::debug!(
                        outlet = %outlet_name,
                        "Message delivered successfully"
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        outlet = %outlet_name,
                        error = %e,
                        "Failed to deliver message to outlet"
                    );
                    errors.push(format!("{}: {}", outlet_name, e));
                }
                Err(e) => {
                    tracing::error!(
                        outlet = %outlet_name,
                        error = %e,
                        "Task panicked while sending message"
                    );
                    errors.push(format!("{}: task panicked - {}", outlet_name, e));
                }
            }
        }

        if !errors.is_empty() {
            anyhow::bail!(
                "Failed to deliver message to {} outlet(s): {}",
                errors.len(),
                errors.join("; ")
            );
        }

        Ok(())
    }

    /// 获取所有配置的入口名称
    pub fn get_inlet_names(&self) -> Vec<&String> {
        self.routes.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{InletConfig, InletType, RetryConfig, ServerConfig};

    #[test]
    fn test_router_creation() {
        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            inlets: vec![InletConfig {
                name: "test".to_string(),
                inlet_type: InletType::Github,
                path: "/webhook".to_string(),
                repositories: None,
                workflow_run_actions: None,
            }],
            outlets: vec![OutletConfig {
                name: "wecom".to_string(),
                outlet_type: OutletType::Wecom,
                webhook_url: Some("https://example.com".to_string()),
                bot_token: None,
                chat_id: None,
            }],
            routes: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), vec!["wecom".to_string()]);
                map
            },
            retry: RetryConfig {
                max_attempts: 3,
                initial_delay_ms: 1000,
            },
        };

        let router = Router::from_config(&config);
        assert!(router.is_ok());
    }
}
