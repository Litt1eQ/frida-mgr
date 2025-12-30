use crate::android::foreground;
use crate::config::ArchType;
use crate::core::error::{FridaMgrError, Result};
use crate::core::ProcessExecutor;
use colored::Colorize;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub model: String,
    pub state: String,
}

pub struct AdbClient {
    adb_path: String,
}

impl AdbClient {
    pub fn new(adb_path: Option<String>) -> Self {
        Self {
            adb_path: adb_path.unwrap_or_else(|| "adb".to_string()),
        }
    }

    pub fn check_installed(&self) -> Result<()> {
        if !ProcessExecutor::check_command_exists(&self.adb_path) {
            return Err(FridaMgrError::Adb(
                "ADB is not installed or not in PATH. Please install Android SDK Platform Tools."
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub async fn list_devices(&self) -> Result<Vec<Device>> {
        self.check_installed()?;

        let output =
            ProcessExecutor::execute_with_output(&self.adb_path, &["devices", "-l"]).await?;

        let mut devices = Vec::new();

        for line in output.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let id = parts[0].to_string();
                let state = parts[1].to_string();

                let model = parts
                    .iter()
                    .find(|p| p.starts_with("model:"))
                    .map(|p| p.strip_prefix("model:").unwrap_or("unknown"))
                    .unwrap_or("unknown")
                    .to_string();

                devices.push(Device { id, model, state });
            }
        }

        Ok(devices)
    }

    pub async fn get_first_device(&self) -> Result<Device> {
        let devices = self.list_devices().await?;

        if devices.is_empty() {
            return Err(FridaMgrError::NoDevice);
        }

        Ok(devices[0].clone())
    }

    pub async fn get_device(&self, device_id: Option<&str>) -> Result<Device> {
        if let Some(id) = device_id {
            let devices = self.list_devices().await?;
            devices
                .into_iter()
                .find(|d| d.id == id)
                .ok_or_else(|| FridaMgrError::DeviceNotFound(id.to_string()))
        } else {
            self.get_first_device().await
        }
    }

    pub async fn get_arch(&self, device_id: &str) -> Result<ArchType> {
        self.check_installed()?;

        let output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "getprop", "ro.product.cpu.abi"],
        )
        .await?;

        let abi = output.trim();
        Ok(ArchType::from_abi(abi))
    }

    pub async fn push_file(&self, device_id: &str, local: &Path, remote: &str) -> Result<()> {
        self.check_installed()?;

        println!(
            "{} Pushing {} to device...",
            "↑".blue().bold(),
            local.file_name().unwrap().to_str().unwrap().yellow()
        );

        let success = ProcessExecutor::execute_with_status(
            &self.adb_path,
            &["-s", device_id, "push", local.to_str().unwrap(), remote],
        )
        .await?;

        if !success {
            return Err(FridaMgrError::Adb(format!(
                "Failed to push file to device {}",
                device_id
            )));
        }

        println!("{} File pushed successfully", "✓".green().bold());

        Ok(())
    }

    pub async fn make_executable(&self, device_id: &str, path: &str) -> Result<()> {
        self.check_installed()?;

        let success = ProcessExecutor::execute_with_status(
            &self.adb_path,
            &["-s", device_id, "shell", "chmod", "755", path],
        )
        .await?;

        if !success {
            return Err(FridaMgrError::Adb(format!(
                "Failed to make {} executable",
                path
            )));
        }

        Ok(())
    }

