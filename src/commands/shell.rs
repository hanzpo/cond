use anyhow::{bail, Result};
use std::path::PathBuf;

const SHELL_FUNCTION: &str = r#"cond() {
  if [ "$1" = "cd" ] && [ -n "$2" ]; then
    local dir
    dir="$(command cond cd "$2")" && cd "$dir"
  else
    command cond "$@"
  fi
}
export COND_SHELL=1
"#;

pub fn shell_setup() -> Result<()> {
    print!("{SHELL_FUNCTION}");
    Ok(())
}

pub fn is_shell_setup() -> bool {
    std::env::var("COND_SHELL").is_ok()
}

pub fn rc_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
    let shell = std::env::var("SHELL").unwrap_or_default();

    let rc = if shell.contains("zsh") {
        ".zshrc"
    } else if shell.contains("bash") {
        if PathBuf::from(&home).join(".bash_profile").exists() {
            ".bash_profile"
        } else {
            ".bashrc"
        }
    } else {
        bail!("unsupported shell: {shell} — add `eval \"$(cond shell-setup)\"` to your shell rc manually");
    };

    Ok(PathBuf::from(home).join(rc))
}
