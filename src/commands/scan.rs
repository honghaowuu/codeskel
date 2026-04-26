use crate::cli::ScanArgs;
use crate::error::CodeskelError;
use crate::lang::lang_from_str;
use crate::models::ScanSummary;
use crate::scanner::{scan, ScanConfig};
use crate::session::delete_session;

pub fn run(args: ScanArgs) -> anyhow::Result<bool> {
    if !args.project_root.is_dir() {
        return Err(CodeskelError::ProjectRootMissing(args.project_root.clone()).into());
    }

    // Validate --lang if provided
    let forced_lang = match &args.lang {
        Some(s) => {
            let l = lang_from_str(s)
                .ok_or_else(|| anyhow::anyhow!(
                    "Unknown language '{}'. Valid: java, python, ts, js, go, rust, cs, cpp, ruby", s
                ))?;
            Some(l)
        }
        None => None,
    };

    // The scan walks the project before we know the final cache_dir, so the
    // lock has to be acquired against the resolved path. ScanConfig.cache_dir
    // (if Some) is the destination; otherwise it falls back to
    // `<project_root>/.codeskel/`. Mirror that here so the lock covers the
    // window between scan() finishing and the session being deleted.
    let lock_dir = args.cache_dir
        .clone()
        .unwrap_or_else(|| args.project_root.join(".codeskel"));
    let _lock = crate::lockfile::lock_cache_dir(&lock_dir)?;

    let result = scan(
        &args.project_root,
        &ScanConfig {
            forced_lang,
            include_globs: args.include,
            exclude_globs: args.exclude,
            min_coverage: args.min_coverage,
            min_docstring_words: args.min_docstring_words,
            cache_dir: args.cache_dir,
            verbose: args.verbose,
        },
    )?;

    if let Some(cache_dir) = result.cache_path.parent() {
        delete_session(cache_dir);
    }

    let summary = ScanSummary {
        project_root: result.project_root,
        detected_languages: result.detected_languages,
        cache: result.cache_path.to_string_lossy().into_owned(),
        stats: result.stats,
    };

    println!("{}", crate::envelope::format_ok(serde_json::to_value(&summary)?));
    Ok(false)
}
