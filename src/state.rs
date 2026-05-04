use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub versions: HashMap<String, VersionRecord>,
    pub capabilities: HashMap<String, SourceCapability>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SourceCapability {
    pub per_arch_downloads: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VersionRecord {
    /// Architecture → version string. Key "universal" for non-arch-specific downloads.
    pub versions: HashMap<String, String>,
    /// Architecture → relative path within cache_dir. Key "universal" for non-arch-specific.
    pub cached_files: HashMap<String, String>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut state = if !path.exists() {
            Self::default()
        } else {
            let content = fs::read_to_string(path)
                .map_err(|e| format!("Cannot read state.yaml: {}", e))?;
            if content.trim().is_empty() {
                Self::default()
            } else {
                serde_yaml::from_str(&content)
                    .map_err(|e| format!("Corrupt state.yaml: {}", e))?
            }
        };
        state.path = Some(path.to_path_buf());
        Ok(state)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = self.path.as_ref()
            .ok_or("State path not set")?;
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("Cannot serialize state: {}", e))?;
        fs::write(path, yaml)
            .map_err(|e| format!("Cannot write state.yaml: {}", e))?;
        Ok(())
    }

    /// Check if a specific architecture has the same version as provided.
    pub fn has_same_version(&self, id: &str, arch: &str, version: &str) -> bool {
        self.versions.get(id)
            .and_then(|r| r.versions.get(arch))
            .map(|v| v == version)
            .unwrap_or(false)
    }

    /// Get version for a specific architecture.
    pub fn get_version(&self, id: &str, arch: &str) -> Option<&str> {
        self.versions.get(id)
            .and_then(|r| r.versions.get(arch))
            .map(|v| v.as_str())
    }

    /// Get cached file path for a specific architecture.
    pub fn get_cached_file(&self, id: &str, arch: &str) -> Option<&str> {
        self.versions.get(id)
            .and_then(|r| r.cached_files.get(arch))
            .map(|v| v.as_str())
    }

    /// Set version and cached file for a specific architecture.
    pub fn set_version(&mut self, id: String, arch: String, version: String, cached_file: String) {
        let record = self.versions.entry(id).or_default();
        record.versions.insert(arch.clone(), version);
        record.cached_files.insert(arch, cached_file);
    }

    /// Get source capability, or None if not yet tested.
    pub fn get_capability(&self, source: &str) -> Option<&SourceCapability> {
        self.capabilities.get(source)
    }

    /// Set source capability.
    pub fn set_capability(&mut self, source: String, capability: SourceCapability) {
        self.capabilities.insert(source, capability);
    }

    /// Preserve old version record (no-op — records are kept until replaced or purged).
    pub fn preserve_old_version(&mut self, _id: &str) {
        // No-op: old versions remain in state until replaced or retention cleanup runs.
    }
}
