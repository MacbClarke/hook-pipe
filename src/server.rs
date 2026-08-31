use crate::config::{Config, InletConfig, InletType};
use crate::inlet::github::GitHubEvent;
use crate::inlet::http::HttpInlet;
use crate::inlet::watchtower::WatchtowerInlet;
use crate::router::Router;
use axum::{
    Json, Router as AxumRouter,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// 应用状态
#[derive(Clone)]
struct AppState {
    router: Arc<Router>,
    inlet_config: Arc<HashMap<String, InletConfig>>,
    path_to_inlet: Arc<HashMap<String, String>>,
}

/// 错误响应
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self.0, "Request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// 启动 HTTP 服务器
pub async fn start_server(config: Config) -> anyhow::Result<()> {
    let router = Arc::new(Router::from_config(&config)?);

    // 创建路径映射
    let mut inlet_config_map = HashMap::new();
    let mut path_to_inlet_map = HashMap::new();

    for inlet in &config.inlets {
        inlet_config_map.insert(inlet.name.clone(), inlet.clone());
        path_to_inlet_map.insert(inlet.path.clone(), inlet.name.clone());
    }

    let state = AppState {
        router,
        inlet_config: Arc::new(inlet_config_map),
        path_to_inlet: Arc::new(path_to_inlet_map),
    };

    // 构建 Axum 路由
    let mut app = AxumRouter::new();

    // 为每个入口注册路由
    for inlet in &config.inlets {
        let path = inlet.path.clone();
        tracing::info!(
            inlet_name = %inlet.name,
            inlet_type = ?inlet.inlet_type,
            path = %path,
            "Registering inlet route"
        );

        app = app.route(&path, post(handle_webhook));
    }

    // 健康检查端点
    app = app.route("/health", axum::routing::get(health_check));

    let app = app.with_state(state);

    // 启动服务器
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!(address = %addr, "Server started");

    axum::serve(listener, app).await?;

    Ok(())
}

/// 健康检查
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "hook-pipe"
    }))
}

/// 统一的 webhook 处理函数
async fn handle_webhook(
    State(state): State<AppState>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let path = uri.path().to_string();

    // 查找对应的入口名称
    let inlet_name = state
        .path_to_inlet
        .get(&path)
        .ok_or_else(|| anyhow::anyhow!("No inlet configured for path: {}", path))?;

    // 获取入口配置
    let inlet_config = state
        .inlet_config
        .get(inlet_name)
        .ok_or_else(|| anyhow::anyhow!("Inlet config not found: {}", inlet_name))?;

    tracing::info!(
        inlet_name = %inlet_config.name,
        inlet_type = ?inlet_config.inlet_type,
        path = %path,
        "Received webhook"
    );

    // 根据入口类型处理
    let message = match inlet_config.inlet_type {
        InletType::Github => {
            // 获取 GitHub 事件类型
            let event_type = headers
                .get("X-GitHub-Event")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("Missing X-GitHub-Event header"))?;

            tracing::debug!(event_type = %event_type, "GitHub event received");

            // 解析 GitHub 事件
            let github_event = GitHubEvent::parse(event_type, payload)?;

            // 检查仓库过滤
            if let Some(repo) = github_event.repository()
                && !inlet_config.is_repo_allowed(&repo.full_name, &repo.name)
            {
                tracing::info!(
                    inlet_name = %inlet_config.name,
                    repo = %repo.full_name,
                    "Ignored GitHub event from unauthorized repository"
                );
                return Ok(Json(serde_json::json!({
                    "status": "ignored",
                    "message": format!("Repository '{}' is not in the allowed repositories list", repo.full_name)
                })));
            }

            github_event.to_message()
        }
        InletType::Http => {
            // 通用 HTTP webhook
            HttpInlet::to_message(payload)
        }
        InletType::Watchtower => {
            // Watchtower webhook (via shoutrrr)
            WatchtowerInlet::to_message(payload)
        }
    };

    // 路由消息到出口
    state.router.route(&inlet_config.name, message).await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Webhook processed successfully"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error() {
        let err = ApiError(anyhow::anyhow!("test error"));
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_handle_webhook_repo_filtering() {
        use crate::config::{default_retry_config, OutletConfig, OutletType, ServerConfig};
        use axum::body::to_bytes;
        use axum::http::Request;
        use tower::ServiceExt;

        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            inlets: vec![InletConfig {
                name: "gh-filter".to_string(),
                inlet_type: InletType::Github,
                path: "/webhook/github".to_string(),
                repositories: Some(vec!["org/allowed-repo".to_string()]),
            }],
            outlets: vec![OutletConfig {
                name: "dummy".to_string(),
                outlet_type: OutletType::Wecom,
                webhook_url: Some("https://example.com/mock".to_string()),
                bot_token: None,
                chat_id: None,
            }],
            routes: {
                let mut map = HashMap::new();
                map.insert("gh-filter".to_string(), vec!["dummy".to_string()]);
                map
            },
            retry: default_retry_config(),
        };

        let mut inlet_config_map = HashMap::new();
        let mut path_to_inlet_map = HashMap::new();
        for inlet in &config.inlets {
            inlet_config_map.insert(inlet.name.clone(), inlet.clone());
            path_to_inlet_map.insert(inlet.path.clone(), inlet.name.clone());
        }

        let router = Arc::new(Router::from_config(&config).unwrap());
        let state = AppState {
            router,
            inlet_config: Arc::new(inlet_config_map),
            path_to_inlet: Arc::new(path_to_inlet_map),
        };

        let app = AxumRouter::new()
            .route("/webhook/github", post(handle_webhook))
            .with_state(state);

        // 1. 测试不在白名单的仓库被过滤 (status: ignored)
        let blocked_payload = serde_json::json!({
            "ref": "refs/heads/main",
            "repository": {
                "name": "other-repo",
                "full_name": "org/other-repo",
                "html_url": "https://github.com/org/other-repo"
            },
            "pusher": { "name": "user" },
            "commits": [],
            "compare": "https://github.com/org/other-repo/compare/a...b"
        });

        let req = Request::builder()
            .method("POST")
            .uri("/webhook/github")
            .header("Content-Type", "application/json")
            .header("X-GitHub-Event", "push")
            .body(axum::body::Body::from(serde_json::to_vec(&blocked_payload).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json_body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json_body["status"], "ignored");
        assert!(json_body["message"].as_str().unwrap().contains("not in the allowed repositories"));
    }
}
