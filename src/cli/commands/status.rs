use crate::android::AdbClient;
use crate::config::{resolve_android_server_target, GlobalConfigManager, ProjectConfigManager};
use crate::core::error::Result;
use colored::Colorize;

pub async fn execute(device_id: Option<String>) -> Result<()> {
    let config_result = ProjectConfigManager::from_current_dir()?.load().await;
    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));

    let device = adb.get_device(device_id.as_deref()).await?;

    println!("{}", "Device Status:".bold());
    println!("  Device ID: {}", device.id.cyan());
    println!("  Model: {}", device.model.yellow());
    println!("  State: {}", device.state.green());

    // Get architecture
    let arch = adb.get_arch(&device.id).await?;
    println!("  Architecture: {}", arch.to_str().yellow());

    // Check server status
    let server_name_override = config_result
        .as_ref()
        .ok()
        .and_then(|c| c.android.server_name.as_deref());
    let target = resolve_android_server_target(
        &global_config.android.default_push_path,
        server_name_override,
    )?;
    let status = adb
        .get_server_status(&device.id, &target.process_name)
        .await?;
    let status_colored = if status == "running" {
        status.green()
    } else {
        status.red()
    };
    println!(
        "  Frida server ({}): {}",
        target.process_name.cyan(),
        status_colored
    );

    // Show project info if available
    if let Ok(config) = config_result {
        println!();
        println!("{}", "Project Configuration:".bold());
        println!("  Frida version: {}", config.frida.version.cyan());
        println!("  Python version: {}", config.python.version.yellow());
        println!(
            "  Server port: {}",
            config.android.server_port.to_string().yellow()
        );
    }

    Ok(())
}
