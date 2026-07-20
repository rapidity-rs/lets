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

use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::interpolate::{self, Placeholder, Resolution};
use crate::output::TaskSink;
use crate::tree::{CommandNode, Config, VarValue};

/// Execution context derived from a CommandNode's settings.
pub(crate) struct ExecContext {
    pub env: Vec<(String, String)>,
    /// Working directory for child processes: the config-file directory,
    /// or `dir` resolved against it.
    pub dir: PathBuf,
    pub shell: Option<String>,
    pub dry_run: bool,
    pub timeout: Option<Duration>,
    pub retry_count: u32,
    pub retry_delay: Option<Duration>,
}

impl ExecContext {
    pub fn from_node(
        node: &CommandNode,
        config: &Config,
        project_root: &Path,
        dry_run: bool,
    ) -> Result<Self> {
        // Commands run from the config-file directory, wherever the user
        // invoked from — matching how `sources` and fingerprints resolve.
        // `join` keeps absolute `dir` values as-is.
        let dir = match &node.exec.dir {
            Some(d) => project_root.join(d),
            None => project_root.to_path_buf(),
        };

        // Seed the location exports first so env values can reference them
        // via `{$LETS_PROJECT_ROOT}` / `{$LETS_INVOCATION_DIR}`.
        let mut env = vec![(
            "LETS_PROJECT_ROOT".to_string(),
            project_root.display().to_string(),
        )];
        if let Ok(invocation_dir) = std::env::current_dir() {
            env.push((
                "LETS_INVOCATION_DIR".to_string(),
                invocation_dir.display().to_string(),
            ));
        }
        // Env layers, later overriding earlier: config env-file, config
        // env, task env-file, task env. All file paths resolve against the
        // config directory, like every other config path.
        if let Some(env_file) = &config.env_file {
            load_env_file(&project_root.join(env_file), &mut env)?;
        }
        for (k, v) in &config.env {
            let rendered = render_env_value(k, v, node, config, project_root, &env)?;
            env.retain(|(ek, _)| ek != k);
            env.push((k.clone(), rendered));
        }
        if let Some(env_file) = &node.env.file {
            load_env_file(&project_root.join(env_file), &mut env)?;
        }
        for (k, v) in &node.env.vars {
            let rendered = render_env_value(k, v, node, config, project_root, &env)?;
            env.retain(|(ek, _)| ek != k);
            env.push((k.clone(), rendered));
        }

        Ok(ExecContext {
            env,
            dir,
            shell: node.exec.shell.clone().or_else(|| config.shell.clone()),
            dry_run,
            timeout: node.exec.timeout,
            retry_count: node.exec.retry_count.unwrap_or(0),
            retry_delay: node.exec.retry_delay,
        })
    }
}

/// Append a .env file's entries to `env`.
fn load_env_file(path: &Path, env: &mut Vec<(String, String)>) -> Result<()> {
    let iter = dotenvy::from_path_iter(path)
        .map_err(|e| Error::Other(format!("failed to read env-file '{}': {e}", path.display())))?;
    for item in iter {
        let (key, value) = item.map_err(|e| {
            Error::Other(format!(
                "failed to parse env-file '{}': {e}",
                path.display()
            ))
        })?;
        env.push((key, value));
    }
    Ok(())
}

/// Render one env value: config vars (static or dynamic) and `{$VAR}`
/// lookups against env declared so far, falling back to the process
/// environment.
fn render_env_value(
    key: &str,
    value: &str,
    node: &CommandNode,
    config: &Config,
    project_root: &Path,
    env: &[(String, String)],
) -> Result<String> {
    interpolate::render(value, |p| match p {
        Placeholder::Variable(name) => match node.lookup_var(name) {
            Some(def) => match resolve_var(name, def, config.shell.as_deref(), project_root) {
                Ok(v) => Resolution::Value(v),
                Err(msg) => Resolution::Error(msg),
            },
            None => Resolution::Unknown,
        },
        Placeholder::EnvVar(name) => env
            .iter()
            .rev()
            .find(|(ek, _)| ek == name)
            .map(|(_, ev)| ev.clone())
            .or_else(|| std::env::var(name).ok())
            .map_or(Resolution::Skip, Resolution::Value),
        _ => Resolution::Unknown,
    })
    .map_err(|e| Error::Other(format!("in env value '{key}' of '{}': {e}", node.name)))
}

