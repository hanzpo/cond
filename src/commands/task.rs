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

    println!("spawned task {id}: \"{description}\"");
    println!("  branch:   {branch}");
    println!("  worktree: {worktree_rel}");

    Ok(())
}

pub fn status(state: &CondState) -> Result<()> {
    if state.tasks.is_empty() {
        println!("no tasks");
        return Ok(());
    }

    println!(
        "{:<4} {:<11} {:<42} {}",
        "ID", "STATUS", "BRANCH", "DESCRIPTION"
    );
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

pub fn kill(repo_root: &Path, state: &mut CondState, id: u32) -> Result<()> {
    let task = state.find_task(id)?;
    let branch = task.branch.clone();
    let worktree_path = repo_root.join(&task.worktree_path);
    let is_merged = task.status == TaskStatus::Merged;

    if worktree_path.exists() {
        let _ = util::run(
            "git",
            &["worktree", "remove", &worktree_path.to_string_lossy(), "--force"],
            Some(repo_root),
        );
    }

    if !is_merged {
        let _ = util::run("git", &["branch", "-D", &branch], Some(repo_root));
    }

    let task = state.find_task_mut(id)?;
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
        if let Err(e) = kill(repo_root, state, id) {
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

pub fn diff(repo_root: &Path, state: &CondState, id: u32) -> Result<()> {
    let task = state.find_task(id)?;
    let worktree_abs = repo_root.join(&task.worktree_path);
    util::run_inherit("git", &["diff", "main..HEAD"], Some(&worktree_abs))?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
