//! Config file discovery.
//!
//! Walks up from the current directory to the filesystem root looking for
//! a `lets.kdl` file, similar to how `git` finds `.git` or `cargo` finds
//! `Cargo.toml`.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const CONFIG_FILENAME: &str = "lets.kdl";

/// Search for `lets.kdl` starting from `start` and walking up to the filesystem root.
pub fn find_config(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(Error::ConfigNotFound {
                start_dir: start.to_path_buf(),
            });
        }
    }
}
