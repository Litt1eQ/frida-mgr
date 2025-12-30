use crate::core::error::{FridaMgrError, Result};
use crate::core::ProcessExecutor;
use colored::Colorize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct UvManager {
    project_dir: PathBuf,
}

impl UvManager {
    pub fn new(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    pub fn check_installed() -> Result<()> {
        if !ProcessExecutor::check_command_exists("uv") {
            return Err(FridaMgrError::PythonEnv(
                "uv is not installed. Please install it first: https://github.com/astral-sh/uv"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub async fn create_venv(&self, python_version: &str) -> Result<()> {
        self.ensure_venv(python_version, false).await
    }

    pub async fn ensure_venv(&self, python_version: &str, recreate: bool) -> Result<()> {
        Self::check_installed()?;

        let venv_path = self.project_dir.join(".venv");

        if venv_path.exists() && recreate {
            println!(
                "{} Recreating virtual environment at {}",
                "⚙".blue().bold(),
                venv_path.display().to_string().yellow()
            );
            tokio::fs::remove_dir_all(&venv_path).await?;
        } else if venv_path.exists() {
            if let Some(found_version) = self.get_venv_python_version().await? {
                if !versions_compatible(python_version, &found_version) {
                    return Err(FridaMgrError::PythonEnv(format!(
                        "Virtual environment Python version mismatch: requested {}, found {}. Run 'frida-mgr sync --recreate-venv' to rebuild the environment.",
                        python_version, found_version
                    )));
                }
            } else {
                println!(
                    "{} Unable to detect virtual environment Python version; keeping existing {}",
                    "⚠".yellow().bold(),
                    venv_path.display().to_string().yellow()
                );
            }

            println!(
                "{} Virtual environment already exists at {}",
                "ℹ".blue().bold(),
                venv_path.display()
            );
            return Ok(());
        }

        println!(
            "{} Creating Python {} virtual environment...",
            "⚙".blue().bold(),
            python_version.cyan()
        );

        let success = ProcessExecutor::execute_with_status(
            "uv",
            &[
                "venv",
                "--python",
                python_version,
                venv_path.to_str().unwrap(),
            ],
        )
        .await?;

        if !success {
            return Err(FridaMgrError::PythonEnv(format!(
                "Failed to create virtual environment with Python {}",
                python_version
            )));
        }

        println!(
            "{} Virtual environment created at {}",
            "✓".green().bold(),
            ".venv".yellow()
        );

        Ok(())
    }

    pub async fn install_python_packages(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        Self::check_installed()?;
        let python_path = self.get_python_path()?;

        println!(
            "{} Installing extra Python packages ({}): {}",
            "⚙".blue().bold(),
            packages.len().to_string().cyan(),
            packages.join(" ").yellow()
        );

        let mut args: Vec<String> = vec![
            "pip".to_string(),
            "install".to_string(),
            "--python".to_string(),
            python_path.to_str().unwrap().to_string(),
        ];
        args.extend(packages.iter().cloned());

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = ProcessExecutor::execute("uv", &args_ref, None).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                eprintln!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprintln!("{}", stderr);
            }
            return Err(FridaMgrError::PythonEnv(
                "Failed to install extra Python packages. See output above for details."
                    .to_string(),
            ));
        }

        println!(
            "{} Extra Python packages installed",
            "✓".green().bold()
        );

        Ok(())
    }

    pub async fn install_frida(
        &self,
        frida_version: &str,
        tools_version: Option<&str>,
    ) -> Result<()> {
        Self::check_installed()?;

        let python_path = self.get_python_path()?;

        let tools_label = tools_version.unwrap_or("auto");
        println!(
            "{} Installing frida=={} and frida-tools=={}...",
            "⚙".blue().bold(),
            frida_version.cyan(),
            tools_label.cyan()
        );

        install_frida_packages(&python_path, frida_version, tools_version, false).await?;

        println!(
            "{} Frida packages installed successfully",
            "✓".green().bold()
        );

        Ok(())
    }

    pub async fn upgrade_frida(
        &self,
        frida_version: &str,
        tools_version: Option<&str>,
    ) -> Result<()> {
        Self::check_installed()?;

        let python_path = self.get_python_path()?;

        let tools_label = tools_version.unwrap_or("auto");
        println!(
            "{} Upgrading to frida=={} and frida-tools=={}...",
            "⚙".blue().bold(),
            frida_version.cyan(),
            tools_label.cyan()
        );

        install_frida_packages(&python_path, frida_version, tools_version, true).await?;

        println!("{} Frida packages upgraded", "✓".green().bold());

        Ok(())
    }

    pub async fn get_installed_version(&self, package: &str) -> Result<Option<String>> {
        let python_path = self.get_python_path()?;

        let output = ProcessExecutor::execute_with_output(
            "uv",
            &[
                "pip",
                "show",
                "--python",
                python_path.to_str().unwrap(),
                package,
            ],
        )
        .await;

        match output {
            Ok(output) => {
                for line in output.lines() {
                    if line.starts_with("Version:") {
                        let version = line.split(':').nth(1).unwrap().trim();
                        return Ok(Some(version.to_string()));
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }

    fn get_python_path(&self) -> Result<PathBuf> {
        let venv_path = self.project_dir.join(".venv");

        if !venv_path.exists() {
            return Err(FridaMgrError::PythonEnv(
                "Virtual environment not found. Run 'frida-mgr init' first.".to_string(),
            ));
        }

        let python_path = if cfg!(windows) {
            venv_path.join("Scripts").join("python.exe")
        } else {
            venv_path.join("bin").join("python")
        };

        if !python_path.exists() {
            return Err(FridaMgrError::PythonEnv(
                "Python executable not found in virtual environment".to_string(),
            ));
        }

        Ok(python_path)
    }

    pub fn get_venv_path(&self) -> PathBuf {
        self.project_dir.join(".venv")
    }

    pub fn venv_exists(&self) -> bool {
        self.get_venv_path().exists()
    }

    pub async fn run_uv_interactive(&self, args: &[String]) -> Result<i32> {
        Self::check_installed()?;

        let status = Command::new("uv")
            .args(args)
            .current_dir(&self.project_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await
            .map_err(|e| FridaMgrError::CommandFailed(format!("Failed to execute uv: {}", e)))?;

        Ok(status.code().unwrap_or(1))
    }

    pub async fn run_uv_pip_interactive(&self, args: &[String]) -> Result<i32> {
        Self::check_installed()?;

        let mut uv_args: Vec<String> = vec!["pip".to_string()];

        let has_python_selector = args.iter().any(|a| a == "--python" || a == "-p");
        if !has_python_selector {
            let python_path = self.get_python_path()?;
            uv_args.push("--python".to_string());
            uv_args.push(python_path.to_string_lossy().to_string());
        }

        uv_args.extend(args.iter().cloned());
        self.run_uv_interactive(&uv_args).await
    }

    async fn get_venv_python_version(&self) -> Result<Option<String>> {
        let cfg_path = self.get_venv_path().join("pyvenv.cfg");
        if !cfg_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(cfg_path).await?;
        for line in content.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("version =") {
                return Ok(Some(v.trim().to_string()));
            }
            if let Some(v) = line.strip_prefix("version_info =") {
                return Ok(Some(v.trim().to_string()));
            }
        }
        Ok(None)
    }
}

async fn install_frida_packages(
    python_path: &PathBuf,
    frida_version: &str,
    tools_version: Option<&str>,
    upgrade: bool,
) -> Result<()> {
    let mut current_tools_version = tools_version;
    let mut retried_unpinned = false;

    loop {
        let mut args: Vec<String> = vec![
            "pip".to_string(),
            "install".to_string(),
            "--python".to_string(),
            python_path.to_str().unwrap().to_string(),
        ];

        if upgrade {
            args.push("--upgrade".to_string());
        }

        args.push(format!("frida=={}", frida_version));
        match current_tools_version {
            Some(v) => args.push(format!("frida-tools=={}", v)),
            None => args.push("frida-tools".to_string()),
        }

        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = ProcessExecutor::execute("uv", &args_ref, None).await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        eprintln!(
            "\n{}",
            if upgrade {
                "Upgrade output:"
            } else {
                "Installation output:"
            }
            .yellow()
            .bold()
        );
        if !stdout.is_empty() {
            eprintln!("{}", stdout);
        }
        if !stderr.is_empty() {
            eprintln!("{}", stderr);
        }

        // If the pinned frida-tools version doesn't exist / can't be resolved, retry unpinned once.
        let should_retry_unpinned = current_tools_version.is_some()
            && !retried_unpinned
            && (stderr.contains("no version of frida-tools==")
                || stderr.contains("there is no version of frida-tools==")
                || stderr.contains("No solution found"));

        if should_retry_unpinned {
            eprintln!(
                "\n{} {}",
                "⚠".yellow().bold(),
                "Pinned frida-tools version failed; retrying with unpinned frida-tools...".yellow()
            );
            retried_unpinned = true;
            current_tools_version = None;
            continue;
        }

        return Err(FridaMgrError::PythonEnv(
            "Failed to install Frida packages. See output above for details.".to_string(),
        ));
    }
}

fn extract_version_parts(input: &str) -> Vec<u32> {
    let mut parts: Vec<u32> = Vec::new();
    let mut buf = String::new();

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            buf.push(ch);
            continue;
        }

        if !buf.is_empty() {
            if let Ok(v) = buf.parse::<u32>() {
                parts.push(v);
            }
            buf.clear();
        }
    }

    if !buf.is_empty() {
        if let Ok(v) = buf.parse::<u32>() {
            parts.push(v);
        }
    }

    parts
}

fn versions_compatible(requested: &str, found: &str) -> bool {
    let req = extract_version_parts(requested);
    let got = extract_version_parts(found);

    if req.is_empty() || got.is_empty() {
        return true;
    }

    if got.len() < req.len() {
        return false;
    }

    for i in 0..req.len() {
        if req[i] != got[i] {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compatibility_matches_major_minor() {
        assert!(versions_compatible("3.11", "3.11.6"));
        assert!(!versions_compatible("3.12", "3.11.6"));
        assert!(!versions_compatible("3.11", "3.10.9"));
    }

    #[test]
    fn version_compatibility_allows_patch_pin() {
        assert!(versions_compatible("3.11.6", "3.11.6"));
        assert!(!versions_compatible("3.11.6", "3.11.5"));
    }

    #[test]
    fn version_parsing_handles_suffixes() {
        assert!(versions_compatible("3.11", "3.11.6.final.0"));
    }
}
