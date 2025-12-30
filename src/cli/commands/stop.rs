use crate::android::AdbClient;
use crate::config::{resolve_android_server_target, GlobalConfigManager, ProjectConfigManager};
use crate::core::error::Result;
use colored::Colorize;

pub async fn execute(device_id: Option<String>) -> Result<()> {
    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));

    let device = adb.get_device(device_id.as_deref()).await?;

    let config = ProjectConfigManager::from_current_dir()?.load().await?;
    let target = resolve_android_server_target(
        &global_config.android.default_push_path,
        config.android.server_name.as_deref(),
    )?;

    adb.kill_server(
        &device.id,
        &target.process_name,
        &config.android.root_command,
    )
    .await?;

    println!(
        "{} {} stopped on {}",
        "✓".green().bold(),
        target.process_name.cyan(),
        device.id.cyan()
    );

    Ok(())
}
