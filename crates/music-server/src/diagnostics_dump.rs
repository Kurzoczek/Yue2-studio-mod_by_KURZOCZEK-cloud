//! Settings -> Diagnostics (Debian), ported from the Android app "AI Music Generator and Cover Maker":
//! a diagnostic log copied to a folder of your choice, and state dumps named after the Windows
//! "Write debugging information" options.
//!
//!  - SMALL     - like a Windows small memory dump: the reason, the studio's settings, memory, processor and
//!                temperature, the engine process, the last lines of the studio's and the engine's log;
//!  - EXTENDED  - like a kernel memory dump: SMALL + the engine's memory map (smaps_rollup, maps), every thread of
//!                the engine and of the studio (state, wait channel), memory / CPU / IO pressure, every thermal
//!                zone, the system's out-of-memory record when the journal lets this account read it;
//!  - FULL      - like a complete memory dump: EXTENDED + a core file of the engine (`gcore`, from gdb; it needs
//!                `kernel.yama.ptrace_scope = 0`). Without them the dump says why and stays EXTENDED;
//!  - AUTOMATIC - EXTENDED for a failed song, an unexpected end and a dump asked for by hand, SMALL for a pause.
//!
//! When: a song that failed, an unexpected end (the studio's last run did not close: it crashed or the system
//! killed it), a pause (overheating, the RAM limit), and the button. The folder keeps the newest `keep` dumps.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DumpType {
    #[default]
    None,
    Small,
    Extended,
    Full,
    Automatic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Copy the diagnostic log (the studio's log, the engine's log and a snapshot every 10 minutes) to the logs
    /// folder.
    pub logs: bool,
    pub dump_type: DumpType,
    /// When a song fails.
    pub on_error: bool,
    /// The studio's last run did not close (a crash, or the system ended it).
    pub on_unexpected_end: bool,
    /// Overheating protection or the RAM limit stepped in.
    pub on_pause: bool,
    /// Dumps kept in the folder (1 = always overwrite, like Windows).
    pub keep: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { logs: false, dump_type: DumpType::None, on_error: true, on_unexpected_end: true, on_pause: false, keep: 5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Error,
    UnexpectedEnd,
    Pause,
    Manual,
}

fn store() -> &'static RwLock<Settings> {
    static STORE: OnceLock<RwLock<Settings>> = OnceLock::new();
    STORE.get_or_init(|| {
        RwLock::new(
            data_file("diagnostics.json")
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
                .map(|mut settings| {
                    settings.keep = settings.keep.clamp(1, 50);
                    settings
                })
                .unwrap_or_default(),
        )
    })
}

pub fn settings() -> Settings {
    store().read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

fn data_file(name: &str) -> Option<PathBuf> {
    crate::studio_data_root().map(|root| root.join(name))
}

/// Where dumps go: the folder chosen in Settings -> Save locations, else `<data>/diagnostics/dumps`.
pub fn dumps_folder() -> Option<PathBuf> {
    crate::save_locations::folder_for("dumps").or_else(|| data_file("diagnostics").map(|dir| dir.join("dumps")))
}

/// Where the diagnostic log is copied: the folder chosen in Settings -> Save locations, else
/// `<data>/diagnostics/logs`.
pub fn logs_folder() -> Option<PathBuf> {
    crate::save_locations::folder_for("logs").or_else(|| data_file("diagnostics").map(|dir| dir.join("logs")))
}

pub fn save(mut new: Settings) -> std::io::Result<()> {
    new.keep = new.keep.clamp(1, 50);
    *store().write().unwrap_or_else(|poisoned| poisoned.into_inner()) = new.clone();
    let Some(path) = data_file("diagnostics.json") else { return Ok(()) };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&new).unwrap_or_default())
}

// ------------------------------------------------------------------------------------------------ triggers

