//! Shell process execution.
//!
//! Handles spawning shell commands (`sh -c` or configured shell), with support
//! for timeout (via process groups and `SIGKILL`), retry with configurable delay,
//! silent mode (capture stdout/stderr, show only on failure), and dry-run.
//!
//! Uses [`nix`] on Unix for proper process group management: each child gets
//! its own process group via `setpgid`, and timeouts kill the entire group
//! via `killpg`.

use std::path::PathBuf;
use std::process;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::tree::{CommandNode, Config};

/// Execution context derived from a CommandNode's settings.
pub(crate) struct ExecContext {
    pub env: Vec<(String, String)>,
    pub dir: Option<PathBuf>,
    pub shell: Option<String>,
    pub dry_run: bool,
    pub timeout: Option<Duration>,
    pub retry_count: u32,
    pub retry_delay: Option<Duration>,
    pub silent: bool,
}

impl ExecContext {
    pub fn from_node(node: &CommandNode, config: &Config, dry_run: bool) -> Result<Self> {
        let mut env = Vec::new();

        // Load env-file first (explicit env vars override).
        if let Some(env_file) = &node.env.file {
            let iter = dotenvy::from_path_iter(env_file).map_err(|e| {
                Error::Other(format!(
                    "failed to read env-file '{}': {e}",
                    env_file.display()
                ))
            })?;
            for item in iter {
                let (key, value) = item.map_err(|e| {
                    Error::Other(format!(
                        "failed to parse env-file '{}': {e}",
                        env_file.display()
                    ))
                })?;
                env.push((key, value));
            }
        }

        // Explicit env vars override env-file.
        for (k, v) in &node.env.vars {
            env.retain(|(ek, _)| ek != k);
            env.push((k.clone(), v.clone()));
        }

        Ok(ExecContext {
            env,
            dir: node.exec.dir.clone(),
            shell: node.exec.shell.clone().or_else(|| config.shell.clone()),
            dry_run,
            timeout: node.exec.timeout,
            retry_count: node.exec.retry_count.unwrap_or(0),
            retry_delay: node.exec.retry_delay,
            silent: node.exec.silent,
        })
    }
}

pub(crate) fn exec_shell(command: &str, ctx: &ExecContext) -> Result<()> {
    if ctx.dry_run {
        println!("[dry-run] {command}");
        return Ok(());
    }

    let attempts = ctx.retry_count.max(1);
    let mut last_err = None;

    for attempt in 1..=attempts {
        let result = exec_shell_once(command, ctx);
        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt < attempts
                    && let Some(delay) = ctx.retry_delay
                {
                    std::thread::sleep(delay);
                }
                last_err = Some(e);
            }
        }
    }

    last_err.map_or(Ok(()), Err)
}

fn exec_shell_once(command: &str, ctx: &ExecContext) -> Result<()> {
    let shell = ctx.shell.as_deref().unwrap_or("sh");
    let mut cmd = process::Command::new(shell);
    cmd.arg("-c").arg(command);

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k, v)));
    }

    if let Some(dir) = &ctx.dir {
        cmd.current_dir(dir);
    }

    if ctx.silent {
        cmd.stdout(process::Stdio::piped());
        cmd.stderr(process::Stdio::piped());
    }

    if let Some(timeout) = ctx.timeout {
        // Spawn child in its own process group so timeout kills the entire tree.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: setpgid is async-signal-safe.
            unsafe {
                cmd.pre_exec(|| {
                    nix::unistd::setpgid(
                        nix::unistd::Pid::from_raw(0),
                        nix::unistd::Pid::from_raw(0),
                    )
                    .map_err(std::io::Error::other)
                });
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let pid = child.id();

        std::thread::spawn(move || {
            let result = child.wait();
            // Send the result; if the receiver is gone (timeout), that's fine.
            let _ = tx.send(result.map(|s| (s, child)));
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok((status, mut child))) => {
                if ctx.silent && !status.success() {
                    dump_child_output(&mut child);
                }
                return check_status(status);
            }
            Ok(Err(e)) => {
                return Err(Error::Other(format!("failed to wait on command: {e}")));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Kill the entire process group on timeout.
                #[cfg(unix)]
                {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;
                    let _ = signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
                }
                #[cfg(not(unix))]
                {
                    let _ = pid;
                }
                return Err(Error::Other(format!("command timed out after {timeout:?}")));
            }
            Err(e) => {
                return Err(Error::Other(format!("wait channel error: {e}")));
            }
        }
    }

    if ctx.silent {
        let output = cmd
            .output()
            .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;
        if !output.status.success() {
            use std::io::Write;
            std::io::stdout().write_all(&output.stdout).ok();
            std::io::stderr().write_all(&output.stderr).ok();
        }
        return check_status(output.status);
    }

    let status = cmd
        .status()
        .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;

    check_status(status)
}

fn check_status(status: process::ExitStatus) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        match status.code() {
            Some(code) => Err(Error::CommandFailed { code }),
            None => Err(Error::CommandSignaled),
        }
    }
}

fn dump_child_output(child: &mut process::Child) {
    use std::io::{Read, Write};
    if let Some(ref mut stdout) = child.stdout {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).ok();
        std::io::stdout().write_all(&buf).ok();
    }
    if let Some(ref mut stderr) = child.stderr {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).ok();
        std::io::stderr().write_all(&buf).ok();
    }
}
