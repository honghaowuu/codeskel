use crate::error::CodeskelError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub cursor: i64,  // -1 = not started / complete
    pub current_file: Option<String>,
    /// Present only in targeted mode. Null in project-mode sessions.
    #[serde(default)]
    pub target: Option<String>,
    /// Ordered chain [dep_0, ..., dep_N-1, target]. Absent in project mode.
    #[serde(default)]
    pub chain: Option<Vec<String>>,
}

impl Default for Session {
    fn default() -> Self {
        Session { cursor: -1, current_file: None, target: None, chain: None }
    }
}

pub fn read_session(cache_dir: &Path) -> Session {
    let path = cache_dir.join("session.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str(&content) {
                Ok(session) => session,
                Err(_) => {
                    eprintln!("[codeskel] Warning: corrupt session.json, resetting");
                    Session::default()
                }
            }
        }
        Err(_) => Session::default(),
    }
}

/// Strict version of `read_session` for callers that need to surface a
/// `SESSION_CORRUPT` envelope code instead of silently resetting. Returns
/// `Ok(None)` when no session exists, `Ok(Some(_))` when one was parsed,
/// and `Err(CodeskelError::SessionCorrupt)` when the file is unparseable.
pub fn try_read_session(cache_dir: &Path) -> anyhow::Result<Option<Session>> {
    let path = cache_dir.join("session.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow::Error::new(e).context(format!(
            "Cannot read session from {}",
            path.display()
        ))),
    };
    match serde_json::from_str::<Session>(&content) {
        Ok(s) => Ok(Some(s)),
        Err(_) => Err(CodeskelError::SessionCorrupt(path).into()),
    }
}

pub fn write_session(cache_dir: &Path, session: &Session) -> anyhow::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join("session.json");
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn delete_session(cache_dir: &Path) {
    let path = cache_dir.join("session.json");
    let _ = std::fs::remove_file(path); // silently ignore if missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_missing_returns_default() {
        let dir = tempdir().unwrap();
        let session = read_session(dir.path());
        assert_eq!(session.cursor, -1);
        assert_eq!(session.current_file, None);
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = tempdir().unwrap();
        let s = Session { cursor: 3, current_file: Some("src/Foo.java".into()), target: None, chain: None };
        write_session(dir.path(), &s).unwrap();
        let back = read_session(dir.path());
        assert_eq!(back, s);
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempdir().unwrap();
        let s = Session { cursor: 0, current_file: Some("x".into()), target: None, chain: None };
        write_session(dir.path(), &s).unwrap();
        assert!(dir.path().join("session.json").exists());
        delete_session(dir.path());
        assert!(!dir.path().join("session.json").exists());
    }

    #[test]
    fn delete_missing_is_silent() {
        let dir = tempdir().unwrap();
        delete_session(dir.path()); // must not panic
    }

    #[test]
    fn read_corrupt_returns_default() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("session.json"), b"not json").unwrap();
        let session = read_session(dir.path());
        assert_eq!(session.cursor, -1);
    }

    #[test]
    fn targeted_session_roundtrip() {
        let dir = tempdir().unwrap();
        let s = Session {
            cursor: 0,
            current_file: Some("src/A.java".into()),
            target: Some("src/C.java".into()),
            chain: Some(vec!["src/A.java".into(), "src/B.java".into(), "src/C.java".into()]),
        };
        write_session(dir.path(), &s).unwrap();
        let back = read_session(dir.path());
        assert_eq!(back.target, Some("src/C.java".into()));
        let expected_chain: Vec<String> = vec!["src/A.java".into(), "src/B.java".into(), "src/C.java".into()];
        assert_eq!(back.chain.as_deref(), Some(expected_chain.as_slice()));
    }
}
