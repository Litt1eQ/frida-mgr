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
    let (tools_version, tools_allow_fallback) = match config.frida.tools_version.as_deref() {
        Some(v) => (Some(v), false),
        None => (
            tools_resolution
                .as_ref()
                .map(|res| res.tools_version.as_str()),
            tools_resolution.is_some(),
        ),
    };

    let objection_resolution = version_map.resolve_objection_version(&resolved_version);
    let (objection_version, objection_allow_fallback) = match config.objection.version.as_deref() {
        Some(v) => (Some(v), false),
        None => (
            objection_resolution
                .as_ref()
                .map(|res| res.objection_version.as_str()),
            objection_resolution.is_some(),
        ),
    };

    println!(
        "{} Switching to Frida {}...",
        "⚙".blue().bold(),
        resolved_version.cyan()
    );
    match (config.frida.tools_version.as_deref(), &tools_resolution) {
        (Some(v), _) => println!("  Frida-tools version: {} (from frida.toml)", v.yellow()),
        (None, Some(res)) => println!(
            "  Frida-tools version: {} (version map preferred)",
            res.tools_version.yellow()
        ),
        (None, None) => println!(
            "  Frida-tools version: {} (let uv resolve)",
            "auto".yellow()
        ),
    }

    match (config.objection.version.as_deref(), &objection_resolution) {
        (Some(v), _) => println!("  Objection version: {} (from frida.toml)", v.yellow()),
        (None, Some(res)) => println!(
            "  Objection version: {} (version map preferred)",
            res.objection_version.yellow()
        ),
        (None, None) => println!("  Objection version: {} (let uv resolve)", "auto".yellow()),
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
        .upgrade_frida(&resolved_version, tools_version, tools_allow_fallback)
        .await?;
    uv_mgr
        .upgrade_objection(objection_version, objection_allow_fallback)
        .await?;

    if tools_allow_fallback {
        if let (Some(pinned), Ok(Some(installed))) = (
            tools_version,
            uv_mgr.get_installed_version("frida-tools").await,
        ) {
            if pinned != installed {
                eprintln!(
                    "{} version map suggested frida-tools=={}, but installed frida-tools=={} (compatible fallback). Consider running {} to refresh your mapping.",
                    "⚠".yellow().bold(),
                    pinned.yellow(),
                    installed.yellow(),
                    "frida-mgr sync --update-map".cyan()
                );
            }
        }
    }

    if objection_allow_fallback {
        if let (Some(pinned), Ok(Some(installed))) = (
            objection_version,
            uv_mgr.get_installed_version("objection").await,
        ) {
            if pinned != installed {
                eprintln!(
                    "{} version map suggested objection=={}, but installed objection=={} (compatible fallback). Consider running {} to refresh your mapping.",
                    "⚠".yellow().bold(),
                    pinned.yellow(),
                    installed.yellow(),
                    "frida-mgr sync --update-map".cyan()
                );
            }
        }
    }

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
