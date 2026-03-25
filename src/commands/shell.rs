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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_function_contains_cond_shell_export() {
        assert!(SHELL_FUNCTION.contains("export COND_SHELL=1"));
    }

    #[test]
    fn shell_function_defines_cond_function() {
        assert!(SHELL_FUNCTION.contains("cond()"));
    }

    #[test]
    fn shell_function_handles_directory_cd() {
        // The shell function should cd when output is a directory
        assert!(SHELL_FUNCTION.contains("cd \"$out\""));
    }

    #[test]
    fn is_shell_setup_reflects_env() {
        // Save and restore
        let original = std::env::var("COND_SHELL").ok();

        std::env::set_var("COND_SHELL", "1");
        assert!(is_shell_setup());

        std::env::remove_var("COND_SHELL");
        assert!(!is_shell_setup());

        // Restore
        if let Some(val) = original {
            std::env::set_var("COND_SHELL", val);
        }
    }

    #[test]
    fn rc_path_returns_zshrc_for_zsh() {
        let original_shell = std::env::var("SHELL").ok();
        let original_home = std::env::var("HOME").ok();

        std::env::set_var("SHELL", "/bin/zsh");
        std::env::set_var("HOME", "/tmp/test-home");

        let path = rc_path().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/test-home/.zshrc"));

        // Restore
        if let Some(v) = original_shell { std::env::set_var("SHELL", v); } else { std::env::remove_var("SHELL"); }
        if let Some(v) = original_home { std::env::set_var("HOME", v); } else { std::env::remove_var("HOME"); }
    }

    #[test]
    fn rc_path_returns_bashrc_for_bash() {
        let original_shell = std::env::var("SHELL").ok();
        let original_home = std::env::var("HOME").ok();

        std::env::set_var("SHELL", "/bin/bash");
        // Use a temp dir where .bash_profile doesn't exist
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path().to_str().unwrap());

        let path = rc_path().unwrap();
        assert_eq!(path, dir.path().join(".bashrc"));

        // Restore
        if let Some(v) = original_shell { std::env::set_var("SHELL", v); } else { std::env::remove_var("SHELL"); }
        if let Some(v) = original_home { std::env::set_var("HOME", v); } else { std::env::remove_var("HOME"); }
    }

    #[test]
    fn rc_path_prefers_bash_profile_when_exists() {
        let original_shell = std::env::var("SHELL").ok();
        let original_home = std::env::var("HOME").ok();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".bash_profile"), "").unwrap();
        std::env::set_var("SHELL", "/bin/bash");
        std::env::set_var("HOME", dir.path().to_str().unwrap());

        let path = rc_path().unwrap();
        assert_eq!(path, dir.path().join(".bash_profile"));

        // Restore
        if let Some(v) = original_shell { std::env::set_var("SHELL", v); } else { std::env::remove_var("SHELL"); }
        if let Some(v) = original_home { std::env::set_var("HOME", v); } else { std::env::remove_var("HOME"); }
    }
}
