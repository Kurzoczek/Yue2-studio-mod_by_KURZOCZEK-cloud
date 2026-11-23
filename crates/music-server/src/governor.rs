//! The resource governor (Debian): Settings -> Memory and Settings -> Processor, ported from the Android app
//! "AI Music Generator and Cover Maker" (`AppSettings` resources, `ResourceGovernor`, `MemoryPlanner`).
//!
//! What it governs is the native engine (acestep.cpp `ace-server`, yue2.cpp `yue-server`), the one process that
//! holds gigabytes of weights and every core for minutes at a time:
//!  - at every start: the thread count, the cores, the priority, a systemd scope with the memory limit, and the
//!    memory plan's launch flags (`music_core::limits`);
//!  - while it runs: the CPU limit and the overheating protection, through the scope's CPUQuota (the kernel keeps
//!    the engine answering, just slower), or a SIGSTOP / SIGCONT duty cycle when there is no systemd scope; and,
//!    without a scope, a watchdog for the memory limit.
//!
//! The settings live in `governor.json` in the studio's data folder; what was measured (the engine's peak memory)
//! in `governor-state.json`. Nothing here needs root.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// Left to the system and the studio's own window in "Auto" (the Android app keeps 1.5 GB for Android).
const SYSTEM_RESERVE_MIB: u64 = 1536;
/// The overheating protection lets the engine work again this many degrees below the level it paused at.
const THERMAL_HYSTERESIS_C: f64 = 8.0;

/// Which engine this studio runs: the launch flags of the memory plan differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// acestep.cpp `ace-server` (ACE-Step Studio).
    Ace,
    /// yue2.cpp `yue-server` (YuE2 Studio).
    Yue,
}

impl Engine {
    fn process_names(self) -> &'static [&'static str] {
        match self {
            Engine::Ace => &["ace-server"],
            Engine::Yue => &["yue-server"],
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Engine::Ace => "ace-step-studio",
            Engine::Yue => "yue2-studio",
        }
    }
}

/// The cores the engine may run on. On an Intel hybrid processor the performance (P) and efficiency (E) cores are
/// told apart; on a processor with one kind of core all three choices are the same.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoreSet {
    #[default]
    All,
    Performance,
    Efficiency,
}

/// Process priority, named like the Windows Task Manager levels and mapped to the Linux nice value. Above normal
/// needs CAP_SYS_NICE (or a raised RLIMIT_NICE): without it the kernel refuses and the engine runs at normal.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl Priority {
    pub fn nice(self) -> i32 {
        match self {
            Priority::Low => 19,
            Priority::BelowNormal => 10,
            Priority::Normal => 0,
            Priority::AboveNormal => -5,
            Priority::High => -10,
            Priority::Realtime => -20,
        }
    }
}

/// What happens when the engine reaches the RAM limit.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnMemoryLimit {
    /// The kernel reclaims the engine's memory (and swaps) instead of letting it grow: slower, but the song is
    /// finished - like the Android app, which works below the model's needs instead of stopping.
    #[default]
    Slow,
    /// The engine is stopped and the song fails with "RAM limit"; the computer never runs out of memory.
    Stop,
}

/// How large the pieces are that the sound decoder (VAE) works in. Smaller pieces need less RAM at the end of a
/// song and take a little longer.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecoderTiles {
    /// Chosen from the RAM limit: smaller when the limit is low.
    #[default]
    Auto,
    /// The engine's own default.
    Large,
    Medium,
    Small,
    Tiny,
}

/// When the overheating protection steps in.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalLevel {
    /// 80 °C.
    Moderate,
    /// 90 °C.
    #[default]
    Severe,
    /// 95 °C.
    Critical,
}

impl ThermalLevel {
    pub fn celsius(self) -> f64 {
        match self {
            ThermalLevel::Moderate => 80.0,
            ThermalLevel::Severe => 90.0,
            ThermalLevel::Critical => 95.0,
        }
    }
}

