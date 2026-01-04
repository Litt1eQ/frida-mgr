use crate::core::{ensure_dir_exists, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VersionOverrides {
    #[serde(default)]
    pub frida_tools: HashMap<String, String>,
    #[serde(default)]
    pub objection: HashMap<String, String>,
}

impl VersionOverrides {
    pub async fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path).await?;
        Ok(toml::from_str(&content)?)
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            ensure_dir_exists(parent).await?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }

    pub fn python_major_minor(python_version: &str) -> Option<String> {
        let v = python_version.trim();
        let mut parts = v.split('.').filter(|s| !s.is_empty());
        let major = parts.next()?;
        let minor = parts.next()?;
        if major.chars().all(|c| c.is_ascii_digit()) && minor.chars().all(|c| c.is_ascii_digit()) {
            Some(format!("{}.{}", major, minor))
        } else {
            None
        }
    }

    fn objection_key(frida_version: &str, python_version: &str) -> String {
        let py = Self::python_major_minor(python_version).unwrap_or_else(|| "unknown".to_string());
        format!("{}@{}", frida_version, py)
    }

    pub fn get_frida_tools(&self, frida_version: &str) -> Option<&str> {
        self.frida_tools.get(frida_version).map(|s| s.as_str())
    }

    pub fn get_objection(&self, frida_version: &str, python_version: &str) -> Option<&str> {
        let key = Self::objection_key(frida_version, python_version);
        self.objection.get(&key).map(|s| s.as_str())
    }

    pub fn set_frida_tools(&mut self, frida_version: &str, tools_version: &str) -> bool {
        match self.frida_tools.get(frida_version) {
            Some(existing) if existing == tools_version => false,
            _ => {
                self.frida_tools
                    .insert(frida_version.to_string(), tools_version.to_string());
                true
            }
        }
    }

    pub fn set_objection(
        &mut self,
        frida_version: &str,
        python_version: &str,
        objection_version: &str,
    ) -> bool {
        let key = Self::objection_key(frida_version, python_version);
        match self.objection.get(&key) {
            Some(existing) if existing == objection_version => false,
            _ => {
                self.objection.insert(key, objection_version.to_string());
                true
            }
        }
    }
}