/// The studio's run started: a marker the next run looks for. A marker left by a run that did not close is an
/// unexpected end; the desktop shell removes it on a clean exit (`mark_clean_exit`).
pub fn mark_running() {
    let Some(marker) = data_file("running.marker") else { return };
    if let Ok(previous) = std::fs::read_to_string(&marker) {
        let previous = previous.trim().to_string();
        if !previous.is_empty() {
            let settings = settings();
            log_line(&format!("the previous run ({previous}) did not close: it crashed or the system ended it"));
            if settings.on_unexpected_end {
                // the previous run's logs are still on disk: the dump keeps them
                std::thread::spawn(move || {
                    let _ = write_dump(Trigger::UnexpectedEnd, &format!("the previous run ({previous}) ended unexpectedly"));
                });
            }
        }
    }
    let _ = std::fs::write(&marker, format!("pid {} started {}", std::process::id(), chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
    start_snapshots();
}

/// The studio is closing on purpose: no unexpected end next time.
pub fn mark_clean_exit() {
    if let Some(marker) = data_file("running.marker") {
        let _ = std::fs::remove_file(marker);
    }
    log_line("the studio closed");
}

/// A song failed.
pub fn job_failed(job: &str, message: &str) {
    log_line(&format!("[ERROR] song {job} failed: {message}"));
    if settings().on_error {
        let reason = format!("song {job} failed: {message}");
        std::thread::spawn(move || {
            let _ = write_dump(Trigger::Error, &reason);
        });
    }
}

/// The governor stepped in (overheating, the RAM limit) or changed something.
pub fn governor_event(kind: &str, text: &str) {
    log_line(&format!("[{kind}] {text}"));
    if matches!(kind, "thermal" | "memory") && text.contains("too hot") | text.contains("limit") && settings().on_pause {
        let reason = text.to_string();
        // one dump per minute at most: a computer at the edge of its limit says so again and again
        static LAST: Mutex<Option<Instant>> = Mutex::new(None);
        let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last = Some(Instant::now());
        std::thread::spawn(move || {
            let _ = write_dump(Trigger::Pause, &reason);
        });
    }
}

/// The button: a dump now, whatever the type chosen (`None` makes an EXTENDED one).
pub fn dump_now() -> Result<PathBuf, String> {
    write_dump(Trigger::Manual, "asked for in Settings -> Diagnostics").map_err(|error| error.to_string())
}

// ------------------------------------------------------------------------------------------------ the log

fn log_line(text: &str) {
    if !settings().logs {
        return;
    }
    let Some(dir) = logs_folder() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("diagnostic-{}.log", chrono::Local::now().format("%Y-%m-%d")));
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {text}", chrono::Local::now().format("%H:%M:%S"));
    }
}

/// Every 10 minutes, while the diagnostic log is on: one line with memory, the engine, the temperature.
fn start_snapshots() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let _ = std::thread::Builder::new().name("diagnostic-snapshots".into()).spawn(|| loop {
            std::thread::sleep(Duration::from_secs(600));
            if settings().logs {
                let page = crate::governor::page();
                log_line(&format!(
                    "snapshot: RAM {} of {} MB free, engine {}, {} °C, CPU limit {} %",
                    page["memory"]["available_mb"],
                    page["memory"]["total_mb"],
                    page["engine"].get("rss_mb").map(|mb| format!("{mb} MB")).unwrap_or_else(|| "not running".into()),
                    page["thermal"]["celsius"].as_f64().map(|celsius| format!("{celsius:.0}")).unwrap_or_else(|| "?".into()),
                    page["cpu"]["applied_percent"],
                ));
                copy_logs();
            }
        });
    });
}

/// The studio's and the engine's log files beside the diagnostic log.
fn copy_logs() {
    let Some(dir) = logs_folder() else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Some(studio) = crate::studio_log::log_dir() {
        let _ = std::fs::copy(studio.join(crate::studio_log::LOG_FILE), dir.join(crate::studio_log::LOG_FILE));
    }
    if let Some(engine) = crate::governor::engine_log_path() {
        if let Some(name) = engine.file_name() {
            let _ = std::fs::copy(&engine, dir.join(name));
        }
    }
}

// ------------------------------------------------------------------------------------------------ the dump

fn level_for(trigger: Trigger, chosen: DumpType) -> DumpType {
    match (chosen, trigger) {
        (DumpType::None, Trigger::Manual) => DumpType::Extended,
        (DumpType::Automatic, Trigger::Pause) => DumpType::Small,
        (DumpType::Automatic, _) => DumpType::Extended,
        (other, _) => other,
    }
}

fn read(path: impl AsRef<Path>) -> Value {
    match std::fs::read_to_string(path.as_ref()) {
        Ok(text) => Value::String(text),
        Err(error) => Value::String(format!("<{}: {error}>", path.as_ref().display())),
    }
}

fn tail(path: &Path, lines: usize) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].iter().map(|line| line.to_string()).collect()
}