    pub async fn start_server(
        &self,
        device_id: &str,
        server_path: &str,
        server_process_name: &str,
        port: u16,
        root_command: &str,
    ) -> Result<()> {
        self.check_installed()?;

        // Kill existing server
        let _ = self
            .kill_server(device_id, server_process_name, root_command)
            .await;

        println!(
            "{} Starting {} on port {} (with {})...",
            "⚙".blue().bold(),
            server_process_name.cyan(),
            port.to_string().cyan(),
            root_command.yellow()
        );

        // Use nohup to properly daemonize and redirect output to log
        let log_path = format!("{}.log", server_path);

        // Clear old log
        let _ = ProcessExecutor::execute_with_status(
            &self.adb_path,
            &["-s", device_id, "shell", "rm", "-f", &log_path],
        )
        .await;

        // Use configured root command (su, sudo, laotie, etc.)
        let cmd = format!(
            "{} -c 'nohup {} -l 0.0.0.0:{} > {} 2>&1 &'",
            root_command, server_path, port, log_path
        );

        let success =
            ProcessExecutor::execute_with_status(&self.adb_path, &["-s", device_id, "shell", &cmd])
                .await?;

        if !success {
            return Err(FridaMgrError::Adb(format!(
                "Failed to execute start command with {}",
                root_command
            )));
        }

        println!(
            "{} Verifying {}...",
            "⚙".blue().bold(),
            server_process_name.cyan()
        );

        // Wait and check multiple times
        for attempt in 0..15 {
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

            // Check if process is still running
            let running = self
                .check_server_running(device_id, server_process_name)
                .await
                .unwrap_or(false);

            if !running {
                // Process died - definitely an error
                let logs = self
                    .get_server_logs(device_id, &log_path)
                    .await
                    .unwrap_or_default();

                eprintln!(
                    "\n{}",
                    format!("✗ {} failed to start", server_process_name)
                        .red()
                        .bold()
                );
                if !logs.trim().is_empty() {
                    eprintln!("\n{}", "Error output:".yellow().bold());
                    eprintln!("{}", logs);
                } else {
                    eprintln!(
                        "No error logs available. The server process terminated immediately."
                    );
                    eprintln!("Possible causes:");
                    eprintln!(
                        "  - Root command '{}' not working (try 'su', 'sudo', or custom)",
                        root_command
                    );
                    eprintln!("  - SELinux blocking execution");
                    eprintln!("  - Incompatible Frida server version");
                }

                return Err(FridaMgrError::Adb(format!(
                    "{} process terminated. See error output above.",
                    server_process_name
                )));
            }

            // Check logs for errors every few attempts
            if attempt % 3 == 2 {
                let logs = self
                    .get_server_logs(device_id, &log_path)
                    .await
                    .unwrap_or_default();

                // Look for error patterns
                if logs.contains("Error:")
                    || logs.contains("error")
                    || logs.contains("Unable to")
                    || logs.contains("failed")
                    || logs.contains("\"type\":\"error\"")
                {
                    eprintln!(
                        "\n{}",
                        format!("✗ {} encountered an error", server_process_name)
                            .red()
                            .bold()
                    );
                    eprintln!("\n{}", "Error output:".yellow().bold());
                    eprintln!("{}", logs);

                    // Kill the broken server
                    let _ = self
                        .kill_server(device_id, server_process_name, root_command)
                        .await;

                    eprintln!("\n{}", "Troubleshooting tips:".cyan().bold());
                    eprintln!(
                        "  1. Check if your device is rooted and '{}' works",
                        root_command
                    );
                    eprintln!("  2. Try a different root command in frida.toml:");
                    eprintln!("     root_command = \"su\" or \"sudo\" or \"laotie\"");
                    eprintln!("  3. Try a different frida version: frida-mgr install <version>");

                    return Err(FridaMgrError::Adb(format!(
                        "{} started but encountered errors. See output above.",
                        server_process_name
                    )));
                }
            }
        }

        // Final check: process still running?
        if !self
            .check_server_running(device_id, server_process_name)
            .await
            .unwrap_or(false)
        {
            let logs = self
                .get_server_logs(device_id, &log_path)
                .await
                .unwrap_or_default();

            eprintln!(
                "\n{}",
                format!("✗ {} not running", server_process_name)
                    .red()
                    .bold()
            );
            if !logs.trim().is_empty() {
                eprintln!("\n{}", "Server output:".yellow().bold());
                eprintln!("{}", logs);
            }

            return Err(FridaMgrError::Adb(format!(
                "{} failed to stay running",
                server_process_name
            )));
        }

        // Check for any warning/error logs
        let logs = self
            .get_server_logs(device_id, &log_path)
            .await
            .unwrap_or_default();
        if !logs.trim().is_empty() {
            if logs.contains("Error:")
                || logs.contains("error")
                || logs.contains("\"type\":\"error\"")
            {
                eprintln!(
                    "\n{}",
                    format!("✗ {} has errors", server_process_name).red().bold()
                );
                eprintln!("\n{}", "Error output:".yellow().bold());
                eprintln!("{}", logs);

                let _ = self
                    .kill_server(device_id, server_process_name, root_command)
                    .await;

                return Err(FridaMgrError::Adb(format!(
                    "{} running but has errors. See output above.",
                    server_process_name
                )));
            } else if logs.len() > 10 {
                // Show any non-trivial output as warning
                eprintln!("\n{}", "Server output:".yellow().bold());
                eprintln!("{}", logs);
            }
        }

        println!(
            "{} {} started",
            "✓".green().bold(),
            server_process_name.cyan()
        );
        println!(
            "  Note: Run {} to verify it's working",
            "frida-mgr ps -U".cyan()
        );

        Ok(())
    }

