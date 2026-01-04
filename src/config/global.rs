use crate::config::schema::GlobalConfig;
use crate::core::error::Result;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use tokio::fs;

const GLOBAL_CONFIG_FILE: &str = "config.toml";

pub struct GlobalConfigManager {
    config_dir: PathBuf,
    config_path: PathBuf,
}

impl GlobalConfigManager {
    pub fn new() -> Result<Self> {
        let config_dir = Self::get_config_dir()?;
        let config_path = config_dir.join(GLOBAL_CONFIG_FILE);

        Ok(Self {
            config_dir,
            config_path,
        })
    }

    fn get_config_dir() -> Result<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("com", "frida-mgr", "frida-mgr") {
            Ok(proj_dirs.config_dir().to_path_buf())
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            Ok(PathBuf::from(home).join(".frida-mgr"))
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub async fn load(&self) -> Result<GlobalConfig> {
        if !self.config_path.exists() {
            return Ok(GlobalConfig::default());
        }

        let content = fs::read_to_string(&self.config_path).await?;
        let config: GlobalConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub async fn save(&self, config: &GlobalConfig) -> Result<()> {
        fs::create_dir_all(&self.config_dir).await?;
        let content = toml::to_string_pretty(config)?;
        fs::write(&self.config_path, content).await?;
        Ok(())
    }

    pub async fn ensure_initialized(&self) -> Result<GlobalConfig> {
        if !self.config_path.exists() {
            let config = GlobalConfig::default();
            self.save(&config).await?;
            Ok(config)
        } else {
            self.load().await
        }
    }

    pub fn get_cache_dir(&self) -> PathBuf {
        self.config_dir.join("cache")
    }

    pub fn get_servers_cache_dir(&self) -> PathBuf {
        self.get_cache_dir().join("servers")
    }

    pub fn get_version_map_path(&self) -> PathBuf {
        self.config_dir.join("version-map.toml")
    }

    pub fn get_version_overrides_path(&self) -> PathBuf {
        self.config_dir.join("version-overrides.toml")
    }
}

impl Default for GlobalConfigManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize global config manager")
    }
}