fn threads_of(pid: u32) -> Vec<Value> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else { return Vec::new() };
    tasks
        .flatten()
        .map(|task| {
            let path = task.path();
            let comm = std::fs::read_to_string(path.join("comm")).unwrap_or_default().trim().to_string();
            let wchan = std::fs::read_to_string(path.join("wchan")).unwrap_or_default().trim().to_string();
            let state = std::fs::read_to_string(path.join("status"))
                .unwrap_or_default()
                .lines()
                .find_map(|line| line.strip_prefix("State:").map(|rest| rest.trim().to_string()))
                .unwrap_or_default();
            json!({ "tid": task.file_name().to_string_lossy(), "name": comm, "state": state, "waits_in": wchan })
        })
        .collect()
}

fn thermal_zones() -> Vec<Value> {
    let Ok(zones) = std::fs::read_dir("/sys/class/thermal") else { return Vec::new() };
    zones
        .flatten()
        .filter(|zone| zone.file_name().to_string_lossy().starts_with("thermal_zone"))
        .map(|zone| {
            let path = zone.path();
            json!({
                "zone": zone.file_name().to_string_lossy(),
                "type": std::fs::read_to_string(path.join("type")).unwrap_or_default().trim(),
                "celsius": std::fs::read_to_string(path.join("temp")).ok().and_then(|text| text.trim().parse::<f64>().ok()).map(|milli| milli / 1000.0),
            })
        })
        .collect()
}

fn command_output(program: &str, arguments: &[&str]) -> Value {
    match std::process::Command::new(program).args(arguments).stdin(std::process::Stdio::null()).output() {
        Ok(output) => Value::String(format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))),
        Err(error) => Value::String(format!("<{program}: {error}>")),
    }
}

fn write_dump(trigger: Trigger, reason: &str) -> std::io::Result<PathBuf> {
    let settings = settings();
    let level = level_for(trigger, settings.dump_type);
    if level == DumpType::None {
        return Err(std::io::Error::other("state dumps are off (Settings -> Diagnostics)"));
    }
    let dir = dumps_folder().ok_or_else(|| std::io::Error::other("the studio has no data folder"))?;
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let name = format!("state-{stamp}-{}", match trigger {
        Trigger::Error => "error",
        Trigger::UnexpectedEnd => "unexpected-end",
        Trigger::Pause => "pause",
        Trigger::Manual => "manual",
    });
    let engine_pid = crate::governor::engine_pid();
    let engine_log = crate::governor::engine_log_path();
    let studio_log = crate::studio_log::log_dir().map(|dir| dir.join(crate::studio_log::LOG_FILE));
    let mut dump = json!({
        "reason": reason,
        "trigger": format!("{trigger:?}"),
        "level": format!("{level:?}"),
        "written": chrono::Local::now().to_rfc3339(),
        "studio": { "version": crate::studio_version(), "pid": std::process::id(), "data": crate::studio_data_root() },
        "governor": crate::governor::summary(),
        "diagnostics_settings": settings,
        "meminfo": read("/proc/meminfo"),
        "loadavg": read("/proc/loadavg"),
        "kernel": read("/proc/version"),
        "engine": engine_pid.map(|pid| json!({ "pid": pid, "status": read(format!("/proc/{pid}/status")), "cmdline": read(format!("/proc/{pid}/cmdline")).as_str().map(|text| text.replace('\0', " ")) })),
        "engine_log_tail": engine_log.as_deref().map(|path| tail(path, 300)).unwrap_or_default(),
        "studio_log_tail": studio_log.as_deref().map(|path| tail(path, 300)).unwrap_or_default(),
    });
    if matches!(level, DumpType::Extended | DumpType::Full) {
        dump["pressure"] = json!({
            "memory": read("/proc/pressure/memory"),
            "cpu": read("/proc/pressure/cpu"),
            "io": read("/proc/pressure/io"),
        });
        dump["thermal_zones"] = Value::Array(thermal_zones());
        dump["studio_threads"] = Value::Array(threads_of(std::process::id()));
        dump["studio_status"] = read("/proc/self/status");
        if let Some(pid) = engine_pid {
            dump["engine"]["smaps_rollup"] = read(format!("/proc/{pid}/smaps_rollup"));
            let maps = read(format!("/proc/{pid}/maps"));
            dump["engine"]["maps"] = maps.as_str().map(|text| Value::String(text.lines().take(4000).collect::<Vec<_>>().join("\n"))).unwrap_or(maps);
            dump["engine"]["threads"] = Value::Array(threads_of(pid));
        }
        // the kernel's out-of-memory record, when the journal lets this account read it
        dump["oom_journal"] = command_output("journalctl", &["-k", "-n", "200", "--no-pager", "-g", "oom|Out of memory|Killed process"]);
        if let Some(unit) = crate::governor::page()["unit"].as_str() {
            dump["scope_status"] = command_output("systemctl", &["--user", "status", "--no-pager", &format!("{unit}.scope")]);
        }
    }
    if level == DumpType::Full {
        dump["core"] = match engine_pid {
            None => Value::String("no engine running: no core file".into()),
            Some(pid) => {
                let core = dir.join(format!("{name}-engine"));
                match std::process::Command::new("gcore").arg("-o").arg(&core).arg(pid.to_string()).stdin(std::process::Stdio::null()).output() {
                    Ok(output) if output.status.success() => Value::String(format!("{}.{pid}", core.display())),
                    Ok(output) => Value::String(format!(
                        "gcore failed (kernel.yama.ptrace_scope must be 0: sudo sysctl kernel.yama.ptrace_scope=0): {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                    Err(error) => Value::String(format!("gcore is not installed (sudo apt install gdb): {error}")),
                }
            }
        };
    }
    let path = dir.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&dump).unwrap_or_default())?;
    // the previous run's full logs beside an unexpected end
    if trigger == Trigger::UnexpectedEnd {
        if let Some(studio_log) = &studio_log {
            let _ = std::fs::copy(studio_log, dir.join(format!("{name}-studio.log")));
        }
        if let Some(engine_log) = &engine_log {
            let _ = std::fs::copy(engine_log, dir.join(format!("{name}-engine.log")));
        }
    }
    crate::studio_log!("[DIAGNOSTICS] state dump ({level:?}) written: {}", path.display());
    prune(&dir, settings.keep.max(1) as usize);
    Ok(path)
}

