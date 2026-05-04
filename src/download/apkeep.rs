use super::{DownloadBackend, Downloadable};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ApkeepBackend {
    pub token: Option<String>,
    pub user: Option<String>,
    pub source: String,
}

impl ApkeepBackend {
    /// Map config source names to apkeep `-d` source identifiers
    fn apkeep_source(&self) -> &str {
        match self.source.as_str() {
            "google_play" => "google-play",
            "apkpure" => "apk-pure",
            "rustore" => "ru-store",
            other => other,
        }
    }
}

#[async_trait]
impl DownloadBackend for ApkeepBackend {
    async fn fetch_version(
        &self,
        package: &dyn Downloadable,
    ) -> Result<String, String> {
        let id = package.id();
        let package_name = id.split(':').nth(1).unwrap_or("");
        let mut cmd = Command::new("apkeep");
        cmd.arg("-d").arg(self.apkeep_source());
        cmd.arg("-a").arg(package_name);
        if let Some(ref user) = self.user {
            cmd.arg("-e").arg(user);
        }
        if let Some(ref token) = self.token {
            cmd.arg("-t").arg(token);
        }
        cmd.arg("-l");

        let output = cmd.output()
            .map_err(|e| format!("apkeep --list-versions failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        // apkeep -l returns CSV: app_id,version_code,version_name,offer_type
        let version = stdout.lines()
            .nth(1) // skip header
            .unwrap_or("")
            .split(',')
            .nth(2) // version_name column
            .unwrap_or("unknown")
            .trim()
            .to_string();

        if version.is_empty() || version == "unknown" {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("apkeep could not resolve version for {}: {}", package_name, stderr));
        }
        Ok(version)
    }

    async fn download(
        &self,
        package: &dyn Downloadable,
        arch: &str,
        target: &Path,
    ) -> Result<PathBuf, String> {
        let id = package.id();
        let package_name = id.split(':').nth(1).unwrap_or("");
        let parent = target.parent().ok_or("Invalid target path")?;

        let mut cmd = Command::new("apkeep");
        cmd.arg("-d").arg(self.apkeep_source());
        cmd.arg("-a").arg(package_name);
        if let Some(ref user) = self.user {
            cmd.arg("-e").arg(user);
        }
        if let Some(ref token) = self.token {
            cmd.arg("-t").arg(token);
        }
        if arch != "universal" {
            cmd.arg("-o").arg(format!("arch={}", arch));
        }
        cmd.arg(parent);

        let output = cmd.output().map_err(|e| format!("apkeep download failed: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "apkeep exited with code {:?}: {}",
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
            if name.starts_with(package_name) && (name.ends_with(".apk") || name.ends_with(".xapk")) {
                found = Some(entry.path());
                break;
            }
        }

        let actual_file = found.ok_or_else(|| format!(
            "apkeep did not produce any file matching {}* in {}",
            package_name, parent.display()
        ))?;

        let ext = actual_file.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("apk");
        let stem = target.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(package_name);
        let final_path = parent.join(format!("{}.{}", stem, ext));

        if actual_file != final_path {
            std::fs::rename(&actual_file, &final_path)
                .map_err(|e| format!("Cannot rename {} -> {}: {}",
                    actual_file.display(), final_path.display(), e))?;
        }
        Ok(final_path)
    }
}
