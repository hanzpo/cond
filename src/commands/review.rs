use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use crate::state::{CondState, TaskStatus};
use crate::util;

fn require_claude() -> Result<()> {
    if !util::check_on_path("claude") {
        anyhow::bail!(
            "claude CLI not found on PATH.\n\
             Install it from https://docs.anthropic.com/en/docs/claude-code and run `claude` once to authenticate."
        );
    }
    Ok(())
}

pub fn review(repo_root: &Path, state: &CondState, query: &str) -> Result<()> {
    require_claude()?;

    let task = state.find_task(query)?;
    let worktree_abs = repo_root.join(&task.worktree_path);
    let description = &task.description;
    let branch = &task.branch;
    let base = util::default_branch(repo_root)?;

    let id = task.id;
    eprintln!("\x1b[1;36mReviewing task {id}:\x1b[0m {description}");

    let diff = util::run_spin(
        "git",
        &["diff", &format!("{base}..{branch}")],
        Some(&worktree_abs),
        "Collecting diff…",
    )?;

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

    eprintln!("Opening Claude for interactive review…");
    // Pass prompt as positional argument for interactive mode
    util::run_inherit("claude", &[&prompt], Some(&worktree_abs))?;

    Ok(())
}

pub fn pr(
    repo_root: &Path,
    state: &mut CondState,
    query: &str,
    title: Option<&str>,
    draft: bool,
) -> Result<()> {
    let task = state.find_task(query)?;
    let id = task.id;

    // If a PR already exists, ask whether to regenerate title/description
    if let Some(pr_num) = task.pr_number {
        let url = task.pr_url.as_deref().unwrap_or("unknown");
        eprintln!("task {id} already has PR #{pr_num}: {url}");
        if !util::confirm("regenerate title and description?") {
            anyhow::bail!("aborted");
        }
        return regenerate_pr(repo_root, state, query, title, pr_num);
    }

    // Don't create PRs for tasks that are already done
    if task.status == TaskStatus::Merged || task.status == TaskStatus::Cleaned {
        anyhow::bail!("task {id} is already {}", task.status);
    }

    require_claude()?;

    eprintln!("\x1b[1;36mCreating PR for task {id}\x1b[0m");

    let worktree_abs = repo_root.join(&task.worktree_path);
    let branch = task.branch.clone();
    let description = task.description.clone();

    // Get diff for claude to analyze
    let base = util::default_branch(repo_root)?;
    let diff = util::run_spin(
        "git",
        &["diff", &format!("{base}..HEAD")],
        Some(&worktree_abs),
        "Collecting diff…",
    )?;
    if diff.is_empty() {
        anyhow::bail!("no changes to create a PR for task {id}");
    }

    // Use claude to generate PR content
    let claude_prompt = format!(
        r#"Generate a PR title, description, and branch name for these changes.
The original task description was: "{description}"

<diff>
{diff}
</diff>

Output ONLY a valid JSON object with these fields (no markdown fences, no extra text):
- "title": concise PR title (under 70 chars)
- "description": markdown PR description (2-5 sentences summarizing the changes)
- "branch": short kebab-case branch name (under 40 chars, no "cond/" prefix)"#
    );

    let (pr_title, pr_body, new_branch_slug) = match util::run_with_stdin_spin(
        "claude",
        &["-p"],
        &claude_prompt,
        Some(&worktree_abs),
        "Claude is generating PR title and description…",
    ) {
        Ok(output) => parse_claude_pr_output(&output, &description, id),
        Err(e) => {
            eprintln!("warning: claude failed ({e}), using defaults");
            (
                description.clone(),
                format!("Task #{id}: {description}\n\nCreated by cond."),
                None::<String>,
            )
        }
    };

    // Use user-provided title if specified, otherwise use claude's
    let pr_title = title.map(|t| t.to_string()).unwrap_or(pr_title);

    // Rename branch if claude suggested a better name
    let final_branch = if let Some(slug) = new_branch_slug {
        let new_branch = format!("cond/task-{id}-{slug}");
        if new_branch != branch {
            util::run(
                "git",
                &["branch", "-m", &branch, &new_branch],
                Some(&worktree_abs),
            )?;
            // Delete the old remote branch if it was already pushed
            let _ = util::run(
                "git",
                &["push", "origin", "--delete", &branch],
                Some(&worktree_abs),
            );
            // Update state with new branch name
            let task = state.find_task_mut(query)?;
            task.branch = new_branch.clone();
            task.updated_at = Utc::now();
            new_branch
        } else {
            branch
        }
    } else {
        branch
    };

    // Push the branch
    util::run_spin(
        "git",
        &["push", "-u", "origin", &final_branch],
        Some(&worktree_abs),
        "Pushing branch…",
    )?;

    // Create PR
    let mut args = vec![
        "pr",
        "create",
        "--base",
        &base,
        "--head",
        &final_branch,
        "--title",
        &pr_title,
        "--body",
        &pr_body,
    ];
    if draft {
        args.push("--draft");
    }

    let output = util::run_spin("gh", &args, Some(&worktree_abs), "Creating pull request…")?;
    let pr_url = output.trim().to_string();
    let pr_number = pr_url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok());

    let task = state.find_task_mut(query)?;
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

