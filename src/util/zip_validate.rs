use std::path::Path;
use zip::ZipArchive;
use std::fs::File;

pub fn validate_zip(path: &Path) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("Cannot open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Bad entry {}: {}", i, e))?;
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf).map_err(|e| format!("Read error entry {}: {}", i, e))?;
    }

    Ok(())
}
