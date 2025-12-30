use crate::android::AdbClient;
use crate::config::{resolve_android_server_target, GlobalConfigManager};
use crate::core::error::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));

    let devices = adb.list_devices().await?;

    if devices.is_empty() {
        println!("{}", "No devices connected".yellow());
        return Ok(());
    }

    println!("{}", "Connected Android devices:".bold());
    println!();

    let target = resolve_android_server_target(&global_config.android.default_push_path, None)?;

    for device in &devices {
        let arch_result = adb.get_arch(&device.id).await;
        let arch_str = arch_result
            .map(|a| a.to_str().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let status = adb
            .get_server_status(&device.id, &target.process_name)
            .await?;
        let status_indicator = if status == "running" {
            "●".green()
        } else {
            "○".red()
        };

        println!(
            "  {} {} ({}) - {}",
            status_indicator,
            device.id.cyan(),
            device.model.yellow(),
            arch_str.blue()
        );
    }

    Ok(())
}
