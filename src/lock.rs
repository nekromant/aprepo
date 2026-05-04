use fs2::FileExt;
use std::fs::OpenOptions;
use std::path::Path;

pub struct Lock {
    _file: std::fs::File,
}

impl Lock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("Cannot open state file for locking: {}", e))?;

        file.lock_exclusive()
            .map_err(|e| format!("Another instance is already running (lock held on {}): {}", path.display(), e))?;

        Ok(Self { _file: file })
    }
}

impl Drop for Lock {
    #[allow(clippy::incompatible_msrv)]
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}