    pub async fn kill_server(
        &self,
        device_id: &str,
        server_process_name: &str,
        root_command: &str,
    ) -> Result<()> {
        self.check_installed()?;

        // First check if server is running
        let was_running = self
            .check_server_running(device_id, server_process_name)
            .await
            .unwrap_or(false);

        if !was_running {
            println!(
                "{} {} is not running",
                "ℹ".blue().bold(),
                server_process_name.cyan()
            );
            return Ok(());
        }

        println!(
            "{} Stopping {} (with {})...",
            "⚙".blue().bold(),
            server_process_name.cyan(),
            root_command.yellow()
        );

        // Use root command to kill server
        let cmd = format!("{} -c 'killall {}'", root_command, server_process_name);

        let success =
            ProcessExecutor::execute_with_status(&self.adb_path, &["-s", device_id, "shell", &cmd])
                .await?;

        if !success {
            eprintln!(
                "{} Failed to kill {} with {}",
                "⚠".yellow().bold(),
                server_process_name.cyan(),
                root_command.yellow()
            );
            eprintln!(
                "  Try manually: adb shell \"{} -c 'killall -9 {}'\"",
                root_command, server_process_name
            );
            return Err(FridaMgrError::Adb(format!(
                "Failed to stop {} with root command '{}'",
                server_process_name, root_command
            )));
        }

        // Wait a bit and verify it's stopped
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let still_running = self
            .check_server_running(device_id, server_process_name)
            .await
            .unwrap_or(false);

        if still_running {
            eprintln!(
                "{} {} may still be running",
                "⚠".yellow().bold(),
                server_process_name.cyan()
            );
            eprintln!(
                "  Try force kill: adb shell \"{} -c 'killall -9 {}'\"",
                root_command, server_process_name
            );
        }

        Ok(())
    }

