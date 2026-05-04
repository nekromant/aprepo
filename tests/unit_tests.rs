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
