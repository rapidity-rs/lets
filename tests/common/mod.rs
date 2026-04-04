use std::fs;
use std::process::Command;

pub fn lets_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lets"))
}

pub fn with_temp_kdl(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lets.kdl");
    fs::write(&path, content).unwrap();
    (dir, path)
}
