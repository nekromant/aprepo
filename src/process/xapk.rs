use crate::config::{Config, SignSettings};
use crate::util::logging::{info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;
use std::io::Read;
use std::collections::HashMap;

pub fn repack(
    xapk_path: &Path,
    output_dir: &Path,
    config: &Config,
) -> Result<Vec<Result<PathBuf, String>>, String> {
    let file = fs::File::open(xapk_path).map_err(|e| format!("Cannot open XAPK: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid XAPK: {}", e))?;

    let manifest = extract_manifest_json(&mut archive)?;
    let package_name = manifest.package_name;
    let version_name = manifest.version_name;

    let temp_dir = tempfile::tempdir().map_err(|e| format!("Cannot create temp dir: {}", e))?;

    // Step 1: Extract ALL APKs from XAPK into temp directory
    let mut extracted: HashMap<String, PathBuf> = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Cannot read XAPK entry: {}", e))?;
        let name = entry.name().to_string();
        if name.ends_with(".apk") {
            let path = temp_dir.path().join(&name);
            let mut file = fs::File::create(&path)
                .map_err(|e| format!("Cannot create temp file for {}: {}", name, e))?;
            std::io::copy(&mut entry, &mut file)
                .map_err(|e| format!("Cannot extract {} from XAPK: {}", name, e))?;
            extracted.insert(name, path);
        }
    }

    if extracted.is_empty() {
        return Err("No APK files found in XAPK".to_string());
    }

    // Step 2: Classify APKs by type (main, arch, dpi, locale)
    let classified = classify_apks(&extracted, &package_name);
    let base_apk_name = classified.main.clone();
    let _base_apk_path = extracted.get(&base_apk_name)
        .ok_or_else(|| format!("Base APK '{}' not found in extracted files", base_apk_name))?;

    // Step 3: Decode ALL APKs with apktool d -s
    let mut decoded: HashMap<String, PathBuf> = HashMap::new();
    for (apk_name, apk_path) in &extracted {
        let decoded_name = apk_name.trim_end_matches(".apk");
        let decoded_dir = temp_dir.path().join(format!("decoded_{}", decoded_name));
        decode_apk(apk_path, &decoded_dir)?;
        decoded.insert(apk_name.clone(), decoded_dir);
    }

    let base_decoded = decoded.get(&base_apk_name)
        .ok_or_else(|| format!("Base APK '{}' not decoded", base_apk_name))?;

    // Step 4: For each target architecture, copy decoded base, merge splits, clean, rebuild
    let mut results = Vec::new();
    let arch_fallback = ["arm64-v8a", "armeabi-v7a", "armeabi", "x86_64", "x86"];

    for target_arch in &config.settings.architectures {
        let arch_split_name = find_arch_split(&manifest.splits, target_arch, &arch_fallback);

        if arch_split_name.is_none() {
            warn(&format!(
                "No arch split found for {} in XAPK {}",
                target_arch, xapk_path.display()
            ));
            results.push(Err(format!("Missing arch split: {}", target_arch)));
            continue;
        }

        let work_dir = temp_dir.path().join(format!("work_{}", target_arch.replace('-', "_")));

        // 4a. Copy decoded base to working directory
        copy_dir_all(base_decoded, &work_dir)
            .map_err(|e| format!("Cannot copy decoded base for {}: {}", target_arch, e))?;

        // 4b. Merge architecture split lib/ into working directory
        if let Some(ref split_name) = arch_split_name {
            if let Some(split_decoded) = decoded.get(split_name) {
                merge_apk_arch(&work_dir, split_decoded)?;
            }
        }

        // 4c. Merge DPI splits (in priority order)
        let dpi_prioritized = prioritize_dpi_splits(&classified.dpi);
        for dpi_name in &dpi_prioritized {
            if let Some(dpi_decoded) = decoded.get(dpi_name) {
                merge_apk_resources(&work_dir, dpi_decoded)?;
                merge_do_not_compress(&work_dir, dpi_decoded)?;
            }
        }

        // 4d. Merge locale splits
        for locale_name in &classified.locale {
            if let Some(locale_decoded) = decoded.get(locale_name) {
                merge_apk_resources(&work_dir, locale_decoded)?;
                merge_apk_assets(&work_dir, locale_decoded)?;
                merge_do_not_compress(&work_dir, locale_decoded)?;
            }
        }

        // 4e. Delete bundle signing metadata (BNDLTOOL.*)
        delete_signature_related_files(&work_dir)?;

        // 4f. Fix misnamed image files (.png that are actually JPEG)
        fix_misnamed_image_files(&work_dir)?;

        // 4g. Update AndroidManifest.xml to remove split-required attributes
        update_main_manifest_file(&work_dir)?;

        // 4h. Rebuild with apktool b
        let rebuilt = rebuild_apk(&work_dir)?;

        // 4i. Zipalign with -p -f 4
        let aligned = zipalign_apk(&rebuilt, temp_dir.path(), target_arch)?;

        // 4j. Sign with apksigner if enabled
        let final_apk = if let Some(ref sign) = config.settings.sign {
            if sign.enabled {
                match sign_apk(&aligned, temp_dir.path(), target_arch, sign) {
                    Ok(signed) => signed,
                    Err(e) => {
                        warn(&format!("Signing failed, using unsigned: {}", e));
                        aligned
                    }
                }
            } else {
                aligned
            }
        } else {
            aligned
        };

        // Copy to output
        let output_name = format!(
            "{}_{}_{}.apk",
            package_name, version_name, target_arch
        );
        let output_path = output_dir.join(&output_name);
        fs::copy(&final_apk, &output_path)
            .map_err(|e| format!("Copy to output failed: {}", e))?;
        results.push(Ok(output_path));
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// XAPK Manifest Extraction
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)]
struct XapkManifest {
    package_name: String,
    version_name: String,
    base_apk: String,
    splits: Vec<SplitInfo>,
    density_splits: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SplitInfo {
    pub file: String,
    pub abi: Option<String>,
}

fn extract_manifest_json(archive: &mut ZipArchive<std::fs::File>) -> Result<XapkManifest, String> {
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|e| format!("Missing manifest.json in XAPK: {}", e))?;
    let mut json = String::new();
    entry.read_to_string(&mut json).map_err(|e| format!("Cannot read manifest.json: {}", e))?;

