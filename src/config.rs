use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Find the actual cached file for a given stem path (no extension).
/// Scans the parent directory for files matching stem.{apk,xapk}.
fn find_cached_file(stem: &Path) -> Option<PathBuf> {
    let parent = stem.parent()?;
    let name = stem.file_name()?.to_string_lossy();
    let entries = std::fs::read_dir(parent).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with(&*name) && (fname.ends_with(".apk") || fname.ends_with(".xapk")) {
            return Some(entry.path());
        }
    }
    None
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub settings: Settings,
    pub sources: HashMap<String, Source>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub cache_dir: String,
    pub output_dir: String,
    #[serde(default = "default_retention_depth")]
    pub retention_depth: u32,
    #[serde(default = "default_architectures")]
    pub architectures: Vec<String>,
    #[serde(default = "default_density")]
    pub density: String,
    #[serde(default = "default_repack_xapk")]
    pub repack_xapk: bool,
    pub sign: Option<SignSettings>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub keystore_file: String,
    #[serde(default)]
    pub keystore_password: String,
    #[serde(default)]
    pub key_alias: String,
    #[serde(default)]
    pub key_password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Source {
    #[serde(default = "default_throttle_policy")]
    pub throttle_policy: String,
    #[serde(default = "default_throttle_interval", deserialize_with = "deserialize_duration")]
    pub throttle_interval: Duration,
    #[serde(default = "default_delay_between_requests", deserialize_with = "deserialize_duration")]
    pub delay_between_requests: Duration,
    pub token: Option<String>,
    pub user: Option<String>,
    pub packages: Vec<serde_yaml::Value>,
}

fn default_retention_depth() -> u32 { 1 }
fn default_architectures() -> Vec<String> { vec!["arm64-v8a".to_string()] }
fn default_density() -> String { "xxhdpi".to_string() }
fn default_repack_xapk() -> bool { false }
fn default_throttle_policy() -> String { "metadata".to_string() }
fn default_throttle_interval() -> Duration { Duration::from_secs(24 * 3600) }
fn default_delay_between_requests() -> Duration { Duration::from_secs(2) }

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    parse_duration(&s).map_err(serde::de::Error::custom)
}

pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let re = Regex::new(r"^(\d+)([hdms])$").unwrap();
    let caps = re.captures(s).ok_or_else(|| format!("Invalid duration format: {}", s))?;
    let num: u64 = caps[1].parse().map_err(|_| format!("Invalid duration number: {}", &caps[1]))?;
    let unit = &caps[2];
    let secs = match unit {
        "h" => num * 3600,
        "d" => num * 86400,
        "m" => num * 60,
        "s" => num,
        _ => return Err(format!("Unknown duration unit: {}", unit)),
    };
    Ok(Duration::from_secs(secs))
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("Configuration file not found: {}", path.display()));
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Cannot read config: {}", e))?;
        let interpolated = interpolate_env_vars(&raw)?;
        let config: Config = serde_yaml::from_str(&interpolated)
            .map_err(|e| format!("Invalid YAML config: {}", e))?;

        config.validate()?;
        config.ensure_dirs()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.settings.cache_dir.is_empty() {
            return Err("settings.cache_dir is required".to_string());
        }
        if self.settings.output_dir.is_empty() {
            return Err("settings.output_dir is required".to_string());
        }

        let mut seen_packages: HashSet<String> = HashSet::new();
        for (source_name, source) in &self.sources {
            if source_name == "google_play" && source.throttle_policy != "dumb" {
                return Err(format!(
                    "PlayStore source '{}' must use throttle_policy: dumb (found: {})",
                    source_name, source.throttle_policy
                ));
            }

            for pkg in &source.packages {
                if let Some(name) = pkg.as_str() {
                    if !seen_packages.insert(name.to_string()) {
                        return Err(format!("Duplicate package '{}' found across store sources", name));
                    }
                }
            }
        }

        if let Some(ref sign) = self.settings.sign {
            if sign.enabled
                && (sign.keystore_file.is_empty() || sign.keystore_password.is_empty()
                    || sign.key_alias.is_empty() || sign.key_password.is_empty())
            {
                eprintln!("WARNING: sign.enabled is true but some signing fields are missing");
            }
        }

        if !self.settings.repack_xapk {
            if let Some(ref sign) = self.settings.sign {
                if sign.enabled {
                    eprintln!("WARNING: sign.enabled is true but repack_xapk is false (signing has no effect)");
                }
            }
        }

        Ok(())
    }

    fn ensure_dirs(&self) -> Result<(), String> {
        let cache = Path::new(&self.settings.cache_dir);
        if !cache.exists() {
            fs::create_dir_all(cache).map_err(|e| format!("Cannot create cache_dir: {}", e))?;
            eprintln!("WARNING: Created cache directory: {}", cache.display());
        }
        let output = Path::new(&self.settings.output_dir);
        if !output.exists() {
            fs::create_dir_all(output).map_err(|e| format!("Cannot create output_dir: {}", e))?;
            eprintln!("WARNING: Created output directory: {}", output.display());
        }
        Ok(())
    }

    pub fn all_packages(&self) -> Vec<Box<dyn crate::download::Downloadable>> {
        let mut result = Vec::new();
        for (source_name, source) in &self.sources {
            for pkg in &source.packages {
                if let Some(name) = pkg.as_str() {
                    result.push(Box::new(StorePackage {
                        source: source_name.clone(),
                        name: name.to_string(),
                        throttle_policy: source.throttle_policy.clone(),
                        throttle_interval: source.throttle_interval,
                        delay_between_requests: source.delay_between_requests,
                        cache_dir: self.settings.cache_dir.clone(),
                        token: source.token.clone(),
                        user: source.user.clone(),
                        architectures: self.settings.architectures.clone(),
                    }) as Box<dyn crate::download::Downloadable>);
                } else if let Some(map) = pkg.as_mapping() {
                    if let (Some(repo), Some(mask)) = (
                        map.get("repo").and_then(|v| v.as_str()),
                        map.get("mask").and_then(|v| v.as_str()),
                    ) {
                        let arch_masks = map.get("arch_masks")
                            .and_then(|v| v.as_mapping())
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k, v)| {
                                        Some((k.as_str()?.to_string(), v.as_str()?.to_string()))
                                    })
                                    .collect()
                            });
                        result.push(Box::new(GitHubPackage {
                            source: source_name.clone(),
                            repo: repo.to_string(),
                            mask: mask.to_string(),
                            arch_masks,
                            throttle_policy: source.throttle_policy.clone(),
                            throttle_interval: source.throttle_interval,
                            delay_between_requests: source.delay_between_requests,
                            cache_dir: self.settings.cache_dir.clone(),
                            token: source.token.clone(),
                            architectures: self.settings.architectures.clone(),
                        }) as Box<dyn crate::download::Downloadable>);
                    } else if let (Some(filename), Some(url)) = (
                        map.get("filename").and_then(|v| v.as_str()),
                        map.get("url").and_then(|v| v.as_str()),
                    ) {
                        let arch_files = map.get("arch_files")
                            .and_then(|v| v.as_mapping())
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k, v)| {
                                        let arch = k.as_str()?.to_string();
                                        let inner = v.as_mapping()?;
                                        let af_filename = inner.get("filename")?.as_str()?.to_string();
                                        let af_url = inner.get("url")?.as_str()?.to_string();
                                        Some((arch, ArchFile { filename: af_filename, url: af_url }))
                                    })
                                    .collect()
                            });
                        result.push(Box::new(WebDlPackage {
                            source: source_name.clone(),
                            filename: filename.to_string(),
                            url: url.to_string(),
                            arch_files,
                            throttle_policy: source.throttle_policy.clone(),
                            throttle_interval: source.throttle_interval,
                            delay_between_requests: source.delay_between_requests,
                            cache_dir: self.settings.cache_dir.clone(),
                            architectures: self.settings.architectures.clone(),
                        }) as Box<dyn crate::download::Downloadable>);
                    }
                }
            }
        }
        result
    }
}