/// Settings -> Memory and Settings -> Processor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// true: the limit is the computer's RAM minus a reserve for the system.
    pub ram_auto: bool,
    /// The limit when `ram_auto` is off; no upper cap other than the computer's RAM.
    pub ram_limit_mb: u64,
    pub on_memory_limit: OnMemoryLimit,
    pub decoder_tiles: DecoderTiles,
    /// 0 = automatic: one per core the engine may use.
    pub threads: u32,
    /// 10..100 % of the cores the engine may use.
    pub cpu_max_percent: u32,
    pub core_set: CoreSet,
    /// 0 = every core of the chosen set, else the first N of it.
    pub core_count: u32,
    pub priority: Priority,
    pub thermal_protection: bool,
    pub thermal_level: ThermalLevel,
    /// The CPU limit while the computer is too hot (the engine keeps answering, so nothing times out).
    pub thermal_throttle_percent: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ram_auto: true,
            ram_limit_mb: 4096,
            on_memory_limit: OnMemoryLimit::Slow,
            decoder_tiles: DecoderTiles::Auto,
            threads: 0,
            cpu_max_percent: 100,
            core_set: CoreSet::All,
            core_count: 0,
            // nice 10: the computer stays responsive while a song is made
            priority: Priority::BelowNormal,
            thermal_protection: true,
            thermal_level: ThermalLevel::Severe,
            thermal_throttle_percent: 10,
        }
    }
}

impl Settings {
    fn sanitized(mut self) -> Self {
        self.cpu_max_percent = self.cpu_max_percent.clamp(10, 100);
        self.thermal_throttle_percent = self.thermal_throttle_percent.clamp(5, 100);
        self.ram_limit_mb = self.ram_limit_mb.max(512);
        self.threads = self.threads.min(1024);
        self
    }

    /// The fields read when the engine starts: changing one restarts it.
    fn start_fields(&self) -> (bool, u64, OnMemoryLimit, DecoderTiles, u32, CoreSet, u32, Priority) {
        (self.ram_auto, self.ram_limit_mb, self.on_memory_limit, self.decoder_tiles, self.threads, self.core_set, self.core_count, self.priority)
    }
}

// ------------------------------------------------------------------------------------------------ the machine

/// Logical CPUs of this computer by kind.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Topology {
    pub all: Vec<usize>,
    pub performance: Vec<usize>,
    pub efficiency: Vec<usize>,
}

impl Topology {
    pub fn read() -> Self {
        let all = read_cpu_list("/sys/devices/system/cpu/online")
            .filter(|cpus| !cpus.is_empty())
            .unwrap_or_else(|| (0..std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)).collect());
        // Intel hybrid: the two PMUs list their cores
        let mut performance = read_cpu_list("/sys/devices/cpu_core/cpus").unwrap_or_default();
        let mut efficiency = read_cpu_list("/sys/devices/cpu_atom/cpus").unwrap_or_default();
        if performance.is_empty() || efficiency.is_empty() {
            // ARM big.LITTLE and AMD: the scheduler's capacity of each CPU
            let capacities: Vec<(usize, u64)> = all
                .iter()
                .filter_map(|&cpu| {
                    std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity"))
                        .ok()
                        .and_then(|text| text.trim().parse().ok())
                        .map(|capacity| (cpu, capacity))
                })
                .collect();
            let top = capacities.iter().map(|(_, capacity)| *capacity).max();
            performance.clear();
            efficiency.clear();
            if let Some(top) = top {
                if capacities.len() == all.len() && capacities.iter().any(|(_, capacity)| *capacity < top) {
                    for (cpu, capacity) in capacities {
                        if capacity == top { performance.push(cpu) } else { efficiency.push(cpu) }
                    }
                }
            }
        }
        Self { all, performance, efficiency }
    }

    pub fn hybrid(&self) -> bool {
        !self.performance.is_empty() && !self.efficiency.is_empty()
    }

    /// The CPUs a choice gives the engine; the whole machine when the choice names nothing here.
    pub fn cpus_for(&self, set: CoreSet, count: u32) -> Vec<usize> {
        let mut cpus = match set {
            CoreSet::All => self.all.clone(),
            CoreSet::Performance if self.hybrid() => self.performance.clone(),
            CoreSet::Efficiency if self.hybrid() => self.efficiency.clone(),
            _ => self.all.clone(),
        };
        if count > 0 && (count as usize) < cpus.len() {
            cpus.truncate(count as usize);
        }
        cpus
    }
}