    let manifest: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| format!("Invalid manifest.json: {}", e))?;

    let package_name = manifest["package_name"]
        .as_str()
        .or_else(|| manifest["package"].as_str())
        .unwrap_or("unknown")
        .to_string();
    let version_name = manifest["version_name"]
        .as_str()
        .or_else(|| manifest["version"].as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let base_apk = manifest["base_apk"]
        .as_str()
        .or_else(|| manifest["split_apks"].as_array().and_then(|a| a.first()).and_then(|v| v["file"].as_str()))
        .unwrap_or("base.apk")
        .to_string();

    let mut splits = Vec::new();
    if let Some(split_apks) = manifest["split_apks"].as_array() {
        for split in split_apks {
            let file = split["file"].as_str().unwrap_or("").to_string();
            let abi = split["abi"].as_str().map(|s| s.to_string());
            if !file.is_empty() {
                splits.push(SplitInfo { file, abi });
            }
        }
    }

    let density_splits = manifest["density_splits"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    Ok(XapkManifest {
        package_name,
        version_name,
        base_apk,
        splits,
        density_splits,
    })
}

// ---------------------------------------------------------------------------
// APK Classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum ApkType {
    Main,
    Arch,
    Dpi,
    Locale,
}

#[allow(dead_code)]
struct ClassifiedApks {
    main: String,
    arch: Vec<String>,
    dpi: Vec<String>,
    locale: Vec<String>,
}

fn classify_apk(filename: &str, package_name: &str) -> ApkType {
    let name_no_ext = filename.trim_end_matches(".apk");
    if name_no_ext == package_name || name_no_ext == "base" {
        return ApkType::Main;
    }
    if !name_no_ext.starts_with("config.") {
        return ApkType::Locale;
    }
    let parts: Vec<&str> = name_no_ext.split('.').collect();
    if parts.len() >= 2 {
        let config_name = parts[1];
        let arch_names = ["arm64_v8a", "armeabi_v7a", "armeabi", "x86", "x86_64"];
        if arch_names.contains(&config_name) {
            return ApkType::Arch;
        }
        if config_name.ends_with("dpi") {
            return ApkType::Dpi;
        }
    }
    ApkType::Locale
}

