use crate::cli::ScanArgs;
use crate::lang::lang_from_str;
use crate::models::ScanSummary;
use crate::scanner::{scan, ScanConfig};
use crate::session::delete_session;

pub fn run(args: ScanArgs) -> anyhow::Result<bool> {
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

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(false)
}