/// "0-3,8,10-11" as the kernel writes CPU lists.
pub fn parse_cpu_list(text: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in text.trim().split(',').map(str::trim).filter(|part| !part.is_empty()) {
        match part.split_once('-') {
            Some((from, to)) => {
                if let (Ok(from), Ok(to)) = (from.parse::<usize>(), to.parse::<usize>()) {
                    if from <= to && to - from < 4096 {
                        cpus.extend(from..=to);
                    }
                }
            }
            None => {
                if let Ok(cpu) = part.parse() {
                    cpus.push(cpu);
                }
            }
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    cpus
}

fn read_cpu_list(path: &str) -> Option<Vec<usize>> {
    std::fs::read_to_string(path).ok().map(|text| parse_cpu_list(&text))
}

/// /proc/meminfo in bytes: (MemTotal, MemAvailable, SwapTotal, SwapFree).
pub fn meminfo() -> (u64, u64, u64, u64) {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kib| kib * 1024)
            .unwrap_or(0)
    };
    (field("MemTotal:"), field("MemAvailable:"), field("SwapTotal:"), field("SwapFree:"))
}

/// The hottest CPU sensor: (°C, its name). Thermal zones first (x86_pkg_temp, TCPU, cpu, acpitz...), then hwmon
/// coretemp / k10temp. None on a computer that reports no temperature.
pub fn cpu_temperature() -> Option<(f64, String)> {
    let mut best: Option<(f64, String)> = None;
    let mut consider = |celsius: f64, name: String| {
        if (1.0..150.0).contains(&celsius) && best.as_ref().is_none_or(|(hottest, _)| celsius > *hottest) {
            best = Some((celsius, name));
        }
    };
    if let Ok(zones) = std::fs::read_dir("/sys/class/thermal") {
        for zone in zones.flatten() {
            let path = zone.path();
            if !zone.file_name().to_string_lossy().starts_with("thermal_zone") {
                continue;
            }
            let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default().trim().to_string();
            let lower = kind.to_ascii_lowercase();
            let cpu_like = ["x86_pkg", "cpu", "tcpu", "acpitz", "soc", "package", "tctl", "k10"].iter().any(|word| lower.contains(word));
            if !cpu_like {
                continue;
            }
            if let Some(milli) = std::fs::read_to_string(path.join("temp")).ok().and_then(|text| text.trim().parse::<f64>().ok()) {
                consider(milli / 1000.0, kind);
            }
        }
    }
    if let Ok(monitors) = std::fs::read_dir("/sys/class/hwmon") {
        for monitor in monitors.flatten() {
            let path = monitor.path();
            let name = std::fs::read_to_string(path.join("name")).unwrap_or_default().trim().to_string();
            if !["coretemp", "k10temp", "zenpower", "cpu_thermal"].contains(&name.as_str()) {
                continue;
            }
            for index in 1..=64 {
                let Ok(text) = std::fs::read_to_string(path.join(format!("temp{index}_input"))) else { continue };
                if let Ok(milli) = text.trim().parse::<f64>() {
                    consider(milli / 1000.0, name.clone());
                }
            }
        }
    }
    best
}

/// The lowest nice value this account may set (RLIMIT_NICE: 20 - rlim). 0 on a stock Debian account.
pub fn lowest_allowed_nice() -> i32 {
    #[cfg(target_os = "linux")]
    {
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        if unsafe { libc::getrlimit(libc::RLIMIT_NICE, &mut limit) } == 0 {
            let cur = limit.rlim_cur.min(40) as i32;
            return if cur == 0 { 0 } else { 20 - cur };
        }
    }
    0
}

// ------------------------------------------------------------------------------------------------ the state

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Measured {
    /// The engine's highest resident memory (VmHWM) seen, in MiB, and when.
    peak_mb: u64,
    peak_at: String,
}

