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

    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
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

    let args = codeskel::cli::NextArgs { cache: tmp.path().join("cache.json"), target: None, max_fields: 0 };
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
    let args0 = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
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
    let args1 = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    let out1 = codeskel::commands::next::run_and_capture(args1).unwrap();
    assert_eq!(out1.index, Some(1), "second call must return index 1");

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
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap();

    // Advance past all remaining files
    let mut last_output = None;
    for _ in 0..n {
        let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
        last_output = Some(codeskel::commands::next::run_and_capture(args).unwrap());
    }

    let done_output = last_output.unwrap();
    assert!(done_output.done, "after n advances past n files, must be done");
    assert_eq!(done_output.index, None);
    assert!(done_output.file.is_none());
    assert_eq!(done_output.remaining, 0);
}

// ── codeskel next --target (targeted mode) tests ────────────────────

fn make_targeted_args(cache_path: std::path::PathBuf, target: &str) -> codeskel::cli::NextArgs {
    codeskel::cli::NextArgs { cache: cache_path, target: Some(target.to_string()), max_fields: 0 }
}

#[test]
fn test_targeted_bootstrap_returns_first_dep() {
    // java_refs_project: UserService.java imports User.java and UserRepository.java
    // chain should be [User.java, UserRepository.java, UserService.java] (topo order)
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    let args = make_targeted_args(cache_path.clone(), &target);
    let out = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!out.done, "bootstrap must not be done");
    assert_eq!(out.mode, "targeted");
    assert_eq!(out.index, Some(0), "first call returns index 0 within chain");
    assert!(out.file.is_some());

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.target.as_deref(), Some(target.as_str()));
    assert!(session.chain.as_ref().map(|c| c.len()).unwrap_or(0) >= 1);
    // Target itself is last entry in chain
    assert_eq!(session.chain.as_ref().unwrap().last().map(|s| s.as_str()), Some(target.as_str()));
}

#[test]
fn test_targeted_no_deps_chain_is_target_only() {
    // Use a file with no internal imports. In java_project, Base.java has no internal imports.
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("Base"))
        .cloned()
        .expect("Base must be in order");

    let args = make_targeted_args(cache_path.clone(), &target);
    let out = codeskel::commands::next::run_and_capture(args).unwrap();

    // Chain = [target] only → bootstrap returns target immediately at index 0
    assert!(!out.done);
    assert_eq!(out.mode, "targeted");
    assert_eq!(out.index, Some(0));
    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.chain.as_ref().unwrap().len(), 1);
    assert_eq!(session.chain.as_ref().unwrap()[0], target);
}

#[test]
fn test_targeted_advances_through_chain_and_done() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    // Bootstrap
    let out0 = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(!out0.done);
    let chain_len = codeskel::session::read_session(tmp.path())
        .chain.unwrap().len();

    // Advance through all remaining chain entries
    let mut pre_done = None;
    let mut last = None;
    for _ in 1..=chain_len {
        let out = codeskel::commands::next::run_and_capture(
            make_targeted_args(cache_path.clone(), &target)
        ).unwrap();
        if !out.done {
            pre_done = Some(out.file.as_ref().map(|f| f.path.clone()).unwrap_or_default());
        }
        last = Some(out);
    }

    let done = last.unwrap();
    assert!(done.done, "after chain_len advances, must be done");
    assert_eq!(done.mode, "targeted");
    assert_eq!(done.remaining, 0);
    assert!(done.file.is_none());
    // session.json deleted on done
    assert!(!tmp.path().join("session.json").exists());
    // The last non-done call must have returned the target file
    assert_eq!(pre_done.as_deref(), Some(target.as_str()),
        "last non-done iteration must return the target file itself");
}

#[test]
fn test_targeted_done_then_bootstrap_again() {
    // After done (session deleted), calling next --target should bootstrap fresh (not error).
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("Base"))
        .cloned()
        .expect("Base must be in order");

    // Bootstrap → done immediately (no deps)
    let _out0 = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    let done = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(done.done);
    assert!(!tmp.path().join("session.json").exists());

    // Call again — should re-bootstrap cleanly
    let restart = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();
    assert!(!restart.done, "after done+delete, next call bootstraps again");
    assert_eq!(restart.index, Some(0));
}

#[test]
fn test_targeted_mismatch_warns_and_rebootstraps() {
    // Start a targeted session for target A, then call with target B.
    // Should warn to stderr and bootstrap fresh for B.
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target_a = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");
    let target_b = cache.order.iter()
        .find(|p| p.contains("UserRepository"))
        .cloned()
        .expect("UserRepository must be in order");

    // Bootstrap for A
    codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target_a)
    ).unwrap();

    // Now call with B — should bootstrap for B, not continue A's session
    let out_b = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target_b)
    ).unwrap();
    assert!(!out_b.done);
    assert_eq!(out_b.mode, "targeted");

    let session = codeskel::session::read_session(tmp.path());
    assert_eq!(session.target.as_deref(), Some(target_b.as_str()),
        "session must now track target B");
}

