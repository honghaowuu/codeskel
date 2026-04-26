use std::fmt;
use std::path::PathBuf;

/// A failure that the `/comment` skill (or other callers) will branch on
/// programmatically. Wrap with `anyhow!(CodeskelError::Foo(..))` at the call
/// site; `main.rs` downcasts and emits the envelope's `error.code` field.
#[derive(Debug)]
pub enum CodeskelError {
    /// Cache file referenced by the caller does not exist.
    CacheNotFound(PathBuf),
    /// `session.json` exists but cannot be parsed.
    SessionCorrupt(PathBuf),
    /// A command requiring a project root was given a path that doesn't exist.
    ProjectRootMissing(PathBuf),
    /// Caller asked about a file that the cache doesn't know about.
    TargetNotInTree(String),
}

impl CodeskelError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::CacheNotFound(_) => "CACHE_NOT_FOUND",
            Self::SessionCorrupt(_) => "SESSION_CORRUPT",
            Self::ProjectRootMissing(_) => "PROJECT_ROOT_MISSING",
            Self::TargetNotInTree(_) => "TARGET_NOT_IN_TREE",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::CacheNotFound(_) => Some("run `codeskel scan <project_root>` first"),
            Self::SessionCorrupt(_) => Some("delete the file and rerun"),
            Self::ProjectRootMissing(_) => None,
            Self::TargetNotInTree(_) => Some("run `codeskel scan` after the file was added"),
        }
    }
}

impl fmt::Display for CodeskelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheNotFound(p) => write!(f, "cache not found at {}", p.display()),
            Self::SessionCorrupt(p) => write!(f, "session file at {} is corrupt", p.display()),
            Self::ProjectRootMissing(p) => write!(f, "project root not found: {}", p.display()),
            Self::TargetNotInTree(t) => write!(f, "'{}' not found in cache", t),
        }
    }
}

impl std::error::Error for CodeskelError {}