/// Resolve a var to its text. Dynamic vars run `shell -c cmd` in the
/// project root on first reference; the trimmed stdout is cached for the
/// whole invocation (failures are cached too, so a broken command doesn't
/// run once per referencing task).
pub(crate) fn resolve_var(
    name: &str,
    value: &VarValue,
    shell: Option<&str>,
    root: &Path,
) -> std::result::Result<String, String> {
    match value {
        VarValue::Static(v) => Ok(v.clone()),
        VarValue::Command { cmd, cache } => cache
            .get_or_init(|| {
                let shell = shell.unwrap_or("sh");
                let output = process::Command::new(shell)
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(root)
                    .output()
                    .map_err(|e| format!("var '{name}': failed to run `{cmd}`: {e}"))?;
                if !output.status.success() {
                    let code = output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_string(), |c| c.to_string());
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let detail = if stderr.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", stderr.trim())
                    };
                    return Err(format!("var '{name}': `{cmd}` exited with {code}{detail}"));
                }
                Ok(String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string())
            })
            .clone(),
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

    cmd.current_dir(&ctx.dir);

    // The signal-handling machinery may leave SIGINT/SIGTERM blocked in this
    // thread; the mask survives fork+exec, which would make children immune
    // to Ctrl-C. Explicitly unblock in the child.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: sigprocmask is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                let mut set = nix::sys::signal::SigSet::empty();
                set.add(nix::sys::signal::Signal::SIGINT);
                set.add(nix::sys::signal::Signal::SIGTERM);
                set.thread_unblock().map_err(std::io::Error::other)
            });
        }
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
    let _pgroup = PgroupGuard::register(pid);

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
    let _pgroup = ctx.timeout.map(|_| PgroupGuard::register(child.id()));

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

/// Children running in their own process groups (timeout-managed): the
/// terminal's SIGINT never reaches them, so interrupt handling forwards
/// SIGTERM explicitly via [`terminate_process_groups`].
static PGROUP_CHILDREN: parking_lot::Mutex<Vec<u32>> = parking_lot::Mutex::new(Vec::new());

/// RAII registration of a process-grouped child for signal forwarding.
struct PgroupGuard {
    pid: u32,
}

impl PgroupGuard {
    fn register(pid: u32) -> Self {
        PGROUP_CHILDREN.lock().push(pid);
        PgroupGuard { pid }
    }
}

impl Drop for PgroupGuard {
    fn drop(&mut self) {
        PGROUP_CHILDREN.lock().retain(|p| *p != self.pid);
    }
}

/// SIGTERM every registered child process group (called from the signal
/// handler so interrupted runs shut their task trees down gracefully).
pub(crate) fn terminate_process_groups() {
    for pid in PGROUP_CHILDREN.lock().iter() {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;
            let _ = signal::killpg(Pid::from_raw(*pid as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
        }
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

/// Run a gate command (precondition/status) quietly with the task's
/// shell/env/dir. Returns whether it exited successfully.
pub(crate) fn check_shell(command: &str, ctx: &ExecContext) -> Result<bool> {
    let shell = ctx.shell.as_deref().unwrap_or("sh");
    let mut cmd = process::Command::new(shell);
    cmd.arg("-c")
        .arg(command)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null());

    if !ctx.env.is_empty() {
        cmd.envs(ctx.env.iter().map(|(k, v)| (k, v)));
    }
    cmd.current_dir(&ctx.dir);

    let status = cmd
        .status()
        .map_err(|e| Error::Other(format!("failed to run check '{command}': {e}")))?;
    Ok(status.success())
}
