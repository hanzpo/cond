use anyhow::{bail, Result};
use std::path::PathBuf;

const SHELL_FUNCTION: &str = r#"cond() {
  local out rc
  out="$(command cond "$@")"
  rc=$?
  if [ $rc -eq 0 ] && [ -n "$out" ] && [ -d "$out" ]; then
    cd "$out"
  elif [ -n "$out" ]; then
    printf '%s\n' "$out"
  fi
  return $rc
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
