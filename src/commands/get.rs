use crate::cache::read_cache;
use crate::cli::GetArgs;
use serde_json::json;
use std::collections::{HashSet, VecDeque};

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
        if args.index.is_some() || args.path.is_some() || args.deps.is_some() {
            anyhow::bail!("--refs cannot be combined with --index, --path, or --deps");
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

    println!("{}", serde_json::to_string_pretty(entry)?);
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
    println!("{}", serde_json::to_string_pretty(&output)?);
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
    println!("{}", serde_json::to_string_pretty(entry)?);
    Ok(false)
}

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

    let output = json!({
        "for": file_path,
        "dependencies": dependencies,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

fn get_refs(_cache: &crate::models::CacheFile, _file_path: &str) -> anyhow::Result<bool> {
    anyhow::bail!("--refs not yet implemented")
}
