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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_repositories",
        alias = "allowed_repositories"
    )]
    pub repositories: Option<Vec<String>>,
}

/// 自定义反序列化函数，支持单一字符串或字符串数组
fn deserialize_repositories<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Single(String),
        Multiple(Vec<String>),
    }

    match Option::<StringOrVec>::deserialize(deserializer)? {
        None => Ok(None),
        Some(StringOrVec::Single(s)) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(vec![s.to_string()]))
            }
        }
        Some(StringOrVec::Multiple(vec)) => {
            let filtered: Vec<String> = vec
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if filtered.is_empty() {
                Ok(None)
            } else {
                Ok(Some(filtered))
            }
        }
    }
}

impl InletConfig {
    /// 检查指定仓库是否被该 inlet 允许
    /// 如果未配置 repositories 或列表为空，则默认允许所有仓库
    pub fn is_repo_allowed(&self, repo_full_name: &str, repo_name: &str) -> bool {
        match &self.repositories {
            Some(allowed_list) if !allowed_list.is_empty() => allowed_list
                .iter()
                .any(|pattern| Self::matches_repo(pattern, repo_full_name, repo_name)),
            _ => true,
        }
    }

    /// 匹配仓库规则
    /// 支持以下格式：
    /// - "owner/repo": 精确匹配 full_name（大小写不敏感）
    /// - "repo": 匹配 name 或 full_name（大小写不敏感）
    /// - "owner/*": 匹配指定组织/用户下的所有仓库
    /// - "*": 匹配所有仓库
    pub fn matches_repo(pattern: &str, repo_full_name: &str, repo_name: &str) -> bool {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return false;
        }

        if pattern == "*" {
            return true;
        }

        if let Some(prefix) = pattern.strip_suffix("/*") {
            let prefix_with_slash = format!("{}/", prefix);
            return repo_full_name
                .to_lowercase()
                .starts_with(&prefix_with_slash.to_lowercase());
        }

        if pattern.contains('/') {
            repo_full_name.eq_ignore_ascii_case(pattern)
        } else {
            repo_name.eq_ignore_ascii_case(pattern)
                || repo_full_name.eq_ignore_ascii_case(pattern)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InletType {
    Github,
    Http,
    Watchtower,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutletConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub outlet_type: OutletType,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutletType {
    Wecom,
    Telegram,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
}

pub fn default_retry_config() -> RetryConfig {
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

            match outlet.outlet_type {
                OutletType::Wecom => {
                    if outlet.webhook_url.as_deref().is_none() {
                        anyhow::bail!("WeCom outlet '{}' requires 'webhook_url'", outlet.name);
                    }
                }
                OutletType::Telegram => {
                    if outlet.bot_token.as_deref().is_none() {
                        anyhow::bail!("Telegram outlet '{}' requires 'bot_token'", outlet.name);
                    }
                    if outlet.chat_id.as_deref().is_none() {
                        anyhow::bail!("Telegram outlet '{}' requires 'chat_id'", outlet.name);
                    }
                }
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
                repositories: None,
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
            retry: default_retry_config(),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_inlet_repositories_deserialize() {
        let json_str = r#"{
            "name": "gh",
            "type": "github",
            "path": "/webhook/github",
            "repositories": ["owner/repo1", "owner/*", "repo2"]
        }"#;

        let inlet: InletConfig = serde_json::from_str(json_str).unwrap();
        assert_eq!(
            inlet.repositories,
            Some(vec![
                "owner/repo1".to_string(),
                "owner/*".to_string(),
                "repo2".to_string()
            ])
        );

        // 单个字符串反序列化
        let json_single = r#"{
            "name": "gh",
            "type": "github",
            "path": "/webhook/github",
            "repositories": "owner/repo1"
        }"#;
        let inlet_single: InletConfig = serde_json::from_str(json_single).unwrap();
        assert_eq!(inlet_single.repositories, Some(vec!["owner/repo1".to_string()]));

        // alias allowed_repositories
        let json_alias = r#"{
            "name": "gh",
            "type": "github",
            "path": "/webhook/github",
            "allowed_repositories": ["owner/repo1"]
        }"#;
        let inlet_alias: InletConfig = serde_json::from_str(json_alias).unwrap();
        assert_eq!(inlet_alias.repositories, Some(vec!["owner/repo1".to_string()]));
    }

    #[test]
    fn test_inlet_repo_matching() {
        let inlet = InletConfig {
            name: "gh".to_string(),
            inlet_type: InletType::Github,
            path: "/webhook/github".to_string(),
            repositories: Some(vec![
                "octocat/hello-world".to_string(),
                "my-org/*".to_string(),
                "special-repo".to_string(),
            ]),
        };

        // 精确匹配 owner/repo（忽略大小写）
        assert!(inlet.is_repo_allowed("octocat/hello-world", "hello-world"));
        assert!(inlet.is_repo_allowed("OctoCat/Hello-World", "Hello-World"));

        // 通配符匹配 owner/*
        assert!(inlet.is_repo_allowed("my-org/project-a", "project-a"));
        assert!(inlet.is_repo_allowed("MY-ORG/project-b", "project-b"));
        assert!(!inlet.is_repo_allowed("other-org/project-a", "project-a"));

        // 单独仓库名称匹配 repo
        assert!(inlet.is_repo_allowed("any-owner/special-repo", "special-repo"));
        assert!(inlet.is_repo_allowed("special-repo", "special-repo"));

        // 未在列表中的仓库
        assert!(!inlet.is_repo_allowed("octocat/other-repo", "other-repo"));

        // 如果未配置 repositories，则默认允许全部
        let inlet_none = InletConfig {
            name: "gh".to_string(),
            inlet_type: InletType::Github,
            path: "/webhook/github".to_string(),
            repositories: None,
        };
        assert!(inlet_none.is_repo_allowed("any/repo", "repo"));
    }
}
