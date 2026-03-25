mod commands;
mod state;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cond",
    about = "Git worktree agent orchestrator",
    infer_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize cond in the current repo
    Init,

    /// Create a new task (worktree + branch)
    Spawn {
        /// Task description
        description: String,
    },

    /// Create a new task (alias for spawn)
    New {
        /// Task description
        description: String,
    },

    /// Show status of all tasks
    Status,

    /// Show status of all tasks (alias for status)
    Ls,

    /// Open claude in a task's worktree to review changes
    Review {
        /// Task ID or name (auto-detected if inside a worktree)
        task: Option<String>,
    },

    /// Create a GitHub PR for a task
    Pr {
        /// Task ID or name (auto-detected if inside a worktree)
        task: Option<String>,
        /// PR title (defaults to task description)
        #[arg(long)]
        title: Option<String>,
        /// Create as draft PR
        #[arg(long)]
        draft: bool,
    },

    /// Merge the PR for a task
    Merge {
        /// Task ID or name (auto-detected if inside a worktree)
        task: Option<String>,
        /// Squash merge
        #[arg(long, default_value_t = true)]
        squash: bool,
        /// Skip uncommitted/unpushed changes check
        #[arg(long)]
        force: bool,
    },

    /// Change directory to a task's worktree (or repo root if no task given)
    Cd {
        /// Task ID or name (omit to get repo root)
        task: Option<String>,
    },

    /// Remove cleaned tasks from state
    Prune,

    /// Show the diff for a task's branch against main
    Diff {
        /// Task ID or name (auto-detected if inside a worktree)
        task: Option<String>,
    },

    /// Remove a task's worktree and branch
    Kill {
        /// Task ID or name
        task: String,
    },

    /// Remove a task's worktree and branch (alias for kill)
    Rm {
        /// Task ID or name
        task: String,
    },

    /// Change directory to the repo root
    Base,

    /// Print shell integration for eval (add `eval "$(cond shell-setup)"` to your shell rc)
    #[command(hide = true)]
    ShellSetup,

    /// Kill all tasks and tear down everything
    Nuke {
        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },
}

/// Resolve task query: use provided value or auto-detect from current worktree.
fn resolve_task_query(
    state: &state::CondState,
    repo_root: &std::path::Path,
    task: Option<&str>,
) -> Result<String> {
    if let Some(t) = task {
        Ok(t.to_string())
    } else {
        util::detect_task_from_cwd(state, repo_root)
            .map(|id| id.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!("not inside a task worktree — provide a task ID or name")
            })
    }
}

/// Ensure shell integration is active (COND_SHELL env var set).
fn ensure_shell_setup() -> Result<()> {
    if !commands::shell::is_shell_setup() {
        let rc = commands::shell::rc_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.zshrc".to_string());
        anyhow::bail!(
            "shell integration not active. Run:\n\n  source {}\n\nIf not yet added, run `cond init` first.",
            rc
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Enforce shell setup for all commands except init, shell-setup, and base
    match &cli.command {
        Commands::Init | Commands::ShellSetup => {}
        _ => ensure_shell_setup()?,
    }

    match cli.command {
        Commands::Init => {
            commands::init::init()?;
        }
        Commands::Spawn { description } | Commands::New { description } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::spawn(&repo_root, &mut state, &description)?;
            state.save(&repo_root)?;
        }
        Commands::Status | Commands::Ls => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            commands::task::status(&state)?;
        }
        Commands::Review { task } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            let query = resolve_task_query(&state, &repo_root, task.as_deref())?;
            commands::review::review(&repo_root, &state, &query)?;
        }
        Commands::Pr { task, title, draft } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            let query = resolve_task_query(&state, &repo_root, task.as_deref())?;
            commands::review::pr(&repo_root, &mut state, &query, title.as_deref(), draft)?;
            state.save(&repo_root)?;
        }
        Commands::Merge {
            task,
            squash,
            force,
        } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            let query = resolve_task_query(&state, &repo_root, task.as_deref())?;
            commands::review::merge(&repo_root, &mut state, &query, squash, force)?;
            state.save(&repo_root)?;
        }
        Commands::Base => {
            let repo_root = util::repo_root()?;
            println!("{}", repo_root.display());
        }
        Commands::Prune => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::prune(&mut state)?;
            state.save(&repo_root)?;
        }
        Commands::ShellSetup => {
            commands::shell::shell_setup()?;
        }
        Commands::Cd { task } => {
            let repo_root = util::repo_root()?;
            if let Some(task) = task {
                let state = state::CondState::load(&repo_root)?;
                let found = state.find_task(&task)?;
                println!("{}", repo_root.join(&found.worktree_path).display());
            } else {
                // No task specified = go to repo root
                println!("{}", repo_root.display());
            }
        }
        Commands::Diff { task } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            let query = resolve_task_query(&state, &repo_root, task.as_deref())?;
            commands::task::diff(&repo_root, &state, &query)?;
        }
        Commands::Kill { task } | Commands::Rm { task } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::kill(&repo_root, &mut state, &task)?;
            state.save(&repo_root)?;
        }
        Commands::Nuke { confirm } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::nuke(&repo_root, &mut state, confirm)?;
        }
    }

    Ok(())
}
