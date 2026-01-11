use crate::config::schema::ProjectConfig;
use crate::config::validate_project_config;
use crate::core::error::{FridaMgrError, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

const PROJECT_CONFIG_FILE: &str = "frida.toml";

pub struct ProjectConfigManager {
    config_path: PathBuf,
}

impl ProjectConfigManager {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            config_path: project_dir.join(PROJECT_CONFIG_FILE),
        }
    }

    pub fn from_current_dir() -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let project_dir = Self::find_project_root(&current_dir).unwrap_or(current_dir);
        Ok(Self::new(&project_dir))
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn exists(&self) -> bool {
        self.config_path.exists()
    }

    pub async fn load(&self) -> Result<ProjectConfig> {
        if !self.exists() {
            return Err(FridaMgrError::NotInitialized);
        }

        let content = fs::read_to_string(&self.config_path).await?;
        let config: ProjectConfig = toml::from_str(&content)?;
        validate_project_config(&config)?;
        Ok(config)
    }

    pub async fn save(&self, config: &ProjectConfig) -> Result<()> {
        validate_project_config(config)?;
        let content = toml::to_string_pretty(config)?;
        fs::write(&self.config_path, content).await?;
        Ok(())
    }

    pub async fn create(&self, config: ProjectConfig) -> Result<()> {
        if self.exists() {
            return Err(FridaMgrError::Config(
                "Project already initialized. frida.toml exists.".to_string(),
            ));
        }

        self.save(&config).await?;
        Ok(())
    }

    pub async fn update_frida_version(&self, version: &str) -> Result<()> {
        let mut config = self.load().await?;
        config.frida.version = version.to_string();
        self.save(&config).await?;
        Ok(())
    }

    pub async fn update_python_version(&self, version: &str) -> Result<()> {
        let mut config = self.load().await?;
        config.python.version = version.to_string();
        self.save(&config).await?;
        Ok(())
    }

    pub fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
        let mut current = start_dir;

        loop {
            let config_path = current.join(PROJECT_CONFIG_FILE);
            if config_path.exists() {
                return Some(current.to_path_buf());
            }

            current = current.parent()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_server_requires_tools_version() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ProjectConfigManager::new(dir.path());

        let toml = r#"
[project]
name = "t"

[python]
version = "3.11"

[frida]
version = "16.6.6"

[android]
arch = "arm64"
server_port = 27042
root_command = "su"

[android.server]
source = "local"

[android.server.local]
path = "./bin/frida-server"
"#;

        tokio::fs::write(mgr.config_path(), toml).await.unwrap();

        let err = mgr.load().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("frida.tools_version"),
            "unexpected error: {}",
            msg
        );
    }

    #[tokio::test]
    async fn local_server_requires_local_path_section() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ProjectConfigManager::new(dir.path());

        let toml = r#"
[project]
name = "t"

[python]
version = "3.11"

[frida]
version = "16.6.6"
tools_version = "13.3.0"

[android]
arch = "arm64"
server_port = 27042
root_command = "su"

[android.server]
source = "local"
"#;

        tokio::fs::write(mgr.config_path(), toml).await.unwrap();

        let err = mgr.load().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("android.server.local"),
            "unexpected error: {}",
            msg
        );
    }

    #[tokio::test]
    async fn download_source_allows_missing_tools_version() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ProjectConfigManager::new(dir.path());

        let toml = r#"
[project]
name = "t"

[python]
version = "3.11"

[frida]
version = "16.6.6"

[android]
arch = "arm64"
server_port = 27042
root_command = "su"
"#;

        tokio::fs::write(mgr.config_path(), toml).await.unwrap();

        let config = mgr.load().await.unwrap();
        assert_eq!(config.frida.tools_version, None);
    }
}
