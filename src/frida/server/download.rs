use crate::config::ArchType;
use crate::core::error::Result;
use crate::core::{decompress_xz, ensure_dir_exists, make_executable, HttpClient};
use colored::Colorize;
use std::path::PathBuf;

pub struct ServerDownloader {
    cache_dir: PathBuf,
    http_client: HttpClient,
}

impl ServerDownloader {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            http_client: HttpClient::new(),
        }
    }

    pub async fn download(&self, version: &str, arch: &ArchType) -> Result<PathBuf> {
        let arch_str = self.get_arch_string(arch);
        let cache_path = self.get_cache_path(version, &arch_str);

        // Check if already cached
        if cache_path.exists() {
            println!(
                "{} Using cached frida-server {} for {}",
                "✓".green().bold(),
                version.cyan(),
                arch_str.yellow()
            );
            return Ok(cache_path);
        }

        println!(
            "{} Downloading frida-server {} for {}...",
            "↓".blue().bold(),
            version.cyan(),
            arch_str.yellow()
        );

        ensure_dir_exists(cache_path.parent().unwrap()).await?;

        let url = self.get_download_url(version, &arch_str);
        let compressed_path = cache_path.with_extension("xz");

        // Download compressed file
        self.http_client
            .download_file(&url, &compressed_path)
            .await?;

        // Decompress
        println!("{} Decompressing...", "⚙".blue().bold());
        decompress_xz(&compressed_path, &cache_path).await?;

        // Make executable
        make_executable(&cache_path).await?;

        // Clean up compressed file
        tokio::fs::remove_file(&compressed_path).await?;

        println!(
            "{} frida-server {} downloaded and cached",
            "✓".green().bold(),
            version.cyan()
        );

        Ok(cache_path)
    }

    fn get_download_url(&self, version: &str, arch: &str) -> String {
        format!(
            "https://github.com/frida/frida/releases/download/{}/frida-server-{}-android-{}.xz",
            version, version, arch
        )
    }

    fn get_cache_path(&self, version: &str, arch: &str) -> PathBuf {
        self.cache_dir
            .join("servers")
            .join(version)
            .join(arch)
            .join("frida-server")
    }

    fn get_arch_string(&self, arch: &ArchType) -> String {
        match arch {
            ArchType::Arm => "arm",
            ArchType::Arm64 => "arm64",
            ArchType::X86 => "x86",
            ArchType::X8664 => "x86_64",
            ArchType::Auto => "arm64", // default
        }
        .to_string()
    }

    pub async fn get_cached(&self, version: &str, arch: &ArchType) -> Option<PathBuf> {
        let arch_str = self.get_arch_string(arch);
        let cache_path = self.get_cache_path(version, &arch_str);

        if cache_path.exists() {
            Some(cache_path)
        } else {
            None
        }
    }

    pub async fn list_cached_versions(&self) -> Result<Vec<String>> {
        let servers_dir = self.cache_dir.join("servers");

        if !servers_dir.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        let mut entries = tokio::fs::read_dir(servers_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    versions.push(name.to_string());
                }
            }
        }

        versions.sort();
        Ok(versions)
    }
}
