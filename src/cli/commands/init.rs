use crate::config::{
    AndroidServerSource, GlobalConfigManager, LocalServerConfig, ProjectConfig,
    ProjectConfigManager, VersionMapping, VersionOverrides,
};
use crate::core::error::Result;
use crate::core::resolve_path;
use crate::frida::ServerDownloader;
use crate::python::{PypiClient, UvManager};
use chrono::{NaiveDate, TimeZone, Utc};
use colored::Colorize;
use std::env;

pub async fn execute(
    frida_version: Option<String>,
    python_version: Option<String>,
    arch: Option<String>,
    name: Option<String>,
    server_source: crate::cli::InitServerSource,
    local_server_path: Option<String>,
    frida_tools: Option<String>,
    objection: Option<String>,
) -> Result<()> {
    let global_mgr = GlobalConfigManager::new()?;
    let global_config = global_mgr.ensure_initialized().await?;
    let version_map = VersionMapping::load_or_init(&global_mgr.get_version_map_path()).await?;
    let overrides_path = global_mgr.get_version_overrides_path();
    let mut overrides = VersionOverrides::load_or_default(&overrides_path).await?;
    let mut overrides_dirty = false;

    let current_dir = env::current_dir()?;
    let project_mgr = ProjectConfigManager::new(&current_dir);

    // Check if already initialized
    if project_mgr.exists() {
        println!("{} Project already initialized", "ℹ".yellow().bold());
        return Ok(());
    }

    // Determine project name
    let project_name = name.unwrap_or_else(|| {
        current_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("frida-project")
            .to_string()
    });

    // Resolve versions
    let (frida_ver, frida_source) = match frida_version {
        Some(v) => (v, "CLI (--frida)".to_string()),
        None => (
            global_config.defaults.frida_version.clone(),
            format!("global defaults ({})", global_mgr.config_path().display()),
        ),
    };
    let python_ver =
        python_version.unwrap_or_else(|| global_config.defaults.python_version.clone());

    // Resolve frida version alias
    let resolved_frida = version_map.resolve_alias(&frida_ver);

    // Determine server source and (optional) frida-tools pinning.
    let (
        server_source_config,
        tools_version_to_install,
        tools_version_source,
        tools_allow_fallback,
    ) = match server_source {
        crate::cli::InitServerSource::Download => {
            let tools_resolution = version_map.resolve_tools_version(&resolved_frida);
            let override_tools = overrides
                .get_frida_tools(&resolved_frida)
                .map(|s| s.to_string());
            let tools_version = frida_tools
                .clone()
                .or_else(|| override_tools.clone())
                .or_else(|| {
                    tools_resolution
                        .as_ref()
                        .map(|res| res.tools_version.clone())
                });
            let source = if frida_tools.is_some() {
                "CLI (--frida-tools)"
            } else if override_tools.is_some() {
                "version overrides (auto-healed)"
            } else if tools_resolution.is_some() {
                "version map (preferred)"
            } else {
                "uv resolver (auto)"
            };

            let allow_fallback =
                frida_tools.is_none() && (override_tools.is_some() || tools_resolution.is_some());

            (
                AndroidServerSource::Download,
                tools_version,
                source.to_string(),
                allow_fallback,
            )
        }
        crate::cli::InitServerSource::Local => {
            let tools = frida_tools
                .clone()
                .expect("clap enforces --frida-tools when --server-source=local");
            (
                AndroidServerSource::Local,
                Some(tools),
                "CLI (--frida-tools, required)".to_string(),
                false,
            )
        }
    };

    // Fail fast for local server mode: validate frida-tools pinning and local binary path.
    if server_source_config == AndroidServerSource::Local {
        let tools = tools_version_to_install
            .as_deref()
            .expect("tools version is required for local source");
        if semver::Version::parse(tools).is_err() {
            return Err(crate::core::error::FridaMgrError::Config(format!(
                "Invalid frida-tools version '{}'; expected a semantic version like '13.3.0'",
                tools
            )));
        }

        let local_path = local_server_path
            .as_deref()
            .expect("clap enforces --local-server-path when --server-source=local");
        let resolved = resolve_path(&current_dir, local_path);
        if !resolved.is_file() {
            return Err(crate::core::error::FridaMgrError::FileNotFound(format!(
                "Local frida-server not found or not a file: {}",
                resolved.display()
            )));
        }
    } else if let Some(tools) = tools_version_to_install.as_deref() {
        if semver::Version::parse(tools).is_err() {
            return Err(crate::core::error::FridaMgrError::Config(format!(
                "Invalid frida-tools version '{}'; expected a semantic version like '13.3.0'",
                tools
            )));
        }
    }

    println!(
        "{} Initializing Frida project: {}",
        "⚙".blue().bold(),
        project_name.cyan()
    );
    println!("  Python version: {}", python_ver.yellow());
    println!(
        "  Frida version: {} (from {})",
        resolved_frida.yellow(),
        frida_source.yellow()
    );
    match tools_version_to_install.as_deref() {
        Some(v) => println!(
            "  Frida-tools version: {} ({})",
            v.yellow(),
            tools_version_source.yellow()
        ),
        None => println!(
            "  Frida-tools version: {} ({})",
            "auto".yellow(),
            tools_version_source.yellow()
        ),
    }

    let (mut objection_version_to_install, mut objection_source, mut objection_allow_fallback) = {
        let mapped = version_map.resolve_objection_version(&resolved_frida);
        let override_objection = overrides
            .get_objection(&resolved_frida, &python_ver)
            .map(|s| s.to_string());

        let version = objection.clone().or_else(|| {
            override_objection
                .clone()
                .or_else(|| mapped.as_ref().map(|m| m.objection_version.clone()))
        });
        let source = if objection.is_some() {
            "CLI (--objection)"
        } else if override_objection.is_some() {
            "version overrides (auto-healed)"
        } else if mapped.is_some() {
            "version map (preferred)"
        } else {
            "uv resolver (auto)"
        };
        let allow_fallback =
            objection.is_none() && (override_objection.is_some() || mapped.is_some());
        (version, source.to_string(), allow_fallback)
    };
    let objection_desired_initial = objection_version_to_install.clone();

    // Preflight: avoid selecting a non-installable PyPI version (e.g., requires newer Python).
    // If the pin comes from map/overrides (not from CLI), try to auto-select the nearest
    // installable version after the Frida release date; otherwise fall back to unpinned.
    if objection.is_none() && objection_allow_fallback {
        if let Some(v) = objection_version_to_install.as_deref() {
            let pypi = PypiClient::new();

            let mut needs_alternative = false;
            let mut reason: Option<String> = None;
            match pypi.requires_python("objection", v).await {
                Ok(Some(req_py)) => {
                    if !pypi.python_satisfies(&req_py, &python_ver) {
                        needs_alternative = true;
                        reason = Some(format!("requires Python {}", req_py.trim()));
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    needs_alternative = true;
                    reason = Some("not available on PyPI".to_string());
                }
            }

            if needs_alternative {
                let after = version_map
                    .mappings
                    .get(&resolved_frida)
                    .and_then(|info| NaiveDate::parse_from_str(&info.released, "%Y-%m-%d").ok())
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .map(|dt| Utc.from_utc_datetime(&dt))
                    .unwrap_or_else(Utc::now);

                match pypi
                    .select_first_compatible_on_or_after("objection", after, &python_ver)
                    .await
                {
                    Ok(Some(v2)) => {
                        objection_version_to_install = Some(v2);
                        objection_source = format!(
                            "PyPI auto-selected ({})",
                            reason.unwrap_or_else(|| "incompatible pin".to_string())
                        );
                        objection_allow_fallback = true;
                    }
                    _ => {
                        objection_version_to_install = None;
                        objection_allow_fallback = false;
                        objection_source = format!(
                            "version map skipped ({})",
                            reason.unwrap_or_else(|| "incompatible pin".to_string())
                        );
                    }
                }
            }
        }
    }

    if let Some(v) = objection_version_to_install.as_deref() {
        if semver::Version::parse(v).is_err() {
            return Err(crate::core::error::FridaMgrError::Config(format!(
                "Invalid objection version '{}'; expected a semantic version like '1.11.0'",
                v
            )));
        }
    }

    match objection_version_to_install.as_deref() {
        Some(v) => println!(
            "  Objection version: {} ({})",
            v.yellow(),
            objection_source.yellow()
        ),
        None => println!(
            "  Objection version: {} ({})",
            "auto".yellow(),
            objection_source.yellow()
        ),
    }

    // Create project config
    let mut config = ProjectConfig::default();
    config.project.name = project_name;
    config.python.version = python_ver.clone();
    config.frida.version = resolved_frida.clone();
    config.frida.tools_version = frida_tools.clone();
    config.objection.version = objection.clone();
    config.android.server.source = server_source_config;

    if config.android.server.source == AndroidServerSource::Local {
        let path = local_server_path
            .clone()
            .expect("clap enforces --local-server-path when --server-source=local");
        config.android.server.local = Some(LocalServerConfig { path });
    }

    if let Some(arch_str) = arch {
        config.android.arch = match arch_str.as_str() {
            "arm" => crate::config::ArchType::Arm,
            "arm64" => crate::config::ArchType::Arm64,
            "x86" => crate::config::ArchType::X86,
            "x86_64" => crate::config::ArchType::X8664,
            "auto" => crate::config::ArchType::Auto,
            _ => {
                println!(
                    "{} Invalid architecture '{}', using 'auto'",
                    "⚠".yellow().bold(),
                    arch_str
                );
                crate::config::ArchType::Auto
            }
        };
    }

    // Save config
    project_mgr.create(config.clone()).await?;
    println!("{} Created {}", "✓".green().bold(), "frida.toml".yellow());

    // Create Python virtual environment
    let uv_mgr = UvManager::new(current_dir.clone());
    uv_mgr.create_venv(&python_ver).await?;

    // Install Frida packages
    uv_mgr
        .install_frida(
            &resolved_frida,
            tools_version_to_install.as_deref(),
            tools_allow_fallback,
        )
        .await?;

    // Install Objection
    uv_mgr
        .install_objection(
            objection_version_to_install.as_deref(),
            objection_allow_fallback,
        )
        .await?;

    // Install any extra project packages (if configured)
    uv_mgr
        .install_python_packages(&config.python.packages)
        .await?;

    let installed_tools = uv_mgr
        .get_installed_version("frida-tools")
        .await
        .ok()
        .flatten();
    if let Some(version) = installed_tools.as_deref() {
        println!(
            "{} frida-tools installed: {}",
            "✓".green().bold(),
            version.yellow()
        );
    }

    let installed_objection = uv_mgr
        .get_installed_version("objection")
        .await
        .ok()
        .flatten();
    if let Some(version) = installed_objection.as_deref() {
        println!(
            "{} objection installed: {}",
            "✓".green().bold(),
            version.yellow()
        );
    }

    // Self-heal: if we didn't explicitly pin versions, record the resolved versions
    // so future runs are reproducible and don't retry incompatible pins.
    let mut config_dirty = false;

    if frida_tools.is_none() {
        if let Some(installed) = installed_tools.as_deref() {
            if tools_version_to_install.as_deref() != Some(installed) {
                overrides_dirty |= overrides.set_frida_tools(&resolved_frida, installed);
                config.frida.tools_version = Some(installed.to_string());
                config_dirty = true;
                println!(
                    "{} Pinned frida.tools_version → {} in {}",
                    "✓".green().bold(),
                    installed.yellow(),
                    project_mgr.config_path().display().to_string().yellow()
                );
            }
        }
    }

    if objection.is_none() {
        if let Some(installed) = installed_objection.as_deref() {
            if objection_desired_initial.as_deref() != Some(installed) {
                overrides_dirty |= overrides.set_objection(&resolved_frida, &python_ver, installed);
                config.objection.version = Some(installed.to_string());
                config_dirty = true;
                println!(
                    "{} Pinned objection.version → {} in {}",
                    "✓".green().bold(),
                    installed.yellow(),
                    project_mgr.config_path().display().to_string().yellow()
                );
            }
        }
    }

    if config_dirty {
        project_mgr.save(&config).await?;
    }
    if overrides_dirty {
        overrides.save(&overrides_path).await?;
    }

    // Download frida-server (only when using download source)
    if config.android.server.source == AndroidServerSource::Download {
        let cache_dir = GlobalConfigManager::new()?.get_cache_dir();
        let downloader = ServerDownloader::new(cache_dir);

        // Download for specified arch or default to arm64
        let download_arch = &config.android.arch;
        downloader.download(&resolved_frida, download_arch).await?;
    } else {
        let local_path = config
            .android
            .server
            .local
            .as_ref()
            .expect("local config must exist when source is local")
            .path
            .clone();
        let resolved = resolve_path(&current_dir, &local_path);
        println!(
            "{} Using local frida-server: {}",
            "✓".green().bold(),
            resolved.display().to_string().yellow()
        );
    }

    println!();
    println!("{} Project initialized successfully!", "✓".green().bold());
    println!();
    println!("Next steps:");
    println!("  1. Connect your Android device");
    println!("  2. Run: {} to push frida-server", "frida-mgr push".cyan());
    println!("  3. Start hacking with Frida!");

    Ok(())
}
