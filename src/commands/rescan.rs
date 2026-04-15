use crate::cache::{read_cache, write_cache};
use crate::cli::RescanArgs;
use crate::generated::is_generated;
use crate::lang::detect_language;
use crate::models::CacheFile;
use crate::parsers::get_parser;
use crate::scanner::apply_min_docstring_words;
use chrono::Utc;
use std::path::Path;

/// Re-parse a single file and update its cache entry.
/// Returns `true` if a warning was emitted (file unreadable / language unknown).
/// Does NOT recompute stats or write cache — callers must do that.
pub fn rescan_one(cache: &mut CacheFile, rel: &str) -> bool {
    let root = Path::new(&cache.project_root);
    let abs = root.join(rel);

    let content = match std::fs::read_to_string(&abs) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[codeskel] Warning: cannot read {}: {}", rel, e);
            return true;
        }
    };

    let lang = match detect_language(&abs) {
        Some(l) => l,
        None => {
            eprintln!("[codeskel] Warning: cannot detect language for {}", rel);
            return true;
        }
    };

    let generated = is_generated(rel, &content);
    let pr = get_parser(&lang).parse(&content);

    if let Some(entry) = cache.files.get_mut(rel) {
        let mut sigs = pr.signatures;
        let cov = apply_min_docstring_words(&mut sigs, cache.min_docstring_words);
        entry.comment_coverage = cov;
        entry.signatures = sigs;
        entry.scanned_at = Some(Utc::now().to_rfc3339());
        if generated {
            entry.skip = true;
            entry.skip_reason = Some("generated".to_string());
        }
    } else {
        eprintln!("[codeskel] Warning: {} not found in cache, skipping", rel);
        return true;
    }

    false
}

/// Recompute `cache.stats` from current `cache.files` and `cache.order`.
pub fn recompute_stats(cache: &mut CacheFile) {
    let total = cache.files.len();
    let skipped_covered = cache.files.values()
        .filter(|e| e.skip_reason.as_deref() == Some("sufficient_coverage"))
        .count();
    let skipped_generated = cache.files.values()
        .filter(|e| e.skip_reason.as_deref() == Some("generated"))
        .count();
    let to_comment = cache.order.iter()
        .filter(|p| cache.files.get(*p).map(|e| !e.skip).unwrap_or(false))
        .count();

    cache.stats = crate::models::Stats {
        total_files: total,
        skipped_covered,
        skipped_generated,
        to_comment,
    };
}

pub fn run(args: RescanArgs) -> anyhow::Result<bool> {
    let mut cache = read_cache(&args.cache_path)?;
    let cache_dir = args.cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut warnings = false;
    for file_path in &args.file_paths {
        let rel = if file_path.is_absolute() {
            match file_path.strip_prefix(Path::new(&cache.project_root)) {
                Ok(r) => r.to_string_lossy().into_owned(),
                Err(_) => file_path.to_string_lossy().into_owned(),
            }
        } else {
            file_path.to_string_lossy().into_owned()
        };

        if rescan_one(&mut cache, &rel) {
            warnings = true;
        }
    }

    recompute_stats(&mut cache);
    write_cache(&cache_dir, &cache)?;
    eprintln!("[codeskel] Rescanned {} file(s)", args.file_paths.len());
    Ok(warnings)
}