    pub async fn check_server_running(
        &self,
        device_id: &str,
        server_process_name: &str,
    ) -> Result<bool> {
        self.check_installed()?;

        let output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "ps", "-A"],
        )
        .await?;

        Ok(output.lines().any(|line| {
            line.split_whitespace()
                .any(|token| token == server_process_name)
                || line.contains(server_process_name)
        }))
    }

    pub async fn check_port_listening(&self, device_id: &str, port: u16) -> Result<bool> {
        self.check_installed()?;

        // Use netstat or ss to check if port is listening
        let port_str = port.to_string();
        let output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "netstat", "-tuln"],
        )
        .await;

        if let Ok(netstat_output) = output {
            return Ok(netstat_output.contains(&format!(":{}", port_str)));
        }

        // Fallback: try ss command
        let output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "ss", "-tuln"],
        )
        .await;

        if let Ok(ss_output) = output {
            return Ok(ss_output.contains(&format!(":{}", port_str)));
        }

        // If both failed, assume port is not listening
        Ok(false)
    }

    pub async fn get_server_logs(&self, device_id: &str, log_path: &str) -> Result<String> {
        self.check_installed()?;

        let output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "cat", log_path],
        )
        .await;

        match output {
            Ok(logs) => Ok(logs),
            Err(_) => Ok(String::new()),
        }
    }

    pub async fn get_server_status(
        &self,
        device_id: &str,
        server_process_name: &str,
    ) -> Result<String> {
        let running = self
            .check_server_running(device_id, server_process_name)
            .await?;

        if running {
            Ok("running".to_string())
        } else {
            Ok("stopped".to_string())
        }
    }

    pub async fn get_foreground_app(&self, device_id: &str) -> Result<foreground::ForegroundApp> {
        self.check_installed()?;

        let activity_output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &[
                "-s",
                device_id,
                "shell",
                "dumpsys",
                "activity",
                "activities",
            ],
        )
        .await?;

        let mut component = foreground::parse_foreground_component_from_dumpsys_activity_activities(
            &activity_output,
        );
        let record_hint = component.as_ref().and_then(|c| {
            foreground::find_process_record_near_activity_record(
                &activity_output,
                c.line_index,
                &c.package,
            )
        });
        let mut pid = record_hint.as_ref().map(|r| r.pid);
        let mut process_hint = record_hint.as_ref().map(|r| r.process.clone());

        if component.is_none() {
            let window_output = ProcessExecutor::execute_with_output(
                &self.adb_path,
                &["-s", device_id, "shell", "dumpsys", "window", "windows"],
            )
            .await?;

            component =
                foreground::parse_foreground_component_from_dumpsys_window_windows(&window_output);
        }

        let component = component.ok_or_else(|| {
            FridaMgrError::Adb(
                "Unable to detect the foreground app (try unlocking the device and opening the target app)."
                    .to_string(),
            )
        })?;

        if pid.is_none() {
            let pidof_output = ProcessExecutor::execute_with_output(
                &self.adb_path,
                &[
                    "-s",
                    device_id,
                    "shell",
                    "pidof",
                    component.package.as_str(),
                ],
            )
            .await;

            if let Ok(pidof_output) = pidof_output {
                pid = pidof_output
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<u32>().ok());
            }
        }

        if process_hint.is_none() {
            if let Some(pid) = pid {
                let proc_path = format!("/proc/{}/cmdline", pid);
                let cmdline_output = ProcessExecutor::execute_with_output(
                    &self.adb_path,
                    &["-s", device_id, "shell", "cat", &proc_path],
                )
                .await;

                if let Ok(cmdline_output) = cmdline_output {
                    let cmdline = cmdline_output.split('\0').next().unwrap_or("").trim();
                    if !cmdline.is_empty() {
                        process_hint = Some(cmdline.to_string());
                    }
                }
            }
        }

        let processes_output = ProcessExecutor::execute_with_output(
            &self.adb_path,
            &["-s", device_id, "shell", "ps", "-A"],
        )
        .await?;

        let package = component.package.clone();
        let package_prefix = format!("{}:", package);
        let package_dot = format!("{}.", package);

        let candidates = processes_output
            .lines()
            .filter_map(|line| line.split_whitespace().last())
            .filter(|name| *name != "NAME" && *name != "CMDLINE")
            .filter(|name| {
                *name == package.as_str()
                    || name.starts_with(&package_prefix)
                    || name.starts_with(&package_dot)
            })
            .map(|name| name.to_string())
            .collect::<Vec<_>>();

        if process_hint.is_none() {
            process_hint = candidates
                .iter()
                .find(|p| p.as_str() == component.package.as_str())
                .or_else(|| candidates.first())
                .cloned();
        }

        let process = process_hint.unwrap_or_else(|| component.package.clone());

        Ok(foreground::ForegroundApp {
            package: component.package,
            activity: Some(component.activity),
            process,
            pid,
        })
    }

    pub async fn get_foreground_process_name(&self, device_id: &str) -> Result<String> {
        Ok(self.get_foreground_app(device_id).await?.process)
    }
}

impl Default for AdbClient {
    fn default() -> Self {
        Self::new(None)
    }
}