struct State {
    settings: RwLock<Settings>,
    engine: OnceLock<Engine>,
    /// The engine's own log (its output since the first start), for state dumps.
    engine_log: OnceLock<PathBuf>,
    /// The scope unit of the engine started last.
    unit: Mutex<Option<String>>,
    starts: AtomicU32,
    thermal_throttled: AtomicBool,
    /// The CPU limit last given to the scope (or the duty cycle), in percent of the engine's cores.
    applied_percent: AtomicU32,
    events: Mutex<VecDeque<Value>>,
    measured: Mutex<Measured>,
    /// The engine the watchdog last saw: (pid, rss MiB, threads, nice, allowed CPUs).
    engine_seen: Mutex<Option<(u32, u64, u64, i32, String)>>,
    temperature: Mutex<Option<(f64, String)>>,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| State {
        settings: RwLock::new(load_settings()),
        engine: OnceLock::new(),
        engine_log: OnceLock::new(),
        unit: Mutex::new(None),
        starts: AtomicU32::new(0),
        thermal_throttled: AtomicBool::new(false),
        applied_percent: AtomicU32::new(100),
        events: Mutex::new(VecDeque::new()),
        measured: Mutex::new(load_measured()),
        engine_seen: Mutex::new(None),
        temperature: Mutex::new(None),
    })
}

fn data_file(name: &str) -> Option<PathBuf> {
    crate::studio_data_root().map(|root| root.join(name))
}

fn load_settings() -> Settings {
    data_file("governor.json")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default()
        .sanitized()
}

