use crate::cache::{read_cache, write_cache};
use crate::cli::RescanArgs;
use crate::generated::is_generated;
use crate::lang::detect_language;
use crate::parsers::get_parser;
use chrono::Utc;
use std::path::Path;

pub fn run(args: RescanArgs) -> anyhow::Result<bool> {
    let mut cache = read_cache(&args.cache_path)?;
    let root = Path::new(&cache.project_root).to_path_buf();
    let cache_dir = args.cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut warnings = false;

    for file_path in &args.file_paths {
        // Make relative path
        let rel = if file_path.is_absolute() {
            match file_path.strip_prefix(&root) {
                Ok(r) => r.to_string_lossy().into_owned(),
                Err(_) => file_path.to_string_lossy().into_owned(),
            }
        } else {
            file_path.to_string_lossy().into_owned()
        };

        let abs = root.join(&rel);
        let content = match std::fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[codeskel] Warning: cannot read {}: {}", rel, e);
                warnings = true;
                continue;
            }
        };

        let lang = match detect_language(&abs) {
            Some(l) => l,
            None => {
                eprintln!("[codeskel] Warning: cannot detect language for {}", rel);
                warnings = true;
                continue;
            }
        };

        let generated = is_generated(&rel, &content);
        let pr = get_parser(&lang).parse(&content);

        if let Some(entry) = cache.files.get_mut(&rel) {
            entry.comment_coverage = pr.coverage;
            entry.signatures = pr.signatures;
            entry.scanned_at = Some(Utc::now().to_rfc3339());
            if generated {
                entry.skip = true;
                entry.skip_reason = Some("generated".to_string());
            }
        } else {
            eprintln!("[codeskel] Warning: {} not found in cache, skipping", rel);
            warnings = true;
        }
    }

    // Recompute stats
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

    write_cache(&cache_dir, &cache)?;
    eprintln!("[codeskel] Rescanned {} file(s)", args.file_paths.len());
    Ok(warnings)
}
