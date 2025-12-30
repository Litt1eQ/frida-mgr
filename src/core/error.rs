use thiserror::Error;

#[derive(Error, Debug)]
pub enum FridaMgrError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Frida version {0} not found in mapping table")]
    VersionNotFound(String),

    #[error("Python environment error: {0}")]
    PythonEnv(String),

    #[error("ADB error: {0}")]
    Adb(String),

    #[error("Download failed: {0}")]
    Download(String),

    #[error("Checksum verification failed for {0}")]
    ChecksumMismatch(String),

    #[error("No Android device connected")]
    NoDevice,

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Invalid architecture: {0}")]
    InvalidArch(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Project not initialized. Run 'frida-mgr init' first")]
    NotInitialized,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, FridaMgrError>;
