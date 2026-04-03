use crate::models::Language;
use std::collections::HashMap;
use std::path::Path;

/// Maps raw import strings to relative project file paths, per language.
pub struct Resolver {
    /// file_index: lookup key → relative path
    file_index: HashMap<String, String>,
}

impl Resolver {
    pub fn new(
        lang: &Language,
        rel_paths: &[String],
        _project_root: &Path,
        go_module: Option<&str>,
    ) -> Self {
        let mut file_index = HashMap::new();
        match lang {
            Language::Java => {
                for p in rel_paths {
                    if let Some(key) = java_path_to_fqn(p) {
                        file_index.insert(key, p.clone());
                    }
                }
            }
            Language::Python => {
                for p in rel_paths {
                    let key = python_path_to_module(p);
                    file_index.insert(key, p.clone());
                }
            }
            Language::TypeScript | Language::JavaScript => {
                for p in rel_paths {
                    let without_ext = strip_extension(p);
                    file_index.insert(without_ext.clone(), p.clone());
                    file_index.insert(p.clone(), p.clone());
                }
            }
            Language::Go => {
                if let Some(module) = go_module {
                    // Group files by package directory for deterministic resolution
                    let mut pkg_files: HashMap<String, Vec<String>> = HashMap::new();
                    for p in rel_paths {
                        let pkg_path = go_pkg_path(p);
                        if !pkg_path.is_empty() {
                            pkg_files.entry(pkg_path).or_default().push(p.clone());
                        }
                    }
                    // Map import path to the first (sorted) file in the package
                    for (pkg_path, mut files) in pkg_files {
                        files.sort();
                        let import_path = format!("{}/{}", module.trim_end_matches('/'), pkg_path);
                        file_index.insert(import_path, files[0].clone());
                    }
                }
                // Without go.mod, all Go imports are external — index nothing
            }
            Language::Rust => {
                for p in rel_paths {
                    let key = rust_path_to_module(p);
                    file_index.insert(key, p.clone());
                }
            }
            Language::CSharp => {
                for p in rel_paths {
                    if let Some(key) = csharp_path_to_namespace(p) {
                        file_index.insert(key, p.clone());
                    }
                }
            }
            Language::Cpp => {
                for p in rel_paths {
                    file_index.insert(p.clone(), p.clone());
                    if let Some(name) = Path::new(p).file_name().and_then(|n| n.to_str()) {
                        file_index.entry(name.to_string()).or_insert(p.clone());
                    }
                }
            }
            Language::Ruby => {
                for p in rel_paths {
                    let without_ext = strip_extension(p);
                    file_index.insert(without_ext, p.clone());
                    file_index.insert(p.clone(), p.clone());
                }
            }
        }
        Self { file_index }
    }

