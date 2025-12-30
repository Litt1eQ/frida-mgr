use crate::core::error::{FridaMgrError, Result};
use std::path::Path;
use std::process::Output;
use tokio::process::Command;

pub struct ProcessExecutor;

impl ProcessExecutor {
    pub async fn execute(cmd: &str, args: &[&str], env: Option<&[(&str, &str)]>) -> Result<Output> {
        let mut command = Command::new(cmd);
        command.args(args);

        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                command.env(key, value);
            }
        }

        let output = command
            .output()
            .await
            .map_err(|e| FridaMgrError::CommandFailed(format!("{}: {}", cmd, e)))?;

        Ok(output)
    }

    pub async fn execute_with_status(cmd: &str, args: &[&str]) -> Result<bool> {
        let output = Self::execute(cmd, args, None).await?;
        Ok(output.status.success())
    }

    pub async fn execute_with_output(cmd: &str, args: &[&str]) -> Result<String> {
        let output = Self::execute(cmd, args, None).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FridaMgrError::CommandFailed(format!(
                "{} failed: {}",
                cmd, stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn check_command_exists(cmd: &str) -> bool {
        std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

pub async fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        tokio::fs::create_dir_all(path).await?;
    }
    Ok(())
}

pub async fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        ensure_dir_exists(parent).await?;
    }
    tokio::fs::copy(from, to).await?;
    Ok(())
}