fn load_measured() -> Measured {
    data_file("governor-state.json")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_json(name: &str, value: &impl Serialize) -> std::io::Result<()> {
    let Some(path) = data_file(name) else { return Ok(()) };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = path.with_extension("json.part");
    std::fs::write(&part, serde_json::to_vec_pretty(value).unwrap_or_default())?;
    std::fs::rename(part, path)
}

/// Something the governor did, for the Activity log and the settings page.
fn event(kind: &str, text: String) {
    crate::studio_log!("[GOVERNOR] {text}");
    let mut events = state().events.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    events.push_back(json!({ "at": chrono_now(), "kind": kind, "text": text }));
    while events.len() > 60 {
        events.pop_front();
    }
    crate::diagnostics_dump::governor_event(kind, &text);
}

fn chrono_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn settings() -> Settings {
    state().settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

// ------------------------------------------------------------------------------------------------ at a start

/// The RAM limit in bytes for these settings.
pub fn limit_bytes(settings: &Settings) -> u64 {
    let (total, ..) = meminfo();
    let auto = total.saturating_sub(SYSTEM_RESERVE_MIB * MIB).max(GIB);
    if settings.ram_auto { auto } else { settings.ram_limit_mb * MIB }
}

/// The decoder tile flags for an engine and a limit.
fn decoder_args(engine: Engine, tiles: DecoderTiles, limit: u64) -> Vec<String> {
    let tiles = match tiles {
        DecoderTiles::Auto if limit < 6 * GIB => DecoderTiles::Small,
        DecoderTiles::Auto if limit < 10 * GIB => DecoderTiles::Medium,
        DecoderTiles::Auto => DecoderTiles::Large,
        other => other,
    };
    let (flag, value) = match (engine, tiles) {
        (_, DecoderTiles::Large | DecoderTiles::Auto) => return Vec::new(),
        (Engine::Ace, DecoderTiles::Medium) => ("--vae-chunk", 512),
        (Engine::Ace, DecoderTiles::Small) => ("--vae-chunk", 256),
        (Engine::Ace, DecoderTiles::Tiny) => ("--vae-chunk", 128),
        (Engine::Yue, DecoderTiles::Medium) => ("--vae-core", 256),
        (Engine::Yue, DecoderTiles::Small) => ("--vae-core", 128),
        (Engine::Yue, DecoderTiles::Tiny) => ("--vae-core", 64),
    };
    vec![flag.into(), value.to_string()]
}

/// The CPU limit in force now, in percent of the engine's cores: the setting, or the throttle while too hot.
fn wanted_percent(settings: &Settings) -> u32 {
    if settings.thermal_protection && state().thermal_throttled.load(Ordering::Relaxed) {
        settings.thermal_throttle_percent.min(settings.cpu_max_percent)
    } else {
        settings.cpu_max_percent
    }
}

/// Everything the next start of the engine is limited by. Called when the engine's launch options are built.
pub fn engine_limits(engine: Engine) -> music_core::limits::Limits {
    let _ = state().engine.set(engine);
    let settings = settings();
    let topology = Topology::read();
    let cpus = topology.cpus_for(settings.core_set, settings.core_count);
    let limit = limit_bytes(&settings);
    let threads = if settings.threads > 0 { settings.threads } else { cpus.len().max(1) as u32 };
    let percent = wanted_percent(&settings);
    let scope = music_core::limits::systemd_scopes_available().then(|| {
        let n = state().starts.fetch_add(1, Ordering::Relaxed) + 1;
        let unit = format!("{}-engine-{}-{n}", engine.slug(), std::process::id());
        *state().unit.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(unit.clone());
        let (high, max) = match settings.on_memory_limit {
            OnMemoryLimit::Slow => (Some(limit), None),
            OnMemoryLimit::Stop => (Some(limit / 100 * 95), Some(limit)),
        };
        music_core::limits::Scope {
            unit,
            cpu_quota_percent: (percent < 100).then(|| percent * cpus.len().max(1) as u32),
            memory_high_bytes: high,
            memory_max_bytes: max,
        }
    });
    state().applied_percent.store(percent, Ordering::Relaxed);
    let limits = music_core::limits::Limits {
        threads: Some(threads),
        nice: (settings.priority != Priority::Normal).then(|| settings.priority.nice()),
        cpus: if cpus.len() == topology.all.len() { Vec::new() } else { cpus.clone() },
        scope,
        args: decoder_args(engine, settings.decoder_tiles, limit),
    };
    event(
        "start",
        format!(
            "engine start: {threads} thread(s) on {} core(s){}, priority {:?} (nice {}), RAM limit {:.1} GB ({:?}){}{}",
            cpus.len(),
            if limits.cpus.is_empty() { String::new() } else { format!(" [{}]", list_text(&cpus)) },
            settings.priority,
            settings.priority.nice(),
            limit as f64 / GIB as f64,
            settings.on_memory_limit,
            if percent < 100 { format!(", CPU limit {percent} %") } else { String::new() },
            match &limits.scope {
                Some(scope) => format!(", systemd scope {}", scope.unit),
                None => ", no systemd scope (duty cycle and watchdog)".into(),
            },
        ),
    );
    limits
}

fn list_text(cpus: &[usize]) -> String {
    cpus.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
}

// ------------------------------------------------------------------------------------------------ while it runs

/// Engine processes: children of the studio whose executable has the engine's name.
fn engine_pids(engine: Engine) -> Vec<u32> {
    let me = std::process::id();
    let names = engine.process_names();
    let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()))
        .filter(|&pid| {
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
            // the command is in parentheses and may hold spaces: fields after the last ')'
            let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else { return false };
            let ppid = rest.split_whitespace().nth(1).and_then(|value| value.parse::<u32>().ok());
            if ppid != Some(me) {
                return false;
            }
            let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
            let name = exe.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned());
            name.is_some_and(|name| names.iter().any(|wanted| name == *wanted))
        })
        .collect()
}

/// (VmRSS MiB, VmHWM MiB, Threads, nice, Cpus_allowed_list) of a process.
fn process_facts(pid: u32) -> Option<(u64, u64, u64, i32, String)> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    let text = |name: &str| status.lines().find_map(|line| line.strip_prefix(name)).map(|rest| rest.trim().to_string()).unwrap_or_default();
    let nice = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(16).and_then(|value| value.parse().ok())))
        .unwrap_or(0);
    Some((kib("VmRSS:") / 1024, kib("VmHWM:") / 1024, kib("Threads:"), nice, text("Cpus_allowed_list:")))
}

