use super::Message;
use crate::util;
use anyhow::Result;
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// GitHub webhook 事件类型
#[derive(Debug, Clone)]
pub enum GitHubEvent {
    Push(PushEvent),
    PullRequest(PullRequestEvent),
    WorkflowRun(WorkflowRunEvent),
    Unknown(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PushEvent {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub repository: Repository,
    pub pusher: Pusher,
    pub commits: Vec<Commit>,
    pub head_commit: Option<Commit>,
    pub compare: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequestEvent {
    pub action: String,
    pub number: u64,
    pub pull_request: PullRequest,
    pub repository: Repository,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRunEvent {
    pub action: String,
    pub workflow_run: WorkflowRun,
    pub repository: Repository,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub html_url: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub run_started_at: String,
    pub triggering_actor: Actor,
    pub head_commit: HeadCommit,
    pub head_branch: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Actor {
    pub login: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeadCommit {
    pub id: String,
    pub message: String,
    pub author: CommitAuthor,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PullRequest {
    pub title: String,
    pub html_url: String,
    pub state: String,
    pub user: GitHubUser,
    pub head: Branch,
    pub base: Branch,
    pub merged: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Branch {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub sha: String,
}

/// 自定义反序列化函数，处理 pushed_at 字段的两种格式
/// - i64: Unix 时间戳（PR 和 Push 事件）
/// - String: ISO 8601 格式如 "2025-10-28T09:40:04Z"（Workflow 事件）
/// - null 或缺失: None
fn deserialize_pushed_at<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TimestampOrString {
        Timestamp(i64),
        String(String),
    }

    match Option::<TimestampOrString>::deserialize(deserializer)? {
        None => Ok(None),
        Some(TimestampOrString::Timestamp(ts)) => Ok(Some(ts)),
        Some(TimestampOrString::String(s)) => {
            // 解析 ISO 8601 格式的时间字符串
            match DateTime::parse_from_rfc3339(&s) {
                Ok(dt) => Ok(Some(dt.timestamp())),
                Err(_) => Ok(None), // 解析失败时返回 None
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    #[serde(default, deserialize_with = "deserialize_pushed_at")]
    pub pushed_at: Option<i64>,
}

/// Push 事件中的 pusher 信息（简单格式）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pusher {
    pub name: String,
    pub email: Option<String>,
}

/// PR 和其他事件中的完整 GitHub 用户信息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubUser {
    pub login: String,
    pub id: u64,
    pub html_url: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Commit {
    pub id: String,
    pub message: String,
    pub author: CommitAuthor,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
}

impl GitHubEvent {
    /// 解析 GitHub webhook payload
    pub fn parse(event_type: &str, payload: Value) -> Result<Self> {
        match event_type {
            "push" => {
                let event: PushEvent = serde_json::from_value(payload)?;
                Ok(GitHubEvent::Push(event))
            }
            "pull_request" => {
                let event: PullRequestEvent = serde_json::from_value(payload)?;
                Ok(GitHubEvent::PullRequest(event))
            }
            "workflow_run" => {
                let event: WorkflowRunEvent = serde_json::from_value(payload)?;
                Ok(GitHubEvent::WorkflowRun(event))
            }
            _ => Ok(GitHubEvent::Unknown(event_type.to_string())),
        }
    }

    /// 获取事件对应的仓库信息（若事件中包含仓库信息）
    pub fn repository(&self) -> Option<&Repository> {
        match self {
            GitHubEvent::Push(event) => Some(&event.repository),
            GitHubEvent::PullRequest(event) => Some(&event.repository),
            GitHubEvent::WorkflowRun(event) => Some(&event.repository),
            GitHubEvent::Unknown(_) => None,
        }
    }

    /// 转换为统一的消息格式
    pub fn to_message(&self) -> Message {
        match self {
            GitHubEvent::Push(event) => self.push_to_message(event),
            GitHubEvent::PullRequest(event) => self.pr_to_message(event),
            GitHubEvent::WorkflowRun(event) => self.workflow_run_to_message(event),
            GitHubEvent::Unknown(event_type) => Message::new(
                "Unknown GitHub Event".to_string(),
                format!("Received unknown event type: {}", event_type),
                "github".to_string(),
                event_type.clone(),
            ),
        }
    }

    fn push_to_message(&self, event: &PushEvent) -> Message {
        let branch = event
            .git_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(&event.git_ref);

        let commits_summary = if event.commits.is_empty() {
            "No commits".to_string()
        } else {
            event
                .commits
                .iter()
                .take(5)
                .map(|c| {
                    let short_id = &c.id[..7.min(c.id.len())];
                    let first_line = c.message.lines().next().unwrap_or("");
                    format!("- `{}` {}", short_id, first_line)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let more_commits = if event.commits.len() > 5 {
            format!("\n\n... and {} more commits", event.commits.len() - 5)
        } else {
            String::new()
        };

        let content = format!(
            "**Repository:** [{}]({})\n\
             **Branch:** `{}`\n\
             **Pusher:** {}\n\
             **Commits:** {}\n\n\
             {}{}\n\n\
             [View comparison]({})",
            event.repository.full_name,
            event.repository.html_url,
            branch,
            event.pusher.name,
            event.commits.len(),
            commits_summary,
            more_commits,
            event.compare
        );

        if let Some(pushed_at) = event.repository.pushed_at {
            Message::new_with_timestamp(
                format!("Push to {}/{}", event.repository.name, branch),
                content,
                "github".to_string(),
                "push".to_string(),
                pushed_at,
            )
        } else {
            Message::new(
                format!("Push to {}/{}", event.repository.name, branch),
                content,
                "github".to_string(),
                "push".to_string(),
            )
        }
    }

    fn pr_to_message(&self, event: &PullRequestEvent) -> Message {
        let pr = &event.pull_request;
        let action_emoji = match event.action.as_str() {
            "opened" => "🆕",
            "closed" if pr.merged.unwrap_or(false) => "🎉",
            "closed" => "❌",
            "reopened" => "🔄",
            "synchronize" => "🔄",
            _ => "📝",
        };

        let action_text = match event.action.as_str() {
            "synchronize" => "updated",
            _ => &event.action,
        };

        let content = format!(
            "{} **Pull Request {}**\n\n\
             **Title:** {}\n\
             **Author:** [{}]({})\n\
             **Status:** {}\n\
             **Branch:** `{}` → `{}`\n\n\
             [View Pull Request]({})",
            action_emoji,
            action_text,
            pr.title,
            pr.user.login,
            pr.user.html_url,
            pr.state,
            pr.head.git_ref,
            pr.base.git_ref,
            pr.html_url
        );

        if let Some(pushed_at) = event.repository.pushed_at {
            Message::new_with_timestamp(
                format!("PR #{}: {}", event.number, pr.title),
                content,
                "github".to_string(),
                format!("pull_request.{}", event.action),
                pushed_at,
            )
        } else {
            Message::new(
                format!("PR #{}: {}", event.number, pr.title),
                content,
                "github".to_string(),
                format!("pull_request.{}", event.action),
            )
        }
    }

    fn workflow_run_to_message(&self, event: &WorkflowRunEvent) -> Message {
        let workflow = &event.workflow_run;

        // 状态表情符号
        let status_emoji = if workflow.status == "completed" {
            match workflow.conclusion.as_deref() {
                Some("success") => "✅",
                Some("failure") => "❌",
                Some("cancelled") => "🚫",
                Some("skipped") => "⏭️",
                _ => "⚪",
            }
        } else {
            "🔄"
        };

        // 状态文本
        let status_text = if workflow.status == "completed" {
            workflow.conclusion.as_deref().unwrap_or("unknown")
        } else {
            &workflow.status
        };

        // 提取commit信息
        let commit_message = workflow.head_commit.message.lines().next().unwrap_or("");
        let short_sha = &workflow.head_commit.id[..7.min(workflow.head_commit.id.len())];

        let content = format!(
            "{} **Workflow {}**\n\n\
             **Workflow:** {}\n\
             **Status:** {}\n\
             **Triggered by:** [{}]({})\n\
             **Branch:** `{}`\n\
             **Started at:** {}\n\
             **Commit:** `{}` {}\n\n\
             [View Workflow Run]({})",
            status_emoji,
            event.action,
            workflow.name,
            status_text,
            workflow.triggering_actor.login,
            workflow.triggering_actor.html_url,
            workflow.head_branch,
            util::format_iso8601_datetime(&workflow.run_started_at),
            short_sha,
            commit_message,
            workflow.html_url
        );

        // 使用本地时间作为消息时间
        Message::new(
            format!("Workflow: {}", workflow.name),
            content,
            "github".to_string(),
            format!("workflow_run.{}", event.action),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_push_event() {
        let payload = serde_json::json!({
            "ref": "refs/heads/main",
            "repository": {
                "name": "test-repo",
                "full_name": "user/test-repo",
                "html_url": "https://github.com/user/test-repo",
                "pushed_at": 1234567890
            },
            "pusher": {
                "name": "testuser",
                "email": "test@example.com"
            },
            "commits": [],
            "compare": "https://github.com/user/test-repo/compare/abc...def"
        });

        let event = GitHubEvent::parse("push", payload).unwrap();
        match event {
            GitHubEvent::Push(_) => {}
            _ => panic!("Expected Push event"),
        }
    }

    #[test]
    fn test_parse_workflow_run_event() {
        let payload = serde_json::json!({
            "action": "completed",
            "workflow_run": {
                "id": 18870474628u64,
                "name": "Docker Build and Push",
                "html_url": "https://github.com/test-org/test-repo/actions/runs/18870474628",
                "status": "completed",
                "conclusion": "success",
                "run_started_at": "2025-10-28T09:40:07Z",
                "head_branch": "main",
                "triggering_actor": {
                    "login": "testuser",
                    "html_url": "https://github.com/testuser"
                },
                "head_commit": {
                    "id": "f20a8a8da460f0a6f737d7cbcad9f3febb3337ce",
                    "message": "Test commit message",
                    "author": {
                        "name": "Test User",
                        "email": "test@example.com"
                    }
                }
            },
            "repository": {
                "name": "test-repo",
                "full_name": "test-org/test-repo",
                "html_url": "https://github.com/test-org/test-repo"
            }
        });

        let event = GitHubEvent::parse("workflow_run", payload).unwrap();
        match event {
            GitHubEvent::WorkflowRun(_) => {}
            _ => panic!("Expected WorkflowRun event"),
        }
    }

    #[test]
    fn test_pushed_at_timestamp_format() {
        // 测试 i64 时间戳格式（Push 和 PR 事件）
        let payload = serde_json::json!({
            "name": "test-repo",
            "full_name": "user/test-repo",
            "html_url": "https://github.com/user/test-repo",
            "pushed_at": 1730108404
        });

        let repo: Repository = serde_json::from_value(payload).unwrap();
        assert_eq!(repo.pushed_at, Some(1730108404));
    }

    #[test]
    fn test_pushed_at_string_format() {
        // 测试 ISO 8601 字符串格式（Workflow 事件）
        let payload = serde_json::json!({
            "name": "test-repo",
            "full_name": "user/test-repo",
            "html_url": "https://github.com/user/test-repo",
            "pushed_at": "2025-10-28T09:40:04Z"
        });

        let repo: Repository = serde_json::from_value(payload).unwrap();
        // "2025-10-28T09:40:04Z" 对应的时间戳是 1761644404
        assert_eq!(repo.pushed_at, Some(1761644404));
    }

    #[test]
    fn test_pushed_at_missing() {
        // 测试缺失 pushed_at 字段
        let payload = serde_json::json!({
            "name": "test-repo",
            "full_name": "user/test-repo",
            "html_url": "https://github.com/user/test-repo"
        });

        let repo: Repository = serde_json::from_value(payload).unwrap();
        assert_eq!(repo.pushed_at, None);
    }

    #[test]
    fn test_parse_pull_request_event() {
        // 使用脱敏的随机用户信息
        let payload = serde_json::json!({
            "action": "opened",
            "number": 42,
            "pull_request": {
                "title": "Fix authentication bug",
                "html_url": "https://github.com/test-org/test-repo/pull/42",
                "state": "open",
                "user": {
                    "login": "testuser123",
                    "id": 12345678,
                    "html_url": "https://github.com/testuser123",
                    "avatar_url": "https://avatars.githubusercontent.com/u/12345678?v=4"
                },
                "head": {
                    "ref": "fix-auth",
                    "sha": "abc1234567890def"
                },
                "base": {
                    "ref": "main",
                    "sha": "def0987654321abc"
                },
                "merged": false
            },
            "repository": {
                "name": "test-repo",
                "full_name": "test-org/test-repo",
                "html_url": "https://github.com/test-org/test-repo",
                "pushed_at": 1730108404
            }
        });

        let event = GitHubEvent::parse("pull_request", payload).unwrap();
        match event {
            GitHubEvent::PullRequest(pr_event) => {
                assert_eq!(pr_event.action, "opened");
                assert_eq!(pr_event.number, 42);
                assert_eq!(pr_event.pull_request.user.login, "testuser123");
                assert_eq!(pr_event.pull_request.user.id, 12345678);
            }
            _ => panic!("Expected PullRequest event"),
        }
    }

    #[test]
    fn test_pr_to_message() {
        // 使用脱敏的随机用户信息
        let payload = serde_json::json!({
            "action": "opened",
            "number": 42,
            "pull_request": {
                "title": "Fix authentication bug",
                "html_url": "https://github.com/test-org/test-repo/pull/42",
                "state": "open",
                "user": {
                    "login": "testuser123",
                    "id": 12345678,
                    "html_url": "https://github.com/testuser123"
                },
                "head": {
                    "ref": "fix-auth",
                    "sha": "abc1234"
                },
                "base": {
                    "ref": "main",
                    "sha": "def5678"
                },
                "merged": false
            },
            "repository": {
                "name": "test-repo",
                "full_name": "test-org/test-repo",
                "html_url": "https://github.com/test-org/test-repo"
            }
        });

        let event = GitHubEvent::parse("pull_request", payload).unwrap();
        let message = event.to_message();

        assert_eq!(message.title, "PR #42: Fix authentication bug");
        assert!(message.content.contains("testuser123"));
        assert!(message.content.contains("https://github.com/testuser123"));
        assert!(message.content.contains("🆕")); // opened emoji
    }

    #[test]
    fn test_workflow_run_to_message() {
        let payload = serde_json::json!({
            "action": "completed",
            "workflow_run": {
                "id": 18870474628u64,
                "name": "Docker Build and Push",
                "html_url": "https://github.com/test-org/test-repo/actions/runs/18870474628",
                "status": "completed",
                "conclusion": "success",
                "run_started_at": "2025-10-28T09:40:07Z",
                "head_branch": "main",
                "triggering_actor": {
                    "login": "testuser",
                    "html_url": "https://github.com/testuser"
                },
                "head_commit": {
                    "id": "f20a8a8da460f0a6f737d7cbcad9f3febb3337ce",
                    "message": "Test commit message",
                    "author": {
                        "name": "Test User",
                        "email": "test@example.com"
                    }
                }
            },
            "repository": {
                "name": "test-repo",
                "full_name": "test-org/test-repo",
                "html_url": "https://github.com/test-org/test-repo"
            }
        });

        let event = GitHubEvent::parse("workflow_run", payload).unwrap();
        let message = event.to_message();

        assert_eq!(message.title, "Workflow: Docker Build and Push");
        assert_eq!(message.metadata.source, "github");
        assert_eq!(message.metadata.message_type, "workflow_run.completed");
        assert!(message.content.contains("✅"));
        assert!(message.content.contains("success"));
        assert!(message.content.contains("testuser"));
        assert!(message.content.contains("f20a8a8"));
    }

    #[test]
    fn test_github_event_repository_getter() {
        let payload = serde_json::json!({
            "ref": "refs/heads/main",
            "repository": {
                "name": "my-repo",
                "full_name": "org/my-repo",
                "html_url": "https://github.com/org/my-repo"
            },
            "pusher": { "name": "user" },
            "commits": [],
            "compare": "https://github.com/org/my-repo/compare/a...b"
        });

        let event = GitHubEvent::parse("push", payload).unwrap();
        let repo = event.repository().unwrap();
        assert_eq!(repo.name, "my-repo");
        assert_eq!(repo.full_name, "org/my-repo");

        let unknown_event = GitHubEvent::Unknown("ping".to_string());
        assert!(unknown_event.repository().is_none());
    }
}
