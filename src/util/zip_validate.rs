use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use std::io::Read;

pub fn validate_zip(path: &Path) -> Result<(), String> {
    // Quick magic-bytes check to detect HTML error pages / truncated files
    {
        let mut file = File::open(path).map_err(|e| format!("Cannot open ZIP: {}", e))?;
        let mut magic = [0u8; 4];
        let n = file.read(&mut magic).map_err(|e| format!("Cannot read ZIP magic: {}", e))?;
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if n < 4 || (magic[0] != 0x50 || magic[1] != 0x4B) {
            let preview = String::from_utf8_lossy(&magic[..n]);
            return Err(format!(
                "Downloaded file is not a valid ZIP archive (size: {} bytes, magic: {:02x?}, preview: {:?}). \
                 The server may have returned an error page instead of the APK/XAPK.",
                size, &magic[..n], preview
            ));
        }
    }

    let file = File::open(path).map_err(|e| format!("Cannot open ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid ZIP: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Bad entry {}: {}", i, e))?;
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf).map_err(|e| format!("Read error entry {}: {}", i, e))?;
    }

    Ok(())
}
