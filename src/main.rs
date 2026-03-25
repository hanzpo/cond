mod commands;
mod state;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cond", about = "Git worktree agent orchestrator", infer_subcommands = true)]
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

    /// Show status of all tasks
    Status,

    /// Show status of all tasks (alias for status)
    Ls,

    /// Open claude in a task's worktree to review changes
    Review {
        /// Task ID or name
        task: String,
    },

    /// Create a GitHub PR for a task
    Pr {
        /// Task ID or name
        task: String,
        /// PR title (defaults to task description)
        #[arg(long)]
        title: Option<String>,
        /// Create as draft PR
        #[arg(long)]
        draft: bool,
    },

    /// Merge the PR for a task
    Merge {
        /// Task ID or name
        task: String,
        /// Squash merge
        #[arg(long, default_value_t = true)]
        squash: bool,
        /// Delete branch after merge
        #[arg(long, default_value_t = true)]
        delete_branch: bool,
    },

    /// Print the worktree path for a task, or "root" for repo root
    Cd {
        /// Task ID, name, or "root"
        task: String,
    },

    /// Remove cleaned tasks from state
    Prune,

    /// Show the diff for a task's branch against main
    Diff {
        /// Task ID or name
        task: String,
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

    /// Print the repo base (root) path
    Base,

    /// Print shell integration for eval (add `eval "$(cond shell-setup)"` to your shell rc)
    ShellSetup,

    /// Kill all tasks and tear down everything
    Nuke {
        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            commands::init::init()?;
        }
        Commands::Spawn { description } => {
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
            commands::review::review(&repo_root, &state, &task)?;
        }
        Commands::Pr { task, title, draft } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::review::pr(&repo_root, &mut state, &task, title.as_deref(), draft)?;
            state.save(&repo_root)?;
        }
        Commands::Merge { task, squash, delete_branch } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::review::merge(&repo_root, &mut state, &task, squash, delete_branch)?;
            state.save(&repo_root)?;
        }
        Commands::Base => {
            let repo_root = util::repo_root()?;
            print!("{}", repo_root.display());
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
            if !commands::shell::is_shell_setup() {
                eprintln!("warning: shell integration not set up — `cond cd` will only print the path.");
                eprintln!("add this to your shell rc file:");
                eprintln!();
                eprintln!("  eval \"$(cond shell-setup)\"");
                eprintln!();
            }
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            let found = state.find_task(&task)?;
            print!("{}", repo_root.join(&found.worktree_path).display());
        }
        Commands::Diff { task } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            commands::task::diff(&repo_root, &state, &task)?;
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
