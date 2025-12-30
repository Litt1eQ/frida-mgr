use crate::android::AdbClient;
use crate::config::{
    resolve_android_server_target, AndroidServerSource, GlobalConfigManager, ProjectConfigManager,
};
use crate::core::error::Result;
use crate::core::resolve_path;
use crate::frida::ServerDownloader;
use colored::Colorize;

pub async fn execute(device_id: Option<String>, auto_start: bool) -> Result<()> {
    let project_mgr = ProjectConfigManager::from_current_dir()?;
    let config = project_mgr.load().await?;
    let project_dir = project_mgr
        .config_path()
        .parent()
        .unwrap_or(std::path::Path::new("."));

    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));

    // Get device
    let device = adb.get_device(device_id.as_deref()).await?;
    println!(
        "{} Target device: {} ({})",
        "ℹ".blue().bold(),
        device.id.cyan(),
        device.model.yellow()
    );

    // Detect architecture if auto
    let target_arch = if config.android.arch == crate::config::ArchType::Auto {
        let detected = adb.get_arch(&device.id).await?;
        println!(
            "{} Detected architecture: {}",
            "ℹ".blue().bold(),
            detected.to_str().yellow()
        );
        detected
    } else {
        config.android.arch.clone()
    };

    let server_path = match config.android.server.source {
        AndroidServerSource::Download => {
            // Get frida-server from cache
            let cache_dir = GlobalConfigManager::new()?.get_cache_dir();
            let downloader = ServerDownloader::new(cache_dir);

            downloader
                .get_cached(&config.frida.version, &target_arch)
                .await
                .ok_or_else(|| {
                    crate::core::error::FridaMgrError::FileNotFound(format!(
                        "frida-server {} for {}. Run 'frida-mgr install {}' first.",
                        config.frida.version,
                        target_arch.to_str(),
                        config.frida.version
                    ))
                })?
        }
        AndroidServerSource::Local => {
            let local_cfg = config
                .android
                .server
                .local
                .as_ref()
                .expect("config validation enforces local config when source=local");
            let resolved = resolve_path(project_dir, &local_cfg.path);
            if !resolved.is_file() {
                return Err(crate::core::error::FridaMgrError::FileNotFound(format!(
                    "Local frida-server not found or not a file: {}",
                    resolved.display()
                )));
            }
            resolved
        }
    };

    let target = resolve_android_server_target(
        &global_config.android.default_push_path,
        config.android.server_name.as_deref(),
    )?;
    let remote_path = target.remote_path;
    let server_name = target.process_name;

    // Push to device
    adb.push_file(&device.id, &server_path, &remote_path)
        .await?;

    // Make executable
    adb.make_executable(&device.id, &remote_path).await?;

    // Start if requested or configured
    let should_start = auto_start || config.android.auto_start;

    if should_start {
        adb.start_server(
            &device.id,
            &remote_path,
            &server_name,
            config.android.server_port,
            &config.android.root_command,
        )
        .await?;

        println!();
        println!(
            "{} {} is running on port {}",
            "✓".green().bold(),
            server_name.cyan(),
            config.android.server_port.to_string().cyan()
        );
    } else {
        println!();
        println!(
            "{} {} pushed to device",
            "✓".green().bold(),
            server_name.cyan()
        );
        println!("  Run {} to start the server", "frida-mgr start".cyan());
    }

    Ok(())
}
