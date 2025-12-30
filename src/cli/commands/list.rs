use crate::config::{GlobalConfigManager, VersionMapping};
use crate::core::error::Result;
use crate::frida::ServerDownloader;
use colored::Colorize;

pub async fn execute(installed: bool) -> Result<()> {
    if installed {
        list_installed().await
    } else {
        list_available().await
    }
}

async fn list_available() -> Result<()> {
    let global_mgr = GlobalConfigManager::new()?;
    let version_map = VersionMapping::load_or_init(&global_mgr.get_version_map_path()).await?;
    let versions = version_map.list_versions();

    println!("{}", "Available Frida versions:".bold());
    println!();

    for version in &versions {
        if let Some(info) = version_map.mappings.get(version) {
            let mut line = format!("  {} → frida-tools {}", version.cyan(), info.tools.yellow());

            // Check for aliases
            for (alias, target) in &version_map.aliases {
                if target == version {
                    line.push_str(&format!(" ({})", alias.green()));
                }
            }

            println!("{}", line);
        }
    }

    println!();
    println!(
        "Use {} to install a specific version",
        "frida-mgr install <version>".cyan()
    );

    Ok(())
}

async fn list_installed() -> Result<()> {
    let cache_dir = GlobalConfigManager::new()?.get_cache_dir();
    let downloader = ServerDownloader::new(cache_dir);

    let versions = downloader.list_cached_versions().await?;

    if versions.is_empty() {
        println!("{}", "No cached frida-server versions found".yellow());
        println!(
            "Run {} to download a version",
            "frida-mgr install <version>".cyan()
        );
        return Ok(());
    }

    println!("{}", "Cached frida-server versions:".bold());
    println!();

    for version in versions {
        println!("  {}", version.cyan());
    }

    Ok(())
}