#[cfg(target_os = "linux")]
fn signal(pid: u32, signal: libc::c_int) {
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
}

/// Starts the watchdog thread, once. `engine` says which processes are the engine, `engine_log` where it writes.
pub fn start(engine: Engine, engine_log: PathBuf) {
    static STARTED: OnceLock<()> = OnceLock::new();
    let _ = state().engine.set(engine);
    let _ = state().engine_log.set(engine_log);
    STARTED.get_or_init(|| {
        crate::diagnostics_dump::mark_running();
        let _ = std::thread::Builder::new().name("resource-governor".into()).spawn(move || watch(engine));
    });
}

fn watch(engine: Engine) {
    const PERIOD: Duration = Duration::from_millis(500);
    let mut last_look = Instant::now() - Duration::from_secs(10);
    let mut warned_ram = false;
    let mut stopped: Vec<u32> = Vec::new();
    loop {
        let settings = settings();
        let pids = engine_pids(engine);
        // ---- every two seconds: memory, temperature, the scope's CPU limit
        if last_look.elapsed() >= Duration::from_secs(2) {
            last_look = Instant::now();
            look(&settings, &pids, &mut warned_ram);
        }
        // ---- the duty cycle, only without a systemd scope
        let percent = wanted_percent(&settings);
        let scoped = music_core::limits::systemd_scopes_available() && state().unit.lock().map(|unit| unit.is_some()).unwrap_or(false);
        #[cfg(target_os = "linux")]
        {
            if !scoped && percent < 100 && !pids.is_empty() {
                let run = PERIOD * percent / 100;
                for &pid in &pids {
                    signal(pid, libc::SIGCONT);
                }
                std::thread::sleep(run);
                for &pid in &pids {
                    signal(pid, libc::SIGSTOP);
                }
                stopped = pids.clone();
                std::thread::sleep(PERIOD - run);
                continue;
            }
            for pid in stopped.drain(..) {
                signal(pid, libc::SIGCONT);
            }
        }
        let _ = scoped;
        std::thread::sleep(PERIOD);
    }
}

fn look(settings: &Settings, pids: &[u32], warned_ram: &mut bool) {
    let s = state();
    // temperature and the overheating protection
    let temperature = cpu_temperature();
    *s.temperature.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = temperature.clone();
    if let Some((celsius, sensor)) = &temperature {
        let pause_at = settings.thermal_level.celsius();
        let throttled = s.thermal_throttled.load(Ordering::Relaxed);
        if settings.thermal_protection && !throttled && *celsius >= pause_at {
            s.thermal_throttled.store(true, Ordering::Relaxed);
            event("thermal", format!("too hot: {celsius:.0} °C ({sensor}) >= {pause_at:.0} °C - the engine is held to {} % until it cools down", settings.thermal_throttle_percent));
        } else if throttled && (!settings.thermal_protection || *celsius <= pause_at - THERMAL_HYSTERESIS_C) {
            s.thermal_throttled.store(false, Ordering::Relaxed);
            event("thermal", format!("cooled down to {celsius:.0} °C - the engine works at {} % again", settings.cpu_max_percent));
        }
    }
    // the scope's CPU limit, when it should change
    let percent = wanted_percent(settings);
    if percent != s.applied_percent.load(Ordering::Relaxed) {
        let unit = s.unit.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        if let Some(unit) = unit {
            let cores = Topology::read().cpus_for(settings.core_set, settings.core_count).len().max(1) as u32;
            let property = if percent >= 100 { "CPUQuota=".to_string() } else { format!("CPUQuota={}%", percent * cores) };
            if music_core::limits::set_scope_property(&unit, &property) {
                s.applied_percent.store(percent, Ordering::Relaxed);
            }
        } else {
            s.applied_percent.store(percent, Ordering::Relaxed);
        }
    }
    // the engine's memory
    let Some(&pid) = pids.first() else {
        *s.engine_seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *warned_ram = false;
        return;
    };
    let Some((rss, hwm, threads, nice, allowed)) = process_facts(pid) else { return };
    *s.engine_seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((pid, rss, threads, nice, allowed));
    {
        let mut measured = s.measured.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if hwm > measured.peak_mb {
            measured.peak_mb = hwm;
            measured.peak_at = chrono_now();
            let _ = write_json("governor-state.json", &*measured);
        }
    }
    let limit_mb = limit_bytes(settings) / MIB;
    let scoped = s.unit.lock().map(|unit| unit.is_some()).unwrap_or(false) && music_core::limits::systemd_scopes_available();
    if rss > limit_mb {
        if !scoped && settings.on_memory_limit == OnMemoryLimit::Stop {
            event("memory", format!("the engine took {rss} MB, above the RAM limit of {limit_mb} MB - it is stopped (Settings -> Memory -> When the limit is reached: stop)"));
            #[cfg(target_os = "linux")]
            signal(pid, libc::SIGKILL);
        } else if !*warned_ram {
            *warned_ram = true;
            event(
                "memory",
                if scoped {
                    format!("the engine reached the RAM limit ({rss} of {limit_mb} MB): the system reclaims its memory, so it works slower")
                } else {
                    format!("the engine took {rss} MB, above the RAM limit of {limit_mb} MB (no systemd scope here to hold it there)")
                },
            );
        }
    }
}