    /// Resolve a raw import to a relative file path.
    /// `importer_dir`: directory of the importing file (for relative TS/JS/Ruby imports)
    pub fn resolve(&self, raw: &str, importer_dir: Option<&str>) -> Option<String> {
        // Direct lookup
        if let Some(v) = self.file_index.get(raw) {
            return Some(v.clone());
        }
        // Relative TS/JS/Ruby imports: join with importer dir and normalize
        if let Some(dir) = importer_dir {
            if raw.starts_with("./") || raw.starts_with("../") {
                let joined = Path::new(dir).join(raw);
                let normalized = normalize_path(&joined.to_string_lossy());
                if let Some(v) = self.file_index.get(&normalized) {
                    return Some(v.clone());
                }
            }
        }
        None
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn strip_extension(p: &str) -> String {
    let path = Path::new(p);
    match (path.parent().and_then(|p| p.to_str()), path.file_stem().and_then(|s| s.to_str())) {
        (Some(parent), Some(stem)) if !parent.is_empty() => format!("{}/{}", parent, stem),
        (_, Some(stem)) => stem.to_string(),
        _ => p.to_string(),
    }
}

fn normalize_path(p: &str) -> String {
    let mut parts: Vec<&str> = vec![];
    for component in p.split('/') {
        match component {
            ".." => { parts.pop(); }
            "." | "" => {}
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// "src/main/java/com/example/model/User.java" → "com.example.model.User"
fn java_path_to_fqn(rel_path: &str) -> Option<String> {
    let path = Path::new(rel_path);
    if path.extension()?.to_str()? != "java" { return None; }
    let s = rel_path.replace('/', ".");
    let s = s.strip_suffix(".java").unwrap_or(&s);
    // Strip common Java source root prefixes
    for prefix in &["src.main.java.", "src.java.", "main.java.", "src."] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return Some(rest.to_string());
        }
    }
    Some(s.to_string())
}

/// "myapp/models.py" → "myapp.models"
/// "myapp/models/__init__.py" → "myapp.models"
fn python_path_to_module(rel_path: &str) -> String {
    let s = if rel_path.ends_with("/__init__.py") {
        rel_path.trim_end_matches("/__init__.py")
    } else {
        rel_path.trim_end_matches(".py")
    };
    s.replace('/', ".")
}

/// "src/models/user.rs" → "crate::models::user"
/// "src/models.rs" → "crate::models"
fn rust_path_to_module(rel_path: &str) -> String {
    let s = rel_path
        .trim_start_matches("src/")
        .trim_end_matches("/mod.rs")
        .trim_end_matches(".rs");
    format!("crate::{}", s.replace('/', "::"))
}

/// Get the package directory path from a Go file path
/// "pkg/models/user.go" → "pkg/models"
fn go_pkg_path(rel_path: &str) -> String {
    Path::new(rel_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string()
}

/// "Models/UserService.cs" → Some("Models.UserService")
fn csharp_path_to_namespace(rel_path: &str) -> Option<String> {
    let stem = Path::new(rel_path).file_stem()?.to_str()?.to_string();
    let dir = Path::new(rel_path).parent()?.to_str()?.replace('/', ".");
    Some(if dir.is_empty() { stem } else { format!("{}.{}", dir, stem) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_java_resolution() {
        let r = Resolver::new(
            &Language::Java,
            &["src/main/java/com/example/model/User.java".to_string()],
            &PathBuf::from("/proj"),
            None,
        );
        assert_eq!(
            r.resolve("com.example.model.User", None),
            Some("src/main/java/com/example/model/User.java".to_string())
        );
    }

    #[test]
    fn test_python_resolution() {
        let r = Resolver::new(
            &Language::Python,
            &["myapp/models.py".to_string()],
            &PathBuf::from("/proj"),
            None,
        );
        assert_eq!(
            r.resolve("myapp.models", None),
            Some("myapp/models.py".to_string())
        );
    }

    #[test]
    fn test_ts_relative_resolution() {
        let r = Resolver::new(
            &Language::TypeScript,
            &["src/services/user.service.ts".to_string()],
            &PathBuf::from("/proj"),
            None,
        );
        // "./services/user.service" from "src/app.ts" should resolve to "src/services/user.service.ts"
        assert_eq!(
            r.resolve("./services/user.service", Some("src")),
            Some("src/services/user.service.ts".to_string())
        );
    }

    #[test]
    fn test_go_resolution() {
        let r = Resolver::new(
            &Language::Go,
            &["pkg/models/user.go".to_string()],
            &PathBuf::from("/proj"),
            Some("github.com/myorg/myapp"),
        );
        assert_eq!(
            r.resolve("github.com/myorg/myapp/pkg/models", None),
            Some("pkg/models/user.go".to_string())
        );
    }

    #[test]
    fn test_cpp_resolution() {
        let r = Resolver::new(
            &Language::Cpp,
            &["mylib/foo.h".to_string()],
            &PathBuf::from("/proj"),
            None,
        );
        assert_eq!(r.resolve("mylib/foo.h", None), Some("mylib/foo.h".to_string()));
        // Also by basename
        assert_eq!(r.resolve("foo.h", None), Some("mylib/foo.h".to_string()));
    }

    #[test]
    fn test_unresolvable_returns_none() {
        let r = Resolver::new(
            &Language::Java,
            &["src/Foo.java".to_string()],
            &PathBuf::from("/proj"),
            None,
        );
        assert_eq!(r.resolve("com.external.Library", None), None);
    }
}