/// Regenerate the title and description of an existing PR using Claude.
fn regenerate_pr(
    repo_root: &Path,
    state: &mut CondState,
    query: &str,
    title_override: Option<&str>,
    pr_number: u32,
) -> Result<()> {
    require_claude()?;

    let task = state.find_task(query)?;
    let id = task.id;
    let worktree_abs = repo_root.join(&task.worktree_path);
    let description = task.description.clone();
    let branch = task.branch.clone();
    let base = util::default_branch(repo_root)?;

    // Push any new commits first
    util::run_spin(
        "git",
        &["push", "-u", "origin", &branch],
        Some(&worktree_abs),
        "Pushing latest changes…",
    )?;

    let diff = util::run_spin(
        "git",
        &["diff", &format!("{base}..HEAD")],
        Some(&worktree_abs),
        "Collecting diff…",
    )?;
    if diff.is_empty() {
        anyhow::bail!("no changes found for task {id}");
    }

    let claude_prompt = format!(
        r#"Generate a PR title, description, and branch name for these changes.
The original task description was: "{description}"

<diff>
{diff}
</diff>

Output ONLY a valid JSON object with these fields (no markdown fences, no extra text):
- "title": concise PR title (under 70 chars)
- "description": markdown PR description (2-5 sentences summarizing the changes)
- "branch": short kebab-case branch name (under 40 chars, no "cond/" prefix)"#
    );

    let (pr_title, pr_body, _) = match util::run_with_stdin_spin(
        "claude",
        &["-p"],
        &claude_prompt,
        Some(&worktree_abs),
        "Claude is generating new PR title and description…",
    ) {
        Ok(output) => parse_claude_pr_output(&output, &description, id),
        Err(e) => {
            eprintln!("warning: claude failed ({e}), using defaults");
            (
                description.clone(),
                format!("Task #{id}: {description}\n\nCreated by cond."),
                None::<String>,
            )
        }
    };

    let pr_title = title_override.map(|t| t.to_string()).unwrap_or(pr_title);

    let pr_num_str = pr_number.to_string();
    util::run_spin(
        "gh",
        &[
            "pr",
            "edit",
            &pr_num_str,
            "--title",
            &pr_title,
            "--body",
            &pr_body,
        ],
        Some(&worktree_abs),
        "Updating pull request…",
    )?;

    let task = state.find_task_mut(query)?;
    task.updated_at = Utc::now();

    let url = task.pr_url.as_deref().unwrap_or("unknown");
    println!("PR #{pr_number} updated for task {id}: {url}");

    Ok(())
}

/// Parse claude's JSON output for PR content. Returns (title, body, optional branch slug).
fn parse_claude_pr_output(
    output: &str,
    fallback_description: &str,
    task_id: u32,
) -> (String, String, Option<String>) {
    // Try to extract JSON from the output (claude may wrap in ```json blocks)
    let json_str = extract_json(output);

    if let Some(json_str) = json_str {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
            let title = val["title"]
                .as_str()
                .unwrap_or(fallback_description)
                .to_string();
            let description = val["description"]
                .as_str()
                .unwrap_or(&format!(
                    "Task #{task_id}: {fallback_description}\n\nCreated by cond."
                ))
                .to_string();
            let branch = val["branch"].as_str().map(|s| {
                // Sanitize: ensure it's a valid branch slug
                crate::util::slugify(s)
            });
            return (title, description, branch);
        }
    }

    // Fallback
    (
        fallback_description.to_string(),
        format!("Task #{task_id}: {fallback_description}\n\nCreated by cond."),
        None,
    )
}

