use anyhow::{bail, Result};
use std::path::PathBuf;

const SHELL_FUNCTION: &str = r#"cond() {
  case "$1" in
    cd)
      if [ -n "$2" ]; then
        local dir
        dir="$(command cond cd "$2")" && cd "$dir"
      else
        cd "$(command cond base)"
      fi
      ;;
    base)
      cd "$(command cond base)"
      ;;
    spawn)
      local dir
      dir="$(command cond "$@")" && cd "$dir"
      ;;
    merge)
      command cond "$@" && cd "$(command cond base)"
      ;;
    *)
      command cond "$@"
      ;;
  esac
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

/// Check if the rc file already contains the shell-setup eval line.
pub fn is_rc_configured() -> bool {
    let Ok(rc) = rc_path() else { return false };
    let Ok(contents) = std::fs::read_to_string(rc) else { return false };
    contents.contains("cond shell-setup")
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
