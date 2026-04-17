use crate::cache::write_cache;
use crate::docstring::count_prose_words;
use crate::generated::is_generated;
use crate::graph::DepGraph;
use crate::models::{CacheFile, FileEntry, Language, Signature, Stats};
use crate::parsers::get_parser;
use crate::resolver::Resolver;
use crate::walker::{walk_project, WalkConfig};
use chrono::Utc;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct ScanConfig {
    pub forced_lang: Option<Language>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub min_coverage: f64,
    /// Minimum prose word count for a docstring to count as documented (0 = presence only).
    pub min_docstring_words: usize,
    pub cache_dir: Option<PathBuf>,
    pub verbose: bool,
}

pub struct ScanResult {
    pub cache_path: PathBuf,
    pub stats: Stats,
    pub detected_languages: Vec<String>,
    pub order: Vec<String>,
    pub project_root: String,
}

pub fn scan(root: &Path, cfg: &ScanConfig) -> anyhow::Result<ScanResult> {
    let root = root.canonicalize()?;

    let walk_cfg = WalkConfig {
        forced_lang: cfg.forced_lang.clone(),
        include_globs: cfg.include_globs.clone(),
        exclude_globs: cfg.exclude_globs.clone(),
    };

    let discovered = walk_project(&root, &walk_cfg)?;
    if cfg.verbose {
        eprintln!("[codeskel] Discovered {} files", discovered.len());
    }

    // Group rel_paths by language for resolver construction
    let mut paths_by_lang: HashMap<Language, Vec<String>> = HashMap::new();
    for f in &discovered {
        paths_by_lang.entry(f.language.clone()).or_default().push(f.rel_path.clone());
    }

    // Read go.mod module path if present
    let go_module = read_go_module(&root);

    // Parse all files in parallel (rayon)
    // Each element: (rel_path, language, Option<(is_generated, ParseResult)>)
    let parse_results: Vec<(String, Language, Option<(bool, crate::parsers::ParseResult)>)> =
        discovered.par_iter().map(|f| {
            let content = match std::fs::read_to_string(&f.abs_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[codeskel] Warning: cannot read {}: {}", f.rel_path, e);
                    return (f.rel_path.clone(), f.language.clone(), None);
                }
            };
            let generated = is_generated(&f.rel_path, &content);
            let parser = get_parser(&f.language);
            let parse_result = parser.parse(&content);
            (f.rel_path.clone(), f.language.clone(), Some((generated, parse_result)))
        }).collect();

    // Build resolvers per language
    let resolvers: HashMap<Language, Resolver> = paths_by_lang.iter().map(|(lang, paths)| {
        let r = Resolver::new(lang, paths, &root, go_module.as_deref());
        (lang.clone(), r)
    }).collect();

    // Build dependency graph and file entries
    let mut graph = DepGraph::new();
    let mut file_entries: HashMap<String, FileEntry> = HashMap::new();
    let mut lang_set: HashSet<String> = HashSet::new();
    let mut skipped_covered = 0usize;
    let mut skipped_generated = 0usize;

    for (rel_path, language, parse_opt) in &parse_results {
        graph.add_node(rel_path);
        lang_set.insert(language.as_str().to_string());

        let (skip, skip_reason, coverage, signatures, raw_imports, package) = match parse_opt {
            None => (true, Some("unreadable".to_string()), 0.0, vec![], vec![], None),
            Some((true, _pr)) => {
                skipped_generated += 1;
                (true, Some("generated".to_string()), 0.0, vec![], vec![], None)
            }
            Some((false, pr)) => {
                let mut sigs = pr.signatures.clone();
                let cov = apply_min_docstring_words(&mut sigs, cfg.min_docstring_words);
                let skip = cfg.min_coverage > 0.0 && cov >= cfg.min_coverage;
                if skip { skipped_covered += 1; }
                let reason = if skip { Some("sufficient_coverage".to_string()) } else { None };
                (skip, reason, cov, sigs, pr.raw_imports.clone(), pr.package.clone())
            }
        };

        // Resolve raw imports to internal file paths
        let importer_dir = Path::new(rel_path).parent()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string());
        let internal_imports: Vec<String> = raw_imports.iter()
            .filter_map(|raw| {
                resolvers.get(language)
                    .and_then(|r| r.resolve(raw, importer_dir.as_deref()))
            })
            .collect();

        // Add graph edges: rel_path depends on each internal import
        for dep in &internal_imports {
            graph.add_edge(rel_path, dep);
        }

        file_entries.insert(rel_path.clone(), FileEntry {
            path: rel_path.clone(),
            language: language.as_str().to_string(),
            package,
            comment_coverage: coverage,
            skip,
            skip_reason,
            cycle_warning: false,
            internal_imports,
            signatures,
            scanned_at: None,
        });
    }

    // Topological sort
    let (full_order, cycle_pairs) = graph.topo_sort();

    // Mark files involved in cycles
    const MAX_CYCLE_SHOWN: usize = 3;
    for (i, (a, b)) in cycle_pairs.iter().enumerate() {
        if i < MAX_CYCLE_SHOWN {
            eprintln!("[codeskel] Warning: cycle between {} and {}", a, b);
        } else if i == MAX_CYCLE_SHOWN {
            eprintln!("[codeskel] Warning: ... and {} more cycle(s)", cycle_pairs.len() - MAX_CYCLE_SHOWN);
        }
        if let Some(e) = file_entries.get_mut(a) { e.cycle_warning = true; }
        if let Some(e) = file_entries.get_mut(b) { e.cycle_warning = true; }
    }

    // Build the ordered list of non-skipped files
    let order: Vec<String> = full_order.into_iter()
        .filter(|p| file_entries.get(p).map(|e| !e.skip).unwrap_or(false))
        .collect();

    let to_comment = order.len();
    let total_files = file_entries.len();
    let mut detected_languages: Vec<String> = lang_set.into_iter().collect();
    detected_languages.sort();

    let stats = Stats { total_files, skipped_covered, skipped_generated, to_comment };

    let cache_dir = cfg.cache_dir.clone()
        .unwrap_or_else(|| root.join(".codeskel"));

    let cache = CacheFile {
        version: 1,
        scanned_at: Utc::now().to_rfc3339(),
        project_root: root.to_string_lossy().into_owned(),
        detected_languages: detected_languages.clone(),
        stats: stats.clone(),
        min_docstring_words: cfg.min_docstring_words,
        order: order.clone(),
        files: file_entries,
    };

    write_cache(&cache_dir, &cache)?;
    let cache_path = cache_dir.join("cache.json");

    if cfg.verbose {
        eprintln!("[codeskel] Cache written to {}", cache_path.display());
        eprintln!("[codeskel] {} files to comment, {} skipped", to_comment, total_files - to_comment);
    }

    Ok(ScanResult {
        cache_path,
        stats,
        detected_languages,
        order,
        project_root: root.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Write a Java file with short docstrings, scan with min_docstring_words = 10,
    /// and verify those short docs are treated as undocumented (has_docstring = false).
    #[test]
    fn test_min_docstring_words_excludes_trivial_docs() {
        let dir = tempfile::tempdir().unwrap();
        let java_file = dir.path().join("Foo.java");
        fs::write(&java_file, r#"
package com.example;

/** Gets the foo. */
public class Foo {
    /** Returns value. */
    public int getValue() { return 0; }
}
"#).unwrap();

        let result = scan(dir.path(), &ScanConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
            min_coverage: 0.0,
            min_docstring_words: 10,
            cache_dir: Some(dir.path().join(".codeskel")),
            verbose: false,
        }).unwrap();

        // Load the cache and check signatures
        let cache_path = dir.path().join(".codeskel/cache.json");
        let cache: crate::models::CacheFile =
            serde_json::from_str(&fs::read_to_string(&cache_path).unwrap()).unwrap();
        let entry = cache.files.values().next().unwrap();

        // Both docstrings are < 10 words — must be treated as undocumented
        for sig in &entry.signatures {
            assert!(!sig.has_docstring,
                "sig '{}' should have has_docstring=false (too short), got true", sig.name);
        }
        // Coverage must reflect that nothing is documented
        assert_eq!(entry.comment_coverage, 0.0,
            "coverage should be 0.0 when all docs are below word threshold");
        // File should NOT be skipped (coverage 0.0 < min_coverage 0.0 threshold irrelevant, but it should appear in to_comment)
        assert_eq!(result.stats.to_comment, 1);
    }

    /// With min_docstring_words = 0, presence-only behaviour is preserved.
    #[test]
    fn test_min_docstring_words_zero_preserves_presence_behaviour() {
        let dir = tempfile::tempdir().unwrap();
        let java_file = dir.path().join("Bar.java");
        fs::write(&java_file, r#"
package com.example;

/** Gets the bar. */
public class Bar {}
"#).unwrap();

        scan(dir.path(), &ScanConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
            min_coverage: 0.0,
            min_docstring_words: 0,
            cache_dir: Some(dir.path().join(".codeskel")),
            verbose: false,
        }).unwrap();

        let cache_path = dir.path().join(".codeskel/cache.json");
        let cache: crate::models::CacheFile =
            serde_json::from_str(&fs::read_to_string(&cache_path).unwrap()).unwrap();
        let entry = cache.files.values().next().unwrap();
        let cls = entry.signatures.iter().find(|s| s.kind == "class").unwrap();
        assert!(cls.has_docstring, "with min_docstring_words=0, any doc counts");
    }
}

/// Re-evaluate `has_docstring` on each signature based on prose word count,
/// then recompute coverage.  A `min_words` of 0 is a no-op.
pub fn apply_min_docstring_words(signatures: &mut Vec<Signature>, min_words: usize) -> f64 {
    for sig in signatures.iter_mut() {
        let words = sig.docstring_text.as_deref()
            .map(count_prose_words)
            .unwrap_or(0);
        sig.existing_word_count = words;
        if min_words > 0 && sig.has_docstring && words < min_words {
            sig.has_docstring = false;
        }
    }

    let documentable: Vec<&Signature> = signatures.iter()
        .filter(|s| matches!(s.kind.as_str(),
            "class" | "interface" | "enum" | "method" | "constructor" | "field" |
            "function" | "struct" | "trait" | "type" | "type_alias"))
        .collect();
    let documented = documentable.iter().filter(|s| s.has_docstring).count();
    let total = documentable.len();
    if total > 0 { documented as f64 / total as f64 } else { 1.0 }
}

fn read_go_module(root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(root.join("go.mod")).ok()?;
    content.lines()
        .find(|l| l.starts_with("module "))
        .map(|l| l.trim_start_matches("module ").trim().to_string())
}
