//! Launch limits of a native engine process (Debian: Settings -> Memory / Processor).
//!
//! What can only be decided when the process starts is decided here:
//!  - the thread count, as `STUDIO_CPU_THREADS` (the engine's `src/backend.h` is patched to read it when it is
//!    built: `scripts/patch-engine-threads.sh`);
//!  - the cores it may run on (`sched_setaffinity`) and its priority (`nice`), set between fork and exec so every
//!    thread the engine creates inherits them;
//!  - a systemd scope of its own (`systemd-run --user --scope`), whose CPUQuota, MemoryHigh and MemoryMax the kernel
//!    enforces and the studio changes while the engine runs (`systemctl --user set-property --runtime`). The scope
//!    is the same process: `systemd-run --scope` registers it and execs the engine in place, so the PID the studio
//!    holds stays the engine's and the death signal set before exec still ties it to the studio.
//!
//! Without systemd (or without a user session bus) the process is started directly and the studio falls back to
//! a duty cycle (SIGSTOP / SIGCONT) for the CPU limit and to a watchdog for the memory limit.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// The systemd scope the engine runs in, with the limits the kernel enforces from its first instruction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    /// The unit name without `.scope`; unique per start.
    pub unit: String,
    /// CPUQuota in percent of ONE core, as systemd counts it (400 = four cores). None: no quota.
    pub cpu_quota_percent: Option<u32>,
    /// MemoryHigh: above it the kernel reclaims (and swaps) the engine's memory instead of letting it grow.
    pub memory_high_bytes: Option<u64>,
    /// MemoryMax: above it the kernel's OOM killer ends the engine.
    pub memory_max_bytes: Option<u64>,
}

/// Everything a start of an engine is limited by.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Limits {
    /// `STUDIO_CPU_THREADS`; None = all cores the engine may use (the patched engine's default).
    pub threads: Option<u32>,
    /// The nice value (-20 highest .. 19 lowest). A value below the studio's own needs CAP_SYS_NICE: refused
    /// quietly by the kernel, the engine then runs at the studio's priority.
    pub nice: Option<i32>,
    /// Logical CPUs the engine may run on; empty = every CPU.
    pub cpus: Vec<usize>,
    /// The systemd scope, when one can be made.
    pub scope: Option<Scope>,
    /// Engine launch flags the memory plan adds (e.g. a smaller VAE tile); placed before the user's own extra
    /// arguments, which can still override them.
    pub args: Vec<String>,
}

/// Whether `systemd-run --user --scope` works here (a systemd user session with its bus). Asked once.
pub fn systemd_scopes_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        if !cfg!(target_os = "linux") || std::env::var_os("STUDIO_NO_SYSTEMD_SCOPE").is_some() {
            return false;
        }
        Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", "--", "true"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// The command that starts `program` under `limits`: `program` itself, or `systemd-run ... -- program` when a
/// scope is asked for. Arguments for the program are added to the returned command as usual.
pub fn command(program: &Path, limits: &Limits) -> Command {
    let mut command = match &limits.scope {
        Some(scope) if systemd_scopes_available() => {
            let mut command = Command::new("systemd-run");
            command.args(["--user", "--scope", "--quiet", "--collect"]);
            command.arg(format!("--unit={}", scope.unit));
            if let Some(quota) = scope.cpu_quota_percent {
                command.arg("-p").arg(format!("CPUQuota={quota}%"));
            }
            if let Some(high) = scope.memory_high_bytes {
                command.arg("-p").arg(format!("MemoryHigh={high}"));
            }
            if let Some(max) = scope.memory_max_bytes {
                command.arg("-p").arg(format!("MemoryMax={max}"));
            }
            command.arg("--").arg(program);
            command
        }
        _ => Command::new(program),
    };
    match limits.threads {
        Some(threads) if threads > 0 => {
            command.env("STUDIO_CPU_THREADS", threads.to_string());
        }
        _ => {
            command.env_remove("STUDIO_CPU_THREADS");
        }
    }
    apply_before_exec(&mut command, limits.nice, &limits.cpus);
    command
}

#[cfg(target_os = "linux")]
fn apply_before_exec(command: &mut Command, nice: Option<i32>, cpus: &[usize]) {
    use std::os::unix::process::CommandExt;
    if nice.is_none() && cpus.is_empty() {
        return;
    }
    let cpus = cpus.to_vec();
    // SAFETY: the closure runs between fork and exec and calls only async-signal-safe functions on data
    // prepared before the fork (no allocation inside: the set is built on the stack).
    unsafe {
        command.pre_exec(move || {
            if let Some(nice) = nice {
                // refused (EACCES) when it would raise the priority without CAP_SYS_NICE: the engine keeps ours
                let _ = libc::setpriority(libc::PRIO_PROCESS, 0, nice.clamp(-20, 19));
            }
            if !cpus.is_empty() {
                let mut set: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_ZERO(&mut set);
                for &cpu in &cpus {
                    if cpu < libc::CPU_SETSIZE as usize {
                        libc::CPU_SET(cpu, &mut set);
                    }
                }
                // a set the machine does not have is refused (EINVAL): the engine then runs on every CPU
                let _ = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_before_exec(_command: &mut Command, _nice: Option<i32>, _cpus: &[usize]) {}

/// Changes a property of a running scope (`CPUQuota=50%`, `MemoryHigh=...`). Returns whether systemd took it.
pub fn set_scope_property(unit: &str, property: &str) -> bool {
    if !systemd_scopes_available() {
        return false;
    }
    Command::new("systemctl")
        .args(["--user", "set-property", "--runtime"])
        .arg(format!("{unit}.scope"))
        .arg(property)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_start_runs_the_program_itself_with_the_thread_count() {
        let limits = Limits { threads: Some(4), ..Limits::default() };
        let command = command(Path::new("/opt/engine/ace-server"), &limits);
        assert_eq!(command.get_program(), "/opt/engine/ace-server");
        let threads = command.get_envs().find(|(name, _)| *name == "STUDIO_CPU_THREADS").and_then(|(_, value)| value);
        assert_eq!(threads.and_then(|value| value.to_str()), Some("4"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn priority_and_cores_reach_the_started_process() {
        let limits = Limits { nice: Some(19), cpus: vec![0], ..Limits::default() };
        let mut command = command(Path::new("/bin/sh"), &limits);
        command.args(["-c", "cut -d' ' -f19 /proc/self/stat; grep Cpus_allowed_list /proc/self/status"]);
        let output = command.output().expect("run sh");
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("19"), "{text}");
        assert_eq!(lines.next().map(|line| line.split_whitespace().last()), Some(Some("0")), "{text}");
    }

    #[test]
    fn no_thread_count_removes_one_the_studio_inherited() {
        let command = command(Path::new("/bin/true"), &Limits::default());
        let threads = command.get_envs().find(|(name, _)| *name == "STUDIO_CPU_THREADS");
        assert!(matches!(threads, Some((_, None))), "removed, not inherited");
    }
}
