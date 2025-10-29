mod config;
mod inlet;
mod outlet;
mod retry;
mod router;
mod server;

use anyhow::{Context, Result};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    init_tracing();

    tracing::info!("Starting hook-pipe...");

    // 读取配置文件路径（默认为 config.json）
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.json".to_string());

    tracing::info!(config_path = %config_path, "Loading configuration");

    // 加载配置
    let config = config::Config::from_file(&config_path)
        .with_context(|| format!("Failed to load configuration from: {}", config_path))?;

    tracing::info!(
        host = %config.server.host,
        port = %config.server.port,
        inlets = %config.inlets.len(),
        outlets = %config.outlets.len(),
        "Configuration loaded successfully"
    );

    // 启动服务器
    server::start_server(config)
        .await
        .context("Failed to start server")?;

    Ok(())
}

/// 初始化日志系统
fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false),
        )
        .init();
}
