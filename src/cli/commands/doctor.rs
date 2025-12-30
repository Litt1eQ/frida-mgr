use crate::android::AdbClient;
use crate::config::GlobalConfigManager;
use crate::core::{error::Result, ProcessExecutor};
use crate::python::UvManager;
use colored::Colorize;
use std::env;

pub async fn execute() -> Result<()> {
    println!("{}", "Running environment checks...".bold());
    println!();

    let mut all_ok = true;

    // Check uv
    print!("Checking uv... ");
    if ProcessExecutor::check_command_exists("uv") {
        let version = ProcessExecutor::execute_with_output("uv", &["--version"]).await;
        match version {
            Ok(v) => println!("{} ({})", "✓".green(), v.trim().yellow()),
            Err(_) => println!("{}", "✓".green()),
        }
    } else {
        println!("{}", "✗ Not found".red());
        println!("  Install from: https://github.com/astral-sh/uv");
        all_ok = false;
    }

    // Check ADB
    print!("Checking adb... ");
    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path.clone()));

    match adb.check_installed() {
        Ok(_) => {
            let version = ProcessExecutor::execute_with_output(
                &global_config.android.adb_path,
                &["--version"],
            )
            .await;
            match version {
                Ok(v) => {
                    let first_line = v.lines().next().unwrap_or(&v);
                    println!("{} ({})", "✓".green(), first_line.trim().yellow())
                }
                Err(_) => println!("{}", "✓".green()),
            }
        }
        Err(_) => {
            println!("{}", "✗ Not found".red());
            println!("  Install Android SDK Platform Tools");
            all_ok = false;
        }
    }

    // Check for project
    print!("Checking project... ");
    let current_dir = env::current_dir()?;
    let uv_mgr = UvManager::new(current_dir);

    if uv_mgr.venv_exists() {
        println!("{}", "✓ Initialized".green());

        // Check frida installation
        if let Ok(Some(version)) = uv_mgr.get_installed_version("frida").await {
            println!("  Frida: {}", version.cyan());
        }

        if let Ok(Some(version)) = uv_mgr.get_installed_version("frida-tools").await {
            println!("  Frida-tools: {}", version.cyan());
        }
    } else {
        println!("{}", "○ Not initialized".yellow());
        println!("  Run {} to initialize", "frida-mgr init".cyan());
    }

    // Check devices
    print!("Checking devices... ");
    match adb.list_devices().await {
        Ok(devices) => {
            if devices.is_empty() {
                println!("{}", "○ No devices connected".yellow());
            } else {
                println!("{} {} device(s) connected", "✓".green(), devices.len());
                for device in &devices {
                    println!("  - {} ({})", device.id.cyan(), device.model.yellow());
                }
            }
        }
        Err(_) => {
            println!("{}", "✗ Failed to check".red());
            all_ok = false;
        }
    }

    println!();
    if all_ok {
        println!("{}", "All checks passed!".green().bold());
    } else {
        println!(
            "{}",
            "Some checks failed. Please fix the issues above."
                .yellow()
                .bold()
        );
    }

    Ok(())
}
