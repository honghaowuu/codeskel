use crate::cache::read_cache;
use crate::cli::GetArgs;
use crate::models::Language;
use serde_json::json;
use std::collections::{HashSet, VecDeque};
use std::str::FromStr;

pub fn run(args: GetArgs) -> anyhow::Result<bool> {
    let cache = read_cache(&args.cache_path)?;

    // --chain (with optional --index modifier)
    if let Some(chain_path) = &args.chain {
        if args.path.is_some() || args.deps.is_some() || args.refs.is_some() {
            anyhow::bail!("--chain cannot be combined with --path, --deps, or --refs");
        }
        return if let Some(idx) = args.index {
            get_chain_entry(&cache, chain_path, idx)
        } else {
            get_chain_count(&cache, chain_path)
        };
    }

    // --refs
    if let Some(refs_path) = &args.refs {
        if args.chain.is_some() || args.index.is_some() || args.path.is_some() || args.deps.is_some() {
            anyhow::bail!("--refs cannot be combined with --chain, --index, --path, or --deps");
        }
        return get_refs(&cache, refs_path);
    }

    // Existing: --index, --path, --deps
    let mode_count = args.index.is_some() as u8
        + args.path.is_some() as u8
        + args.deps.is_some() as u8;

    if mode_count == 0 {
        anyhow::bail!("One of --index, --path, --deps, --chain, or --refs is required");
    }
    if mode_count > 1 {
        anyhow::bail!("Only one of --index, --path, or --deps may be used at a time");
    }

    if let Some(deps_path) = &args.deps {
        return get_deps(&cache, deps_path);
    }

    let entry = if let Some(idx) = args.index {
        let rel = cache.order.get(idx).ok_or_else(|| {
            anyhow::anyhow!(
                "Index {} out of range (cache has {} items in order)",
                idx,
                cache.order.len()
            )
        })?;
        cache.files.get(rel).ok_or_else(|| anyhow::anyhow!("File {} not in cache", rel))?
    } else {
        let path = args.path.as_ref().unwrap();
        cache.files.get(path)
            .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", path))?
    };

    println!("{}", serde_json::to_string(entry)?);
    Ok(false)
}

/// Returns the transitive dependency list for `file_path`, in the same order as `cache.order`
/// (leaves/files-with-no-deps first). `file_path` itself is excluded from the result.
///
/// Only includes files present in `cache.order` (non-skipped files). Transitive deps that were
/// skipped by the scanner (e.g., already well-commented) are silently excluded — this matches
/// `cache.order` semantics and means `count` reflects only the files that need commenting.
pub fn chain_order(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<Vec<String>> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    // BFS starting from file_path's imports — file_path itself is excluded
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for dep in &entry.internal_imports {
        if visited.insert(dep.clone()) {
            queue.push_back(dep.clone());
        }
    }
    while let Some(current) = queue.pop_front() {
        if let Some(dep_entry) = cache.files.get(&current) {
            for dep in &dep_entry.internal_imports {
                if visited.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    // Filter cache.order (already topo-sorted, leaves first) to visited set
    let chain: Vec<String> = cache.order.iter()
        .filter(|p| visited.contains(*p))
        .cloned()
        .collect();

    Ok(chain)
}

fn get_chain_count(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let chain = chain_order(cache, file_path)?;
    let output = json!({ "for": file_path, "count": chain.len() });
    println!("{}", serde_json::to_string(&output)?);
    Ok(false)
}

fn get_chain_entry(cache: &crate::models::CacheFile, file_path: &str, index: usize) -> anyhow::Result<bool> {
    let chain = chain_order(cache, file_path)?;
    let dep_path = chain.get(index)
        .ok_or_else(|| anyhow::anyhow!(
            "Index {} out of range (chain has {} entries for '{}')",
            index, chain.len(), file_path
        ))?;
    // dep_path came from cache.order, so it is guaranteed to be in cache.files
    let entry = cache.files.get(dep_path)
        .expect("dep from cache.order must exist in cache.files");
    println!("{}", serde_json::to_string(entry)?);
    Ok(false)
}

const REVERSE_DEP_KINDS: &[&str] = &["interface", "abstract_class", "annotation"];
const MAX_REVERSE_DEPS: usize = 5;

fn get_deps(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    let dependencies: Vec<serde_json::Value> = entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| {
                json!({
                    "path": dep_entry.path,
                    "signatures": dep_entry.signatures,
                })
            })
        })
        .collect();

    let mut output = json!({
        "for": file_path,
        "dependencies": dependencies,
    });

    if REVERSE_DEP_KINDS.contains(&entry.file_kind.as_str()) {
        let reverse_dep_signatures: Vec<serde_json::Value> = entry.reverse_deps.iter()
            .take(MAX_REVERSE_DEPS)
            .filter_map(|rdep| {
                cache.files.get(rdep).map(|rdep_entry| {
                    json!({
                        "path": rdep_entry.path,
                        "signatures": rdep_entry.signatures,
                    })
                })
            })
            .collect();
        output["reverse_dep_signatures"] = serde_json::json!(reverse_dep_signatures);
    }

    println!("{}", serde_json::to_string(&output)?);
    Ok(false)
}

/// Builds the refs map for `file_path`: dep_file_path → [symbol names referenced].
/// Returns an error if the file is not in cache or its source cannot be read.
pub fn compute_refs(
    cache: &crate::models::CacheFile,
    file_path: &str,
) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
    let entry = cache.files.get(file_path)
        .ok_or_else(|| anyhow::anyhow!("Path '{}' not found in cache", file_path))?;

    // Build import_map: simple_name → dep_file_path
    // simple_name = filename stem (strip .java / .py / etc.)
    let mut import_map = std::collections::HashMap::new();
    for dep_path in &entry.internal_imports {
        if let Some(stem) = std::path::Path::new(dep_path)
            .file_stem()
            .and_then(|s| s.to_str())
        {
            import_map.insert(stem.to_string(), dep_path.clone());
        }
    }

    // NOTE: file_stem-as-simple-name works for Java (filename == class name convention).
    // For other languages, this assumption may not hold.

    // Build dep_sigs: dep_file_path → [all sig names regardless of kind]
    let mut dep_sigs = std::collections::HashMap::new();
    for dep_path in &entry.internal_imports {
        if let Some(dep_entry) = cache.files.get(dep_path) {
            let names: Vec<String> = dep_entry.signatures.iter()
                .map(|s| s.name.clone())
                .collect();
            dep_sigs.insert(dep_path.clone(), names);
        }
    }

    // No internal imports → empty refs
    if import_map.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Read source from disk
    let source_path = std::path::Path::new(&cache.project_root).join(file_path);
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", source_path.display(), e))?;

    // Dispatch to language-specific analyzer
    let lang = Language::from_str(&entry.language)
        .map_err(|e| anyhow::anyhow!("Unknown language '{}': {}", entry.language, e))?;

    match crate::refs::get_refs_analyzer(&lang) {
        Some(analyzer) => Ok(analyzer.extract_refs(&source, &import_map, &dep_sigs)),
        None => {
            eprintln!("[codeskel] --refs: language '{}' not yet supported; returning empty refs",
                entry.language);
            Ok(std::collections::HashMap::new())
        }
    }
}

fn get_refs(cache: &crate::models::CacheFile, file_path: &str) -> anyhow::Result<bool> {
    let refs = compute_refs(cache, file_path)?;
    let output = json!({ "for": file_path, "refs": refs });
    println!("{}", serde_json::to_string(&output)?);
    Ok(false)
}