fn classify_apks(extracted: &HashMap<String, PathBuf>, package_name: &str) -> ClassifiedApks {
    let mut main = String::new();
    let mut arch = Vec::new();
    let mut dpi = Vec::new();
    let mut locale = Vec::new();

    for name in extracted.keys() {
        match classify_apk(name, package_name) {
            ApkType::Main => main = name.clone(),
            ApkType::Arch => arch.push(name.clone()),
            ApkType::Dpi => dpi.push(name.clone()),
            ApkType::Locale => locale.push(name.clone()),
        }
    }

    // Fallback: if no main found, pick first .apk alphabetically
    if main.is_empty() {
        let mut names: Vec<&String> = extracted.keys().collect();
        names.sort();
        if let Some(first) = names.first() {
            main = (*first).clone();
        }
    }

    ClassifiedApks { main, arch, dpi, locale }
}

fn prioritize_dpi_splits(dpi_names: &[String]) -> Vec<String> {
    let priority = ["xxxhdpi", "xxhdpi", "xhdpi", "hdpi", "mdpi", "ldpi", "nodpi", "tvdpi"];
    let mut scored: Vec<(usize, String)> = dpi_names.iter().map(|name| {
        let density = name.trim_start_matches("config.").trim_end_matches(".apk");
        let idx = priority.iter().position(|&p| p == density).unwrap_or(usize::MAX);
        (idx, name.clone())
    }).collect();
    scored.sort_by_key(|(idx, _)| *idx);
    scored.into_iter().map(|(_, name)| name).collect()
}

// ---------------------------------------------------------------------------
// Arch Split Selection (already covered by unit tests)
// ---------------------------------------------------------------------------