fn interpolate_env_vars(content: &str) -> Result<String, String> {
    let re = Regex::new(r"\$\$|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for cap in re.captures_iter(content) {
        let m = cap.get(0).unwrap();
        result.push_str(&content[last_end..m.start()]);

        if &cap[0] == "$$" {
            result.push('$');
        } else {
            let var_name = &cap[1];
            let value = std::env::var(var_name)
                .map_err(|_| format!("Environment variable not set: {}", var_name))?;
            result.push_str(&value);
        }
        last_end = m.end();
    }
    result.push_str(&content[last_end..]);
    Ok(result)
}

#[derive(Clone)]
#[allow(dead_code)]
struct StorePackage {
    source: String,
    name: String,
    throttle_policy: String,
    throttle_interval: Duration,
    delay_between_requests: Duration,
    cache_dir: String,
    token: Option<String>,
    user: Option<String>,
    architectures: Vec<String>,
}

impl crate::download::Downloadable for StorePackage {
    fn id(&self) -> String {
        format!("{}:{}", self.source, self.name)
    }
    fn source(&self) -> &str {
        &self.source
    }
    fn throttle_policy(&self) -> &str {
        &self.throttle_policy
    }
    fn delay_between_requests(&self) -> Option<Duration> {
        Some(self.delay_between_requests)
    }
    fn architectures(&self) -> Vec<String> {
        self.architectures.clone()
    }
    fn is_throttled(&self, state: &crate::state::State, cache_dir: String) -> Option<String> {
        if self.throttle_policy == "dumb" {
            let cache_path = self.cache_path(&cache_dir);
            if let Some(actual) = find_cached_file(&cache_path) {
                if let Ok(meta) = std::fs::metadata(&actual) {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(elapsed) = mtime.elapsed() {
                            if elapsed < self.throttle_interval {
                                return Some(format!("Dumb throttle: {} remaining", self.throttle_interval.as_secs() - elapsed.as_secs()));
                            }
                        }
                    }
                }
            }
        } else if self.throttle_policy == "metadata"
            && state.has_same_version(&self.id(), "universal", &self.id())
        {
            let cache_path = self.cache_path(&cache_dir);
            if let Some(actual) = find_cached_file(&cache_path) {
                if let Ok(meta) = std::fs::metadata(&actual) {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(elapsed) = mtime.elapsed() {
                            if elapsed < self.throttle_interval {
                                return Some(format!("Metadata throttle: {} remaining", self.throttle_interval.as_secs() - elapsed.as_secs()));
                            }
                        }
                    }
                }
            }
        }
        None
    }
    fn cache_path(&self, cache_dir: &str) -> std::path::PathBuf {
        // Return stem without extension; backend will append actual extension
        std::path::Path::new(cache_dir).join(&self.source).join(&self.name)
    }
    fn backend(&self) -> Result<Box<dyn crate::download::DownloadBackend>, String> {
        match self.source.as_str() {
            "google_play" | "rustore" | "apkpure" => {
                Ok(Box::new(crate::download::apkeep::ApkeepBackend {
                    token: self.token.clone(),
                    user: self.user.clone(),
                    source: self.source.clone(),
                }))
            }
            _ => Err(format!("Unsupported source: {}", self.source)),
        }
    }
    fn is_store_package(&self, filter: &str) -> bool {
        self.name == filter
    }
}

#[derive(Clone)]
struct GitHubPackage {
    source: String,
    repo: String,
    mask: String,
    arch_masks: Option<HashMap<String, String>>,
    throttle_policy: String,
    throttle_interval: Duration,
    delay_between_requests: Duration,
    cache_dir: String,
    token: Option<String>,
    architectures: Vec<String>,
}