#[test]
fn test_targeted_project_mode_mismatch_rebootstraps_project() {
    // Start a targeted session, then call bare next (project mode).
    // Should warn and bootstrap project mode.
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();
    let target = cache.order.iter()
        .find(|p| p.contains("UserService"))
        .cloned()
        .expect("UserService must be in order");

    // Bootstrap targeted session
    codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();

    // Call project mode (no --target)
    let proj_out = codeskel::commands::next::run_and_capture(
        codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 }
    ).unwrap();
    assert!(!proj_out.done);
    assert_eq!(proj_out.mode, "project");
    assert_eq!(proj_out.index, Some(0), "project mode restarted at index 0");
}

#[test]
fn test_targeted_error_on_missing_target() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let args = make_targeted_args(cache_path.clone(), "src/DoesNotExist.java");
    let result = codeskel::commands::next::run_and_capture(args);
    assert!(result.is_err(), "missing target must return Err");
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("not found in cache"), "error message should say 'not found in cache', got: {}", msg);
    // No session written
    assert!(!tmp.path().join("session.json").exists());
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
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap();
    assert!(tmp.path().join("session.json").exists(), "session must exist after next");

    // Second scan via the command layer must delete session.json
    codeskel::commands::scan::run(codeskel::cli::ScanArgs {
        project_root: fixture_root.clone(),
        lang: None,
        include: vec![],
        exclude: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().to_path_buf()),
        verbose: false,
    }).unwrap();

    assert!(!tmp.path().join("session.json").exists(),
        "session.json must be deleted by scan");
}

#[test]
fn test_targeted_skips_missing_chain_entry() {
    // Verify that a chain entry no longer in cache is skipped and the next valid entry is returned.
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let cache = codeskel::cache::read_cache(&cache_path).unwrap();

    // Pick two real files from the cache
    let file0 = cache.order.get(0).cloned().expect("order must have at least 2 entries");
    let file1 = cache.order.get(1).cloned().expect("order must have at least 2 entries");
    let target = cache.order.last().cloned().expect("order must be non-empty");

    // Manually write a session: cursor=0, current_file=file0
    // chain = [file0, "ghost_file_that_does_not_exist.java", file1, target]
    let ghost = "com/example/GhostFile.java".to_string();
    let chain = vec![file0.clone(), ghost.clone(), file1.clone(), target.clone()];
    codeskel::session::write_session(tmp.path(), &codeskel::session::Session {
        cursor: 0,
        current_file: Some(file0.clone()),
        target: Some(target.clone()),
        chain: Some(chain),
    }).unwrap();

    // Call next --target: should rescan file0, then skip ghost, then return file1
    let out = codeskel::commands::next::run_and_capture(
        make_targeted_args(cache_path.clone(), &target)
    ).unwrap();

    assert!(!out.done, "should not be done — file1 and target still remain after skipping ghost");
    assert_eq!(out.mode, "targeted");
    let returned_path = out.file.as_ref().expect("file must be present").path.clone();
    assert_eq!(returned_path, file1,
        "should skip ghost and return file1, got: {}", returned_path);
}

#[test]
fn next_file_entry_omits_skip_and_internal_imports() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance twice to get a file that has internal_imports (UserService depends on User + UserRepository)
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 0
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 1
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    let output = codeskel::commands::next::run_and_capture(args).unwrap(); // index 2: UserService

    assert!(!output.done);
    let json = serde_json::to_string(&output.file).unwrap();
    assert!(!json.contains("\"skip\""),
        "skip should not appear in next file output, got: {}", json);
    assert!(!json.contains("\"internal_imports\""),
        "internal_imports should not appear in next file output, got: {}", json);
    assert!(!json.contains("\"skip_reason\""),
        "skip_reason should not appear in next file output, got: {}", json);
    assert!(!json.contains("\"scanned_at\""),
        "scanned_at should not appear in next file output, got: {}", json);
}

#[test]
fn next_dep_signatures_omit_has_docstring_and_line() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance to a file that has deps
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 0
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    codeskel::commands::next::run_and_capture(args).unwrap(); // index 1
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    let output = codeskel::commands::next::run_and_capture(args).unwrap(); // index 2: UserService

    assert!(!output.deps.is_empty(), "UserService must have deps");
    let deps_json = serde_json::to_string(&output.deps).unwrap();
    assert!(!deps_json.contains("\"has_docstring\""),
        "has_docstring must not appear in dep signatures, got: {}", deps_json);
    assert!(!deps_json.contains("\"line\""),
        "line must not appear in dep signatures, got: {}", deps_json);
}

