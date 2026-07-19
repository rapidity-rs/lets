//! Shell process execution.
//!
//! Handles spawning shell commands (`sh -c` or configured shell), with support
//! for timeout (via process groups and `SIGKILL`), retry with configurable delay,
//! and dry-run. Output is routed through a [`TaskSink`]: inherited stdio,
//! line-prefixed streaming, or buffered blocks (which also implements silent
//! mode — buffered output flushed only on failure).
//!
//! Uses [`nix`] on Unix for proper process group management: each child gets
//! its own process group via `setpgid`, and timeouts kill the entire group
//! via `killpg`.

use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::output::TaskSink;
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
        })
    }
}

pub(crate) fn exec_shell(command: &str, ctx: &ExecContext, sink: &TaskSink) -> Result<()> {
    if ctx.dry_run {
        println!("[dry-run] {command}");
        return Ok(());
    }

    let attempts = ctx.retry_count.max(1);
    let mut last_err = None;

    for attempt in 1..=attempts {
        let result = exec_shell_once(command, ctx, sink);
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

fn exec_shell_once(command: &str, ctx: &ExecContext, sink: &TaskSink) -> Result<()> {
    let shell = ctx.shell.as_deref().unwrap_or("sh");
    let mut cmd = process::Command::new(shell);
    cmd.arg("-c").arg(command);

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k, v)));
    }

    if let Some(dir) = &ctx.dir {
        cmd.current_dir(dir);
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
        let _ = timeout;
    }

    if sink.captures() {
        exec_captured(cmd, ctx, sink)
    } else {
        exec_inherited(cmd, ctx)
    }
}

/// Run with inherited stdio (interleaved output).
fn exec_inherited(mut cmd: process::Command, ctx: &ExecContext) -> Result<()> {
    let Some(timeout) = ctx.timeout else {
        let status = cmd
            .status()
            .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;
        return check_status(status);
    };

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;

    let (tx, rx) = std::sync::mpsc::channel();
    let pid = child.id();

    std::thread::spawn(move || {
        let result = child.wait();
        // Send the result; if the receiver is gone (timeout), that's fine.
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(status)) => check_status(status),
        Ok(Err(e)) => Err(Error::Other(format!("failed to wait on command: {e}"))),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            kill_process_group(pid);
            Err(Error::Other(format!("command timed out after {timeout:?}")))
        }
        Err(e) => Err(Error::Other(format!("wait channel error: {e}"))),
    }
}

/// Run with stdout+stderr merged into one pipe, drained through the sink.
fn exec_captured(mut cmd: process::Command, ctx: &ExecContext, sink: &TaskSink) -> Result<()> {
    let (reader, writer) =
        std::io::pipe().map_err(|e| Error::Other(format!("failed to create output pipe: {e}")))?;
    let writer_clone = writer
        .try_clone()
        .map_err(|e| Error::Other(format!("failed to clone output pipe: {e}")))?;
    cmd.stdout(writer);
    cmd.stderr(writer_clone);

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn shell: {e}")))?;
    // Command retains its Stdio handles for potential respawn; drop them so
    // the pipe sees EOF when the child exits.
    drop(cmd);

    // Watchdog thread kills the process group at the deadline while this
    // thread drains the pipe.
    let watchdog = ctx.timeout.map(|timeout| {
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<()>();
        let timed_out = std::sync::Arc::new(AtomicBool::new(false));
        let flag = timed_out.clone();
        let pid = child.id();
        std::thread::spawn(move || {
            if cancel_rx.recv_timeout(timeout).is_err() {
                flag.store(true, Ordering::SeqCst);
                kill_process_group(pid);
            }
        });
        (cancel_tx, timed_out)
    });

    sink.consume(reader);

    let status = child
        .wait()
        .map_err(|e| Error::Other(format!("failed to wait on command: {e}")))?;

    if let Some((cancel_tx, timed_out)) = watchdog {
        let _ = cancel_tx.send(());
        if timed_out.load(Ordering::SeqCst) {
            let timeout = ctx.timeout.unwrap_or_default();
            return Err(Error::Other(format!("command timed out after {timeout:?}")));
        }
    }

    check_status(status)
}

fn kill_process_group(pid: u32) {
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
