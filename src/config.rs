use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub inlets: Vec<InletConfig>,
    pub outlets: Vec<OutletConfig>,
    pub routes: HashMap<String, Vec<String>>,
    #[serde(default = "default_retry_config")]
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InletConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub inlet_type: InletType,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InletType {
    Github,
    Http,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutletConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub outlet_type: OutletType,
    pub webhook_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutletType {
    Wecom,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
}

fn default_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 1000,
    }
}

impl Config {
    /// 从文件加载配置
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;

        let config: Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;

        config.validate()?;
        Ok(config)
    }

    /// 验证配置的有效性
    fn validate(&self) -> Result<()> {
        // 检查入口名称唯一性
        let mut inlet_names = std::collections::HashSet::new();
        for inlet in &self.inlets {
            if !inlet_names.insert(inlet.name.clone()) {
                anyhow::bail!("Duplicate inlet name: {}", inlet.name);
            }
        }

        // 检查出口名称唯一性
        let mut outlet_names = std::collections::HashSet::new();
        for outlet in &self.outlets {
            if !outlet_names.insert(outlet.name.clone()) {
                anyhow::bail!("Duplicate outlet name: {}", outlet.name);
            }
        }

        // 验证路由配置
        for (inlet_name, outlet_list) in &self.routes {
            // 检查入口是否存在
            if !inlet_names.contains(inlet_name) {
                anyhow::bail!("Route references non-existent inlet: {}", inlet_name);
            }

            // 检查出口是否存在
            for outlet_name in outlet_list {
                if !outlet_names.contains(outlet_name) {
                    anyhow::bail!(
                        "Route '{}' references non-existent outlet: {}",
                        inlet_name,
                        outlet_name
                    );
                }
            }
        }

        Ok(())
    }

    /// 根据入口名称查找路由的出口
    pub fn find_outlets_for_inlet(&self, inlet_name: &str) -> Vec<&OutletConfig> {
        self.routes
            .get(inlet_name)
            .map(|outlet_names| {
                outlet_names
                    .iter()
                    .filter_map(|name| self.outlets.iter().find(|o| &o.name == name))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            inlets: vec![InletConfig {
                name: "test".to_string(),
                inlet_type: InletType::Github,
                path: "/webhook".to_string(),
            }],
            outlets: vec![OutletConfig {
                name: "wecom".to_string(),
                outlet_type: OutletType::Wecom,
                webhook_url: "https://example.com".to_string(),
            }],
            routes: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), vec!["wecom".to_string()]);
                map
            },
            retry: default_retry_config(),
        };

        assert!(config.validate().is_ok());
    }
}
