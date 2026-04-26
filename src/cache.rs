use crate::error::CodeskelError;
use crate::models::CacheFile;
use std::path::Path;
use anyhow::Context;

pub fn write_cache(cache_dir: &Path, cache: &CacheFile) -> anyhow::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join("cache.json");
    let json = serde_json::to_string_pretty(cache)?;
    atomic_write(&path, json.as_bytes())
        .with_context(|| format!("Cannot write cache to {}", path.display()))?;
    Ok(())
}

/// Write `contents` to `path` atomically: write to a sibling tempfile, then
/// rename. A SIGKILL between write and rename leaves the tempfile but never a
/// half-written `path`, so concurrent readers always see a complete file.
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".into());
    let tmp = parent.join(format!(".{}.tmp.{}", file_name, std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp, path)
}

/// Verify that `cache.project_root` (canonicalized) matches the given
/// canonicalized cwd. Returns `Err(ProjectRootMismatch)` on mismatch — used by
/// `next` to refuse caches built for a different project. If either path can't
/// be canonicalized (e.g. the cache root no longer exists), falls back to the
/// raw stored string for the comparison.
pub fn verify_project_root_matches(cache: &CacheFile, cwd: &Path) -> anyhow::Result<()> {
    let cwd_canonical = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let cache_root_path = std::path::Path::new(&cache.project_root);
    let cache_canonical = cache_root_path
        .canonicalize()
        .unwrap_or_else(|_| cache_root_path.to_path_buf());
    if cwd_canonical != cache_canonical {
        return Err(CodeskelError::ProjectRootMismatch {
            cache_root: cache.project_root.clone(),
            cwd: cwd.display().to_string(),
        }
        .into());
    }
    Ok(())
}

pub fn read_cache(cache_path: &Path) -> anyhow::Result<CacheFile> {
    let content = match std::fs::read_to_string(cache_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CodeskelError::CacheNotFound(cache_path.to_path_buf()).into());
        }
        Err(e) => {
            return Err(anyhow::Error::new(e).context(format!(
                "Cannot read cache from {}",
                cache_path.display()
            )));
        }
    };
    let cache: CacheFile = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse cache at {}", cache_path.display()))?;
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CacheFile, Stats};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn dummy_cache() -> CacheFile {
        CacheFile {
            version: 1,
            scanned_at: "2026-04-02T10:00:00Z".into(),
            project_root: "/tmp/proj".into(),
            detected_languages: vec!["java".into()],
            stats: Stats { total_files: 1, skipped_covered: 0, skipped_generated: 0, to_comment: 1 },
            min_docstring_words: 0,
            order: vec!["src/Foo.java".into()],
            files: HashMap::new(),
        }
    }

    #[test]
    fn test_write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let cache = dummy_cache();
        write_cache(dir.path(), &cache).unwrap();
        let back = read_cache(&dir.path().join("cache.json")).unwrap();
        assert_eq!(back.version, 1);
        assert_eq!(back.detected_languages, vec!["java"]);
        assert_eq!(back.order, vec!["src/Foo.java"]);
    }

    #[test]
    fn test_read_missing_file() {
        let result = read_cache(Path::new("/nonexistent/cache.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_creates_cache_dir() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("nested").join(".codeskel");
        write_cache(&subdir, &dummy_cache()).unwrap();
        assert!(subdir.join("cache.json").exists());
    }
}
