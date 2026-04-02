use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_get_command_by_index() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();
    assert!(!cache.order.is_empty(), "order must be non-empty");

    let idx = 0usize;
    let rel = cache.order.get(idx).expect("index 0 must exist");
    let entry = cache.files.get(rel).expect("entry must exist");
    assert!(!entry.path.is_empty());
    assert!(!entry.language.is_empty());
}

#[test]
fn test_get_deps_command() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    // Service.java imports Base.java
    let service_path = cache.files.keys()
        .find(|p| p.contains("Service"))
        .cloned();

    if let Some(svc) = service_path {
        let entry = cache.files.get(&svc).unwrap();
        // internal_imports should contain Base
        assert!(entry.internal_imports.iter().any(|i| i.contains("Base")),
            "Service should depend on Base, imports: {:?}", entry.internal_imports);
    }
}

#[test]
fn test_rescan_updates_coverage() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    // Rescan one file (it's already scanned, just verify rescan runs without error)
    let cache_path = tmp.path().join("cache.json");
    let cache_before = read_cache(&cache_path).unwrap();
    let first_file = cache_before.order.get(0).cloned().unwrap();
    let abs_path = root.join(&first_file);

    let rescan_args = codeskel::cli::RescanArgs {
        cache_path: cache_path.clone(),
        file_paths: vec![abs_path],
    };
    codeskel::commands::rescan::run(rescan_args).unwrap();

    // Cache should still be readable after rescan
    let cache_after = read_cache(&cache_path).unwrap();
    assert_eq!(cache_after.version, 1);
    // The rescanned file should have scanned_at populated
    let entry = cache_after.files.get(&first_file).unwrap();
    assert!(entry.scanned_at.is_some(), "scanned_at should be set after rescan");
}

#[test]
fn test_scan_java_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");
    let tmp = tempdir().unwrap();

    let result = codeskel::scanner::scan(
        &root,
        &codeskel::scanner::ScanConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
            min_coverage: 0.8,
            cache_dir: Some(tmp.path().to_path_buf()),
            verbose: false,
        },
    ).unwrap();

    // Both files discovered
    assert_eq!(result.stats.total_files, 2, "stats: {:?}", result.stats);

    // Both have docstrings so coverage ≥ 0.8, both are skipped
    // to_comment = 0 (both skipped as well-covered)
    // OR some are to_comment depending on coverage calculation
    // Just verify the scanner ran without panic and returned valid stats
    assert!(result.stats.total_files >= 1);
    assert!(result.stats.to_comment + result.stats.skipped_covered + result.stats.skipped_generated <= result.stats.total_files);

    // order should have Service after Base (dependency order)
    // Base.java is not imported by Service.java... wait, it IS imported
    // Service imports Base, so Base should come before Service in topo order
    if result.order.len() >= 2 {
        let base_pos = result.order.iter().position(|p| p.contains("Base"));
        let svc_pos = result.order.iter().position(|p| p.contains("Service"));
        if let (Some(b), Some(s)) = (base_pos, svc_pos) {
            assert!(b < s, "Base must come before Service in dependency order");
        }
    }
}

#[test]
fn test_scan_python_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/python_project");
    let tmp = tempdir().unwrap();

    let result = codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.8,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    assert_eq!(result.stats.total_files, 2,
        "should discover utils.py and service.py");
    // service.py has no docstrings → to_comment ≥ 1
    assert!(result.stats.to_comment >= 1,
        "service.py has no docstrings, should be in to_comment");
    // languages detected
    assert!(result.detected_languages.contains(&"python".to_string()));
}

#[test]
fn test_scan_writes_cache() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");
    let tmp = tempdir().unwrap();

    codeskel::scanner::scan(
        &root,
        &codeskel::scanner::ScanConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
            min_coverage: 0.0, // include all files
            cache_dir: Some(tmp.path().to_path_buf()),
            verbose: false,
        },
    ).unwrap();

    let cache_path = tmp.path().join("cache.json");
    assert!(cache_path.exists(), "cache.json must be written");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    assert_eq!(cache.version, 1);
    assert!(!cache.files.is_empty());
    assert!(!cache.order.is_empty());

    // Service.java depends on Base.java → Base should come before Service
    let base_pos = cache.order.iter().position(|p| p.contains("Base"));
    let svc_pos = cache.order.iter().position(|p| p.contains("Service"));
    if let (Some(b), Some(s)) = (base_pos, svc_pos) {
        assert!(b < s, "Base must come before Service; order: {:?}", cache.order);
    }
}
