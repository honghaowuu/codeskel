use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Session {
    pub cursor: i64,  // -1 = not started / complete
    pub current_file: Option<String>,
}

pub fn read_session(cache_dir: &Path) -> Session {
    let path = cache_dir.join("session.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Session { cursor: -1, current_file: None },
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
        let s = Session { cursor: 3, current_file: Some("src/Foo.java".into()) };
        write_session(dir.path(), &s).unwrap();
        let back = read_session(dir.path());
        assert_eq!(back, s);
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempdir().unwrap();
        let s = Session { cursor: 0, current_file: Some("x".into()) };
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
}
