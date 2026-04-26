use crate::cache::{read_cache, write_cache};
use crate::cli::NextArgs;
use crate::commands::get::chain_order;
use crate::commands::rescan::{rescan_one, recompute_stats};
use crate::error::CodeskelError;
use crate::models::{FileEntry, Param, Signature};
use crate::session::{delete_session, try_read_session, write_session, Session};
use serde::{Deserialize, Serialize};

fn is_zero(n: &usize) -> bool { *n == 0 }

/// Signature stripped for dep context — no `has_docstring` or `line`,
/// since Claude uses dep signatures for understanding, not for documenting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepSignature {
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub modifiers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<Param>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub throws: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub implements: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub annotations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring_text: Option<String>,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub existing_word_count: usize,
}

impl From<&Signature> for DepSignature {
    fn from(sig: &Signature) -> Self {
        DepSignature {
            kind: sig.kind.clone(),
            name: sig.name.clone(),
            modifiers: sig.modifiers.clone(),
            params: sig.params.clone(),
            return_type: sig.return_type.clone(),
            throws: sig.throws.clone(),
            extends: sig.extends.clone(),
            implements: sig.implements.clone(),
            annotations: sig.annotations.clone(),
            docstring_text: sig.docstring_text.clone(),
            existing_word_count: sig.existing_word_count,
        }
    }
}

/// One dependency entry returned by `next` — the dep's path and its filtered signatures.
#[derive(Debug, Serialize, Deserialize)]
pub struct DepEntry {
    pub path: String,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub fields_omitted: usize,
    pub signatures: Vec<DepSignature>,
}

/// Slimmed-down file entry for `next` output — omits fields that are always
/// false/empty in the loop or redundant with `deps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextFileEntry {
    pub path: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub comment_coverage: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cycle_warning: bool,
    pub signatures: Vec<Signature>,
}

impl From<&FileEntry> for NextFileEntry {
    fn from(fe: &FileEntry) -> Self {
        NextFileEntry {
            path: fe.path.clone(),
            language: fe.language.clone(),
            package: fe.package.clone(),
            comment_coverage: fe.comment_coverage,
            cycle_warning: fe.cycle_warning,
            signatures: fe.signatures.clone(),
        }
    }
}

/// The structured output from a `next` call. Used both for JSON printing and for
/// test assertions (via `run_and_capture`).
#[derive(Debug, Serialize, Deserialize)]
pub struct NextOutput {
    pub done: bool,
    pub mode: String,  // "project" | "targeted"
    pub index: Option<usize>,
    pub remaining: usize,
    pub file: Option<NextFileEntry>,
    pub deps: Vec<DepEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub reverse_deps: Vec<DepEntry>,
}

pub fn run(args: NextArgs) -> anyhow::Result<bool> {
    let output = run_and_capture(args)?;
    println!("{}", crate::envelope::format_ok(serde_json::to_value(&output)?));
    Ok(false)
}

/// Core logic — returns a `NextOutput` instead of printing, so tests can assert on it.
pub fn run_and_capture(args: NextArgs) -> anyhow::Result<NextOutput> {
    match args.target {
        Some(target) => run_targeted(args.cache, target, args.max_fields),
        None => run_project(args.cache, args.max_fields),
    }
}

fn run_project(cache_path: std::path::PathBuf, max_fields: usize) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;
    let session = try_read_session(&cache_dir)?.unwrap_or_default();

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
                let _ = rescan_one(&mut cache, prev_file);
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
            reverse_deps: vec![],
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

    let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;
    let remaining = cache.order.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "project".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(NextFileEntry::from(&file_entry)),
        deps,
        reverse_deps,
    })
}