// ------------------------------------------------------------------------------------------------ the page

/// Settings -> Memory and Processor: the settings and everything measured.
pub fn page() -> Value {
    let s = state();
    let settings = settings();
    let topology = Topology::read();
    let (total, available, swap_total, swap_free) = meminfo();
    let auto = Settings { ram_auto: true, ..settings.clone() };
    let engine = s.engine_seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let measured = s.measured.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let temperature = s.temperature.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().or_else(cpu_temperature);
    let events: Vec<Value> = s.events.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter().cloned().collect();
    let limit = limit_bytes(&settings);
    let kind = s.engine.get().copied().unwrap_or(Engine::Ace);
    json!({
        "settings": settings,
        "defaults": Settings::default(),
        "memory": {
            "total_mb": total / MIB,
            "available_mb": available / MIB,
            "swap_total_mb": swap_total / MIB,
            "swap_free_mb": swap_free / MIB,
            "auto_limit_mb": limit_bytes(&auto) / MIB,
            "limit_mb": limit / MIB,
            "reserve_mb": SYSTEM_RESERVE_MIB,
            "peak_mb": measured.peak_mb,
            "peak_at": measured.peak_at,
            "fits": measured.peak_mb == 0 || measured.peak_mb * MIB <= limit,
            "decoder_args": decoder_args(kind, settings.decoder_tiles, limit),
        },
        "cpu": {
            "cores": topology.all.len(),
            "performance": topology.performance,
            "efficiency": topology.efficiency,
            "hybrid": topology.hybrid(),
            "chosen": topology.cpus_for(settings.core_set, settings.core_count),
            "lowest_nice": lowest_allowed_nice(),
            "applied_percent": s.applied_percent.load(Ordering::Relaxed),
        },
        "thermal": {
            "available": temperature.is_some(),
            "celsius": temperature.as_ref().map(|(celsius, _)| *celsius),
            "sensor": temperature.as_ref().map(|(_, sensor)| sensor.clone()),
            "throttled": s.thermal_throttled.load(Ordering::Relaxed),
            "pause_at": settings.thermal_level.celsius(),
            "resume_at": settings.thermal_level.celsius() - THERMAL_HYSTERESIS_C,
        },
        "engine": engine.map(|(pid, rss, threads, nice, allowed)| json!({
            "pid": pid, "rss_mb": rss, "threads": threads, "nice": nice, "cpus": allowed,
        })),
        "systemd_scope": music_core::limits::systemd_scopes_available(),
        "unit": s.unit.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone(),
        "events": events,
    })
}

