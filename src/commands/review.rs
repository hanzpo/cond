use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use crate::state::{CondState, TaskStatus};
use crate::util;

pub fn review(repo_root: &Path, state: &CondState, id: u32) -> Result<()> {
    let task = state.find_task(id)?;
    let worktree_abs = repo_root.join(&task.worktree_path);
    let description = &task.description;
    let branch = &task.branch;

    let diff = util::run("git", &["diff", &format!("main..{branch}")], Some(&worktree_abs))?;

    if diff.is_empty() {
        println!("no changes to review for task {id}");
        return Ok(());
    }

    let prompt = format!(
        r#"You are reviewing code for task: "{description}"

<diff>
{diff}
</diff>

Review for correctness, bugs, style, and security. If changes are needed, make them directly. If the code is good, say so."#
    );

    util::run_inherit("claude", &["--prompt", &prompt], Some(&worktree_abs))?;

    Ok(())
}

pub fn pr(
    repo_root: &Path,
    state: &mut CondState,
    id: u32,
    title: Option<&str>,
    draft: bool,
) -> Result<()> {
    let task = state.find_task(id)?;
    let worktree_abs = repo_root.join(&task.worktree_path);
    let branch = &task.branch;
    let description = &task.description;
    let pr_title = title.unwrap_or(description);

    // Push the branch
    util::run("git", &["push", "-u", "origin", branch], Some(&worktree_abs))?;

    // Create PR
    let body = format!("Task #{id}: {description}\n\nCreated by cond.");
    let mut args = vec![
        "pr", "create", "--base", "main", "--head", branch,
        "--title", pr_title, "--body", &body,
    ];
    if draft {
        args.push("--draft");
    }

    let output = util::run("gh", &args, Some(&worktree_abs))?;
    let pr_url = output.trim().to_string();
    let pr_number = pr_url.rsplit('/').next().and_then(|s| s.parse::<u32>().ok());

    let task = state.find_task_mut(id)?;
    task.pr_url = Some(pr_url.clone());
    task.pr_number = pr_number;
    task.status = TaskStatus::PrCreated;
    task.updated_at = Utc::now();

    if let Some(num) = pr_number {
        println!("PR #{num} created for task {id}: {pr_url}");
    } else {
        println!("PR created for task {id}: {pr_url}");
    }

    Ok(())
}

pub fn merge(
    repo_root: &Path,
    state: &mut CondState,
    id: u32,
    squash: bool,
    delete_branch: bool,
) -> Result<()> {
    let task = state.find_task(id)?;
    let pr_number = task
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("task {id} has no PR — run `cond pr {id}` first"))?;

    let pr_num_str = pr_number.to_string();
    let mut args = vec!["pr", "merge", &pr_num_str];
    if squash {
        args.push("--squash");
    }
    if delete_branch {
        args.push("--delete-branch");
    }

    util::run("gh", &args, Some(repo_root))?;

    let task = state.find_task_mut(id)?;
    task.status = TaskStatus::Merged;
    task.updated_at = Utc::now();

    println!("merged task {id} (PR #{pr_number})");

    Ok(())
}
