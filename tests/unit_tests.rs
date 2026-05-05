#[test]
fn test_duration_parsing() {
    use std::time::Duration;
    assert_eq!(aprepo::config::parse_duration("1h").unwrap(), Duration::from_secs(3600));
    assert_eq!(aprepo::config::parse_duration("1d").unwrap(), Duration::from_secs(86400));
    assert_eq!(aprepo::config::parse_duration("30m").unwrap(), Duration::from_secs(1800));
    assert_eq!(aprepo::config::parse_duration("10s").unwrap(), Duration::from_secs(10));
    assert!(aprepo::config::parse_duration("1x").is_err());
}

#[test]
fn test_state_roundtrip() {
    use aprepo::state::{State, SourceCapability};
    let tmp = std::env::temp_dir().join("aprepo_test_state.yaml");
    let mut state = State::load(&tmp).unwrap();
    state.set_version("google_play:com.example.app".to_string(), "universal".to_string(), "1.0.0".to_string(), "cache/google_play/com.example.app.xapk".to_string());
    state.set_capability("google_play".to_string(), SourceCapability { per_arch_downloads: Some(true) });
    state.save().unwrap();

    let loaded = State::load(&tmp).unwrap();
    assert_eq!(loaded.get_version("google_play:com.example.app", "universal"), Some("1.0.0"));
    assert_eq!(loaded.get_capability("google_play").unwrap().per_arch_downloads, Some(true));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_zip_validate_empty() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join("aprepo_test.zip");
    {
        let mut file = std::fs::File::create(&tmp).unwrap();
        // Write a minimal ZIP file
        let mut zip = zip::ZipWriter::new(&mut file);
        let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        zip.start_file("test.txt", options).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
    }
    assert!(aprepo::util::zip_validate::validate_zip(&tmp).is_ok());
    let _ = std::fs::remove_file(&tmp);
}

/// Bug: Servers sometimes return HTML error pages instead of APKs.
/// ZIP validation should detect non-ZIP magic bytes early and report clearly.
#[test]
fn test_zip_validate_rejects_html_error_page() {
    let tmp = std::env::temp_dir().join("aprepo_test_fake.zip");
    std::fs::write(&tmp, b"<html><body>Error: rate limited</body></html>").unwrap();
    let result = aprepo::util::zip_validate::validate_zip(&tmp);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("not a valid ZIP archive"), "Expected clear error for HTML page, got: {}", err);
    assert!(err.contains("rate limited") || err.contains("preview"), "Expected preview in error, got: {}", err);
    let _ = std::fs::remove_file(&tmp);
}

// ------------------------------------------------------------------
// Regression tests for real-world bugs found during token testing
// ------------------------------------------------------------------

/// Bug 1: Package names with dots (e.g. com.shazam.android) were truncated
/// because PathBuf::file_stem() treated ".android" as an extension.
#[test]
fn test_arch_cache_path_with_dots_in_package_name() {
    use std::path::Path;
    use aprepo::download::compute_arch_cache_path;

    // Store package without extension (e.g. com.shazam.android)
    let p = Path::new("cache/apkpure/com.shazam.android");
    let result = compute_arch_cache_path(p, "arm64-v8a");
    assert_eq!(result, Path::new("cache/apkpure/com.shazam.android_arm64-v8a.xapk"));

    // WebDL with real extension (e.g. test.apk)
    let p = Path::new("cache/webdl/test.apk");
    let result = compute_arch_cache_path(p, "arm64-v8a");
    assert_eq!(result, Path::new("cache/webdl/test_arm64-v8a.apk"));

    // Universal arch should keep original path
    let p = Path::new("cache/webdl/test.apk");
    let result = compute_arch_cache_path(p, "universal");
    assert_eq!(result, Path::new("cache/webdl/test.apk"));
}