/// Saves new settings. Returns whether the engine has to restart for them (thread count, cores, priority, memory
/// plan); the CPU limit and the overheating protection change while it runs.
pub fn save(new: Settings) -> std::io::Result<bool> {
    let new = new.sanitized();
    let restart = {
        let mut current = state().settings.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let restart = current.start_fields() != new.start_fields();
        *current = new.clone();
        restart
    };
    write_json("governor.json", &new)?;
    event("settings", format!("settings saved{}", if restart { " - the engine restarts to take them" } else { "" }));
    Ok(restart)
}

/// A short summary for a state dump.
pub fn summary() -> Value {
    page()
}

/// Which engine this studio runs (set by `start` and `engine_limits`).
pub fn kind() -> Engine {
    state().engine.get().copied().unwrap_or(Engine::Ace)
}

/// The engine's log file, once `start` has run.
pub fn engine_log_path() -> Option<PathBuf> {
    state().engine_log.get().cloned()
}

/// The engine process the watchdog sees now.
pub fn engine_pid() -> Option<u32> {
    state().engine.get().and_then(|engine| engine_pids(*engine).first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_lists_read_like_the_kernel_writes_them() {
        assert_eq!(parse_cpu_list("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(parse_cpu_list(""), Vec::<usize>::new());
        assert_eq!(parse_cpu_list("5"), vec![5]);
        assert_eq!(parse_cpu_list("3-1,x"), Vec::<usize>::new());
    }

    #[test]
    fn a_core_choice_falls_back_to_the_whole_machine_without_hybrid_cores() {
        let flat = Topology { all: vec![0, 1, 2, 3], performance: vec![], efficiency: vec![] };
        assert_eq!(flat.cpus_for(CoreSet::Efficiency, 0), vec![0, 1, 2, 3]);
        assert_eq!(flat.cpus_for(CoreSet::All, 2), vec![0, 1]);
        let hybrid = Topology { all: (0..12).collect(), performance: (0..4).collect(), efficiency: (4..12).collect() };
        assert_eq!(hybrid.cpus_for(CoreSet::Efficiency, 0), (4..12).collect::<Vec<_>>());
        assert_eq!(hybrid.cpus_for(CoreSet::Performance, 2), vec![0, 1]);
    }

    #[test]
    fn a_low_limit_asks_for_smaller_decoder_tiles() {
        assert_eq!(decoder_args(Engine::Ace, DecoderTiles::Auto, 4 * GIB), vec!["--vae-chunk", "256"]);
        assert_eq!(decoder_args(Engine::Ace, DecoderTiles::Auto, 8 * GIB), vec!["--vae-chunk", "512"]);
        assert!(decoder_args(Engine::Ace, DecoderTiles::Auto, 16 * GIB).is_empty());
        assert_eq!(decoder_args(Engine::Yue, DecoderTiles::Tiny, 16 * GIB), vec!["--vae-core", "64"]);
        assert!(decoder_args(Engine::Yue, DecoderTiles::Large, GIB).is_empty());
    }

    #[test]
    fn priorities_are_nice_values() {
        assert_eq!(Priority::Low.nice(), 19);
        assert_eq!(Priority::BelowNormal.nice(), 10);
        assert_eq!(Priority::Realtime.nice(), -20);
        assert_eq!(Settings::default().priority, Priority::BelowNormal);
    }

    #[test]
    fn only_start_fields_need_a_restart() {
        let base = Settings::default();
        let live = Settings { cpu_max_percent: 50, thermal_level: ThermalLevel::Moderate, ..base.clone() };
        assert_eq!(base.start_fields(), live.start_fields());
        let start = Settings { threads: 2, ..base.clone() };
        assert_ne!(base.start_fields(), start.start_fields());
    }

    #[test]
    fn old_or_partial_settings_read_with_defaults() {
        let read: Settings = serde_json::from_str(r#"{"cpu_max_percent": 3, "priority": "low"}"#).unwrap();
        let read = read.sanitized();
        assert_eq!(read.cpu_max_percent, 10);
        assert_eq!(read.priority, Priority::Low);
        assert!(read.ram_auto);
    }

    #[test]
    fn the_date_is_rfc3339() {
        let now = chrono_now();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.as_bytes()[10] == b'T' && now.ends_with('Z'));
    }
}
