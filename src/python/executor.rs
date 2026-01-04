use crate::core::error::{FridaMgrError, Result};
use colored::Colorize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct VenvExecutor {
    venv_path: PathBuf,
    project_dir: PathBuf,
}

pub struct CapturedOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl VenvExecutor {
    pub fn new(project_dir: PathBuf) -> Self {
        let venv_path = project_dir.join(".venv");
        Self {
            venv_path,
            project_dir,
        }
    }

    pub fn venv_exists(&self) -> bool {
        self.venv_path.exists()
    }

    fn get_venv_bin_dir(&self) -> PathBuf {
        if cfg!(windows) {
            self.venv_path.join("Scripts")
        } else {
            self.venv_path.join("bin")
        }
    }

    fn get_executable_path(&self, command: &str) -> PathBuf {
        let bin_dir = self.get_venv_bin_dir();
        if cfg!(windows) {
            bin_dir.join(format!("{}.exe", command))
        } else {
            bin_dir.join(command)
        }
    }

    /// Run a command in the virtual environment with full stdio passthrough
    pub async fn run_interactive(&self, command: &str, args: &[String]) -> Result<i32> {
        if !self.venv_exists() {
            return Err(FridaMgrError::PythonEnv(
                "Virtual environment not found. Run 'frida-mgr init' first.".to_string(),
            ));
        }

        let executable = self.get_executable_path(command);

        if !executable.exists() {
            return Err(FridaMgrError::PythonEnv(format!(
                "Command '{}' not found in virtual environment. Is it installed?",
                command
            )));
        }

        // Set up environment variables
        let bin_dir = self.get_venv_bin_dir();
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);

        let status = Command::new(&executable)
            .args(args)
            .env("VIRTUAL_ENV", &self.venv_path)
            .env("PATH", new_path)
            .current_dir(&self.project_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await
            .map_err(|e| {
                FridaMgrError::CommandFailed(format!("Failed to execute {}: {}", command, e))
            })?;

        Ok(status.code().unwrap_or(1))
    }

    /// Run a command in the virtual environment and capture stdout/stderr.
    pub async fn run_captured(&self, command: &str, args: &[String]) -> Result<CapturedOutput> {
        if !self.venv_exists() {
            return Err(FridaMgrError::PythonEnv(
                "Virtual environment not found. Run 'frida-mgr init' first.".to_string(),
            ));
        }

        let executable = self.get_executable_path(command);

        if !executable.exists() {
            return Err(FridaMgrError::PythonEnv(format!(
                "Command '{}' not found in virtual environment. Is it installed?",
                command
            )));
        }

        let bin_dir = self.get_venv_bin_dir();
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);

        let output = Command::new(&executable)
            .args(args)
            .env("VIRTUAL_ENV", &self.venv_path)
            .env("PATH", new_path)
            .current_dir(&self.project_dir)
            .output()
            .await
            .map_err(|e| {
                FridaMgrError::CommandFailed(format!("Failed to execute {}: {}", command, e))
            })?;

        Ok(CapturedOutput {
            exit_code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Spawn an interactive shell in the virtual environment
    pub async fn spawn_shell(&self) -> Result<i32> {
        if !self.venv_exists() {
            return Err(FridaMgrError::PythonEnv(
                "Virtual environment not found. Run 'frida-mgr init' first.".to_string(),
            ));
        }

        let bin_dir = self.get_venv_bin_dir();
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);

        // Detect shell
        let shell = if cfg!(windows) {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        };

        println!("{} Entering virtual environment shell", "→".blue().bold());
        println!("  Type {} to exit", "exit".yellow());
        println!();

        let status = Command::new(&shell)
            .env("VIRTUAL_ENV", &self.venv_path)
            .env("PATH", new_path)
            .env("PS1", "(venv) $ ") // Custom prompt for bash/zsh
            .current_dir(&self.project_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await
            .map_err(|e| FridaMgrError::CommandFailed(format!("Failed to spawn shell: {}", e)))?;

        Ok(status.code().unwrap_or(0))
    }

    /// Check if a command exists in the virtual environment
    pub fn command_exists(&self, command: &str) -> bool {
        self.get_executable_path(command).exists()
    }

    /// List all executables in the virtual environment
    pub fn list_executables(&self) -> Result<Vec<String>> {
        if !self.venv_exists() {
            return Ok(Vec::new());
        }

        let bin_dir = self.get_venv_bin_dir();
        let mut executables = Vec::new();

        let entries = std::fs::read_dir(bin_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Filter out common non-executable files
                    if !name.ends_with(".pyc")
                        && !name.ends_with(".pyo")
                        && !name.starts_with("activate")
                        && !name.contains("python")
                    {
                        executables.push(name.to_string());
                    }
                }
            }
        }

        executables.sort();
        Ok(executables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venv_executor_creation() {
        let project_dir = PathBuf::from("/tmp/test");
        let executor = VenvExecutor::new(project_dir.clone());

        assert_eq!(executor.project_dir, project_dir);
        assert_eq!(executor.venv_path, project_dir.join(".venv"));
    }

    #[test]
    fn test_executable_path() {
        let project_dir = PathBuf::from("/tmp/test");
        let executor = VenvExecutor::new(project_dir);

        let path = executor.get_executable_path("frida");

        #[cfg(unix)]
        assert!(path.ends_with(".venv/bin/frida"));

        #[cfg(windows)]
        assert!(path.ends_with(".venv\\Scripts\\frida.exe"));
    }
}