/// Bug 2: APKPure XAPK manifests have abi=null for all splits and use
/// underscore arch names like "config.armeabi_v7a.apk".
#[test]
fn test_find_arch_split_null_abi_with_underscores() {
    use aprepo::process::xapk::find_arch_split;
    use aprepo::process::xapk::SplitInfo;

    let splits = vec![
        SplitInfo { file: "com.shazam.android.apk".to_string(), abi: None },
        SplitInfo { file: "config.armeabi_v7a.apk".to_string(), abi: None },
        SplitInfo { file: "config.fr.apk".to_string(), abi: None },
    ];

    let fallback = ["arm64-v8a", "armeabi-v7a", "armeabi", "x86_64", "x86"];

    // armeabi-v7a should match via filename fallback
    let found = find_arch_split(&splits, "armeabi-v7a", &fallback);
    assert_eq!(found, Some("config.armeabi_v7a.apk".to_string()));

    // arm64-v8a has no direct match, but falls back to armeabi-v7a
    let found = find_arch_split(&splits, "arm64-v8a", &fallback);
    assert_eq!(found, Some("config.armeabi_v7a.apk".to_string()));
}

/// Bug 2 continued: Explicit abi field takes precedence over filename fallback.
#[test]
fn test_find_arch_split_explicit_abi_precedence() {
    use aprepo::process::xapk::find_arch_split;
    use aprepo::process::xapk::SplitInfo;

    let splits = vec![
        SplitInfo { file: "base.apk".to_string(), abi: None },
        SplitInfo { file: "config.arm64.apk".to_string(), abi: Some("arm64-v8a".to_string()) },
        SplitInfo { file: "config.armeabi_v7a.apk".to_string(), abi: Some("armeabi-v7a".to_string()) },
    ];

    let fallback = ["arm64-v8a", "armeabi-v7a", "armeabi", "x86_64", "x86"];

    let found = find_arch_split(&splits, "arm64-v8a", &fallback);
    assert_eq!(found, Some("config.arm64.apk".to_string()));

    let found = find_arch_split(&splits, "armeabi-v7a", &fallback);
    assert_eq!(found, Some("config.armeabi_v7a.apk".to_string()));
}

/// Bug 4: aapt2 dump badging output parsing.
#[test]
fn test_extract_attr_from_aapt2_output() {
    use aprepo::process::apk::extract_attr;

    let line = "package: name='com.ouraring.oura' versionCode='260421164' versionName='7.12.1' platformBuildVersionName='16'";

    assert_eq!(extract_attr(line, "name="), Some("com.ouraring.oura".to_string()));
    assert_eq!(extract_attr(line, "versionName="), Some("7.12.1".to_string()));
    assert_eq!(extract_attr(line, "versionCode="), Some("260421164".to_string()));
    assert_eq!(extract_attr(line, "missing="), None);
}

/// FR-018b: Config must reject invalid architectures at startup.
#[test]
fn test_config_rejects_invalid_architecture() {
    use aprepo::config::Config;

    let yaml = r#"
settings:
  cache_dir: "/tmp/cache"
  output_dir: "/tmp/output"
  architectures:
    - arm64-v8a
    - invalid_arch
sources:
  apkpure:
    packages:
      - com.example.app
"#;
    let result = serde_yaml::from_str::<Config>(yaml);
    assert!(result.is_ok());
    let config = result.unwrap();
    let validation = config.validate();
    assert!(validation.is_err());
    let err = validation.unwrap_err();
    assert!(err.contains("Invalid architecture"));
    assert!(err.contains("invalid_arch"));
}

#[test]
fn test_config_accepts_valid_architectures() {
    use aprepo::config::Config;

    let yaml = r#"
settings:
  cache_dir: "/tmp/cache"
  output_dir: "/tmp/output"
  architectures:
    - arm64-v8a
    - armeabi-v7a
    - x86_64
    - x86
    - armeabi
sources:
  apkpure:
    packages:
      - com.example.app
"#;
    let result = serde_yaml::from_str::<Config>(yaml);
    assert!(result.is_ok());
    let config = result.unwrap();
    let validation = config.validate();
    assert!(validation.is_ok());
}
