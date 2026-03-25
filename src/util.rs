use anyhow::{Context, Result};
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
pub fn repo_root() -> Result<std::path::PathBuf> {
    let root = run("git", &["rev-parse", "--show-toplevel"], None)?;
    Ok(std::path::PathBuf::from(root))
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
