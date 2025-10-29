use super::Message;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// GitHub webhook 事件类型
#[derive(Debug, Clone)]
pub enum GitHubEvent {
    Push(PushEvent),
    PullRequest(PullRequestEvent),
    Unknown(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PushEvent {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub repository: Repository,
    pub pusher: User,
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
pub struct PullRequest {
    pub title: String,
    pub html_url: String,
    pub state: String,
    pub user: User,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub pushed_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub name: String,
    pub email: Option<String>,
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
            _ => Ok(GitHubEvent::Unknown(event_type.to_string())),
        }
    }

    /// 转换为统一的消息格式
    pub fn to_message(&self) -> Message {
        match self {
            GitHubEvent::Push(event) => self.push_to_message(event),
            GitHubEvent::PullRequest(event) => self.pr_to_message(event),
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
             **Author:** {}\n\
             **Status:** {}\n\
             **Branch:** `{}` → `{}`\n\n\
             [View Pull Request]({})",
            action_emoji,
            action_text,
            pr.title,
            pr.user.name,
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
}