fn run_targeted(cache_path: std::path::PathBuf, target: String, max_fields: usize) -> anyhow::Result<NextOutput> {
    let cache_dir = cache_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid cache path: no parent directory"))?
        .to_path_buf();

    let mut cache = read_cache(&cache_path)?;

    // Error if target not in cache
    if !cache.files.contains_key(&target) {
        return Err(CodeskelError::TargetNotInTree(target).into());
    }

    let session = try_read_session(&cache_dir)?.unwrap_or_default();

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

        let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;
        let remaining = chain.len() - 1;

        return Ok(NextOutput {
            done: false,
            mode: "targeted".into(),
            index: Some(0),
            remaining,
            file: Some(NextFileEntry::from(&file_entry)),
            deps,
            reverse_deps,
        });
    }

    // Subsequent call: rescan current_file, advance cursor
    let chain = session.chain.as_ref()
        .ok_or_else(|| anyhow::anyhow!("session.chain missing in targeted mode"))?
        .clone();

    if let Some(prev_file) = session.current_file.as_deref() {
        if cache.files.contains_key(prev_file) {
            let _ = rescan_one(&mut cache, prev_file);
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
                reverse_deps: vec![],
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

    let (deps, reverse_deps) = build_deps_with_reverse(&cache, &file_entry, max_fields)?;
    let remaining = chain.len() - next_cursor - 1;

    Ok(NextOutput {
        done: false,
        mode: "targeted".into(),
        index: Some(next_cursor),
        remaining,
        file: Some(NextFileEntry::from(&file_entry)),
        deps,
        reverse_deps,
    })
}

const TOP_LEVEL_KINDS: &[&str] = &["class", "interface", "enum", "struct", "trait", "type", "type_alias"];
const REVERSE_DEP_KINDS: &[&str] = &["interface", "abstract_class", "annotation"];
const MAX_REVERSE_DEPS: usize = 5;

fn build_deps(
    cache: &crate::models::CacheFile,
    file_entry: &FileEntry,
    max_fields: usize,
) -> anyhow::Result<Vec<DepEntry>> {
    // Attempt refs analysis; on failure, fall back to unfiltered (all deps get all sigs)
    let refs_map = match crate::commands::get::compute_refs(cache, &file_entry.path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "[codeskel] Warning: refs analysis failed for '{}': {}; using unfiltered deps",
                file_entry.path, e
            );
            std::collections::HashMap::new()
        }
    };

    let entries = file_entry.internal_imports.iter()
        .filter_map(|dep| {
            cache.files.get(dep).map(|dep_entry| {
                let referenced = refs_map.get(dep.as_str());
                let all_sigs: Vec<DepSignature> = dep_entry.signatures.iter()
                    .filter(|sig| {
                        if TOP_LEVEL_KINDS.contains(&sig.kind.as_str()) {
                            return true;
                        }
                        match referenced {
                            Some(names) if !names.is_empty() => names.contains(&sig.name),
                            _ => true,
                        }
                    })
                    .map(DepSignature::from)
                    .collect();

                let (non_fields, fields): (Vec<_>, Vec<_>) =
                    all_sigs.into_iter().partition(|s| s.kind != "field");
                let fields_total = fields.len();
                let kept_fields: Vec<_> = fields.into_iter().take(max_fields).collect();
                let fields_omitted = fields_total - kept_fields.len();

                let signatures: Vec<DepSignature> = non_fields.into_iter().chain(kept_fields).collect();
                if signatures.is_empty() {
                    return None;
                }
                Some(DepEntry {
                    path: dep_entry.path.clone(),
                    fields_omitted,
                    signatures,
                })
            })
            .flatten()
        })
        .collect();

    Ok(entries)
}

fn build_deps_with_reverse(
    cache: &crate::models::CacheFile,
    file_entry: &FileEntry,
    max_fields: usize,
) -> anyhow::Result<(Vec<DepEntry>, Vec<DepEntry>)> {
    let deps = build_deps(cache, file_entry, max_fields)?;

    let reverse_deps = if REVERSE_DEP_KINDS.contains(&file_entry.file_kind.as_str()) {
        file_entry.reverse_deps.iter()
            .take(MAX_REVERSE_DEPS)
            .filter_map(|rdep| {
                cache.files.get(rdep).map(|rdep_entry| {
                    let all_sigs: Vec<DepSignature> = rdep_entry.signatures.iter()
                        .map(DepSignature::from)
                        .collect();
                    let (non_fields, fields): (Vec<_>, Vec<_>) =
                        all_sigs.into_iter().partition(|s| s.kind != "field");
                    let fields_total = fields.len();
                    let kept_fields: Vec<_> = fields.into_iter().take(max_fields).collect();
                    let fields_omitted = fields_total - kept_fields.len();
                    let signatures: Vec<DepSignature> = non_fields.into_iter().chain(kept_fields).collect();
                    if signatures.is_empty() { return None; }
                    Some(DepEntry {
                        path: rdep_entry.path.clone(),
                        fields_omitted,
                        signatures,
                    })
                })
                .flatten()
            })
            .collect()
    } else {
        vec![]
    };

    Ok((deps, reverse_deps))
}
