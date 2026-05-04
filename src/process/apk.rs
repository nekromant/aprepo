use std::fs;
use std::path::Path;
use std::process::Command;
use zip::ZipArchive;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Cursor, Read};

pub fn extract_manifest(apk_path: &Path) -> Result<ManifestInfo, String> {
    let file = fs::File::open(apk_path).map_err(|e| format!("Cannot open APK: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid APK: {}", e))?;

    let mut manifest_entry = archive
        .by_name("AndroidManifest.xml")
        .map_err(|e| format!("Missing AndroidManifest.xml: {}", e))?;
    let mut manifest_xml = Vec::new();
    manifest_entry
        .read_to_end(&mut manifest_xml)
        .map_err(|e| format!("Cannot read manifest: {}", e))?;

    // Try plain XML first (synthetic fixtures), fall back to aapt2 for binary AXML
    match parse_manifest(&manifest_xml) {
        Ok(info) => Ok(info),
        Err(_) => parse_with_aapt2(apk_path),
    }
}

#[derive(Debug)]
pub struct ManifestInfo {
    pub package: String,
    pub version_name: String,
}

fn parse_with_aapt2(apk_path: &Path) -> Result<ManifestInfo, String> {
    let output = Command::new("aapt2")
        .arg("dump")
        .arg("badging")
        .arg(apk_path)
        .output()
        .map_err(|e| format!("aapt2 not available: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "aapt2 failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let first_line = text.lines().next().unwrap_or("");

    let package = extract_attr(first_line, "name=")
        .ok_or_else(|| "aapt2 output missing name attribute".to_string())?;
    let version_name = extract_attr(first_line, "versionName=")
        .unwrap_or_default();

    Ok(ManifestInfo { package, version_name })
}

pub fn extract_attr(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let quote_char = line.chars().nth(start)?;
    if quote_char != '\'' {
        return None;
    }
    let end = line[start + 1..].find('\'')?;
    Some(line[start + 1..start + 1 + end].to_string())
}

fn parse_manifest(xml: &[u8]) -> Result<ManifestInfo, String> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if e.name().as_ref() == b"manifest" =>
            {
                let mut package = String::new();
                let mut version_name = String::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| format!("XML attribute error: {}", e))?;
                    let key = String::from_utf8_lossy(attr.key.as_ref());
                    let val = attr.unescape_value().map_err(|e| format!("XML value error: {}", e))?;
                    if key == "package" {
                        package = val.to_string();
                    }
                    if key.ends_with("versionName") || key == "android:versionName" {
                        version_name = val.to_string();
                    }
                }
                if package.is_empty() {
                    return Err("Missing package attribute in manifest".to_string());
                }
                return Ok(ManifestInfo { package, version_name });
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Err("manifest element not found in AndroidManifest.xml".to_string())
}
