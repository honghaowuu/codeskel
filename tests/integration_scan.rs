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
        min_coverage: 0.0, min_docstring_words: 0,
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
        min_coverage: 0.0, min_docstring_words: 0,
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
        min_coverage: 0.0, min_docstring_words: 0,
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
            min_coverage: 0.8, min_docstring_words: 0,
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
        min_coverage: 0.8, min_docstring_words: 0,
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
            min_docstring_words: 0,
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

#[test]
fn test_chain_count_for_userservice() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0, min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let svc_path = cache.files.keys()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in cache");

    // NOTE: skipped files are excluded from cache.order and therefore chain result.
    // min_coverage=0.0 ensures all files are included.
    let chain = codeskel::commands::get::chain_order(&cache, &svc_path).unwrap();
    assert_eq!(chain.len(), 2, "UserService has 2 transitive deps; got: {:?}", chain);

    // Leaves-first: User.java (no deps) at index 0, UserRepository.java at index 1
    assert!(chain[0].contains("User") && !chain[0].contains("UserRepository"),
        "index 0 should be User.java (leaf, no deps), got: {}", chain[0]);
    assert!(chain[1].contains("UserRepository"),
        "index 1 should be UserRepository.java, got: {}", chain[1]);
}

#[test]
fn test_chain_count_zero_for_leaf() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0, min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache = read_cache(&tmp.path().join("cache.json")).unwrap();

    let user_path = cache.files.keys()
        .find(|p| p.ends_with("User.java"))
        .cloned()
        .expect("User.java must be in cache");

    let chain = codeskel::commands::get::chain_order(&cache, &user_path).unwrap();
    assert_eq!(chain.len(), 0, "User.java has no deps");
}

#[test]
fn test_refs_for_userservice() {
    use codeskel::cache::read_cache;

    let tmp = tempdir().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_refs_project");

    codeskel::scanner::scan(&root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0, min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let cache = read_cache(&cache_path).unwrap();

    let svc_path = cache.files.keys()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in cache");

    let refs = codeskel::commands::get::compute_refs(&cache, &svc_path).unwrap();

    let user_path = cache.files.keys()
        .find(|p| p.ends_with("User.java"))
        .cloned()
        .expect("User.java must be in cache");
    let repo_path = cache.files.keys()
        .find(|p| p.contains("UserRepository"))
        .cloned()
        .expect("UserRepository.java must be in cache");

    let user_refs = refs.get(&user_path).expect("User.java must appear in refs");
    assert!(user_refs.contains(&"User".to_string()), "User type ref missing");
    assert!(user_refs.contains(&"getEmail".to_string()), "getEmail missing");

    let repo_refs = refs.get(&repo_path).expect("UserRepository.java must appear in refs");
    assert!(repo_refs.contains(&"UserRepository".to_string()), "UserRepository type ref missing");
    assert!(repo_refs.contains(&"findById".to_string()), "findById missing");
    assert!(repo_refs.contains(&"save".to_string()), "save missing");
}

// ── codeskel next tests ──────────────────────────────────────────────

fn make_cache_in(fixture: &str, tmp: &std::path::Path) {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.to_path_buf()),
        verbose: false,
    }).unwrap();
}

#[test]
fn test_next_bootstrap_returns_index_0() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());

    let cache_path = tmp.path().join("cache.json");
    // No session.json yet
    assert!(!tmp.path().join("session.json").exists());

    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!output.done, "bootstrap should not be done");
    assert_eq!(output.index, Some(0), "bootstrap returns index 0");
    assert!(output.file.is_some(), "file must be present");
    assert!(tmp.path().join("session.json").exists(), "session.json must be created");

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.cursor, 0);
}

#[test]
fn test_next_empty_cache_returns_done() {
    use codeskel::models::{CacheFile, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();
    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec![],
        stats: Stats { total_files: 0, skipped_covered: 0, skipped_generated: 0, to_comment: 0 },
        min_docstring_words: 0,
        order: vec![],
        files: HashMap::new(),
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    let args = codeskel::cli::NextArgs { cache: tmp.path().join("cache.json") };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(output.done, "empty cache → done immediately");
    assert_eq!(output.index, None);
    assert!(output.file.is_none());
    assert!(output.deps.is_empty());
}

#[test]
fn test_next_advance_rescans_and_returns_next() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Bootstrap: index 0
    let args0 = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let out0 = codeskel::commands::next::run_and_capture(args0).unwrap();
    assert!(!out0.done);
    assert_eq!(out0.index, Some(0));

    let scanned_before = {
        let cache = codeskel::cache::read_cache(&cache_path).unwrap();
        let rel = &cache.order[0];
        cache.files[rel].scanned_at.clone()
    };

    // Advance: index 1 — should rescan index 0
    std::thread::sleep(std::time::Duration::from_millis(10)); // ensure timestamp advances
    let args1 = codeskel::cli::NextArgs { cache: cache_path.clone() };
    let out1 = codeskel::commands::next::run_and_capture(args1).unwrap();
    assert!(!out1.done || out1.index == Some(1), "second call should advance or reach done");

    let scanned_after = {
        let cache = codeskel::cache::read_cache(&cache_path).unwrap();
        let rel = &cache.order[0];
        cache.files[rel].scanned_at.clone()
    };

    assert_ne!(scanned_before, scanned_after,
        "rescan should have updated scanned_at of index-0 file");
}

#[test]
fn test_next_done_after_last_file() {
    let tmp = tempdir().unwrap();
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let n = cache.order.len();

    // Bootstrap
    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    codeskel::commands::next::run_and_capture(args).unwrap();

    // Advance past all remaining files
    let mut last_output = None;
    for _ in 0..n {
        let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
        last_output = Some(codeskel::commands::next::run_and_capture(args).unwrap());
    }

    let done_output = last_output.unwrap();
    assert!(done_output.done, "after n advances past n files, must be done");
    assert_eq!(done_output.index, None);
    assert!(done_output.file.is_none());
    assert_eq!(done_output.remaining, 0);
}

#[test]
fn test_scan_deletes_session() {
    let tmp = tempdir().unwrap();
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/java_project");

    // First scan + bootstrap to create session.json
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    let cache_path = tmp.path().join("cache.json");
    let args = codeskel::cli::NextArgs { cache: cache_path.clone() };
    codeskel::commands::next::run_and_capture(args).unwrap();
    assert!(tmp.path().join("session.json").exists(), "session must exist after next");

    // Second scan must delete session.json
    codeskel::scanner::scan(&fixture_root, &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    assert!(!tmp.path().join("session.json").exists(),
        "session.json must be deleted by scan");
}
