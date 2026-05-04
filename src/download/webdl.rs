use super::{DownloadBackend, Downloadable};
use async_trait::async_trait;
use reqwest;
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct WebDlBackend {
    pub url: String,
    pub arch_files: Option<std::collections::HashMap<String, crate::config::ArchFile>>,
}

#[async_trait]
impl DownloadBackend for WebDlBackend {
    async fn fetch_version(
        &self,
        _package: &dyn Downloadable,
    ) -> Result<String, String> {
        let client = reqwest::Client::new();
        let resp = client
            .head(&self.url)
            .send()
            .await
            .map_err(|e| format!("HEAD request failed: {}", e))?;

        let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).unwrap_or("");
        let last_mod = resp
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let content_len = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let mut hasher = Sha256::new();
        hasher.update(etag.as_bytes());
        hasher.update(last_mod.as_bytes());
        hasher.update(content_len.as_bytes());
        Ok(hex::encode(hasher.finalize())[..16].to_string())
    }

    async fn download(
        &self,
        _package: &dyn Downloadable,
        arch: &str,
        target: &Path,
    ) -> Result<std::path::PathBuf, String> {
        let url = if arch == "universal" {
            &self.url
        } else {
            match &self.arch_files {
                Some(files) => match files.get(arch) {
                    Some(af) => &af.url,
                    None => return Err(format!("No arch_files entry for architecture: {}", arch)),
                },
                None => &self.url,
            }
        };

        let client = reqwest::Client::new();
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("GET request failed: {}", e))?;

        let bytes = resp.bytes().await.map_err(|e| format!("Read body failed: {}", e))?;
        std::fs::write(target, bytes).map_err(|e| format!("Write failed: {}", e))?;
        Ok(target.to_path_buf())
    }
}
