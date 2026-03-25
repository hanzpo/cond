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

    /// Find a task by ID (numeric) or by name (matched against slugified description).
    pub fn find_task(&self, query: &str) -> Result<&Task> {
        let idx = self.resolve_task_index(query)?;
        Ok(&self.tasks[idx])
    }

    /// Find a task mutably by ID (numeric) or by name.
    pub fn find_task_mut(&mut self, query: &str) -> Result<&mut Task> {
        let idx = self.resolve_task_index(query)?;
        Ok(&mut self.tasks[idx])
    }

    /// Resolve a query to a single task index.
    fn resolve_task_index(&self, query: &str) -> Result<usize> {
        if let Ok(id) = query.parse::<u32>() {
            if let Some(idx) = self.tasks.iter().position(|t| t.id == id) {
                return Ok(idx);
            }
        }
        let slug = crate::util::slugify(query);
        let indices: Vec<_> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                let task_slug = crate::util::slugify(&t.description);
                task_slug == slug || task_slug.contains(&slug)
            })
            .map(|(i, _)| i)
            .collect();
        match indices.len() {
            0 => anyhow::bail!("task '{}' not found", query),
            1 => Ok(indices[0]),
            _ => {
                let names: Vec<_> = indices
                    .iter()
                    .map(|&i| format!("{} ({})", self.tasks[i].id, self.tasks[i].description))
                    .collect();
                anyhow::bail!(
                    "ambiguous task name '{}', matches: {}",
                    query,
                    names.join(", ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_task(id: u32, description: &str, status: TaskStatus) -> Task {
        let now = Utc::now();
        Task {
            id,
            description: description.to_string(),
            branch: format!("cond/task-{id}-{}", crate::util::slugify(description)),
            worktree_path: format!(".cond-worktrees/task-{id}"),
            status,
            created_at: now,
            updated_at: now,
            pr_number: None,
            pr_url: None,
        }
    }

    fn make_state(tasks: Vec<Task>) -> CondState {
        CondState {
            version: 1,
            next_id: tasks.len() as u32 + 1,
            repo_root: "/tmp/repo".to_string(),
            tasks,
        }
    }

    // --- TaskStatus Display ---

    #[test]
    fn task_status_display() {
        assert_eq!(TaskStatus::Active.to_string(), "active");
        assert_eq!(TaskStatus::PrCreated.to_string(), "pr_created");
        assert_eq!(TaskStatus::Merged.to_string(), "merged");
        assert_eq!(TaskStatus::Cleaned.to_string(), "cleaned");
    }

    // --- Serialization round-trip ---

    #[test]
    fn task_status_serde_round_trip() {
        let statuses = vec![
            TaskStatus::Active,
            TaskStatus::PrCreated,
            TaskStatus::Merged,
            TaskStatus::Cleaned,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn task_status_serde_snake_case() {
        let json = serde_json::to_string(&TaskStatus::PrCreated).unwrap();
        assert_eq!(json, "\"pr_created\"");
    }

    #[test]
    fn state_serde_round_trip() {
        let state = make_state(vec![
            make_task(1, "fix login bug", TaskStatus::Active),
            make_task(2, "add search", TaskStatus::PrCreated),
        ]);

        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: CondState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.version, 1);
        assert_eq!(restored.tasks.len(), 2);
        assert_eq!(restored.tasks[0].id, 1);
        assert_eq!(restored.tasks[1].description, "add search");
    }

    // --- find_task ---

    #[test]
    fn find_task_by_id() {
        let state = make_state(vec![
            make_task(1, "fix login", TaskStatus::Active),
            make_task(2, "add search", TaskStatus::Active),
        ]);

        let task = state.find_task("2").unwrap();
        assert_eq!(task.id, 2);
        assert_eq!(task.description, "add search");
    }

    #[test]
    fn find_task_by_name_exact() {
        let state = make_state(vec![
            make_task(1, "fix login bug", TaskStatus::Active),
            make_task(2, "add search feature", TaskStatus::Active),
        ]);

        let task = state.find_task("add search feature").unwrap();
        assert_eq!(task.id, 2);
    }

    #[test]
    fn find_task_by_name_substring() {
        let state = make_state(vec![
            make_task(1, "fix login bug", TaskStatus::Active),
            make_task(2, "add search feature", TaskStatus::Active),
        ]);

        let task = state.find_task("search").unwrap();
        assert_eq!(task.id, 2);
    }

    #[test]
    fn find_task_not_found() {
        let state = make_state(vec![make_task(1, "fix login", TaskStatus::Active)]);

        let result = state.find_task("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn find_task_ambiguous() {
        let state = make_state(vec![
            make_task(1, "fix login bug", TaskStatus::Active),
            make_task(2, "fix login error", TaskStatus::Active),
        ]);

        let result = state.find_task("fix login");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ambiguous"));
    }

    #[test]
    fn find_task_id_not_found_falls_through_to_name() {
        let state = make_state(vec![make_task(1, "task 99", TaskStatus::Active)]);

        // "99" doesn't match any ID, but it matches the name slug "task-99"
        let task = state.find_task("99").unwrap();
        assert_eq!(task.id, 1);
    }

    #[test]
    fn find_task_mut_modifies() {
        let mut state = make_state(vec![make_task(1, "fix login", TaskStatus::Active)]);

        let task = state.find_task_mut("1").unwrap();
        task.status = TaskStatus::PrCreated;
        task.pr_number = Some(42);

        assert_eq!(state.tasks[0].status, TaskStatus::PrCreated);
        assert_eq!(state.tasks[0].pr_number, Some(42));
    }

    // --- load / save ---

    #[test]
    fn load_missing_state_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = CondState::load(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".cond")).unwrap();

        let state = make_state(vec![make_task(1, "test task", TaskStatus::Active)]);

        state.save(dir.path()).unwrap();
        let loaded = CondState::load(dir.path()).unwrap();

        assert_eq!(loaded.version, state.version);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].description, "test task");
    }

    #[test]
    fn state_path_is_correct() {
        let path = CondState::state_path(Path::new("/foo/bar"));
        assert_eq!(path, PathBuf::from("/foo/bar/.cond/state.json"));
    }
}