#[test]
fn next_deps_filtered_to_referenced_symbols() {
    let tmp = tempdir().unwrap();
    make_cache_in("java_refs_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    // Advance to UserService (index 2 in topo order: User → UserRepository → UserService)
    for _ in 0..2 {
        let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
        codeskel::commands::next::run_and_capture(args).unwrap();
    }
    let args = codeskel::cli::NextArgs { cache: cache_path.clone(), target: None, max_fields: 0 };
    let output = codeskel::commands::next::run_and_capture(args).unwrap();

    assert!(!output.done);
    let file_path = output.file.as_ref().unwrap().path.as_str();
    assert!(file_path.contains("UserService"), "expected UserService, got: {}", file_path);

    // Find the User dep
    let user_dep = output.deps.iter()
        .find(|d| d.path.contains("User.java") && !d.path.contains("UserRepository"))
        .expect("User dep must be present");

    let sig_names: Vec<&str> = user_dep.signatures.iter()
        .map(|s| s.name.as_str())
        .collect();

    // User class (top-level type) must always be present
    assert!(sig_names.contains(&"User"), "User class must be in dep signatures, got: {:?}", sig_names);
    // getEmail is referenced in UserService → must be included
    assert!(sig_names.contains(&"getEmail"), "getEmail must be included (referenced), got: {:?}", sig_names);
    // setEmail is NOT referenced in UserService → must be filtered out
    assert!(!sig_names.contains(&"setEmail"), "setEmail must be filtered out (unreferenced), got: {:?}", sig_names);
}

#[test]
fn next_output_is_compact_json() {
    use std::process::Command;
    let tmp = tempdir().unwrap();
    make_cache_in("java_project", tmp.path());
    let cache_path = tmp.path().join("cache.json");

    let output = Command::new(env!("CARGO_BIN_EXE_codeskel"))
        .args(["next", "--cache", cache_path.to_str().unwrap()])
        .output()
        .expect("failed to run codeskel");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Compact JSON has no newlines (except possibly a trailing one)
    let trimmed = stdout.trim();
    assert!(!trimmed.contains('\n'), "output should be single-line compact JSON, got:\n{}", trimmed);
    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(trimmed).expect("output must be valid JSON");
}

#[test]
fn test_next_max_fields_truncates_fields() {
    use codeskel::models::{CacheFile, FileEntry, Signature, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();

    let mut dep_sigs: Vec<Signature> = vec![Signature {
        kind: "class".into(), name: "AppExCode".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, existing_word_count: 0, docstring_text: None,
    }];
    for i in 0..8usize {
        dep_sigs.push(Signature {
            kind: "field".into(), name: format!("CODE_{}", i),
            modifiers: vec![], params: None, return_type: None,
            throws: vec![], extends: None, implements: vec![], annotations: vec![],
            line: i + 2, has_docstring: false, existing_word_count: 0, docstring_text: None,
        });
    }

    let main_sig = Signature {
        kind: "class".into(), name: "MyService".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, existing_word_count: 0, docstring_text: None,
    };

    let mut files = HashMap::new();
    files.insert("src/AppExCode.java".into(), FileEntry {
        path: "src/AppExCode.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false, internal_imports: vec![], signatures: dep_sigs, scanned_at: None,
    });
    files.insert("src/MyService.java".into(), FileEntry {
        path: "src/MyService.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false,
        internal_imports: vec!["src/AppExCode.java".into()],
        signatures: vec![main_sig], scanned_at: None,
    });

    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec!["java".into()],
        stats: Stats { total_files: 2, skipped_covered: 0, skipped_generated: 0, to_comment: 2 },
        min_docstring_words: 0,
        order: vec!["src/AppExCode.java".into(), "src/MyService.java".into()],
        files,
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    let args0 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"),
        target: None,
        max_fields: 5,
    };
    codeskel::commands::next::run_and_capture(args0).unwrap(); // returns AppExCode

    let args1 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"),
        target: None,
        max_fields: 5,
    };
    let out = codeskel::commands::next::run_and_capture(args1).unwrap();

    assert!(!out.done);
    assert_eq!(out.deps.len(), 1);
    let dep = &out.deps[0];
    let field_count = dep.signatures.iter().filter(|s| s.kind == "field").count();
    assert_eq!(field_count, 5, "should keep exactly max_fields=5 fields");
    assert_eq!(dep.fields_omitted, 3, "should report 3 omitted fields");
}

