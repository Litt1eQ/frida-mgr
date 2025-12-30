use crate::config::schema::{AndroidServerSource, ProjectConfig};
use crate::core::error::{FridaMgrError, Result};

pub fn validate_android_server_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(FridaMgrError::Config(
            "android.server_name cannot be empty".to_string(),
        ));
    }

    if name.starts_with('-') {
        return Err(FridaMgrError::Config(
            "android.server_name cannot start with '-'".to_string(),
        ));
    }

    if name.chars().any(|c| c == '/' || c == '\\') {
        return Err(FridaMgrError::Config(
            "android.server_name must be a file name (no path separators)".to_string(),
        ));
    }

    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if !valid {
        return Err(FridaMgrError::Config(
            "android.server_name may only contain ASCII letters/digits and . _ -".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_project_config(config: &ProjectConfig) -> Result<()> {
    if config.project.name.trim().is_empty() {
        return Err(FridaMgrError::Config(
            "project.name cannot be empty".to_string(),
        ));
    }

    if config.python.version.trim().is_empty() {
        return Err(FridaMgrError::Config(
            "python.version cannot be empty".to_string(),
        ));
    }

    if config.python.packages.iter().any(|p| p.trim().is_empty()) {
        return Err(FridaMgrError::Config(
            "python.packages cannot contain empty entries".to_string(),
        ));
    }

    if config.frida.version.trim().is_empty() {
        return Err(FridaMgrError::Config(
            "frida.version cannot be empty".to_string(),
        ));
    }

    if let Some(name) = config.android.server_name.as_deref() {
        validate_android_server_name(name)?;
    }

    if config.android.server_port == 0 {
        return Err(FridaMgrError::Config(
            "android.server_port must be > 0".to_string(),
        ));
    }

    if config.android.root_command.trim().is_empty() {
        return Err(FridaMgrError::Config(
            "android.root_command cannot be empty".to_string(),
        ));
    }

    if config.android.server.source == AndroidServerSource::Local {
        let tools_version_ok = config
            .frida
            .tools_version
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        if !tools_version_ok {
            return Err(FridaMgrError::Config(
                "frida.tools_version is required when android.server.source = \"local\""
                    .to_string(),
            ));
        }

        let local = config.android.server.local.as_ref().ok_or_else(|| {
            FridaMgrError::Config(
                "android.server.local is required when android.server.source = \"local\""
                    .to_string(),
            )
        })?;

        if local.path.trim().is_empty() {
            return Err(FridaMgrError::Config(
                "android.server.local.path cannot be empty".to_string(),
            ));
        }
    }

    Ok(())
}
