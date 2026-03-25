use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::process::Command;

/// Run a command, capture stdout, return trimmed output. Errors on non-zero exit.
pub fn run(cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let output = command.output().with_context(|| format!("failed to run {cmd}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} {} failed: {}", cmd, args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a command, inheriting stdin/stdout/stderr.
pub fn run_inherit(cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let status = command.status().with_context(|| format!("failed to run {cmd}"))?;
    if !status.success() {
        anyhow::bail!("{} {} failed with exit code {:?}", cmd, args.join(" "), status.code());
    }
    Ok(())
}

/// Check if a command exists on PATH.
pub fn check_on_path(binary: &str) -> bool {
    Command::new("which")
        .arg(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get the git repo root.
///
/// If the current working directory no longer exists (e.g. after a worktree
/// was removed), walks up the path to find a valid ancestor before asking git.
pub fn repo_root() -> Result<std::path::PathBuf> {
    // Determine a valid cwd to run git from. If the real cwd was deleted
    // (e.g. worktree removed), walk up until we find a directory that exists.
    let cwd = std::env::current_dir().ok();
    let run_dir = match &cwd {
        Some(dir) if dir.exists() => None, // use default cwd
        _ => {
            // CWD is gone — walk up from the original path to find an existing ancestor
            let mut dir = cwd.unwrap_or_else(|| std::path::PathBuf::from("/"));
            while !dir.exists() {
                if !dir.pop() {
                    break;
                }
            }
            Some(dir)
        }
    };

    let git_cwd = run_dir.as_deref();

    // --show-toplevel gives us the cwd repo/worktree root (needed to resolve relative paths)
    let toplevel = run("git", &["rev-parse", "--show-toplevel"], git_cwd)?;
    // --git-common-dir points to the main repo's .git even from a worktree
    let git_common = run("git", &["rev-parse", "--git-common-dir"], git_cwd)?;
    let git_common = std::path::PathBuf::from(&toplevel).join(&git_common);

    // .git/worktrees/foo -> .git -> repo root
    // .git (bare) -> repo root
    let root = git_common
        .parent()
        .ok_or_else(|| anyhow::anyhow!("could not determine repo root"))?;

    Ok(std::fs::canonicalize(root)?)
}

/// Run a command with stdin piped, capture stdout. Errors on non-zero exit.
pub fn run_with_stdin(cmd: &str, args: &[&str], stdin_data: &str, cwd: Option<&Path>) -> Result<String> {
    use std::io::Write;
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let mut child = command.spawn().with_context(|| format!("failed to run {cmd}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} {} failed: {}", cmd, args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Create a styled spinner with a message.
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Run a command with a spinner, capture stdout. Errors on non-zero exit.
pub fn run_spin(cmd: &str, args: &[&str], cwd: Option<&Path>, message: &str) -> Result<String> {
    let pb = spinner(message);
    let result = run(cmd, args, cwd);
    pb.finish_and_clear();
    result
}

/// Run a command with stdin piped and a spinner. Errors on non-zero exit.
pub fn run_with_stdin_spin(
    cmd: &str,
    args: &[&str],
    stdin_data: &str,
    cwd: Option<&Path>,
    message: &str,
) -> Result<String> {
    let pb = spinner(message);
    let result = run_with_stdin(cmd, args, stdin_data, cwd);
    pb.finish_and_clear();
    result
}

/// Detect if CWD is inside a task worktree, return task ID if so.
pub fn detect_task_from_cwd(state: &crate::state::CondState, repo_root: &Path) -> Option<u32> {
    let cwd = std::env::current_dir().ok()?;
    let cwd = std::fs::canonicalize(&cwd).ok()?;
    for task in &state.tasks {
        let worktree_abs = repo_root.join(&task.worktree_path);
        if let Ok(worktree_canon) = std::fs::canonicalize(&worktree_abs) {
            if cwd.starts_with(&worktree_canon) {
                return Some(task.id);
            }
        }
    }
    None
}

/// Prompt the user for yes/no confirmation. Returns true if they confirm.
pub fn confirm(message: &str) -> bool {
    use std::io::Write;
    eprint!("{message} [y/N] ");
    std::io::stderr().flush().ok();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Detect the default branch (e.g. main, master) from the remote.
pub fn default_branch(repo_root: &Path) -> Result<String> {
    // Try the symbolic ref first (most reliable when remote is configured)
    if let Ok(out) = run(
        "git",
        &["symbolic-ref", "refs/remotes/origin/HEAD"],
        Some(repo_root),
    ) {
        if let Some(branch) = out.rsplit('/').next() {
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }
    // Fallback: check if "main" or "master" exists locally
    if run("git", &["rev-parse", "--verify", "main"], Some(repo_root)).is_ok() {
        return Ok("main".to_string());
    }
    if run("git", &["rev-parse", "--verify", "master"], Some(repo_root)).is_ok() {
        return Ok("master".to_string());
    }
    anyhow::bail!("could not detect default branch — set origin HEAD with: git remote set-head origin --auto")
}

/// Slugify a description for use in branch names.
pub fn slugify(text: &str) -> String {
    let slug: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    let result = result.trim_matches('-').to_string();
    if result.len() > 40 {
        result[..40].trim_end_matches('-').to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- slugify ---

    #[test]
    fn slugify_simple() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("Fix bug #123!"), "fix-bug-123");
    }

    #[test]
    fn slugify_consecutive_separators() {
        assert_eq!(slugify("foo---bar   baz"), "foo-bar-baz");
    }

    #[test]
    fn slugify_leading_trailing_separators() {
        assert_eq!(slugify("  -hello- "), "hello");
    }

    #[test]
    fn slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_truncates_long_strings() {
        let long = "a".repeat(60);
        let result = slugify(&long);
        assert!(result.len() <= 40);
        assert_eq!(result, "a".repeat(40));
    }

    #[test]
    fn slugify_truncation_strips_trailing_hyphens() {
        // 39 a's + space + more = slug will be "aaa...a-bbb" and truncated at 40
        let input = format!("{} {}", "a".repeat(39), "bbbbb");
        let result = slugify(&input);
        assert!(result.len() <= 40);
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn slugify_unicode() {
        // Rust's is_alphanumeric() treats accented chars as alphanumeric
        assert_eq!(slugify("café résumé"), "café-résumé");
    }

    #[test]
    fn slugify_only_special_chars() {
        assert_eq!(slugify("!!!@@@###"), "");
    }

    #[test]
    fn slugify_mixed_case() {
        assert_eq!(slugify("MyTaskName"), "mytaskname");
    }

    #[test]
    fn slugify_numbers_only() {
        assert_eq!(slugify("123"), "123");
    }

    // --- check_on_path ---

    #[test]
    fn check_on_path_finds_git() {
        assert!(check_on_path("git"));
    }

    #[test]
    fn check_on_path_nonexistent() {
        assert!(!check_on_path("definitely-not-a-real-binary-xyzzy"));
    }

    // --- run ---

    #[test]
    fn run_captures_stdout() {
        let out = run("echo", &["hello"], None).unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn run_fails_on_bad_command() {
        let result = run("false", &[], None);
        assert!(result.is_err());
    }

    #[test]
    fn run_with_cwd() {
        let out = run("pwd", &[], Some(Path::new("/tmp"))).unwrap();
        assert!(out.contains("tmp") || out.contains("private/tmp"));
    }

    // --- run_with_stdin ---

    #[test]
    fn run_with_stdin_pipes_data() {
        let out = run_with_stdin("cat", &[], "hello from stdin", None).unwrap();
        assert_eq!(out, "hello from stdin");
    }

    #[test]
    fn run_with_stdin_fails_on_bad_exit() {
        let result = run_with_stdin("false", &[], "", None);
        assert!(result.is_err());
    }
}
