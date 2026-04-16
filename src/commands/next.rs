use crate::cache::{read_cache, write_cache};
use crate::cli::NextArgs;
use crate::commands::get::chain_order;
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
    pub mode: String,  // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<FileEntry>,
    pub deps: Vec<DepEntry>,
}

pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let output = run_and_capture(args)?;
    println!("{}", serde_json::to_string(&output)?);
    Ok(false)
}

/// Core logic — returns a `NextOutput` instead of printing, so tests can assert on it.
pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    match args.target {
        Some(target) => run_targeted(args.cache, target),
        None => run_project(args.cache),
    }
}

fn run_project(cache_path: std::path::PathBuf) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;
    let session = read_session(&cache_dir);

    // Mismatch: session was in targeted mode → warn and restart project mode from index 0
    let was_targeted = session.target.is_some();
    if was_targeted {
        eprintln!("[codeskel] Warning: session was in targeted mode; restarting project-mode session.");
    }

    // Rescan previous file if session is active and was in project mode
    if session.cursor >= 0 && !was_targeted {
        match session.current_file.as_deref() {
            None => {
                eprintln!("[codeskel] Warning: session has active cursor but no current_file; skipping rescan.");
            }
            Some(prev_file) => {
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

    let next_cursor = if was_targeted {
        0
    } else {
        (session.cursor + 1) as usize
    };

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

    let deps = build_deps(&cache, &file_entry);
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

fn run_targeted(cache_path: std::path::PathBuf, target: String) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;

    // Error if target not in cache
    if !cache.files.contains_key(&target) {
        anyhow::bail!("target '{}' not found in cache — run codeskel scan first", target);
    }

    let session = read_session(&cache_dir);

    // Detect mode mismatch (different target or was project mode with active cursor)
    let is_mismatch = session.target.as_deref() != Some(target.as_str());
    if is_mismatch && session.cursor >= 0 {
        if let Some(prev_target) = &session.target {
            eprintln!("[codeskel] Warning: session was targeting '{}'; restarting for '{}'.", prev_target, target);
        } else {
            eprintln!("[codeskel] Warning: session was in project mode; restarting as targeted session for '{}'.", target);
        }
    }

    // Bootstrap: no session, done/fresh (cursor < 0), or mismatch
    if is_mismatch || session.cursor < 0 {
        let deps_chain = chain_order(&cache, &target)?;
        let mut chain = deps_chain;
        chain.push(target.clone());

        let first = chain[0].clone();
        write_session(&cache_dir, &Session {
            cursor: 0,
            current_file: Some(first.clone()),
            target: Some(target.clone()),
            chain: Some(chain.clone()),
        })?;

        let file_entry = cache.files.get(&first)
            .ok_or_else(|| anyhow::anyhow!("File {} in chain but missing from files map", first))?
            .clone();

        let deps = build_deps(&cache, &file_entry);
        let remaining = chain.len() - 1;

        return Ok(NextOutput {
            done: false,
            mode: "targeted".into(),
            index: Some(0),
            remaining,
            file: Some(file_entry),
            deps,
        });
    }

    // Subsequent call: rescan current_file, advance cursor
    let chain = session.chain.as_ref()
        .ok_or_else(|| anyhow::anyhow!("session.chain missing in targeted mode"))?
        .clone();

    if let Some(prev_file) = session.current_file.as_deref() {
        if cache.files.contains_key(prev_file) {
            rescan_one(&mut cache, prev_file);
            recompute_stats(&mut cache);
            write_cache(&cache_dir, &cache)?;
        } else {
            eprintln!("[codeskel] Warning: '{}' no longer in cache; skipping rescan.", prev_file);
        }
    } else {
        eprintln!("[codeskel] Warning: session has active cursor but no current_file; skipping rescan.");
    }

    // Advance past any chain entries that are no longer in cache
    let mut next_cursor = (session.cursor + 1) as usize;
    loop {
        if next_cursor >= chain.len() {
            delete_session(&cache_dir);
            return Ok(NextOutput {
                done: true,
                mode: "targeted".into(),
                index: None,
                remaining: 0,
                file: None,
                deps: vec![],
            });
        }
        let candidate = &chain[next_cursor];
        if cache.files.contains_key(candidate.as_str()) {
            break; // found a valid entry
        }
        eprintln!("[codeskel] Warning: chain entry '{}' no longer in cache; skipping.", candidate);
        next_cursor += 1;
    }
    let next_file = chain[next_cursor].clone();

    write_session(&cache_dir, &Session {
        cursor: next_cursor as i64,
        current_file: Some(next_file.clone()),
        target: Some(target.clone()),
        chain: Some(chain.clone()),
    })?;

    let file_entry = cache.files.get(&next_file)
        .ok_or_else(|| anyhow::anyhow!("File {} in chain but missing from files map", next_file))?
        .clone();

    let deps = build_deps(&cache, &file_entry);
    let remaining = chain.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "targeted".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(file_entry),
        deps,
    })
}

fn build_deps(cache: &crate::models::CacheFile, file_entry: &FileEntry) -> Vec<DepEntry> {
    file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| DepEntry {
                path: dep_entry.path.clone(),
                signatures: dep_entry.signatures.clone(),
            })
        })
        .collect()
}
