use crate::config::{
    AndroidServerSource, GlobalConfigManager, ProjectConfigManager, VersionMapping,
};
use crate::core::error::{FridaMgrError, Result};
use crate::frida::ServerDownloader;
use crate::python::UvManager;
use colored::Colorize;
use std::env;

pub async fn execute(
    update_map: bool,
    prerelease: bool,
    no_project: bool,
    recreate_venv: bool,
) -> Result<()> {
    let global_mgr = GlobalConfigManager::new()?;
    let map_path = global_mgr.get_version_map_path();

    let version_map = if update_map {
        println!(
            "{} Refreshing version mapping from GitHub releases...",
            "⚙".blue().bold()
        );
        let map = VersionMapping::build_from_github_releases(prerelease).await?;
        if map.mappings.is_empty() {
            return Err(FridaMgrError::Download(
                "Version mapping sync produced 0 entries; refusing to overwrite mapping file"
                    .to_string(),
            ));
        }
        map.save(&map_path).await?;
        println!(
            "{} Updated mapping file: {} ({} entries)",
            "✓".green().bold(),
            map_path.display().to_string().yellow(),
            map.mappings.len().to_string().cyan()
        );
        map
    } else {
        VersionMapping::load_or_init(&map_path).await?
    };

    if no_project {
        return Ok(());
    }

    let current_dir = env::current_dir()?;
    let project_mgr = ProjectConfigManager::from_current_dir()?;
    let config = match project_mgr.load().await {
        Ok(c) => c,
        Err(FridaMgrError::NotInitialized) if update_map => {
            println!(
                "{} No project found ({}). Mapping updated only.",
                "ℹ".blue().bold(),
                project_mgr.config_path().display().to_string().yellow()
            );
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    let resolved_frida = version_map.resolve_alias(&config.frida.version);
    let tools_resolution = version_map.resolve_tools_version(&resolved_frida);
    let tools_version = config.frida.tools_version.as_deref().or_else(|| {
        tools_resolution
            .as_ref()
            .map(|res| res.tools_version.as_str())
    });

    println!(
        "{} Syncing project to Frida {}...",
        "⚙".blue().bold(),
        resolved_frida.cyan()
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

    let uv_mgr = UvManager::new(current_dir);
    uv_mgr
        .ensure_venv(&config.python.version, recreate_venv)
        .await?;
    uv_mgr.upgrade_frida(&resolved_frida, tools_version).await?;
    uv_mgr
        .install_python_packages(&config.python.packages)
        .await?;

    if config.android.server.source == AndroidServerSource::Download {
        let downloader = ServerDownloader::new(global_mgr.get_cache_dir());
        downloader
            .download(&resolved_frida, &config.android.arch)
            .await?;
    }

    if config.frida.version != resolved_frida {
        project_mgr.update_frida_version(&resolved_frida).await?;
        println!(
            "{} Updated {} frida.version → {}",
            "✓".green().bold(),
            project_mgr.config_path().display().to_string().yellow(),
            resolved_frida.cyan()
        );
    }

    Ok(())
}