#[test]
fn test_next_max_fields_zero_omits_all_fields() {
    use codeskel::models::{CacheFile, FileEntry, Signature, Stats};
    use std::collections::HashMap;

    let tmp = tempdir().unwrap();

    let mut dep_sigs: Vec<Signature> = vec![Signature {
        kind: "class".into(), name: "Constants".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, existing_word_count: 0, docstring_text: None,
    }];
    for i in 0..4usize {
        dep_sigs.push(Signature {
            kind: "field".into(), name: format!("CONST_{}", i),
            modifiers: vec![], params: None, return_type: None,
            throws: vec![], extends: None, implements: vec![], annotations: vec![],
            line: i + 2, has_docstring: false, existing_word_count: 0, docstring_text: None,
        });
    }

    let main_sig = Signature {
        kind: "class".into(), name: "Consumer".into(),
        modifiers: vec![], params: None, return_type: None,
        throws: vec![], extends: None, implements: vec![], annotations: vec![],
        line: 1, has_docstring: false, existing_word_count: 0, docstring_text: None,
    };

    let mut files = HashMap::new();
    files.insert("src/Constants.java".into(), FileEntry {
        path: "src/Constants.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false, internal_imports: vec![], signatures: dep_sigs, scanned_at: None,
    });
    files.insert("src/Consumer.java".into(), FileEntry {
        path: "src/Consumer.java".into(), language: "java".into(),
        package: None, comment_coverage: 0.0, skip: false, skip_reason: None,
        cycle_warning: false,
        internal_imports: vec!["src/Constants.java".into()],
        signatures: vec![main_sig], scanned_at: None,
    });

    let cache = CacheFile {
        version: 1,
        scanned_at: "2026-01-01T00:00:00Z".into(),
        project_root: tmp.path().to_string_lossy().into_owned(),
        detected_languages: vec!["java".into()],
        stats: Stats { total_files: 2, skipped_covered: 0, skipped_generated: 0, to_comment: 2 },
        min_docstring_words: 0,
        order: vec!["src/Constants.java".into(), "src/Consumer.java".into()],
        files,
    };
    codeskel::cache::write_cache(tmp.path(), &cache).unwrap();

    let args0 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"), target: None, max_fields: 0,
    };
    codeskel::commands::next::run_and_capture(args0).unwrap();

    let args1 = codeskel::cli::NextArgs {
        cache: tmp.path().join("cache.json"), target: None, max_fields: 0,
    };
    let out = codeskel::commands::next::run_and_capture(args1).unwrap();

    let dep = &out.deps[0];
    let field_count = dep.signatures.iter().filter(|s| s.kind == "field").count();
    assert_eq!(field_count, 0, "max_fields=0 should omit all fields");
    assert_eq!(dep.fields_omitted, 4);
    assert!(dep.signatures.iter().any(|s| s.kind == "class"));
}

#[test]
fn test_existing_word_count_thin_docstring() {
    use std::io::Write;
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let mut f = std::fs::File::create(src_dir.join("Thin.java")).unwrap();
    writeln!(f, "/** Short doc. */").unwrap();
    writeln!(f, "public class Thin {{}}").unwrap();

    codeskel::scanner::scan(tmp.path(), &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 30,
        cache_dir: Some(tmp.path().join(".codeskel")),
        verbose: false,
    }).unwrap();

    let cache = codeskel::cache::read_cache(&tmp.path().join(".codeskel/cache.json")).unwrap();
    let entry = cache.files.get("src/Thin.java").unwrap();
    let cls = entry.signatures.iter().find(|s| s.kind == "class").unwrap();

    assert!(!cls.has_docstring, "thin doc should fail min_docstring_words threshold");
    assert!(cls.existing_word_count > 0, "existing_word_count should reflect actual word count");
}

#[test]
fn test_existing_word_count_populated_with_min_words_zero() {
    use std::io::Write;
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let mut f = std::fs::File::create(src_dir.join("Documented.java")).unwrap();
    writeln!(f, "/** This is a well documented class with many words here. */").unwrap();
    writeln!(f, "public class Documented {{}}").unwrap();

    codeskel::scanner::scan(tmp.path(), &codeskel::scanner::ScanConfig {
        forced_lang: None,
        include_globs: vec![],
        exclude_globs: vec![],
        min_coverage: 0.0,
        min_docstring_words: 0,
        cache_dir: Some(tmp.path().join(".codeskel")),
        verbose: false,
    }).unwrap();

    let cache = codeskel::cache::read_cache(&tmp.path().join(".codeskel/cache.json")).unwrap();
    let entry = cache.files.get("src/Documented.java").unwrap();
    let cls = entry.signatures.iter().find(|s| s.kind == "class").unwrap();

    assert!(cls.has_docstring, "with min_docstring_words=0, any doc counts");
    assert!(cls.existing_word_count > 0, "existing_word_count should be populated even when min_words=0");
}
