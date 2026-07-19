//! Source fingerprinting: skip tasks whose inputs haven't changed.
//!
//! A task that declares `sources` gets checksum-based up-to-date detection.
//! The digest covers the canonical task key, the task's run commands and env
//! (so config edits invalidate), and the content of every file matching the
//! sources globs. Fingerprints are computed before the task body runs and
//! recorded only after it succeeds, in `.lets/fingerprints/` next to the
//! config file.
//!
//! This is an optimization, never a correctness guarantee: `--force` always
//! runs, and deleting `.lets/` resets all state.

use std::io::Read;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::tree::CommandNode;

/// Outcome of an up-to-date check.
pub enum Freshness {
    /// The task declares no sources; fingerprinting doesn't apply.
    NoInputs,
    /// Inputs unchanged since the last successful run (and all `generates`
    /// globs match at least one existing file): skip the task.
    Current,
    /// Inputs changed (or never recorded). Carries the digest to record
    /// after the task succeeds.
    Stale(String),
}

/// Check whether a task is up to date. `root` is the config-file directory;
/// sources/generates patterns are relative to it.
pub fn check(root: &Path, key: &[String], node: &CommandNode) -> Result<Freshness> {
    if node.sources.is_empty() {
        return Ok(Freshness::NoInputs);
    }

    let digest = compute_digest(root, key, node)?;

    let recorded = std::fs::read_to_string(cache_path(root, key)).ok();
    if recorded.as_deref() != Some(digest.as_str()) {
        return Ok(Freshness::Stale(digest));
    }

    // Declared outputs must exist for the task to count as done.
    if !node.generates.is_empty() && !generates_present(root, &node.generates)? {
        return Ok(Freshness::Stale(digest));
    }

    Ok(Freshness::Current)
}

/// Record a fingerprint after a successful run.
pub fn record(root: &Path, key: &[String], digest: &str) -> Result<()> {
    let path = cache_path(root, key);
    let dir = path.parent().expect("cache path always has a parent");
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::Other(format!("failed to create '{}': {e}", dir.display())))?;

    // Keep the cache out of version control without asking users to
    // maintain their .gitignore (same trick as cargo's target dir).
    let lets_dir = root.join(".lets");
    let marker = lets_dir.join(".gitignore");
    if !marker.exists() {
        let _ = std::fs::write(&marker, "*\n");
    }

    std::fs::write(&path, digest)
        .map_err(|e| Error::Other(format!("failed to write '{}': {e}", path.display())))
}

fn cache_path(root: &Path, key: &[String]) -> PathBuf {
    let mut hasher = Sha256::new();
    for token in key {
        hasher.update(token.as_bytes());
        hasher.update([0x1f]);
    }
    let name = hex(&hasher.finalize());
    root.join(".lets").join("fingerprints").join(name)
}

/// Digest of everything that defines "this task ran against these inputs".
fn compute_digest(root: &Path, key: &[String], node: &CommandNode) -> Result<String> {
    let mut hasher = Sha256::new();

    for token in key {
        hasher.update(token.as_bytes());
        hasher.update([0x1f]);
    }
    for command in node.run.resolve() {
        hasher.update(command.as_bytes());
        hasher.update([0x1f]);
    }
    for (k, v) in &node.env.vars {
        hasher.update(k.as_bytes());
        hasher.update([0x1e]);
        hasher.update(v.as_bytes());
        hasher.update([0x1f]);
    }

    let mut files = matching_files(root, &node.sources)?;
    files.sort();
    for file in files {
        let relative = file.strip_prefix(root).unwrap_or(&file);
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0x1e]);
        hasher.update(file_digest(&file)?);
        hasher.update([0x1f]);
    }

    Ok(hex(&hasher.finalize()))
}

fn file_digest(path: &Path) -> Result<[u8; 32]> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| Error::Other(format!("failed to read '{}': {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| Error::Other(format!("failed to read '{}': {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

/// All files under `root` matching the globs. The walk starts from each
/// glob's literal prefix (e.g. `src/**/*.rs` walks only `src/`), so large
/// undeclared trees like target/ are never scanned.
fn matching_files(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    let globs = build_globset(patterns)?;

    let mut walk_roots: Vec<PathBuf> = patterns
        .iter()
        .map(|p| root.join(literal_prefix(p)))
        .collect();
    walk_roots.sort();
    walk_roots.dedup();
    // Nested roots would double-count files: keep only the outermost.
    walk_roots.dedup_by(|next, kept| next.starts_with(kept));

    let mut files = Vec::new();
    for walk_root in walk_roots {
        if !walk_root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&walk_root).follow_links(false) {
            let entry = entry.map_err(|e| Error::Other(format!("failed to walk sources: {e}")))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            if globs.is_match(relative) {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    Ok(files)
}

/// Whether every `generates` glob matches at least one existing file.
fn generates_present(root: &Path, patterns: &[String]) -> Result<bool> {
    for pattern in patterns {
        if matching_files(root, std::slice::from_ref(pattern))?.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        // Validated at config load time.
        builder.add(Glob::new(pattern).map_err(|e| Error::Other(e.to_string()))?);
    }
    builder.build().map_err(|e| Error::Other(e.to_string()))
}

/// Longest prefix of a glob with no meta characters, as a walk starting
/// point. `src/**/*.rs` -> `src`, `Cargo.toml` -> `Cargo.toml`, `*.css` -> ``.
fn literal_prefix(pattern: &str) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in Path::new(pattern).components() {
        let text = component.as_os_str().to_string_lossy();
        if text.contains(['*', '?', '[', '{']) {
            break;
        }
        prefix.push(component);
    }
    prefix
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
