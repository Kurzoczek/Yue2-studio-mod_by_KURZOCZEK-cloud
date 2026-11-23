//! How fast things go now (Debian build): the bytes coming in from the network for every download, and what each
//! disk the studio keeps its folders on reads and writes - the data folder and every download folder picked
//! elsewhere (a USB disk, say). Asked by the window about once a second; the rates are over the time between two
//! askings, at least half a second.
//!
//! The disks are read from the kernel's own counters (`/sys/class/block/<device>/stat`), so they show everything
//! that disk does - the studio's downloads and an engine loading its model, and any other program's work there too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;

static RECEIVED: AtomicU64 = AtomicU64::new(0);

/// Bytes of a download arrived from the network.
pub fn received(bytes: u64) {
    RECEIVED.fetch_add(bytes, Ordering::Relaxed);
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Disk {
    /// The kernel's name of it: `sdb1`, `mmcblk0p2`, `loop0`.
    pub device: String,
    /// Where it is mounted.
    pub mount: String,
    /// Its file system: `ntfs3`, `fuseblk`, `btrfs`...
    pub file_system: String,
    /// The studio's folders on it: `data` for the data folder, else the download folders' names.
    pub folders: Vec<String>,
    pub read_bytes_per_second: u64,
    pub write_bytes_per_second: u64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Activity {
    pub download_bytes_per_second: u64,
    pub disks: Vec<Disk>,
}

struct Sample {
    at: Instant,
    received: u64,
    /// Sectors read and written, by device.
    sectors: BTreeMap<String, (u64, u64)>,
    activity: Activity,
}

static LAST: Mutex<Option<Sample>> = Mutex::new(None);

/// What goes on now, for the data folder `data_root` and its download folders.
pub fn now(data_root: &Path) -> Activity {
    let mut places: Vec<(String, PathBuf)> = vec![("data".to_owned(), data_root.to_path_buf())];
    for name in crate::model_storage::FOLDERS {
        let folder = crate::model_storage::folder(data_root, name);
        if folder.exists() {
            places.push(((*name).to_owned(), folder));
        }
    }
    let mounts = mounts();
    let mut disks: BTreeMap<String, Disk> = BTreeMap::new();
    for (label, place) in places {
        let Some(mount) = mount_of(&place, &mounts) else { continue };
        let Some(device) = block_device(&mount.source) else { continue };
        let disk = disks.entry(device.clone()).or_insert_with(|| Disk {
            device,
            mount: mount.point.display().to_string(),
            file_system: mount.file_system.clone(),
            ..Disk::default()
        });
        if !disk.folders.contains(&label) && label != crate::model_storage::MODEL_OBJECTS {
            disk.folders.push(label);
        }
    }
    let sectors: BTreeMap<String, (u64, u64)> = disks.keys().filter_map(|device| Some((device.clone(), sectors_of(device)?))).collect();
    let received = RECEIVED.load(Ordering::Relaxed);

    let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let at = Instant::now();
    let previous = last.take();
    let activity = match &previous {
        // asked again too soon (two windows, say): the last rates still hold
        Some(previous) if at.duration_since(previous.at).as_secs_f64() < 0.5 && previous.activity.disks.len() == disks.len() => {
            *last = previous_clone(previous);
            return previous.activity.clone();
        }
        Some(previous) => {
            let seconds = at.duration_since(previous.at).as_secs_f64().max(0.001);
            let rate = |now: u64, then: u64| (now.saturating_sub(then) as f64 / seconds) as u64;
            Activity {
                download_bytes_per_second: rate(received, previous.received),
                disks: disks
                    .into_values()
                    .map(|mut disk| {
                        if let (Some(now), Some(then)) = (sectors.get(&disk.device), previous.sectors.get(&disk.device)) {
                            disk.read_bytes_per_second = rate(now.0 * 512, then.0 * 512);
                            disk.write_bytes_per_second = rate(now.1 * 512, then.1 * 512);
                        }
                        disk
                    })
                    .collect(),
            }
        }
        // the first asking: nothing to measure against yet
        None => Activity { download_bytes_per_second: 0, disks: disks.into_values().collect() },
    };
    *last = Some(Sample { at, received, sectors, activity: activity.clone() });
    activity
}

fn previous_clone(sample: &Sample) -> Option<Sample> {
    Some(Sample { at: sample.at, received: sample.received, sectors: sample.sectors.clone(), activity: sample.activity.clone() })
}

#[derive(Debug, Clone)]
struct Mount {
    point: PathBuf,
    source: String,
    file_system: String,
}

/// The mounts, from `/proc/self/mountinfo`: its fields 5 (mount point), and after the ` - ` the file system and
/// the source. Spaces and the like come escaped as `\040`.
fn mounts() -> Vec<Mount> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else { return Vec::new() };
    text.lines().filter_map(parse_mount).collect()
}

fn parse_mount(line: &str) -> Option<Mount> {
    let (before, after) = line.split_once(" - ")?;
    let point = before.split(' ').nth(4)?;
    let mut after = after.split(' ');
    let file_system = after.next()?.to_owned();
    let source = after.next()?.to_owned();
    Some(Mount { point: PathBuf::from(unescape(point)), source: unescape(&source), file_system })
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' && at + 3 < bytes.len() && bytes[at + 1..at + 4].iter().all(|byte| (b'0'..=b'7').contains(byte)) {
            out.push(u8::from_str_radix(&text[at + 1..at + 4], 8).unwrap_or(b'?'));
            at += 4;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The mount a path lies on: the longest mount point it is inside of.
fn mount_of(path: &Path, mounts: &[Mount]) -> Option<Mount> {
    let path = std::fs::canonicalize(path).ok()?;
    mounts.iter().filter(|mount| path.starts_with(&mount.point)).max_by_key(|mount| mount.point.as_os_str().len()).cloned()
}

/// The kernel's name of the block device a mount's source names: `/dev/sdb1` gives `sdb1`, a `/dev/mapper/...` or
/// `/dev/disk/by-uuid/...` link what it leads to. None for what is no disk (tmpfs, a network share).
fn block_device(source: &str) -> Option<String> {
    if !source.starts_with("/dev/") {
        return None;
    }
    let real = std::fs::canonicalize(source).unwrap_or_else(|_| PathBuf::from(source));
    let name = real.file_name()?.to_str()?.to_owned();
    Path::new("/sys/class/block").join(&name).join("stat").exists().then_some(name)
}

/// Sectors (512 bytes, whatever the disk's own) read and written so far: fields 3 and 7 of the device's `stat`.
fn sectors_of(device: &str) -> Option<(u64, u64)> {
    let text = std::fs::read_to_string(Path::new("/sys/class/block").join(device).join("stat")).ok()?;
    parse_stat(&text)
}

fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let fields: Vec<u64> = text.split_whitespace().filter_map(|field| field.parse().ok()).collect();
    Some((*fields.get(2)?, *fields.get(6)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mount_line_gives_its_place_source_and_file_system() {
        let line = r"36 25 8:17 / /media/kurzoczek/Nowy\040dysk rw,nosuid,nodev,relatime shared:1 - ntfs3 /dev/sdb1 rw,uid=1000";
        let mount = parse_mount(line).unwrap();
        assert_eq!(mount.point, PathBuf::from("/media/kurzoczek/Nowy dysk"));
        assert_eq!((mount.source.as_str(), mount.file_system.as_str()), ("/dev/sdb1", "ntfs3"));
        let mounts = [
            Mount { point: "/".into(), source: "/dev/mmcblk0p2".into(), file_system: "btrfs".into() },
            mount,
        ];
        let inside = mounts.iter().filter(|m| Path::new("/media/kurzoczek/Nowy dysk/Modele").starts_with(&m.point)).max_by_key(|m| m.point.as_os_str().len()).unwrap();
        assert_eq!(inside.source, "/dev/sdb1", "the longest mount point a path is in");
    }

    #[test]
    fn a_stat_line_gives_sectors_read_and_written() {
        let text = "  12345     678  9000000   4321   23456    789 1600000  9876    0  5432  14197    0 0 0 0";
        assert_eq!(parse_stat(text), Some((9_000_000, 1_600_000)));
        assert_eq!(parse_stat("1 2"), None);
    }

    #[test]
    fn the_downloads_are_counted_and_measured_between_two_askings() {
        let data = std::env::temp_dir().join(format!("activity-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&data).unwrap();
        let _ = now(&data);
        std::thread::sleep(std::time::Duration::from_millis(600));
        received(3_000_000);
        let activity = now(&data);
        // at least these bytes over the time slept (other tests may add their own)
        assert!(activity.download_bytes_per_second > 1_000_000, "{activity:?}");
        std::fs::remove_dir_all(&data).ok();
    }
}