/// Extract JSON object from text that may contain markdown fences or other content.
fn extract_json(text: &str) -> Option<&str> {
    // Try ```json blocks
    if let Some(start) = text.find("```json") {
        let start = start + 7;
        if let Some(end) = text[start..].find("```") {
            return Some(text[start..start + end].trim());
        }
    }
    // Try ``` blocks
    if let Some(start) = text.find("```\n") {
        let start = start + 4;
        if let Some(end) = text[start..].find("```") {
            return Some(text[start..start + end].trim());
        }
    }
    // Try raw JSON object
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Some(&text[start..=end]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_json ---

    #[test]
    fn extract_json_from_fenced_block() {
        let input = r#"Here is the output:
```json
{"title": "Fix bug", "description": "Fixes the login bug"}
```
Done."#;
        let result = extract_json(input).unwrap();
        assert!(result.contains("Fix bug"));
    }

    #[test]
    fn extract_json_from_plain_fence() {
        let input = "```\n{\"title\": \"hello\"}\n```";
        let result = extract_json(input).unwrap();
        assert!(result.contains("hello"));
    }

    #[test]
    fn extract_json_raw_object() {
        let input = r#"{"title": "raw json"}"#;
        let result = extract_json(input).unwrap();
        assert!(result.contains("raw json"));
    }

    #[test]
    fn extract_json_embedded_in_text() {
        let input = r#"Here is the answer: {"title": "embedded"} and more text"#;
        let result = extract_json(input).unwrap();
        assert!(result.starts_with('{'));
        assert!(result.ends_with('}'));
        assert!(result.contains("embedded"));
    }

    #[test]
    fn extract_json_no_json() {
        assert!(extract_json("just plain text").is_none());
    }

    #[test]
    fn extract_json_empty() {
        assert!(extract_json("").is_none());
    }

    // --- parse_claude_pr_output ---

    #[test]
    fn parse_valid_json_output() {
        let output = r#"{"title": "Fix login bug", "description": "Fixes the auth flow", "branch": "fix-login"}"#;
        let (title, body, branch) = parse_claude_pr_output(output, "fallback", 1);
        assert_eq!(title, "Fix login bug");
        assert_eq!(body, "Fixes the auth flow");
        assert_eq!(branch.unwrap(), "fix-login");
    }

    #[test]
    fn parse_json_missing_fields_uses_fallback() {
        let output = r#"{}"#;
        let (title, body, branch) = parse_claude_pr_output(output, "my fallback", 5);
        assert_eq!(title, "my fallback");
        assert!(body.contains("Task #5"));
        assert!(branch.is_none());
    }

    #[test]
    fn parse_invalid_json_uses_fallback() {
        let output = "this is not json at all";
        let (title, body, branch) = parse_claude_pr_output(output, "fallback desc", 3);
        assert_eq!(title, "fallback desc");
        assert!(body.contains("Task #3"));
        assert!(branch.is_none());
    }

    #[test]
    fn parse_json_in_fenced_block() {
        let output = r#"Sure! Here's the PR content:
```json
{"title": "Add search", "description": "Adds search functionality", "branch": "add-search"}
```"#;
        let (title, body, branch) = parse_claude_pr_output(output, "fallback", 1);
        assert_eq!(title, "Add search");
        assert_eq!(body, "Adds search functionality");
        assert!(branch.is_some());
    }

    #[test]
    fn parse_branch_gets_slugified() {
        let output = r#"{"title": "t", "description": "d", "branch": "My Branch Name!!"}"#;
        let (_, _, branch) = parse_claude_pr_output(output, "f", 1);
        assert_eq!(branch.unwrap(), "my-branch-name");
    }
}

pub fn merge(
    repo_root: &Path,
    state: &mut CondState,
    query: &str,
    squash: bool,
    force: bool,
) -> Result<()> {
    let task = state.find_task(query)?;
    let id = task.id;
    let branch = task.branch.clone();
    let worktree_path = repo_root.join(&task.worktree_path);
    let pr_number = task
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("task {id} has no PR — run `cond pr {id}` first"))?;

    let pr_num_str = pr_number.to_string();

    // Check for uncommitted or unpushed changes unless --force
    if !force && worktree_path.exists() {
        let mut warnings = Vec::new();

        // Check for uncommitted changes
        if let Ok(status) = util::run("git", &["status", "--porcelain"], Some(&worktree_path)) {
            if !status.is_empty() {
                warnings.push("uncommitted changes");
            }
        }

        // Check for unpushed commits
        if let Ok(log) = util::run(
            "git",
            &["log", &format!("origin/{branch}..HEAD"), "--oneline"],
            Some(&worktree_path),
        ) {
            if !log.is_empty() {
                warnings.push("unpushed commits");
            }
        }

        if !warnings.is_empty() {
            let warn_str = warnings.join(" and ");
            eprintln!("warning: task {id} has {warn_str}");
            eprintln!("these changes will be lost after merge — this is not recommended.");
            if !util::confirm("merge anyway?") {
                anyhow::bail!("aborted");
            }
        }
    }

    // Merge the PR (without --delete-branch, we handle cleanup ourselves)
    let mut args = vec!["pr", "merge", &pr_num_str];
    if squash {
        args.push("--squash");
    }

    eprintln!("\x1b[1;36mMerging task {id}\x1b[0m (PR #{pr_number})");

    if let Err(e) = util::run_spin("gh", &args, Some(repo_root), "Merging PR on GitHub…") {
        // If the PR is already merged, continue with cleanup
        let state_json = util::run(
            "gh",
            &["pr", "view", &pr_num_str, "--json", "state", "-q", ".state"],
            Some(repo_root),
        )
        .unwrap_or_default();
        if state_json != "MERGED" {
            return Err(e);
        }
        eprintln!("PR already merged, continuing with cleanup…");
    }

    // Only remove worktree + local branch after merge succeeds
    // Worktree must be removed before branch deletion, otherwise git refuses
    if worktree_path.exists() {
        eprintln!("Cleaning up worktree and branch…");
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
    let _ = util::run("git", &["branch", "-D", &branch], Some(repo_root));
    // Clean up the remote branch
    let _ = util::run(
        "git",
        &["push", "origin", "--delete", &branch],
        Some(repo_root),
    );

    // Update state
    let task = state.find_task_mut(query)?;
    task.status = TaskStatus::Merged;
    task.updated_at = Utc::now();

    eprintln!("\x1b[1;32mDone!\x1b[0m Task {id} merged and cleaned up.");
    // Print repo root to stdout so the shell wrapper can cd there
    println!("{}", repo_root.display());

    Ok(())
}
