pub mod global;
pub mod overrides;
pub mod project;
pub mod schema;
pub mod validation;
pub mod version_map;

use crate::core::error::Result;

pub use global::GlobalConfigManager;
pub use overrides::VersionOverrides;
pub use project::ProjectConfigManager;
pub use schema::{
    AgentBuildTool, AndroidServerSource, ArchType, GlobalConfig, LocalServerConfig, ProjectConfig,
    DEFAULT_ANDROID_SERVER_NAME,
};
pub use validation::{validate_android_server_name, validate_project_config};
pub use version_map::VersionMapping;

#[derive(Debug, Clone)]
pub struct AndroidServerTarget {
    pub remote_path: String,
    pub process_name: String,
}

pub fn resolve_android_server_target(
    default_push_path: &str,
    server_name_override: Option<&str>,
) -> Result<AndroidServerTarget> {
    let default_is_dir = default_push_path.ends_with('/');
    let default_trimmed = default_push_path.trim_end_matches('/');

    let (process_name, remote_path) = match server_name_override {
        Some(name) => {
            validate_android_server_name(name)?;

            let dir = if default_is_dir {
                default_trimmed
            } else {
                default_trimmed
                    .rsplit_once('/')
                    .map(|(dir, _)| dir)
                    .unwrap_or("")
            };

            let remote_path = if dir.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", dir, name)
            };

            (name.to_string(), remote_path)
        }
        None => {
            if default_is_dir {
                let name = DEFAULT_ANDROID_SERVER_NAME;
                validate_android_server_name(name)?;
                (name.to_string(), format!("{}/{}", default_trimmed, name))
            } else {
                let name = default_trimmed
                    .rsplit_once('/')
                    .map(|(_, name)| name)
                    .unwrap_or(default_trimmed);
                validate_android_server_name(name)?;
                (name.to_string(), default_trimmed.to_string())
            }
        }
    };

    Ok(AndroidServerTarget {
        remote_path,
        process_name,
    })
}
