use super::{DownloadBackend, Downloadable};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct GitHubBackend {
    pub repo: String,
    pub mask: String,
    pub arch_masks: Option<std::collections::HashMap<String, String>>,
    pub token: Option<String>,
}

#[async_trait]
impl DownloadBackend for GitHubBackend {
    async fn fetch_version(
        &self,
        _package: &dyn Downloadable,
    ) -> Result<String, String> {
        let mut cmd = Command::new("gh");
        cmd.arg("api")
            .arg(format!("/repos/{}/releases/latest", self.repo));
        if let Some(ref token) = self.token {
            cmd.env("GITHUB_TOKEN", token);
        }

        let output = cmd.output().map_err(|e| format!("gh api failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("gh exited with code {:?}", output.status.code()));
        }

        let json = String::from_utf8_lossy(&output.stdout);
        // Extract tag_name from single-line JSON using regex
        let re = regex::Regex::new(r#""tag_name"\s*:\s*"([^"]+)""#).unwrap();
        let tag = re.captures(&json)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(tag)
    }

    async fn download(
        &self,
        _package: &dyn Downloadable,
        arch: &str,
        target: &Path,
    ) -> Result<PathBuf, String> {
        let parent = target.parent().unwrap_or(Path::new("."));

        let mask = if arch == "universal" {
            &self.mask
        } else {
            match &self.arch_masks {
                Some(masks) => match masks.get(arch) {
                    Some(m) => m.as_str(),
                    None => return Err(format!("No arch_masks entry for architecture: {}", arch)),
                },
                None => &self.mask,
            }
        };

        let mut cmd = Command::new("gh");
        cmd.arg("release")
            .arg("download")
            .arg("--repo")
            .arg(&self.repo)
            .arg("--pattern")
            .arg(mask)
            .arg("--dir")
            .arg(parent)
            .arg("--clobber");
        if let Some(ref token) = self.token {
            cmd.env("GITHUB_TOKEN", token);
        }

        let output = cmd.output().map_err(|e| format!("gh download failed: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "gh exited with code {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let entries = std::fs::read_dir(parent)
            .map_err(|e| format!("Cannot read cache dir: {}", e))?;
        let mut found = None;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".apk") || name.ends_with(".xapk") {
                found = Some(entry.path());
                break;
            }
        }

        let actual_file = found.ok_or_else(|| format!(
            "gh did not produce any APK in {}",
            parent.display()
        ))?;

        let ext = actual_file.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("apk");
        let stem = target.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("download");
        let final_path = parent.join(format!("{}.{}", stem, ext));

        if actual_file != final_path {
            std::fs::rename(&actual_file, &final_path)
                .map_err(|e| format!("Cannot rename {} -> {}: {}",
                    actual_file.display(), final_path.display(), e))?;
        }
        Ok(final_path)
    }
}