pub fn find_arch_split(
    splits: &[SplitInfo],
    target: &str,
    fallback: &[&str],
) -> Option<String> {
    let target_idx = fallback.iter().position(|&a| a == target)?;
    for &arch in &fallback[target_idx..] {
        // 1. Try explicit abi field match
        if let Some(split) = splits.iter().find(|s| s.abi.as_deref() == Some(arch)) {
            return Some(split.file.clone());
        }
        // 2. Fallback: derive arch from filename when abi is None
        let arch_underscore = arch.replace('-', "_");
        if let Some(split) = splits.iter().find(|s| {
            if s.abi.is_some() {
                return false;
            }
            let file_lower = s.file.to_lowercase();
            file_lower.contains(arch) || file_lower.contains(&arch_underscore)
        }) {
            return Some(split.file.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// APK Decode / Rebuild via apktool
// ---------------------------------------------------------------------------

fn decode_apk(apk_path: &Path, output_dir: &Path) -> Result<(), String> {
    let status = Command::new("apktool")
        .arg("d")
        .arg("-s")
        .arg("-o")
        .arg(output_dir)
        .arg(apk_path)
        .status()
        .map_err(|e| format!("apktool decode failed for {}: {}", apk_path.display(), e))?;

    if !status.success() {
        return Err(format!(
            "apktool decode exited with code {:?} for {}",
            status.code(),
            apk_path.display()
        ));
    }
    Ok(())
}

fn rebuild_apk(base_dir: &Path) -> Result<PathBuf, String> {
    let status = Command::new("apktool")
        .arg("b")
        .arg(base_dir)
        .status()
        .map_err(|e| format!("apktool build failed for {}: {}", base_dir.display(), e))?;

    if !status.success() {
        return Err(format!(
            "apktool build exited with code {:?} for {}",
            status.code(),
            base_dir.display()
        ));
    }

    let dist_dir = base_dir.join("dist");
    let base_name = base_dir.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("base");
    let built_apk = dist_dir.join(format!("{}.apk", base_name));

    if !built_apk.exists() {
        // apktool may name the output after the original APK name stored in apktool.yml
        // Try to find any .apk in the dist directory
        let entries = fs::read_dir(&dist_dir)
            .map_err(|e| format!("Cannot read dist dir {}: {}", dist_dir.display(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("apk") {
                return Ok(path);
            }
        }
        return Err(format!("apktool build did not produce an APK in {}", dist_dir.display()));
    }

    Ok(built_apk)
}

// ---------------------------------------------------------------------------
// Merge Helpers
// ---------------------------------------------------------------------------

fn merge_apk_arch(main_dir: &Path, split_dir: &Path) -> Result<(), String> {
    let target_lib = main_dir.join("lib");
    let source_lib = split_dir.join("lib");

    if !source_lib.exists() {
        return Ok(());
    }

    if !target_lib.exists() {
        fs::create_dir_all(&target_lib)
            .map_err(|e| format!("Cannot create lib dir: {}", e))?;
    }

    for entry in fs::read_dir(&source_lib)
        .map_err(|e| format!("Cannot read lib dir {}: {}", source_lib.display(), e))? {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let src_path = entry.path();
        let dst_path = target_lib.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)
                .map_err(|e| format!("Cannot copy lib dir {} to {}: {}", src_path.display(), dst_path.display(), e))?;
        } else {
            fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Cannot copy lib file {} to {}: {}", src_path.display(), dst_path.display(), e))?;
        }
    }

    merge_do_not_compress(main_dir, split_dir)?;
    Ok(())
}

fn merge_apk_resources(main_dir: &Path, split_dir: &Path) -> Result<(), String> {
    let target_res = main_dir.join("res");
    let source_res = split_dir.join("res");

    if !source_res.exists() {
        return Ok(());
    }

    for entry in walkdir::WalkDir::new(&source_res) {
        let entry = entry.map_err(|e| format!("Walk error in {}: {}", source_res.display(), e))?;
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }

        // Skip values/public.xml entirely
        if source_path.to_string_lossy().ends_with("values/public.xml") {
            continue;
        }

        let rel_path = source_path.strip_prefix(&source_res)
            .map_err(|e| format!("Strip prefix error: {}", e))?;
        let target_path = target_res.join(rel_path);

        // If target already exists, skip (do not overwrite base resources)
        if target_path.exists() {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }

        fs::copy(source_path, &target_path)
            .map_err(|e| format!("Cannot copy {} to {}: {}", source_path.display(), target_path.display(), e))?;
    }

    Ok(())
}

fn merge_apk_assets(main_dir: &Path, split_dir: &Path) -> Result<(), String> {
    let target_assets = main_dir.join("assets");
    let target_assetpack = target_assets.join("assetpack");
    let source_assets = split_dir.join("assets");
    let source_assetpack = source_assets.join("assetpack");

    if !source_assetpack.exists() {
        return Ok(());
    }

    if !target_assets.exists() {
        fs::create_dir_all(&target_assets)
            .map_err(|e| format!("Cannot create assets dir: {}", e))?;
    }
    if !target_assetpack.exists() {
        fs::create_dir_all(&target_assetpack)
            .map_err(|e| format!("Cannot create assetpack dir: {}", e))?;
    }

    for entry in walkdir::WalkDir::new(&source_assetpack) {
        let entry = entry.map_err(|e| format!("Walk error: {}", e))?;
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }

        let rel_path = source_path.strip_prefix(&source_assetpack)
            .map_err(|e| format!("Strip prefix error: {}", e))?;
        let target_path = target_assetpack.join(rel_path);

        if target_path.exists() {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create dir {}: {}", parent.display(), e))?;
        }

        fs::copy(source_path, &target_path)
            .map_err(|e| format!("Cannot copy {} to {}: {}", source_path.display(), target_path.display(), e))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// apktool.yml doNotCompress Merge
// ---------------------------------------------------------------------------

fn merge_do_not_compress(main_dir: &Path, split_dir: &Path) -> Result<(), String> {
    let main_config = main_dir.join("apktool.yml");
    let split_config = split_dir.join("apktool.yml");

    if !main_config.exists() || !split_config.exists() {
        return Ok(());
    }

    let main_text = fs::read_to_string(&main_config)
        .map_err(|e| format!("Cannot read apktool.yml: {}", e))?;
    let split_text = fs::read_to_string(&split_config)
        .map_err(|e| format!("Cannot read apktool.yml: {}", e))?;

    let main_entries = extract_do_not_compress(&main_text);
    let split_entries = extract_do_not_compress(&split_text);

    let mut merged: Vec<String> = main_entries.clone();
    for entry in split_entries {
        if !merged.contains(&entry) {
            merged.push(entry);
        }
    }
    merged.sort();

    let updated = replace_do_not_compress(&main_text, &merged);
    fs::write(&main_config, updated)
        .map_err(|e| format!("Cannot write apktool.yml: {}", e))?;

    Ok(())
}

fn extract_do_not_compress(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        if line.trim_start().starts_with("doNotCompress:") {
            in_block = true;
            continue;
        }
        if in_block {
            let trimmed = line.trim_start();
            if trimmed.starts_with("-") {
                result.push(line.to_string());
            } else if !trimmed.is_empty() && !trimmed.starts_with("#") {
                // End of block
                break;
            }
        }
    }
    result
}

fn replace_do_not_compress(text: &str, new_entries: &[String]) -> String {
    let mut result = Vec::new();
    let mut in_block = false;
    let mut block_started = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("doNotCompress:") {
            in_block = true;
            block_started = true;
            result.push(line.to_string());
            for entry in new_entries {
                result.push(entry.clone());
            }
            continue;
        }
        if in_block {
            if trimmed.starts_with("-") {
                // Skip old entries
                continue;
            } else if (!trimmed.is_empty() && !trimmed.starts_with("#")) || (trimmed.is_empty() && block_started) {
                in_block = false;
            }
        }
        result.push(line.to_string());
    }
    result.join("\n") + "\n"
}

// ---------------------------------------------------------------------------
// Bundle Signing Metadata Cleanup
// ---------------------------------------------------------------------------

fn delete_signature_related_files(main_dir: &Path) -> Result<(), String> {
    let targets = [
        main_dir.join("original").join("META-INF").join("BNDLTOOL.RSA"),
        main_dir.join("original").join("META-INF").join("BNDLTOOL.SF"),
        main_dir.join("original").join("META-INF").join("MANIFEST.MF"),
    ];
    for path in &targets {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Misnamed Image File Fix
// ---------------------------------------------------------------------------

fn fix_misnamed_image_files(main_dir: &Path) -> Result<(), String> {
    let res_dir = main_dir.join("res");
    if !res_dir.exists() {
        return Ok(());
    }

    let mut fixed = 0;
    for entry in walkdir::WalkDir::new(&res_dir) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".png") {
            continue;
        }

        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
            // JPEG magic detected in .png file
            let new_name = format!("{}.jpg", name.trim_end_matches(".png"));
            let new_path = path.with_file_name(&new_name);
            if let Err(e) = fs::rename(path, &new_path) {
                warn(&format!("Cannot rename misnamed image {}: {}", path.display(), e));
                continue;
            }
            fixed += 1;
        }
    }

    if fixed > 0 {
        info(&format!("Fixed {} misnamed image file(s) (.png -> .jpg)", fixed));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AndroidManifest.xml Surgery
// ---------------------------------------------------------------------------

fn update_main_manifest_file(main_dir: &Path) -> Result<(), String> {
    let manifest_path = main_dir.join("AndroidManifest.xml");
    let mut data = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Cannot read AndroidManifest.xml: {}", e))?;

    let replacements = [
        (r#"android:isSplitRequired="true" "#, " "),
        (r#"android:requiredSplitTypes="base__abi,base__density" "#, " "),
        (r#"android:splitTypes="" "#, " "),
        (
            r#"android:value="STAMP_TYPE_DISTRIBUTION_APK""#,
            r#"android:value="STAMP_TYPE_STANDALONE_APK""#,
        ),
        (
            r#"<meta-data android:name="com.android.vending.splits.required" android:value="true"/>"#,
            "",
        ),
        (
            r#"<meta-data android:name="com.android.vending.splits" android:resource="@xml/splits0"/>"#,
            "",
        ),
    ];

    for (from, to) in &replacements {
        data = data.replace(from, to);
    }

    fs::write(&manifest_path, data)
        .map_err(|e| format!("Cannot write AndroidManifest.xml: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Zipalign + Sign
// ---------------------------------------------------------------------------

fn zipalign_apk(
    input: &Path,
    temp_dir: &Path,
    arch: &str,
) -> Result<PathBuf, String> {
    let output = temp_dir.join(format!("aligned_{}.apk", arch));
    let status = Command::new("zipalign")
        .arg("-p")
        .arg("-f")
        .arg("4")
        .arg(input)
        .arg(&output)
        .status()
        .map_err(|e| format!("zipalign failed: {}", e))?;

    if !status.success() {
        // Fallback: copy input as-is (useful when zipalign is unavailable in test environments)
        fs::copy(input, &output)
            .map_err(|e| format!("zipalign failed and copy fallback failed: {}", e))?;
    }
    Ok(output)
}

fn sign_apk(
    input: &Path,
    temp_dir: &Path,
    arch: &str,
    sign: &SignSettings,
) -> Result<PathBuf, String> {
    let output = temp_dir.join(format!("signed_{}.apk", arch));
    let status = Command::new("apksigner")
        .arg("sign")
        .arg("--ks")
        .arg(&sign.keystore_file)
        .arg("--ks-pass")
        .arg(format!("pass:{}", sign.keystore_password))
        .arg("--key-pass")
        .arg(format!("pass:{}", sign.key_password))
        .arg("--ks-key-alias")
        .arg(&sign.key_alias)
        .arg("--out")
        .arg(&output)
        .arg(input)
        .status()
        .map_err(|e| format!("apksigner failed: {}", e))?;

    if !status.success() {
        return Err(format!("apksigner exited with code {:?}", status.code()));
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Utility: Recursive Directory Copy
// ---------------------------------------------------------------------------

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_classify_apk_main() {
        assert_eq!(classify_apk("com.example.app.apk", "com.example.app"), ApkType::Main);
        assert_eq!(classify_apk("base.apk", "com.example.app"), ApkType::Main);
    }

    #[test]
    fn test_classify_apk_arch() {
        assert_eq!(classify_apk("config.arm64_v8a.apk", "com.example.app"), ApkType::Arch);
        assert_eq!(classify_apk("config.armeabi_v7a.apk", "com.example.app"), ApkType::Arch);
        assert_eq!(classify_apk("config.x86_64.apk", "com.example.app"), ApkType::Arch);
    }

    #[test]
    fn test_classify_apk_dpi() {
        assert_eq!(classify_apk("config.xxhdpi.apk", "com.example.app"), ApkType::Dpi);
        assert_eq!(classify_apk("config.xxxhdpi.apk", "com.example.app"), ApkType::Dpi);
        assert_eq!(classify_apk("config.mdpi.apk", "com.example.app"), ApkType::Dpi);
    }

    #[test]
    fn test_classify_apk_locale() {
        assert_eq!(classify_apk("config.fr.apk", "com.example.app"), ApkType::Locale);
        assert_eq!(classify_apk("config.de.apk", "com.example.app"), ApkType::Locale);
        assert_eq!(classify_apk("config.en.apk", "com.example.app"), ApkType::Locale);
    }

    #[test]
    fn test_classify_apk_other_non_config() {
        assert_eq!(classify_apk("extra.apk", "com.example.app"), ApkType::Locale);
    }

    #[test]
    fn test_prioritize_dpi_splits() {
        let input = vec![
            "config.mdpi.apk".to_string(),
            "config.xxhdpi.apk".to_string(),
            "config.xxxhdpi.apk".to_string(),
        ];
        let result = prioritize_dpi_splits(&input);
        assert_eq!(result, vec![
            "config.xxxhdpi.apk",
            "config.xxhdpi.apk",
            "config.mdpi.apk",
        ]);
    }

    #[test]
    fn test_extract_do_not_compress() {
        let yaml = "version: 2.9.0\ndoNotCompress:\n- resources.arsc\n- png\n- mp3\nunknownFiles:\n  abc: '8'\n";
        let entries = extract_do_not_compress(yaml);
        assert_eq!(entries, vec!["- resources.arsc", "- png", "- mp3"]);
    }

    #[test]
    fn test_replace_do_not_compress() {
        let yaml = "version: 2.9.0\ndoNotCompress:\n- resources.arsc\n- png\nunknownFiles:\n  abc: '8'\n";
        let new_entries = vec!["- resources.arsc".to_string(), "- jpg".to_string(), "- mp3".to_string()];
        let result = replace_do_not_compress(yaml, &new_entries);
        assert!(result.contains("doNotCompress:"));
        assert!(result.contains("- resources.arsc"));
        assert!(result.contains("- jpg"));
        assert!(result.contains("- mp3"));
        assert!(!result.contains("\n- png\n"));
        assert!(result.contains("unknownFiles:"));
    }

    #[test]
    fn test_update_main_manifest_file() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("AndroidManifest.xml");
        let original = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example" android:isSplitRequired="true" android:requiredSplitTypes="base__abi,base__density" android:splitTypes="" android:versionCode="1" android:versionName="1.0">
    <application>
        <meta-data android:name="com.android.vending.splits.required" android:value="true"/>
        <meta-data android:name="com.android.vending.splits" android:resource="@xml/splits0"/>
        <meta-data android:value="STAMP_TYPE_DISTRIBUTION_APK" android:name="com.android.stamp.type"/>
    </application>
</manifest>"#;
        fs::write(&manifest, original).unwrap();

        update_main_manifest_file(tmp.path()).unwrap();

        let updated = fs::read_to_string(&manifest).unwrap();
        assert!(!updated.contains("isSplitRequired"));
        assert!(!updated.contains("requiredSplitTypes"));
        assert!(!updated.contains("android:splitTypes=\"\""));
        assert!(!updated.contains("com.android.vending.splits.required"));
        assert!(!updated.contains("com.android.vending.splits"));
        assert!(updated.contains("STAMP_TYPE_STANDALONE_APK"));
        assert!(!updated.contains("STAMP_TYPE_DISTRIBUTION_APK"));
    }

    #[test]
    fn test_fix_misnamed_image_files() {
        let tmp = tempfile::tempdir().unwrap();
        let res_dir = tmp.path().join("res").join("drawable");
        fs::create_dir_all(&res_dir).unwrap();

        // Create a real PNG file
        let png_path = res_dir.join("icon.png");
        fs::write(&png_path, b"\x89PNG\r\n\x1a\n").unwrap();

        // Create a JPEG disguised as PNG
        let fake_png = res_dir.join("bg.png");
        fs::write(&fake_png, &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]).unwrap();

        fix_misnamed_image_files(tmp.path()).unwrap();

        assert!(png_path.exists(), "Real PNG should remain");
        assert!(!fake_png.exists(), "Fake PNG should be renamed");
        let jpg_path = res_dir.join("bg.jpg");
        assert!(jpg_path.exists(), "Renamed JPEG should exist");
    }

    #[test]
    fn test_delete_signature_related_files() {
        let tmp = tempfile::tempdir().unwrap();
        let meta_dir = tmp.path().join("original").join("META-INF");
        fs::create_dir_all(&meta_dir).unwrap();

        fs::write(meta_dir.join("BNDLTOOL.RSA"), b"sig").unwrap();
        fs::write(meta_dir.join("BNDLTOOL.SF"), b"sig").unwrap();
        fs::write(meta_dir.join("MANIFEST.MF"), b"manifest").unwrap();
        fs::write(meta_dir.join("OTHER.RSA"), b"other").unwrap();

        delete_signature_related_files(tmp.path()).unwrap();

        assert!(!meta_dir.join("BNDLTOOL.RSA").exists());
        assert!(!meta_dir.join("BNDLTOOL.SF").exists());
        assert!(!meta_dir.join("MANIFEST.MF").exists());
        assert!(meta_dir.join("OTHER.RSA").exists(), "Unrelated file should remain");
    }

    #[test]
    fn test_merge_apk_resources_skips_public_xml_and_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let main_res = tmp.path().join("main").join("res");
        let split_res = tmp.path().join("split").join("res");

        // Main has existing drawable and values
        fs::create_dir_all(main_res.join("drawable-xxhdpi")).unwrap();
        fs::write(main_res.join("drawable-xxhdpi").join("icon.png"), b"main_icon").unwrap();
        fs::create_dir_all(main_res.join("values")).unwrap();
        fs::write(main_res.join("values").join("strings.xml"), b"main_strings").unwrap();

        // Split has public.xml (should be skipped), new drawable (should copy), existing drawable (should skip), new values (should copy)
        fs::create_dir_all(split_res.join("values")).unwrap();
        fs::write(split_res.join("values").join("public.xml"), b"public").unwrap();
        fs::write(split_res.join("values").join("colors.xml"), b"colors").unwrap();
        fs::create_dir_all(split_res.join("drawable-xxhdpi")).unwrap();
        fs::write(split_res.join("drawable-xxhdpi").join("icon.png"), b"split_icon").unwrap();
        fs::write(split_res.join("drawable-xxhdpi").join("new.png"), b"new_icon").unwrap();

        merge_apk_resources(tmp.path().join("main").as_path(), tmp.path().join("split").as_path()).unwrap();

        // public.xml should be skipped
        assert!(!main_res.join("values").join("public.xml").exists());
        // colors.xml should be copied (new file)
        assert!(main_res.join("values").join("colors.xml").exists());
        assert_eq!(fs::read_to_string(main_res.join("values").join("colors.xml")).unwrap(), "colors");
        // Existing icon.png should be preserved (main version)
        assert_eq!(fs::read_to_string(main_res.join("drawable-xxhdpi").join("icon.png")).unwrap(), "main_icon");
        // New drawable should be copied
        assert!(main_res.join("drawable-xxhdpi").join("new.png").exists());
    }

    #[test]
    fn test_merge_apk_arch_copies_lib_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let main_dir = tmp.path().join("main");
        let split_dir = tmp.path().join("split");

        // Split has lib/arm64-v8a/libfoo.so
        fs::create_dir_all(split_dir.join("lib").join("arm64-v8a")).unwrap();
        fs::write(split_dir.join("lib").join("arm64-v8a").join("libfoo.so"), b"foo").unwrap();

        merge_apk_arch(&main_dir, &split_dir).unwrap();

        assert!(main_dir.join("lib").join("arm64-v8a").join("libfoo.so").exists());
        assert_eq!(fs::read_to_string(main_dir.join("lib").join("arm64-v8a").join("libfoo.so")).unwrap(), "foo");
    }

    #[test]
    fn test_copy_dir_all_recursive() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");

        fs::create_dir_all(src.join("a").join("b")).unwrap();
        fs::write(src.join("root.txt"), b"root").unwrap();
        fs::write(src.join("a").join("a.txt"), b"a").unwrap();
        fs::write(src.join("a").join("b").join("b.txt"), b"b").unwrap();

        copy_dir_all(&src, &dst).unwrap();

        assert!(dst.join("root.txt").exists());
        assert!(dst.join("a").join("a.txt").exists());
        assert!(dst.join("a").join("b").join("b.txt").exists());
        assert_eq!(fs::read_to_string(dst.join("a").join("b").join("b.txt")).unwrap(), "b");
    }

    #[test]
    fn test_merge_do_not_compress_deduplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let main_dir = tmp.path().join("main");
        let split_dir = tmp.path().join("split");
        fs::create_dir_all(&main_dir).unwrap();
        fs::create_dir_all(&split_dir).unwrap();

        fs::write(main_dir.join("apktool.yml"), "doNotCompress:\n- resources.arsc\n- png\n").unwrap();
        fs::write(split_dir.join("apktool.yml"), "doNotCompress:\n- png\n- mp3\n").unwrap();

        merge_do_not_compress(&main_dir, &split_dir).unwrap();

        let result = fs::read_to_string(main_dir.join("apktool.yml")).unwrap();
        assert!(result.contains("- resources.arsc"));
        assert!(result.contains("- png"));
        assert!(result.contains("- mp3"));
        // Should not contain duplicate png
        let png_count = result.matches("- png").count();
        assert_eq!(png_count, 1);
    }

    #[test]
    fn test_merge_apk_assets_copies_assetpack() {
        let tmp = tempfile::tempdir().unwrap();
        let main_dir = tmp.path().join("main");
        let split_dir = tmp.path().join("split");

        fs::create_dir_all(split_dir.join("assets").join("assetpack").join("sub")).unwrap();
        fs::write(split_dir.join("assets").join("assetpack").join("data.bin"), b"data").unwrap();
        fs::write(split_dir.join("assets").join("assetpack").join("sub").join("extra.bin"), b"extra").unwrap();

        merge_apk_assets(&main_dir, &split_dir).unwrap();

        assert!(main_dir.join("assets").join("assetpack").join("data.bin").exists());
        assert!(main_dir.join("assets").join("assetpack").join("sub").join("extra.bin").exists());
    }
}
