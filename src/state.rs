use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    PrCreated,
    Merged,
    Cleaned,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Active => write!(f, "active"),
            TaskStatus::PrCreated => write!(f, "pr_created"),
            TaskStatus::Merged => write!(f, "merged"),
            TaskStatus::Cleaned => write!(f, "cleaned"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub description: String,
    pub branch: String,
    pub worktree_path: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub pr_number: Option<u32>,
    pub pr_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CondState {
    pub version: u32,
    pub next_id: u32,
    pub repo_root: String,
    pub tasks: Vec<Task>,
}

impl CondState {
    pub fn state_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".cond").join("state.json")
    }

    pub fn load(repo_root: &Path) -> Result<Self> {
        let path = Self::state_path(repo_root);
        if !path.exists() {
            anyhow::bail!("cond is not initialized. Run `cond init` first.");
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&contents).context("failed to parse state.json")
    }

    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let path = Self::state_path(repo_root);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents).context("failed to write state.json")
    }

    pub fn find_task(&self, id: u32) -> Result<&Task> {
        self.tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("task {} not found", id))
    }

    pub fn find_task_mut(&mut self, id: u32) -> Result<&mut Task> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("task {} not found", id))
    }
}
