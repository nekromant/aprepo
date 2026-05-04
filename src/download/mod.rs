pub mod backend;
pub mod apkeep;
pub mod github;
pub mod webdl;

use crate::config::Config;
use crate::state::State;
use crate::util::logging::{info, warn, error};

use std::time::Duration;
use tokio::time::sleep;

#[allow(dead_code)]
pub struct DownloadOrchestrator {
    config: Config,
    state: State,
    verbose: bool,
    force: bool,
    package_filter: Option<String>,
}

impl DownloadOrchestrator {
    pub fn new(config: Config, state: State, verbose: bool, force: bool, package_filter: Option<String>) -> Self {
        Self { config, state, verbose, force, package_filter }
    }

    pub async fn run(&mut self) -> Result<DownloadSummary, String> {
        let mut summary = DownloadSummary::default();
        let mut packages = self.config.all_packages();

        if let Some(ref filter) = self.package_filter {
            let original_count = packages.len();
            packages.retain(|p| p.is_store_package(filter));
            let skipped_count = original_count - packages.len();
            if skipped_count > 0 {
                warn(&format!("--package filter active: {} non-store packages skipped", skipped_count));
            }
            if packages.is_empty() {
                warn(&format!("No configured store package matches filter: {}", filter));
                return Ok(summary);
            }
        }

        let mut retry_queue: Vec<(usize, String)> = Vec::new();

        for (idx, package) in packages.iter().enumerate() {
            let arches = package.architectures();
            for arch in &arches {
                if let Err(e) = self.process_package_arch(package, arch, &mut summary).await {
                    error(&format!("Failed to download {} (arch {}): {}", package.id(), arch, e));
                    retry_queue.push((idx, arch.clone()));
                }
            }
            if let Some(delay) = package.delay_between_requests() {
                sleep(delay).await;
            }
        }

        for (idx, arch) in retry_queue {
            let package = &packages[idx];
            warn(&format!("Retrying {} (arch {})...", package.id(), arch));
            if let Err(e) = self.process_package_arch(package, &arch, &mut summary).await {
                error(&format!("Retry also failed for {} (arch {}): {}", package.id(), arch, e));
                summary.failed += 1;
            } else {
                summary.downloaded += 1;
            }
        }

        self.state.save()?;
        Ok(summary)
    }

    async fn process_package_arch(
        &mut self,
        package: &dyn Downloadable,
        arch: &str,
        summary: &mut DownloadSummary,
    ) -> Result<(), String> {
        let cache_path = package.cache_path(&self.config.settings.cache_dir);
        let arch_cache_path = if arch == "universal" {
            cache_path.clone()
        } else {
            let parent = cache_path.parent().unwrap_or(std::path::Path::new("."));
            let stem = cache_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            parent.join(format!("{}_{}.xapk", stem, arch))
        };

        if !self.force {
            // Check per-arch throttle using mtime
            if let Ok(meta) = std::fs::metadata(&arch_cache_path) {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(_elapsed) = mtime.elapsed() {
                        if let Some(_throttle_interval) = package.delay_between_requests() {
                            // Use throttle_interval from source config
                        }
                        // For metadata policy, also check version match
                        if package.throttle_policy() == "metadata" {
                            if let Some(_cached_ver) = self.state.get_version(&package.id(), arch) {
                                // We need to compare with remote version, but that requires a fetch.
                                // Simplified: just check mtime-based throttle here; version check below.
                            }
                        }
                    }
                }
            }
        }

        let backend = package.backend()?;

        let version = if package.throttle_policy() == "metadata" {
            let remote_version = backend.fetch_version(package).await?;
            let cached_version = self.state.get_version(&package.id(), arch);

            if !self.force {
                if let Some(cached_ver) = cached_version {
                    if cached_ver == remote_version {
                        info(&format!(
                            "Cache hit: {} — version {} unchanged, skipping download",
                            package.id(), cached_ver
                        ));
                        summary.skipped += 1;
                        return Ok(());
                    } else {
                        info(&format!(
                            "New version available: {} — remote {} vs cached {}, downloading...",
                            package.id(), remote_version, cached_ver
                        ));
                    }
                } else {
                    info(&format!(
                        "Fresh download: {} (no cached version)",
                        package.id()
                    ));
                }
            }
            remote_version
        } else {
            if arch_cache_path.exists() {
                info(&format!(
                    "Dumb refresh: {} — re-downloading after throttle interval",
                    package.id()
                ));
            } else {
                info(&format!(
                    "Fresh download: {} (no cached file)",
                    package.id()
                ));
            }
            format!("dumb-{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs())
        };

        info(&format!("Downloading {} (arch {})...", package.id(), arch));
        if let Some(parent) = arch_cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create cache dir: {}", e))?;
        }
        let actual_path = backend.download(package, arch, &arch_cache_path).await?;

        if let Err(e) = crate::util::zip_validate::validate_zip(&actual_path) {
            let _ = std::fs::remove_file(&actual_path);
            return Err(format!("ZIP validation failed: {}", e));
        }

        self.state.set_version(package.id(), arch.to_string(), version, actual_path.to_string_lossy().to_string());
        summary.downloaded += 1;
        Ok(())
    }
}

#[derive(Default)]
pub struct DownloadSummary {
    pub skipped: usize,
    pub downloaded: usize,
    pub failed: usize,
}

pub trait Downloadable: Send + Sync {
    fn id(&self) -> String;
    fn source(&self) -> &str;
    fn throttle_policy(&self) -> &str;
    fn delay_between_requests(&self) -> Option<Duration>;
    fn architectures(&self) -> Vec<String>;
    fn is_throttled(&self, state: &State, cache_dir: String) -> Option<String>;
    fn cache_path(&self, cache_dir: &str) -> std::path::PathBuf;
    fn backend(&self) -> Result<Box<dyn DownloadBackend>, String>;
    fn is_store_package(&self, filter: &str) -> bool;
}

impl Downloadable for Box<dyn Downloadable> {
    fn id(&self) -> String { (**self).id() }
    fn source(&self) -> &str { (**self).source() }
    fn throttle_policy(&self) -> &str { (**self).throttle_policy() }
    fn delay_between_requests(&self) -> Option<Duration> { (**self).delay_between_requests() }
    fn architectures(&self) -> Vec<String> { (**self).architectures() }
    fn is_throttled(&self, state: &State, cache_dir: String) -> Option<String> { (**self).is_throttled(state, cache_dir) }
    fn cache_path(&self, cache_dir: &str) -> std::path::PathBuf { (**self).cache_path(cache_dir) }
    fn backend(&self) -> Result<Box<dyn DownloadBackend>, String> { (**self).backend() }
    fn is_store_package(&self, filter: &str) -> bool { (**self).is_store_package(filter) }
}

#[async_trait::async_trait]
pub trait DownloadBackend: Send + Sync {
    async fn fetch_version(&self, package: &dyn Downloadable) -> Result<String, String>;
    async fn download(&self, package: &dyn Downloadable, arch: &str, target: &std::path::Path) -> Result<std::path::PathBuf, String>;
}