/// Keeps the newest `keep` dumps (a dump is its .json with the files that share its name).
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut dumps: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("state-") && name.ends_with(".json"))
        .map(|name| name.trim_end_matches(".json").to_string())
        .collect();
    dumps.sort();
    let excess = dumps.len().saturating_sub(keep);
    for stem in dumps.into_iter().take(excess) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&stem) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Settings -> Diagnostics: the settings, the folders and the dumps there.
pub fn page() -> Value {
    let dir = dumps_folder();
    let mut dumps: Vec<Value> = dir
        .as_ref()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
                .map(|entry| json!({ "name": entry.file_name().to_string_lossy(), "bytes": entry.metadata().map(|meta| meta.len()).unwrap_or(0) }))
                .collect()
        })
        .unwrap_or_default();
    dumps.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
    json!({
        "settings": settings(),
        "defaults": Settings::default(),
        "dumps_folder": dir,
        "logs_folder": logs_folder(),
        "dumps": dumps,
        "gcore": which("gcore"),
        "ptrace_scope": std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").ok().map(|text| text.trim().to_string()),
    })
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_is_extended_except_for_a_pause() {
        assert_eq!(level_for(Trigger::Error, DumpType::Automatic), DumpType::Extended);
        assert_eq!(level_for(Trigger::Pause, DumpType::Automatic), DumpType::Small);
        assert_eq!(level_for(Trigger::Manual, DumpType::None), DumpType::Extended);
        assert_eq!(level_for(Trigger::Error, DumpType::None), DumpType::None);
        assert_eq!(level_for(Trigger::Pause, DumpType::Full), DumpType::Full);
    }

    #[test]
    fn the_folder_keeps_the_newest_dumps() {
        let dir = std::env::temp_dir().join(format!("studio-dumps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for stamp in ["2026-01-01", "2026-01-02", "2026-01-03"] {
            std::fs::write(dir.join(format!("state-{stamp}-error.json")), "{}").unwrap();
            std::fs::write(dir.join(format!("state-{stamp}-error-engine.log")), "").unwrap();
        }
        prune(&dir, 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, vec!["state-2026-01-02-error-engine.log", "state-2026-01-02-error.json", "state-2026-01-03-error-engine.log", "state-2026-01-03-error.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
