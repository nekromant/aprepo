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
        let arch_cache_path = compute_arch_cache_path(&cache_path, arch);

        let backend = package.backend()?;

        let version = if package.throttle_policy() == "metadata" {
            // Mtime guard: avoid API calls when within throttle interval
            if !self.force {
                if let Ok(meta) = std::fs::metadata(&arch_cache_path) {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(elapsed) = mtime.elapsed() {
                            if elapsed < package.throttle_interval() {
                                if let Some(cached_ver) = self.state.get_version(&package.id(), arch) {
                                    info(&format!(
                                        "Cache hit: {} — version {} (mtime throttle, {}s remaining)",
                                        package.id(), cached_ver,
                                        package.throttle_interval().as_secs() - elapsed.as_secs()
                                    ));
                                    summary.skipped += 1;
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }

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
            // Dumb policy: skip if file exists and mtime is within throttle interval
            if !self.force && arch_cache_path.exists() {
                if let Ok(meta) = std::fs::metadata(&arch_cache_path) {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(elapsed) = mtime.elapsed() {
                            if elapsed < package.throttle_interval() {
                                info(&format!(
                                    "Cache hit: {} — dumb throttle ({}s remaining), skipping download",
                                    package.id(),
                                    package.throttle_interval().as_secs() - elapsed.as_secs()
                                ));
                                summary.skipped += 1;
                                return Ok(());
                            }
                        }
                    }
                }
                info(&format!(
                    "Dumb refresh: {} — re-downloading after throttle interval",
                    package.id()
                ));
            } else if !arch_cache_path.exists() {
                info(&format!(
                    "Fresh download: {} (no cached file)",
                    package.id()
                ));
            } else {
                info(&format!(
                    "Dumb refresh: {} — force flag active, re-downloading",
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
    fn throttle_interval(&self) -> Duration;
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
    fn throttle_interval(&self) -> Duration { (**self).throttle_interval() }
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

/// Compute the per-architecture cache path from a base cache path.
/// This is extracted for testability.
pub fn compute_arch_cache_path(cache_path: &std::path::Path, arch: &str) -> std::path::PathBuf {
    if arch == "universal" {
        cache_path.to_path_buf()
    } else {
        let parent = cache_path.parent().unwrap_or(std::path::Path::new("."));
        let known_exts = ["apk", "xapk"];
        let ext = cache_path.extension().and_then(|s| s.to_str());
        if ext.is_some_and(|e| known_exts.contains(&e)) {
            // Real APK/XAPK extension (e.g. webdl/test.apk) -> insert arch before it
            let base = cache_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            parent.join(format!("{}_{}.{}", base, arch, ext.unwrap()))
        } else {
            // No real extension (store packages like com.shazam.android) -> append _arch.xapk
            let name = cache_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            parent.join(format!("{}_{}.xapk", name, arch))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Settings};
    use std::collections::HashMap;
    use std::io::Write;
    use std::time::Duration;

    struct MockPkg {
        id: String,
        policy: String,
        interval: Duration,
        arch: String,
        cache_dir: String,
    }

    impl Downloadable for MockPkg {
        fn id(&self) -> String { self.id.clone() }
        fn source(&self) -> &str { "mock" }
        fn throttle_policy(&self) -> &str { &self.policy }
        fn throttle_interval(&self) -> Duration { self.interval }
        fn delay_between_requests(&self) -> Option<Duration> { None }
        fn architectures(&self) -> Vec<String> { vec![self.arch.clone()] }
        fn is_throttled(&self, _state: &State, _cache_dir: String) -> Option<String> { None }
        fn cache_path(&self, _cache_dir: &str) -> std::path::PathBuf {
            std::path::Path::new(&self.cache_dir).join(&self.id)
        }
        fn backend(&self) -> Result<Box<dyn DownloadBackend>, String> {
            Ok(Box::new(MockBackend))
        }
        fn is_store_package(&self, _filter: &str) -> bool { false }
    }

    struct MockBackend;

    #[async_trait::async_trait]
    impl DownloadBackend for MockBackend {
        async fn fetch_version(&self,
            _package: &dyn Downloadable,
        ) -> Result<String, String> {
            Ok("1.0.0".to_string())
        }
        async fn download(
            &self,
            _package: &dyn Downloadable,
            _arch: &str,
            target: &std::path::Path,
        ) -> Result<std::path::PathBuf, String> {
            let file = std::fs::File::create(target)
                .map_err(|e| format!("Cannot create file: {}", e))?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("test.txt", options)
                .map_err(|e| format!("ZIP start_file failed: {}", e))?;
            zip.write_all(b"hello")
                .map_err(|e| format!("ZIP write failed: {}", e))?;
            zip.finish()
                .map_err(|e| format!("ZIP finish failed: {}", e))?;
            Ok(target.to_path_buf())
        }
    }

    fn make_orchestrator(cache_dir: &std::path::Path) -> DownloadOrchestrator {
        let config = Config {
            settings: Settings {
                cache_dir: cache_dir.to_string_lossy().to_string(),
                output_dir: cache_dir.join("out").to_string_lossy().to_string(),
                retention_depth: 1,
                architectures: vec!["universal".to_string()],
                density: "xxhdpi".to_string(),
                repack_xapk: false,
                sign: None,
            },
            sources: HashMap::new(),
        };
        let state = State::load(&cache_dir.join("state.yaml")).unwrap();
        DownloadOrchestrator::new(config, state, false, false, None)
    }

    #[tokio::test]
    async fn test_dumb_throttle_skips_recent_file() {
        let tmp = std::env::temp_dir().join("aprepo_test_dumb_skip");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let cache_path = compute_arch_cache_path(&tmp.join("mock_pkg"), "universal");
        std::fs::write(&cache_path, "cached").unwrap();

        let pkg = MockPkg {
            id: "mock_pkg".to_string(),
            policy: "dumb".to_string(),
            interval: Duration::from_secs(3600),
            arch: "universal".to_string(),
            cache_dir: tmp.to_string_lossy().to_string(),
        };

        let mut orch = make_orchestrator(&tmp);
        let mut summary = DownloadSummary::default();
        let result = orch.process_package_arch(&pkg, "universal", &mut summary).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(summary.skipped, 1, "Expected skip due to dumb throttle");
        assert_eq!(summary.downloaded, 0);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_dumb_throttle_downloads_when_expired() {
        let tmp = std::env::temp_dir().join("aprepo_test_dumb_expired");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let cache_path = compute_arch_cache_path(&tmp.join("mock_pkg"), "universal");
        std::fs::write(&cache_path, "cached").unwrap();

        // Sleep so the file mtime exceeds a 0-second throttle interval
        std::thread::sleep(Duration::from_millis(50));

        let pkg = MockPkg {
            id: "mock_pkg".to_string(),
            policy: "dumb".to_string(),
            interval: Duration::from_secs(0),
            arch: "universal".to_string(),
            cache_dir: tmp.to_string_lossy().to_string(),
        };

        let mut orch = make_orchestrator(&tmp);
        let mut summary = DownloadSummary::default();
        let result = orch.process_package_arch(&pkg, "universal", &mut summary).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(summary.skipped, 0, "Should not skip when throttle expired");
        assert_eq!(summary.downloaded, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_metadata_throttle_skips_recent_file_with_cached_version() {
        let tmp = std::env::temp_dir().join("aprepo_test_meta_skip");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let cache_path = compute_arch_cache_path(&tmp.join("mock_pkg"), "universal");
        std::fs::write(&cache_path, "cached").unwrap();

        let pkg = MockPkg {
            id: "mock_pkg".to_string(),
            policy: "metadata".to_string(),
            interval: Duration::from_secs(3600),
            arch: "universal".to_string(),
            cache_dir: tmp.to_string_lossy().to_string(),
        };

        let mut orch = make_orchestrator(&tmp);
        // Pre-populate state with a cached version so the mtime guard triggers
        orch.state.set_version(
            "mock_pkg".to_string(),
            "universal".to_string(),
            "1.0.0".to_string(),
            cache_path.to_string_lossy().to_string(),
        );

        let mut summary = DownloadSummary::default();
        let result = orch.process_package_arch(&pkg, "universal", &mut summary).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(summary.skipped, 1, "Expected skip due to metadata mtime throttle");
        assert_eq!(summary.downloaded, 0);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
