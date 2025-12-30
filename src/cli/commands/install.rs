use crate::config::{
    AndroidServerSource, GlobalConfigManager, ProjectConfigManager, VersionMapping,
};
use crate::core::error::Result;
use crate::frida::ServerDownloader;
use crate::python::UvManager;
use colored::Colorize;
use std::env;

pub async fn execute(version: String) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_mgr = ProjectConfigManager::from_current_dir()?;
    let config = project_mgr.load().await?;

    let global_mgr = GlobalConfigManager::new()?;
    let version_map = VersionMapping::load_or_init(&global_mgr.get_version_map_path()).await?;
    let resolved_version = version_map.resolve_alias(&version);

    let tools_resolution = version_map.resolve_tools_version(&resolved_version);
    let tools_version = config.frida.tools_version.as_deref().or_else(|| {
        tools_resolution
            .as_ref()
            .map(|res| res.tools_version.as_str())
    });

    println!(
        "{} Switching to Frida {}...",
        "⚙".blue().bold(),
        resolved_version.cyan()
    );
    match (config.frida.tools_version.as_deref(), &tools_resolution) {
        (Some(v), _) => println!("  Frida-tools version: {} (from frida.toml)", v.yellow()),
        (None, Some(res)) => println!(
            "  Frida-tools version: {} (pinned)",
            res.tools_version.yellow()
        ),
        (None, None) => println!(
            "  Frida-tools version: {} (let uv resolve)",
            "auto".yellow()
        ),
    }

    // Download frida-server if needed
    if config.android.server.source == AndroidServerSource::Download {
        let global_config = GlobalConfigManager::new()?;
        let cache_dir = global_config.get_cache_dir();
        let downloader = ServerDownloader::new(cache_dir);

        downloader
            .download(&resolved_version, &config.android.arch)
            .await?;
    }

    // Update Python packages
    let uv_mgr = UvManager::new(current_dir);
    uv_mgr
        .upgrade_frida(&resolved_version, tools_version)
        .await?;

    // Update config
    project_mgr.update_frida_version(&resolved_version).await?;

    println!();
    println!(
        "{} Successfully switched to Frida {}",
        "✓".green().bold(),
        resolved_version.cyan()
    );
    println!("  Run {} to update the device", "frida-mgr push".cyan());

    Ok(())
}
