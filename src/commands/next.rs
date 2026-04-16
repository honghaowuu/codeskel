use crate::cache::{read_cache, write_cache};
use crate::cli::NextArgs;
use crate::commands::rescan::{rescan_one, recompute_stats};
use crate::models::{FileEntry, Signature};
use crate::session::{delete_session, read_session, write_session, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    pub signatures: Vec<Signature>,
}

/// The structured output from a `next` call. Used both for JSON printing and for
/// test assertions (via `run_and_capture`).
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,   // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<FileEntry>,
    pub deps: Vec<DepEntry>,
}

pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let output = run_and_capture(args)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(false)
}

/// Core logic — returns a `NextOutput` instead of printing, so tests can assert on it.
pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    let cache_dir = args.cache.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&args.cache)?;
    let session = read_session(&cache_dir);

    // ── Step 1: rescan previous file if session is active ──────────────────
    if session.cursor >= 0 {
        match session.current_file.as_deref() {
            None => {
                eprintln!("[codeskel] Warning: session has active cursor but no current_file; skipping rescan.");
            }
            Some(prev_file) => {
                // Sanity check: warn if session drifted from cache.order
                let expected = cache.order.get(session.cursor as usize).map(|s| s.as_str());
                if expected != Some(prev_file) {
                    eprintln!(
                        "[codeskel] Warning: session mismatch — expected {:?}, found {:?}. Rescanning session file.",
                        expected, prev_file
                    );
                }

                rescan_one(&mut cache, prev_file);
                recompute_stats(&mut cache);
                write_cache(&cache_dir, &cache)?;
            }
        }
    }

    // ── Step 2: advance cursor ─────────────────────────────────────────────
    let next_cursor = (session.cursor + 1) as usize;

    if next_cursor >= cache.order.len() {
        delete_session(&cache_dir);
        return Ok(NextOutput {
            done: true,
            mode: "project".into(),
            index: None,
            remaining: 0,
            file: None,
            deps: vec![],
        });
    }

    // ── Step 3: save session and build response ────────────────────────────
    let rel = cache.order[next_cursor].clone();
    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(rel.clone()),
        target: None,
        chain: None,
    })?;

    let file_entry = cache.files.get(&rel)
        .ok_or_else(|| anyhow::anyhow!("File {} in order but missing from files map", rel))?
        .clone();

    let deps: Vec<DepEntry> = file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| DepEntry {
                path: dep_entry.path.clone(),
                signatures: dep_entry.signatures.clone(),
            })
        })
        .collect();

    let remaining = cache.order.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "project".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(file_entry),
        deps,
    })
}
