use crate::cache::read_cache;
use crate::cli::GetArgs;
use serde_json::json;

pub fn run(args: GetArgs) -> anyhow::Result<bool> {
    let cache = read_cache(&args.cache_path)?;

    let mode_count = args.index.is_some() as u8
        + args.path.is_some() as u8
        + args.deps.is_some() as u8;

    if mode_count == 0 {
        anyhow::bail!("One of --index, --path, or --deps is required");
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
