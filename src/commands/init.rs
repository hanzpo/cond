use anyhow::{bail, Result};
use std::io::{self, Write};
use std::path::Path;

use crate::commands::shell;
use crate::state::CondState;
use crate::util;

pub fn init() -> Result<()> {
    let repo_root = util::repo_root()?;
    let cond_dir = repo_root.join(".cond");
    let state_path = cond_dir.join("state.json");

    if state_path.exists() {
        println!("already initialized");
        return Ok(());
    }

    if !util::check_on_path("git") {
        bail!("git is not installed or not in PATH");
    }
    if !util::check_on_path("gh") {
        eprintln!("warning: gh (GitHub CLI) not found — pr/merge commands won't work");
    }
    if !util::check_on_path("claude") {
        eprintln!("warning: claude (Claude Code CLI) not found — review command won't work");
    }

    std::fs::create_dir_all(&cond_dir)?;
    std::fs::create_dir_all(repo_root.join(".cond-worktrees"))?;

    add_to_gitignore(&repo_root, ".cond-worktrees/")?;

    let state = CondState {
        version: 1,
        next_id: 1,
        repo_root: repo_root.to_string_lossy().to_string(),
        tasks: vec![],
    };
    state.save(&repo_root)?;

    println!("initialized cond in {}", repo_root.display());

    if !shell::is_shell_setup() {
        eprintln!();
        eprint!("set up shell integration so `cond cd` works natively? [y/N] ");
        io::stderr().flush()?;

        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;

        if answer.trim().eq_ignore_ascii_case("y") {
            let rc_path = shell::rc_path()?;
            let line = "\neval \"$(cond shell-setup)\"\n";

            // check if already present
            let contents = std::fs::read_to_string(&rc_path).unwrap_or_default();
            if contents.contains("cond shell-setup") {
                eprintln!("already in {}", rc_path.display());
            } else {
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&rc_path)?;
                file.write_all(line.as_bytes())?;
                eprintln!("added to {} — restart your shell or run: source {}", rc_path.display(), rc_path.display());
            }
        } else {
            eprintln!("skipped. you can set it up later with:");
            eprintln!("  eval \"$(cond shell-setup)\"");
        }
    }

    Ok(())
}

fn add_to_gitignore(repo_root: &Path, entry: &str) -> Result<()> {
    let gitignore = repo_root.join(".gitignore");
    if gitignore.exists() {
        let contents = std::fs::read_to_string(&gitignore)?;
        if contents.lines().any(|line| line.trim() == entry) {
            return Ok(());
        }
        let prefix = if contents.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&gitignore, format!("{contents}{prefix}{entry}\n"))?;
    } else {
        std::fs::write(&gitignore, format!("{entry}\n"))?;
    }
    Ok(())
}
