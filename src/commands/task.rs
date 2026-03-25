use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use crate::state::{CondState, Task, TaskStatus};
use crate::util;

pub fn spawn(repo_root: &Path, state: &mut CondState, description: &str) -> Result<()> {
    let id = state.next_id;
    state.next_id += 1;

    let slug = util::slugify(description);
    let branch = format!("cond/task-{id}-{slug}");
    let worktree_rel = format!(".cond-worktrees/task-{id}");
    let worktree_abs = repo_root.join(&worktree_rel);

    util::run("git", &["branch", &branch], Some(repo_root))?;
    util::run(
        "git",
        &["worktree", "add", &worktree_abs.to_string_lossy(), &branch],
        Some(repo_root),
    )?;

    let now = Utc::now();
    state.tasks.push(Task {
        id,
        description: description.to_string(),
        branch: branch.clone(),
        worktree_path: worktree_rel.clone(),
        status: TaskStatus::Active,
        created_at: now,
        updated_at: now,
        pr_number: None,
        pr_url: None,
    });

    eprintln!("spawned task {id}: \"{description}\"");
    eprintln!("  branch:   {branch}");
    eprintln!("  worktree: {worktree_rel}");
    println!("{}", worktree_abs.display());

    Ok(())
}

pub fn status(state: &CondState) -> Result<()> {
    if state.tasks.is_empty() {
        println!("no tasks");
        return Ok(());
    }

    println!("{:<4} {:<11} {:<42} DESCRIPTION", "ID", "STATUS", "BRANCH");
    for task in &state.tasks {
        println!(
            "{:<4} {:<11} {:<42} {}",
            task.id,
            task.status.to_string(),
            truncate(&task.branch, 42),
            task.description,
        );
    }

    Ok(())
}

pub fn kill(repo_root: &Path, state: &mut CondState, query: &str) -> Result<()> {
    let task = state.find_task(query)?;
    let branch = task.branch.clone();
    let worktree_path = repo_root.join(&task.worktree_path);

    if worktree_path.exists() {
        let _ = util::run(
            "git",
            &[
                "worktree",
                "remove",
                &worktree_path.to_string_lossy(),
                "--force",
            ],
            Some(repo_root),
        );
    }

    // Always attempt to delete local branch (ignore errors if already gone)
    let _ = util::run("git", &["branch", "-D", &branch], Some(repo_root));

    let task = state.find_task_mut(query)?;
    let id = task.id;
    task.status = TaskStatus::Cleaned;
    task.updated_at = Utc::now();

    println!("killed task {id}");

    Ok(())
}

pub fn nuke(repo_root: &Path, state: &mut CondState, confirm: bool) -> Result<()> {
    if !confirm {
        eprintln!("this will kill all tasks and tear down everything.");
        eprintln!("run with --confirm to proceed.");
        return Ok(());
    }

    let task_ids: Vec<u32> = state
        .tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Cleaned && t.status != TaskStatus::Merged)
        .map(|t| t.id)
        .collect();

    for id in task_ids {
        if let Err(e) = kill(repo_root, state, &id.to_string()) {
            eprintln!("warning: failed to kill task {id}: {e}");
        }
    }

    let worktrees_dir = repo_root.join(".cond-worktrees");
    if worktrees_dir.exists() {
        let _ = std::fs::remove_dir_all(&worktrees_dir);
    }

    let cond_dir = repo_root.join(".cond");
    if cond_dir.exists() {
        std::fs::remove_dir_all(&cond_dir)?;
    }

    println!("nuked everything");

    Ok(())
}

pub fn prune(state: &mut CondState) -> Result<()> {
    let before = state.tasks.len();
    state.tasks.retain(|t| t.status != TaskStatus::Cleaned);
    let removed = before - state.tasks.len();

    if removed == 0 {
        println!("nothing to prune");
    } else {
        println!("pruned {removed} cleaned task(s)");
    }

    Ok(())
}

pub fn diff(repo_root: &Path, state: &CondState, query: &str) -> Result<()> {
    let task = state.find_task(query)?;
    let worktree_abs = repo_root.join(&task.worktree_path);
    let base = util::default_branch(repo_root)?;
    let range = format!("{base}..HEAD");

    // Commit count
    let commit_count = util::run("git", &["rev-list", "--count", &range], Some(&worktree_abs))
        .unwrap_or_else(|_| "?".to_string());

    // Header
    eprintln!("\x1b[1;36m{}\x1b[0m", task.description);
    eprintln!(
        "\x1b[2m{}  •  {} commit(s) ahead of {base}\x1b[0m",
        task.branch, commit_count
    );
    eprintln!();

    // File-level stat summary, then full coloured diff
    util::run_inherit(
        "git",
        &["diff", "--color=always", "--stat", "-p", &range],
        Some(&worktree_abs),
    )?;

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CondState, Task, TaskStatus};
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
            next_id: tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1,
            repo_root: "/tmp/repo".to_string(),
            tasks,
        }
    }

    // --- truncate ---

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_just_over() {
        assert_eq!(truncate("abcdef", 5), "ab...");
    }

    // --- prune ---

    #[test]
    fn prune_removes_cleaned_tasks() {
        let mut state = make_state(vec![
            make_task(1, "active task", TaskStatus::Active),
            make_task(2, "cleaned task", TaskStatus::Cleaned),
            make_task(3, "merged task", TaskStatus::Merged),
            make_task(4, "another cleaned", TaskStatus::Cleaned),
        ]);

        prune(&mut state).unwrap();

        assert_eq!(state.tasks.len(), 2);
        assert_eq!(state.tasks[0].id, 1);
        assert_eq!(state.tasks[1].id, 3);
    }

    #[test]
    fn prune_nothing_to_prune() {
        let mut state = make_state(vec![make_task(1, "active", TaskStatus::Active)]);

        prune(&mut state).unwrap();
        assert_eq!(state.tasks.len(), 1);
    }

    #[test]
    fn prune_empty_state() {
        let mut state = make_state(vec![]);
        prune(&mut state).unwrap();
        assert!(state.tasks.is_empty());
    }

    // --- nuke without confirm ---

    #[test]
    fn nuke_without_confirm_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = make_state(vec![make_task(1, "task", TaskStatus::Active)]);

        nuke(dir.path(), &mut state, false).unwrap();

        // Task should still be there since we didn't confirm
        assert_eq!(state.tasks.len(), 1);
        assert_eq!(state.tasks[0].status, TaskStatus::Active);
    }
}
