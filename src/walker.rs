use crate::lang::detect_language;
use crate::models::Language;
use anyhow::Context;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct WalkConfig {
    pub forced_lang: Option<Language>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
}

const DEFAULT_EXCLUDES: &[&str] = &[
    "**/test/**",
    "**/tests/**",
    "**/*Test.java",
    "**/node_modules/**",
    "**/vendor/**",
    "**/.git/**",
    "**/target/**",
    "**/build/**",
    "**/dist/**",
];

pub struct DiscoveredFile {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub language: Language,
}

pub fn walk_project(root: &Path, cfg: &WalkConfig) -> anyhow::Result<Vec<DiscoveredFile>> {
    let exclude_set = build_glob_set(&cfg.exclude_globs, DEFAULT_EXCLUDES)?;
    let include_set = if cfg.include_globs.is_empty() {
        None
    } else {
        Some(build_glob_set(&cfg.include_globs, &[])?)
    };

    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[codeskel] Warning: {}", e);
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = entry.path().to_path_buf();
        let rel = abs
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        if exclude_set.is_match(&rel) {
            continue;
        }
        if let Some(ref inc) = include_set {
            if !inc.is_match(&rel) {
                continue;
            }
        }

        let lang = if let Some(ref l) = cfg.forced_lang {
            l.clone()
        } else if let Some(l) = detect_language(&abs) {
            l
        } else {
            continue;
        };

        files.push(DiscoveredFile { abs_path: abs, rel_path: rel, language: lang });
    }
    Ok(files)
}

fn build_glob_set(user: &[String], defaults: &[&str]) -> anyhow::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for g in defaults.iter().map(|s| s.to_string()).chain(user.iter().cloned()) {
        builder.add(Glob::new(&g).with_context(|| format!("invalid glob pattern: {}", g))?);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_walk_discovers_java() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("Foo.java"), "public class Foo {}").unwrap();
        fs::write(src.join("README.md"), "# readme").unwrap();

        let files = walk_project(dir.path(), &WalkConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
        }).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].language, Language::Java);
    }

    #[test]
    fn test_excludes_test_dir() {
        let dir = tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir(&tests_dir).unwrap();
        fs::write(tests_dir.join("FooTest.java"), "class FooTest {}").unwrap();

        let files = walk_project(dir.path(), &WalkConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec![],
        }).unwrap();

        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_custom_exclude() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("main.py"), "print('hi')").unwrap();
        fs::write(dir.path().join("skip.py"), "print('skip')").unwrap();

        let files = walk_project(dir.path(), &WalkConfig {
            forced_lang: None,
            include_globs: vec![],
            exclude_globs: vec!["**/skip.py".to_string()],
        }).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].rel_path.contains("main.py"));
    }
}
