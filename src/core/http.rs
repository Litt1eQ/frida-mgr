use crate::core::error::{FridaMgrError, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::Client;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "application/atom+xml, application/xml;q=0.9, application/vnd.github+json;q=0.8, application/json;q=0.7, */*;q=0.5",
            ),
        );

        let client = Client::builder()
            .user_agent(format!("frida-mgr/{}", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        let response =
            self.client.get(url).send().await.map_err(|e| {
                FridaMgrError::Download(format!("Failed to download {}: {}", url, e))
            })?;

        if !response.status().is_success() {
            return Err(FridaMgrError::Download(format!(
                "HTTP error {}: {}",
                response.status(),
                url
            )));
        }

        let total_size = response.content_length().unwrap_or(0);

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .expect("Invalid progress bar template")
                .progress_chars("#>-"),
        );

        let mut file = File::create(dest).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| FridaMgrError::Download(e.to_string()))?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }

        pb.finish_with_message("Download complete");
        file.flush().await?;

        Ok(())
    }

    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        self.fetch_text_with_retry(url, 3).await
    }

    pub async fn fetch_text_with_retry(&self, url: &str, max_attempts: usize) -> Result<String> {
        let mut attempt = 0usize;
        let mut backoff = Duration::from_millis(500);

        loop {
            attempt += 1;
            let response = self.client.get(url).send().await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(resp.text().await?);
                    }

                    // Retry on 429 / 5xx to be polite with transient failures or rate limiting.
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if retryable && attempt < max_attempts {
                        let mut wait = backoff;
                        if let Some(retry_after) = resp.headers().get("retry-after") {
                            if let Ok(s) = retry_after.to_str() {
                                if let Ok(secs) = s.trim().parse::<u64>() {
                                    wait = Duration::from_secs(secs.min(30));
                                }
                            }
                        }
                        sleep(wait).await;
                        backoff = (backoff * 2).min(Duration::from_secs(8));
                        continue;
                    }

                    return Err(FridaMgrError::Download(format!(
                        "HTTP error {}: {}",
                        status, url
                    )));
                }
                Err(e) => {
                    if attempt < max_attempts {
                        sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(8));
                        continue;
                    }
                    return Err(FridaMgrError::Download(format!(
                        "Failed to fetch {}: {}",
                        url, e
                    )));
                }
            }
        }
    }

    pub async fn fetch_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let text = self.fetch_text(url).await?;
        let data = serde_json::from_str(&text)
            .map_err(|e| FridaMgrError::Download(format!("Failed to parse JSON: {}", e)))?;
        Ok(data)
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