impl crate::download::Downloadable for GitHubPackage {
    fn id(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.repo);
        hasher.update(&self.mask);
        format!("github:{}", &hex::encode(hasher.finalize())[..16])
    }
    fn source(&self) -> &str {
        &self.source
    }
    fn throttle_policy(&self) -> &str {
        &self.throttle_policy
    }
    fn delay_between_requests(&self) -> Option<Duration> {
        Some(self.delay_between_requests)
    }
    fn architectures(&self) -> Vec<String> {
        match &self.arch_masks {
            Some(masks) => {
                self.architectures.iter()
                    .filter(|a| masks.contains_key(*a))
                    .cloned()
                    .collect()
            }
            None => vec!["universal".to_string()],
        }
    }
    fn is_throttled(&self, _state: &crate::state::State, _cache_dir: String) -> Option<String> {
        if self.throttle_policy == "metadata" || self.throttle_policy == "dumb" {
            let cache_path = self.cache_path(&self.cache_dir);
            if let Some(actual) = find_cached_file(&cache_path) {
                if let Ok(meta) = std::fs::metadata(&actual) {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(elapsed) = mtime.elapsed() {
                            if elapsed < self.throttle_interval {
                                return Some(format!("Throttle: {} remaining", self.throttle_interval.as_secs() - elapsed.as_secs()));
                            }
                        }
                    }
                }
            }
        }
        None
    }
    fn cache_path(&self, cache_dir: &str) -> std::path::PathBuf {
        std::path::Path::new(cache_dir).join(&self.source).join(self.repo.replace('/', "_"))
    }
    fn backend(&self) -> Result<Box<dyn crate::download::DownloadBackend>, String> {
        Ok(Box::new(crate::download::github::GitHubBackend {
            repo: self.repo.clone(),
            mask: self.mask.clone(),
            arch_masks: self.arch_masks.clone(),
            token: self.token.clone(),
        }))
    }
    fn is_store_package(&self, _filter: &str) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub struct ArchFile {
    pub filename: String,
    pub url: String,
}

#[derive(Clone)]
struct WebDlPackage {
    source: String,
    filename: String,
    url: String,
    arch_files: Option<HashMap<String, ArchFile>>,
    throttle_policy: String,
    throttle_interval: Duration,
    delay_between_requests: Duration,
    cache_dir: String,
    architectures: Vec<String>,
}

impl crate::download::Downloadable for WebDlPackage {
    fn id(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        if let Some(arch_files) = &self.arch_files {
            let mut keys: Vec<&String> = arch_files.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(af) = arch_files.get(key) {
                    hasher.update(key.as_bytes());
                    hasher.update(af.url.as_bytes());
                }
            }
        } else {
            hasher.update(&self.url);
        }
        format!("webdl:{}", &hex::encode(hasher.finalize())[..16])
    }
    fn source(&self) -> &str {
        &self.source
    }
    fn throttle_policy(&self) -> &str {
        &self.throttle_policy
    }
    fn delay_between_requests(&self) -> Option<Duration> {
        Some(self.delay_between_requests)
    }
    fn architectures(&self) -> Vec<String> {
        match &self.arch_files {
            Some(files) => {
                self.architectures.iter()
                    .filter(|a| files.contains_key(*a))
                    .cloned()
                    .collect()
            }
            None => vec!["universal".to_string()],
        }
    }
    fn is_throttled(&self, _state: &crate::state::State, _cache_dir: String) -> Option<String> {
        let cache_path = self.cache_path(&self.cache_dir);
        if let Ok(meta) = std::fs::metadata(&cache_path) {
            if let Ok(mtime) = meta.modified() {
                if let Ok(elapsed) = mtime.elapsed() {
                    if elapsed < self.throttle_interval {
                        return Some(format!("Throttle: {} remaining", self.throttle_interval.as_secs() - elapsed.as_secs()));
                    }
                }
            }
        }
        None
    }
    fn cache_path(&self, cache_dir: &str) -> std::path::PathBuf {
        std::path::Path::new(cache_dir).join(&self.source).join(&self.filename)
    }
    fn backend(&self) -> Result<Box<dyn crate::download::DownloadBackend>, String> {
        Ok(Box::new(crate::download::webdl::WebDlBackend {
            url: self.url.clone(),
            arch_files: self.arch_files.clone(),
        }))
    }
    fn is_store_package(&self, _filter: &str) -> bool {
        false
    }
}
