use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_ANDROID_SERVER_NAME: &str = "frida-server";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub project: ProjectMeta,
    pub python: PythonConfig,
    pub frida: FridaConfig,
    #[serde(default)]
    pub objection: ObjectionConfig,
    pub android: AndroidConfig,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PythonConfig {
    pub version: String,
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FridaConfig {
    pub version: String,
    #[serde(default)]
    pub tools_version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ObjectionConfig {
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AndroidConfig {
    #[serde(default = "default_arch")]
    pub arch: ArchType,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default = "default_port")]
    pub server_port: u16,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_root_command")]
    pub root_command: String,
    #[serde(default, skip_serializing_if = "AndroidServerConfig::is_default")]
    pub server: AndroidServerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AndroidServerConfig {
    #[serde(default)]
    pub source: AndroidServerSource,
    #[serde(default)]
    pub local: Option<LocalServerConfig>,
}

impl Default for AndroidServerConfig {
    fn default() -> Self {
        Self {
            source: AndroidServerSource::default(),
            local: None,
        }
    }
}

impl AndroidServerConfig {
    fn is_default(&self) -> bool {
        self.source == AndroidServerSource::Download && self.local.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AndroidServerSource {
    Download,
    Local,
}

impl Default for AndroidServerSource {
    fn default() -> Self {
        Self::Download
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalServerConfig {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArchType {
    Auto,
    Arm,
    Arm64,
    X86,
    #[serde(rename = "x86_64")]
    X8664,
}

impl ArchType {
    pub fn to_str(&self) -> &str {
        match self {
            ArchType::Auto => "auto",
            ArchType::Arm => "arm",
            ArchType::Arm64 => "arm64",
            ArchType::X86 => "x86",
            ArchType::X8664 => "x86_64",
        }
    }

    pub fn from_abi(abi: &str) -> Self {
        match abi {
            "arm64-v8a" | "aarch64" => ArchType::Arm64,
            "armeabi-v7a" | "armeabi" | "arm" => ArchType::Arm,
            "x86_64" => ArchType::X8664,
            "x86" => ArchType::X86,
            _ => ArchType::Arm64, // default to arm64
        }
    }
}

fn default_arch() -> ArchType {
    ArchType::Auto
}

fn default_server_name() -> String {
    DEFAULT_ANDROID_SERVER_NAME.to_string()
}

fn default_port() -> u16 {
    27042
}

fn default_root_command() -> String {
    "su".to_string()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project: ProjectMeta {
                name: "frida-project".to_string(),
                description: String::new(),
            },
            python: PythonConfig {
                version: "3.11".to_string(),
                packages: Vec::new(),
            },
            frida: FridaConfig {
                version: "16.6.6".to_string(),
                tools_version: None,
            },
            objection: ObjectionConfig { version: None },
            android: AndroidConfig {
                arch: default_arch(),
                server_name: Some(default_server_name()),
                server_port: default_port(),
                auto_start: false,
                root_command: default_root_command(),
                server: AndroidServerConfig::default(),
            },
            environment: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalConfig {
    pub cache: CacheConfig,
    pub uv: UvConfig,
    pub android: GlobalAndroidConfig,
    pub network: NetworkConfig,
    pub defaults: DefaultsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    pub dir: String,
    #[serde(default = "default_max_cache_gb")]
    pub max_size_gb: u64,
    #[serde(default = "default_true")]
    pub auto_clean: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UvConfig {
    pub cache_dir: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalAndroidConfig {
    #[serde(default = "default_adb_path")]
    pub adb_path: String,
    #[serde(default = "default_push_path")]
    pub default_push_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfig {
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_mirror")]
    pub mirror: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DefaultsConfig {
    pub python_version: String,
    pub frida_version: String,
}

fn default_max_cache_gb() -> u64 {
    10
}

fn default_true() -> bool {
    true
}

fn default_adb_path() -> String {
    "adb".to_string()
}

fn default_push_path() -> String {
    "/data/local/tmp/frida-server".to_string()
}

fn default_timeout() -> u64 {
    300
}

fn default_retries() -> u32 {
    3
}

fn default_mirror() -> String {
    "github".to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cache_dir = format!("{}/.frida-mgr/cache", home);
        let uv_cache_dir = format!("{}/.frida-mgr/uv-cache", home);

        Self {
            cache: CacheConfig {
                dir: cache_dir,
                max_size_gb: default_max_cache_gb(),
                auto_clean: default_true(),
            },
            uv: UvConfig {
                cache_dir: uv_cache_dir,
            },
            android: GlobalAndroidConfig {
                adb_path: default_adb_path(),
                default_push_path: default_push_path(),
            },
            network: NetworkConfig {
                timeout_seconds: default_timeout(),
                max_retries: default_retries(),
                mirror: default_mirror(),
            },
            defaults: DefaultsConfig {
                python_version: "3.11".to_string(),
                frida_version: "16.6.6".to_string(),
            },
        }
    }
}
