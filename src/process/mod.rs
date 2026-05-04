pub mod apk;
pub mod xapk;

use crate::config::Config;
use crate::state::State;
use crate::util::logging::{info, warn, error};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::collections::HashMap;

#[allow(dead_code)]
pub struct ProcessOrchestrator {
    config: Config,
    state: State,
    verbose: bool,
    force: bool,
    package_filter: Option<String>,
}

impl ProcessOrchestrator {
    pub fn new(config: Config, state: State, verbose: bool, force: bool, package_filter: Option<String>) -> Self {
        Self { config, state, verbose, force, package_filter }
    }

    pub fn run(&self) -> Result<ProcessSummary, String> {
        let mut summary = ProcessSummary::default();
        let cache_dir = Path::new(&self.config.settings.cache_dir);
        let output_dir = Path::new(&self.config.settings.output_dir);

        if !output_dir.exists() {
            fs::create_dir_all(output_dir).map_err(|e| format!("Cannot create output_dir: {}", e))?;
        }

        // Collect all cached files and group by their manifest package name
        let mut files_by_package: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for entry in walkdir::WalkDir::new(cache_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "apk" && ext != "xapk" {
                continue;
            }

            // Determine package name: extract from manifest for APKs, from XAPK manifest for XAPKs
            let package_name = if ext == "apk" {
                match apk::extract_manifest(path) {
                    Ok(manifest) => manifest.package,
                    Err(e) => {
                        warn(&format!("Cannot extract manifest from {}: {}", path.display(), e));
                        continue;
                    }
                }
            } else {
                // XAPK: use file stem as placeholder; actual name comes from XAPK manifest during repack
                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
            };

            // Apply --package filter
            if let Some(ref filter) = self.package_filter {
                if package_name != *filter {
                    continue;
                }
            }

            files_by_package.entry(package_name).or_default().push(path.to_path_buf());
        }

        for (_package_name, files) in files_by_package {
            for cache_path in files {
                let ext = cache_path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if ext == "xapk" && self.config.settings.repack_xapk {
                    let results = xapk::repack(&cache_path, output_dir, &self.config)?;
                    for result in results {
                        match result {
                            Ok(path) => {
                                info(&format!("Repacked: {}", path.display()));
                                summary.processed += 1;
                            }
                            Err(e) => {
                                warn(&format!("XAPK repack warning: {}", e));
                            }
                        }
                    }
                } else if ext == "apk" {
                    if let Err(e) = self.process_apk(&cache_path, output_dir, &mut summary) {
                        error(&format!("Failed to process {}: {}", cache_path.display(), e));
                        summary.errors += 1;
                    }
                } else {
                    // XAPK with repack_xapk=false: copy unchanged
                    if self.should_skip(&cache_path, output_dir) {
                        summary.skipped += 1;
                        continue;
                    }
                    let output_path = output_dir.join(cache_path.file_name().unwrap());
                    fs::copy(&cache_path, &output_path).map_err(|e| format!("Copy failed: {}", e))?;
                    summary.processed += 1;
                }
            }
        }

        self.apply_retention(cache_dir, output_dir)?;
        Ok(summary)
    }

    fn process_apk(&self,
        cache_path: &Path,
        output_dir: &Path,
        summary: &mut ProcessSummary,
    ) -> Result<(), String> {
        let manifest = apk::extract_manifest(cache_path)?;
        let arch = extract_arch_from_filename(cache_path);
        let output_name = format!(
            "{}_{}_{}.apk",
            manifest.package,
            manifest.version_name,
            arch
        );
        let output_path = output_dir.join(&output_name);

        if !self.force && output_path.exists() {
            let cache_mtime = fs::metadata(cache_path).ok().and_then(|m| m.modified().ok());
            let output_mtime = fs::metadata(&output_path).ok().and_then(|m| m.modified().ok());
            if let (Some(c), Some(o)) = (cache_mtime, output_mtime) {
                if o >= c {
                    summary.skipped += 1;
                    return Ok(());
                }
            }
        }

        fs::copy(cache_path, &output_path).map_err(|e| format!("Copy failed: {}", e))?;
        summary.processed += 1;
        Ok(())
    }

    fn should_skip(&self, cache_path: &Path, output_dir: &Path) -> bool {
        if self.force {
            return false;
        }
        let output_path = output_dir.join(cache_path.file_name().unwrap());
        if !output_path.exists() {
            return false;
        }
        let cache_mtime = fs::metadata(cache_path).ok().and_then(|m| m.modified().ok());
        let output_mtime = fs::metadata(&output_path).ok().and_then(|m| m.modified().ok());
        match (cache_mtime, output_mtime) {
            (Some(c), Some(o)) => o >= c,
            _ => false,
        }
    }

    fn apply_retention(&self, cache_dir: &Path, output_dir: &Path) -> Result<(), String> {
        let retention = self.config.settings.retention_depth as usize;
        let mut package_files: HashMap<String, Vec<(PathBuf, SystemTime)>> = HashMap::new();

        for entry in walkdir::WalkDir::new(cache_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // Use manifest package name for grouping, or file stem as fallback
            let name = if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                match apk::extract_manifest(path) {
                    Ok(manifest) => manifest.package,
                    Err(_) => path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                }
            } else {
                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
            };
            let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
            if let Some(t) = mtime {
                package_files.entry(name).or_default().push((path.to_path_buf(), t));
            }
        }

        for (_name, mut files) in package_files {
            files.sort_by_key(|b| std::cmp::Reverse(b.1));
            if files.len() > retention + 1 {
                for (path, _) in files.into_iter().skip(retention + 1) {
                    let _ = fs::remove_file(&path);
                    let output_path = output_dir.join(path.file_name().unwrap());
                    let _ = fs::remove_file(&output_path);
                }
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct ProcessSummary {
    pub skipped: usize,
    pub processed: usize,
    pub errors: usize,
}

/// Extract architecture suffix from a cached filename like "com.app_arm64-v8a.xapk".
/// Returns "universal" if no arch suffix is found.
fn extract_arch_from_filename(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let known_archs = ["arm64-v8a", "armeabi-v7a", "armeabi", "x86_64", "x86"];
    for arch in &known_archs {
        if stem.ends_with(arch) {
            return arch.to_string();
        }
    }
    "universal".to_string()
}
