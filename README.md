# Hook-Pipe

一个用 Rust 编写的 webhook 集中管理、转换和分发系统。

## 功能特性

- **多入口支持**
  - GitHub Webhook
  - 通用 HTTP Webhook
  - 可扩展的入口系统

- **多出口支持**
  - 企业微信群机器人
  - Telegram Bot
  - 可扩展的出口系统

- **智能路由**
  - 灵活的入口到出口映射
  - 支持一对多分发
  - 基于配置的路由规则

- **可靠性**
  - 自动重试机制
  - 指数退避策略
  - 并发消息发送

- **易于使用**
  - JSON 配置文件
  - 结构化日志
  - 健康检查端点

## 快速开始

### 安装

```bash
# 克隆仓库
git clone <your-repo-url>
cd hook-pipe

# 编译
cargo build --release
```

### 配置

1. 复制示例配置文件：

```bash
cp config.example.json config.json
```

2. 编辑 `config.json`：

```json
{
  "server": {
    "host": "0.0.0.0",
    "port": 8080
  },
  "inlets": [
    {
      "name": "github-main",
      "type": "github",
      "path": "/webhook/github"
    },
    {
      "name": "generic-webhook",
      "type": "http",
      "path": "/webhook/http"
    }
  ],
  "outlets": [
    {
      "name": "wecom-dev",
      "type": "wecom",
      "webhook_url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=YOUR_KEY"
    },
    {
      "name": "telegram-alerts",
      "type": "telegram",
      "bot_token": "123456:ABCDEF",
      "chat_id": "-1001234567890"
    }
  ],
  "routes": {
    "github-main": ["wecom-dev"],
    "generic-webhook": ["wecom-dev", "telegram-alerts"]
  },
  "retry": {
    "max_attempts": 3,
    "initial_delay_ms": 1000
  }
}
```

### 运行

```bash
# 使用默认配置文件 (config.json)
cargo run --release

# 或指定配置文件路径
cargo run --release -- /path/to/config.json

# 设置日志级别
RUST_LOG=debug cargo run --release
```

## 配置说明

### Server 配置

- `host`: 监听地址 (默认: "0.0.0.0")
- `port`: 监听端口 (默认: 8080)

### Inlets（入口）

每个入口配置包含：

- `name`: 入口名称（唯一标识）
- `type`: 入口类型
  - `github`: GitHub Webhook
  - `http`: 通用 HTTP Webhook
  - `watchtower`: Watchtower Webhook
- `path`: webhook 接收路径
- `repositories`: （可选，针对 GitHub inlet）允许接收消息的仓库列表，未配置或为空时接收所有仓库消息。支持格式：
  - `"owner/repo"`: 精确匹配指定仓库（大小写不敏感）
  - `"repo"`: 匹配仓库名（大小写不敏感）
  - `"owner/*"`: 匹配指定组织或用户下的所有仓库
  - `"*"`: 匹配所有仓库
  - 也支持单一字符串格式，例如 `"repositories": "owner/repo"`，别名支持 `allowed_repositories`

### Outlets（出口）

每个出口配置包含：

- `name`: 出口名称（唯一标识）
- `type`: 出口类型
  - `wecom`: 企业微信群机器人
  - `telegram`: Telegram Bot
- `wecom` 需要 `webhook_url`
- `telegram` 需要 `bot_token` 和 `chat_id`

### Routes（路由）

使用入口名称映射到出口名称列表：

```json
{
  "routes": {
    "入口名称": ["出口1", "出口2"]
  }
}
```

### Retry（重试配置）

- `max_attempts`: 最大重试次数
- `initial_delay_ms`: 初始延迟毫秒数（使用指数退避）

## 使用示例

### 配置 GitHub Webhook

1. 在 GitHub 仓库设置中添加 Webhook
2. Payload URL: `http://your-server:8080/webhook/github`
3. Content type: `application/json`
4. 选择需要的事件（如 Push, Pull Request）

### 测试通用 HTTP Webhook

HTTP body 为 JSON，支持以下字段：

- `title`: 标题，缺省为 `HTTP Webhook`
- `content`: 正文，缺省为空字符串
- `type`: 内容类型标识，缺省为 `generic`
- `source`: 来源标识，缺省为 `http`

```bash
curl -X POST http://localhost:8080/webhook/http \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Test Alert",
    "content": "This is a test message",
    "type": "text",
    "source": "my-service"
  }'
```

### 健康检查

```bash
curl http://localhost:8080/health
```

## 支持的 GitHub 事件

当前支持以下 GitHub 事件的转换：

- **Push**: 推送到仓库
  - 显示分支、提交者、提交列表
  - 提供对比链接

- **Pull Request**: PR 相关事件
  - 打开、关闭、合并、更新
  - 显示 PR 标题、作者、分支

其他事件将被标记为 "Unknown" 但仍会转发。

## 日志

使用 `RUST_LOG` 环境变量控制日志级别：

```bash
# 只显示错误
RUST_LOG=error cargo run

# 显示所有信息
RUST_LOG=debug cargo run

# 针对特定模块
RUST_LOG=hook_pipe=debug cargo run
```

## 开发

### 运行测试

```bash
cargo test
```

### 代码检查

```bash
cargo clippy
```

### 格式化代码

```bash
cargo fmt
```

## 扩展

### 添加新的入口类型

1. 在 `src/inlet/` 下创建新的模块
2. 实现消息解析和转换逻辑
3. 在 `config.rs` 中添加新的 `InletType`
4. 在 `server.rs` 中添加处理逻辑

### 添加新的出口类型

1. 在 `src/outlet/` 下创建新的模块
2. 实现 `Outlet` trait
3. 在 `config.rs` 中添加新的 `OutletType`
4. 在 `router.rs` 的 `create_outlet` 中添加创建逻辑

## License

MIT
