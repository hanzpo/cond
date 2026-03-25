mod commands;
mod state;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cond", about = "Git worktree agent orchestrator")]
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

    /// Open claude in a task's worktree to review changes
    Review {
        /// Task ID
        id: u32,
    },

    /// Create a GitHub PR for a task
    Pr {
        /// Task ID
        id: u32,
        /// PR title (defaults to task description)
        #[arg(long)]
        title: Option<String>,
        /// Create as draft PR
        #[arg(long)]
        draft: bool,
    },

    /// Merge the PR for a task
    Merge {
        /// Task ID
        id: u32,
        /// Squash merge
        #[arg(long, default_value_t = true)]
        squash: bool,
        /// Delete branch after merge
        #[arg(long, default_value_t = true)]
        delete_branch: bool,
    },

    /// Print the worktree path for a task (use with: cd $(cond cd <id>))
    Cd {
        /// Task ID
        id: u32,
    },

    /// Remove cleaned tasks from state
    Prune,

    /// Show the diff for a task's branch against main
    Diff {
        /// Task ID
        id: u32,
    },

    /// Remove a task's worktree and branch
    Kill {
        /// Task ID
        id: u32,
    },

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
        Commands::Status => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            commands::task::status(&state)?;
        }
        Commands::Review { id } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            commands::review::review(&repo_root, &state, id)?;
        }
        Commands::Pr { id, title, draft } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::review::pr(&repo_root, &mut state, id, title.as_deref(), draft)?;
            state.save(&repo_root)?;
        }
        Commands::Merge { id, squash, delete_branch } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::review::merge(&repo_root, &mut state, id, squash, delete_branch)?;
            state.save(&repo_root)?;
        }
        Commands::Prune => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::prune(&mut state)?;
            state.save(&repo_root)?;
        }
        Commands::Cd { id } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            let task = state.find_task(id)?;
            print!("{}", repo_root.join(&task.worktree_path).display());
        }
        Commands::Diff { id } => {
            let repo_root = util::repo_root()?;
            let state = state::CondState::load(&repo_root)?;
            commands::task::diff(&repo_root, &state, id)?;
        }
        Commands::Kill { id } => {
            let repo_root = util::repo_root()?;
            let mut state = state::CondState::load(&repo_root)?;
            commands::task::kill(&repo_root, &mut state, id)?;
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
