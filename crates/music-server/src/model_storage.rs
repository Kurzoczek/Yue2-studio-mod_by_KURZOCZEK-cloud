//! Where the studio's downloads are kept: the model sets, the music engines and their models, the LoRAs, the stem
//! separator, karaoke, the audio.cpp runtime, the assistant's models and Audio to MIDI. They live in the studio's
//! data folder until a person picks another folder for any of them.
//!
//! Debian build: a folder picked for a download is where its files really are - no link is left in the data folder
//! (links cannot be made on NTFS, FAT or exFAT, where the data folder or the chosen one may well be). The studio keeps
//! the folder of each in `storage-folders.json` in its data folder, and every part of it that reads or writes those
//! files asks `folder()` where they are, every time, so a move takes effect at once. A link an older studio left in
//! the data folder is still followed, and taken away when that folder is moved again.
//! What a person made - the library, datasets, training runs - is never moved.
//!
//! A folder is copied whole before anything is taken away: on the same disk it is renamed, on another it is copied
//! beside its new name, checked file by file and only then published; the studio switches to it after that, and the
//! old copy goes last. A move that is stopped or fails leaves the folder where it was, still in use.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{api_error, ApiError, AppState};

/// The folders of the data folder that hold downloads, in the order they are shown and moved.
/// The trainer's models and runtime stay with the datasets and training runs, in the data folder.
pub const FOLDERS: &[&str] = &["models", "music-engines", "music-models", MODEL_OBJECTS, "native-music-runtimes", "adapters", "separation", "karaoke", "audiocpp", "engine-runtime", "assistant", "hotstep", "midi"];

/// The folders the studio starts programs from: the disk they go to has to let programs run.
const PROGRAMS: &[&str] = &["music-engines", "native-music-runtimes", "audiocpp", "engine-runtime", "hotstep", "assistant", "midi"];

/// The weights the engines' model packages share (`music-models` holds hard links to them). It goes wherever
/// `music-models` goes, so the links stay on one disk; it is not shown, its size counts with the models.
pub const MODEL_OBJECTS: &str = "music-model-objects";

/// Where the folder picked for each download is written down, in the data folder.
const CHOSEN_FILE: &str = "storage-folders.json";

/// The folder made inside the one a person picks, so the studio's folders never mix with what is there.
const HOME: &str = "YuE2 Studio models";

const STAGED: &str = ".arriving";
const LEAVING: &str = ".leaving";

/// A download writes its `.part` every few seconds; one touched this recently is still running.
const PART_FRESH: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FolderUse {
    pub name: &'static str,
    pub bytes: u64,
    /// Kept outside the data folder.
    pub elsewhere: bool,
    /// Where its files are: the folder picked for it, or the one in the data folder.
    pub location: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Storage {
    /// Where the downloads are now: the folder chosen, or the data folder.
    pub directory: String,
    /// The folder a person chose; none while everything is in the data folder.
    pub chosen: Option<String>,
    pub default: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub folders: Vec<FolderUse>,
}

/// What one folder's move came to.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Moved {
    pub name: &'static str,
    pub bytes: u64,
    /// Why it stayed where it was; none when it moved.
    pub error: Option<String>,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|name| name.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Where a link leads, when `path` is one.
pub fn link_target(path: &Path) -> Option<PathBuf> {
    let meta = fs::symlink_metadata(path).ok()?;
    #[cfg(windows)]
    {
        if junction::exists(path).unwrap_or(false) {
            return junction::get_target(path).ok();
        }
    }
    meta.file_type().is_symlink().then(|| fs::read_link(path).ok()).flatten()
}

fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        junction::create(target, link)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link)
    }
}

fn remove_link(link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        fs::remove_dir(link)
    }
    #[cfg(not(windows))]
    {
        fs::remove_file(link)
    }
}

/// Every file under `root`, added up; a link inside is not followed.
pub fn tree_bytes(root: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(root) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => tree_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

pub fn space(path: &Path) -> (u64, u64) {
    let existing = path.ancestors().find(|candidate| candidate.exists()).unwrap_or(path);
    (fs2::available_space(existing).unwrap_or(0), fs2::total_space(existing).unwrap_or(0))
}

/// The folder the downloads go into when a person picks `picked`: a folder of the studio's own inside it.
pub fn home_in(picked: &Path) -> PathBuf {
    if picked.file_name().is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(HOME)) {
        picked.to_path_buf()
    } else {
        picked.join(HOME)
    }
}

/// The folder a moved `name` sits in, from where its link leads: `<chosen>/training/models` gives `<chosen>`.
fn chosen_of(name: &str, target: &Path) -> Option<PathBuf> {
    let mut chosen = target;
    for _ in name.split('/') {
        chosen = chosen.parent()?;
    }
    Some(chosen.to_path_buf())
}

// ---- where each download folder is

type Chosen = std::collections::BTreeMap<String, PathBuf>;

fn chosen_cache() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Chosen>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, Chosen>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The folders picked for the downloads of `data_root`, as `storage-folders.json` names them. A name the studio does
/// not know, or a path that is not a full one, is left out.
fn chosen_all(data_root: &Path) -> Chosen {
    let mut cache = chosen_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .entry(data_root.to_path_buf())
        .or_insert_with(|| {
            fs::read(data_root.join(CHOSEN_FILE))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Chosen>(&bytes).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|(name, path)| FOLDERS.contains(&name.as_str()) && path.is_absolute())
                .collect()
        })
        .clone()
}

/// Writes down where `name` is kept: `Some` a picked folder, `None` back in the data folder. On disk first, whole,
/// and only then used.
fn set_chosen(data_root: &Path, name: &str, place: Option<&Path>) -> Result<()> {
    let mut all = chosen_all(data_root);
    match place {
        Some(place) => all.insert(name.to_owned(), place.to_path_buf()),
        None => all.remove(name),
    };
    let mut file = tempfile::NamedTempFile::new_in(data_root).with_context(|| format!("write into {}", data_root.display()))?;
    file.write_all(&serde_json::to_vec_pretty(&all)?)?;
    file.as_file().sync_all()?;
    file.persist(data_root.join(CHOSEN_FILE)).map_err(|error| error.error).with_context(|| format!("write {}", data_root.join(CHOSEN_FILE).display()))?;
    chosen_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(data_root.to_path_buf(), all);
    Ok(())
}

/// Where the files of the download folder `name` are: the folder picked for it, else `<data folder>/<name>` (which an
/// older studio may have made a link). Every part of the studio asks this each time it reads or writes them.
pub fn folder(data_root: &Path, name: &str) -> PathBuf {
    chosen_all(data_root).get(name).cloned().unwrap_or_else(|| data_root.join(name))
}

/// A path given inside the data folder (`audiocpp/models/...`): inside the folder its first part names, when that
/// is a download folder kept elsewhere.
pub fn under(data_root: &Path, relative: &str) -> PathBuf {
    let relative = Path::new(relative);
    let mut parts = relative.components();
    match parts.next().and_then(|first| first.as_os_str().to_str()).filter(|first| FOLDERS.contains(first)) {
        Some(first) => folder(data_root, first).join(parts.as_path()),
        None => data_root.join(relative),
    }
}

/// The real place of `name` and how it got there: picked (Debian), behind an older link, or in the data folder.
fn whereabouts(data_root: &Path, name: &str) -> (PathBuf, bool) {
    if let Some(picked) = chosen_all(data_root).get(name) {
        return (picked.clone(), true);
    }
    let place = data_root.join(name);
    match link_target(&place) {
        Some(target) => (target, true),
        None => (place, false),
    }
}

pub fn storage(data_root: &Path) -> Storage {
    let mut chosen = None;
    let folders: Vec<FolderUse> = FOLDERS
        .iter()
        .filter(|name| **name != MODEL_OBJECTS)
        .map(|name| {
            let (real, elsewhere) = whereabouts(data_root, name);
            if chosen.is_none() && elsewhere {
                chosen = chosen_of(name, &real);
            }
            FolderUse { name, bytes: tree_bytes(&real), elsewhere, location: real.display().to_string() }
        })
        .collect();
    let directory = chosen.clone().unwrap_or_else(|| data_root.to_path_buf());
    let (free_bytes, total_bytes) = space(&directory);
    Storage {
        directory: directory.display().to_string(),
        chosen: chosen.map(|path| path.display().to_string()),
        default: data_root.display().to_string(),
        free_bytes,
        total_bytes,
        used_bytes: unique_bytes(&FOLDERS.iter().map(|name| whereabouts(data_root, name).0).collect::<Vec<_>>()),
        folders,
    }
}

/// A download is being written into one of the folders right now.
pub fn download_running(data_root: &Path) -> bool {
    fn fresh_part(dir: &Path, depth: usize) -> bool {
        let Ok(entries) = fs::read_dir(dir) else { return false };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                return depth > 0 && fresh_part(&path, depth - 1);
            }
            path.extension().is_some_and(|extension| extension == "part")
                && entry.metadata().ok().and_then(|meta| meta.modified().ok()).and_then(|at| at.elapsed().ok()).is_some_and(|age| age < PART_FRESH)
        })
    }
    FOLDERS.iter().any(|name| fresh_part(&folder(data_root, name), 6))
}

/// What is asked of a folder picked for the downloads, before anything moves: it is a full path outside the data
/// folder, it can be written to, and its disk has room for what will be copied there.
#[cfg(test)]
pub fn check_place(data_root: &Path, picked: &Path) -> Result<PathBuf> {
    check_place_for(data_root, picked, false)
}

/// `check_place`, and when `programs` the disk must also let programs run from it: an engine kept on a disk mounted
/// noexec, or on one whose file system keeps no execute permission, would not start.
pub fn check_place_for(data_root: &Path, picked: &Path, programs: bool) -> Result<PathBuf> {
    if !picked.is_absolute() {
        bail!("the folder must be a full path");
    }
    let home = home_in(picked);
    let inside = |path: &Path, root: &Path| fs::canonicalize(root).ok().is_some_and(|root| fs::canonicalize(path.ancestors().find(|p| p.exists()).unwrap_or(path)).is_ok_and(|path| path.starts_with(root)));
    if inside(&home, data_root) {
        bail!("that folder is inside the studio's own data folder");
    }
    fs::create_dir_all(&home).with_context(|| format!("create {}", home.display()))?;
    let probe = home.join(".studio-write-probe");
    fs::write(&probe, b"probe").with_context(|| format!("write into {}", home.display()))?;
    let _ = fs::remove_file(&probe);
    if programs {
        can_run_programs(&home)?;
    }
    Ok(home)
}

#[cfg(unix)]
fn can_run_programs(home: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let probe = home.join(".studio-run-probe");
    fs::write(&probe, b"#!/bin/sh\nexit 0\n").with_context(|| format!("write into {}", home.display()))?;
    let ran = fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).is_ok()
        && std::process::Command::new(&probe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
    let _ = fs::remove_file(&probe);
    if !ran {
        bail!("programs cannot run from {} (the disk is mounted noexec, or its file system keeps no execute permission), so the engines cannot be kept there: pick a folder on another disk, or move only the models", home.display());
    }
    Ok(())
}

#[cfg(not(unix))]
fn can_run_programs(_home: &Path) -> Result<()> {
    Ok(())
}

/// The folders a move takes: those named, or all of them. A name the studio does not know is refused.
pub fn selected(names: Option<&[String]>) -> Result<Vec<&'static str>> {
    let Some(names) = names else { return Ok(FOLDERS.to_vec()) };
    let mut chosen = names
        .iter()
        .map(|name| FOLDERS.iter().copied().filter(|known| *known != MODEL_OBJECTS).find(|known| *known == name.as_str()).ok_or_else(|| anyhow!("no download folder is called {name}")))
        .collect::<Result<Vec<_>>>()?;
    if chosen.contains(&"music-models") {
        chosen.push(MODEL_OBJECTS);
    }
    Ok(chosen)
}

/// Whether any of `names` holds programs the studio starts: also one with nothing in it yet, as its place is made
/// for the engines to be downloaded there.
pub fn moves_programs(names: &[&str]) -> bool {
    names.iter().any(|name| PROGRAMS.contains(name))
}

/// Copies the tree at `source` to `staging`, file by file, telling how far it is; a link inside refuses the whole tree.
/// Files copied in one move, by the file they were copied from (its disk and inode): a file that has several names
/// (the engines' models are hard links to their shared weights) is copied once and linked again at the new place,
/// so it takes its room there once, as it did here.
type Copied = std::collections::HashMap<(u64, u64), PathBuf>;

#[cfg(unix)]
fn identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

/// The bytes of the trees at `roots` together, a file with several names counted once.
fn unique_bytes(roots: &[PathBuf]) -> u64 {
    fn walk(dir: &Path, seen: &mut std::collections::HashSet<(u64, u64)>) -> u64 {
        let Ok(entries) = fs::read_dir(dir) else { return 0 };
        entries
            .flatten()
            .map(|entry| match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(&entry.path(), seen),
                Ok(kind) if kind.is_file() => match entry.metadata() {
                    Ok(meta) if identity(&meta).is_none_or(|key| seen.insert(key)) => meta.len(),
                    _ => 0,
                },
                _ => 0,
            })
            .sum()
    }
    let mut seen = std::collections::HashSet::new();
    roots.iter().map(|root| walk(root, &mut seen)).sum()
}

fn copy_tree(source: &Path, staging: &Path, cancel: &AtomicBool, done: &mut u64, tell: &mut dyn FnMut(u64), copied: &mut Copied) -> Result<()> {
    fs::create_dir_all(staging).with_context(|| format!("create {}", staging.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let (from, to) = (entry.path(), staging.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_symlink() || link_target(&from).is_some() {
            bail!("{} is a link to another place; a folder with one is not moved", from.display());
        }
        if kind.is_dir() {
            copy_tree(&from, &to, cancel, done, tell, copied)?;
            continue;
        }
        let meta = entry.metadata()?;
        let key = identity(&meta);
        if let Some(first) = key.and_then(|key| copied.get(&key)) {
            if fs::hard_link(first, &to).is_ok() {
                *done += meta.len();
                tell(*done);
                continue;
            }
        }
        let mut reader = fs::File::open(&from).with_context(|| format!("open {}", from.display()))?;
        let mut writer = fs::File::create(&to).with_context(|| format!("create {}", to.display()))?;
        let mut buffer = vec![0u8; 4 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let read = reader.read(&mut buffer).with_context(|| format!("read {}", from.display()))?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read]).with_context(|| format!("write {}", to.display()))?;
            *done += read as u64;
            tell(*done);
        }
        writer.sync_all().with_context(|| format!("write {}", to.display()))?;
        let (written, wanted) = (fs::metadata(&to)?.len(), meta.len());
        if written != wanted {
            bail!("{} was copied as {written} bytes, not {wanted}", to.display());
        }
        if let Some(key) = key {
            copied.insert(key, to);
        }
    }
    Ok(())
}

/// Puts the tree at `source` at `target`. Answers whether `source` is still there: it is gone when a rename was
/// enough (the same disk), and left untouched when the tree had to be copied.
fn carry(source: &Path, target: &Path, cancel: &AtomicBool, tell: &mut dyn FnMut(u64), copied: &mut Copied) -> Result<bool> {
    if target.exists() {
        if fs::read_dir(target).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
            bail!("{} is already there and is not empty: a move that was cut off left it, or another studio keeps its models there. Remove it or pick another folder", target.display());
        }
        fs::remove_dir(target).with_context(|| format!("replace the empty {}", target.display()))?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if fs::rename(source, target).is_ok() {
        return Ok(false);
    }
    let staging = sibling(target, STAGED);
    if staging.exists() {
        fs::remove_dir_all(&staging).with_context(|| format!("clear what an earlier move left at {}", staging.display()))?;
    }
    let mut done = 0;
    let before: std::collections::HashSet<_> = copied.keys().copied().collect();
    if let Err(error) = copy_tree(source, &staging, cancel, &mut done, tell, copied) {
        let _ = fs::remove_dir_all(&staging);
        copied.retain(|key, _| before.contains(key));
        return Err(error);
    }
    fs::rename(&staging, target).with_context(|| format!("publish {}", target.display()))?;
    // what was copied now has its final names
    for path in copied.values_mut() {
        if let Ok(rest) = path.strip_prefix(&staging) {
            *path = target.join(rest);
        }
    }
    Ok(true)
}

/// Puts `name` at `target` and the studio's reading of it there: copied (or renamed, on the same disk) first, then
/// written down as its place, then the old copy taken away. Until it is written down the studio uses the old place,
/// so a move cut off or failed leaves the folder where it was, in use.
fn move_to(data_root: &Path, name: &str, target: &Path, cancel: &AtomicBool, tell: &mut dyn FnMut(u64), copied: &mut Copied) -> Result<()> {
    let place = data_root.join(name);
    let (real, _) = whereabouts(data_root, name);
    let behind_link = link_target(&place).is_some();
    if behind_link && target == place {
        // an older link brought home: it stands where the folder goes, so it goes first; put back on a failure
        remove_link(&place).with_context(|| format!("remove the link at {}", place.display()))?;
    }
    let was_copied = match carry(&real, target, cancel, tell, copied) {
        Ok(was_copied) => was_copied,
        Err(error) => {
            if behind_link && target == place && fs::symlink_metadata(&place).is_err() {
                let _ = make_link(&real, &place);
            }
            return Err(error);
        }
    };
    let chosen = if target == place { None } else { Some(target) };
    if let Err(error) = set_chosen(data_root, name, chosen) {
        // back to how it was: the folder where it was, nothing at the new place
        if was_copied {
            let _ = fs::remove_dir_all(target);
        } else {
            let _ = fs::rename(target, &real);
        }
        return Err(error);
    }
    if behind_link && target != place {
        let _ = remove_link(&place);
    }
    if was_copied {
        let aside = sibling(&real, LEAVING);
        fs::rename(&real, &aside)
            .and_then(|()| fs::remove_dir_all(&aside))
            .with_context(|| format!("the folder moved, but its old copy at {} could not be removed; remove it yourself", real.display()))?;
    }
    Ok(())
}

/// Moves the download folders into `home`, or back into the data folder when `home` is none. A folder that cannot
/// move stays where it is and says why; the others still move. `tell` hears the folder at work, the bytes copied of
/// it and its size.
#[cfg(test)]
pub fn relocate(data_root: &Path, home: Option<&Path>, cancel: &AtomicBool, tell: impl FnMut(&'static str, u64, u64)) -> Vec<Moved> {
    relocate_only(data_root, home, FOLDERS, cancel, tell)
}

/// `relocate`, for the folders named in `only`; the others stay where they are.
pub fn relocate_only(data_root: &Path, home: Option<&Path>, only: &[&'static str], cancel: &AtomicBool, mut tell: impl FnMut(&'static str, u64, u64)) -> Vec<Moved> {
    let mut moved = Vec::new();
    let mut copied = Copied::new();
    for name in FOLDERS.iter().filter(|name| only.contains(name)) {
        let target = match home {
            Some(home) => home.join(name),
            None => data_root.join(name),
        };
        let (real, elsewhere) = whereabouts(data_root, name);
        if real == target && (home.is_some() || !elsewhere) {
            continue;
        }
        if !real.is_dir() {
            // a place made ahead of a download is not a move to report; one that could not be made is
            if let Err(error) = prepare_empty(data_root, name, &target, &real, elsewhere) {
                moved.push(Moved { name, bytes: 0, error: Some(format!("{error:#}")) });
            }
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            moved.push(Moved { name, bytes: 0, error: Some("cancelled".into()) });
            continue;
        }
        let bytes = tree_bytes(&real);
        tell(name, 0, bytes);
        let mut progress = |done: u64| tell(name, done, bytes);
        let outcome = move_to(data_root, name, &target, cancel, &mut progress, &mut copied);
        moved.push(Moved { name, bytes, error: outcome.err().map(|error| format!("{error:#}")) });
    }
    moved
}

/// A folder that holds nothing yet (an engine or its models not downloaded so far) gets its place before the
/// download: the folder made at `target` and written down, so the download goes there straight away. What is at
/// `target` already (the models of an earlier installation, say) is used as it is. A folder kept on a disk that is
/// not there now (not connected) is never pointed elsewhere: its files may be on it.
fn prepare_empty(data_root: &Path, name: &str, target: &Path, real: &Path, elsewhere: bool) -> Result<()> {
    if elsewhere && fs::symlink_metadata(real).is_err() {
        bail!("it is kept in {}, which is not there now: connect that disk, then move it", real.display());
    }
    let made = !target.exists();
    fs::create_dir_all(target).with_context(|| format!("create {}", target.display()))?;
    let place = data_root.join(name);
    if link_target(&place).is_some() {
        remove_link(&place).with_context(|| format!("remove the link at {}", place.display()))?;
    }
    let chosen = if target == place { None } else { Some(target) };
    set_chosen(data_root, name, chosen).inspect_err(|_| {
        if made {
            let _ = fs::remove_dir(target);
        }
    })
}

/// Sets right what an interrupted move left: the old copy of a folder that had already moved is removed, and an
/// unfinished copy at the new place (never written down, so never used) is removed.
pub fn sweep(data_root: &Path) {
    for name in FOLDERS {
        let (real, _) = whereabouts(data_root, name);
        for place in [data_root.join(name), real] {
            let aside = sibling(&place, LEAVING);
            if aside.is_dir() {
                let _ = fs::remove_dir_all(&aside);
            }
            let arriving = sibling(&place, STAGED);
            if arriving.is_dir() {
                let _ = fs::remove_dir_all(&arriving);
            }
        }
    }
}

/// How much of the folders named in `only` would have to be copied to `home`: nothing for those on the same disk.
pub fn bytes_to_copy_of(data_root: &Path, home: &Path, only: &[&str]) -> u64 {
    let existing = |path: &Path| path.ancestors().find(|p| p.exists()).unwrap_or(path).to_path_buf();
    #[cfg(unix)]
    let same_disk = |a: &Path, b: &Path| {
        use std::os::unix::fs::MetadataExt;
        matches!((fs::metadata(existing(a)), fs::metadata(existing(b))), (Ok(a), Ok(b)) if a.dev() == b.dev())
    };
    #[cfg(not(unix))]
    let same_disk = |a: &Path, b: &Path| {
        let root = |path: &Path| fs::canonicalize(existing(path)).ok().and_then(|path| path.components().next().map(|c| c.as_os_str().to_os_string()));
        root(a).is_some() && root(a) == root(b)
    };
    let to_copy: Vec<PathBuf> = FOLDERS
        .iter()
        .filter(|name| only.contains(name))
        .map(|name| whereabouts(data_root, name).0)
        // already there, or on the same disk (renamed, not copied): nothing to copy
        .filter(|real| !real.starts_with(home) && !same_disk(real, home))
        .collect();
    unique_bytes(&to_copy)
}

pub fn room_for_these(data_root: &Path, home: &Path, only: &[&str]) -> Result<()> {
    let need = bytes_to_copy_of(data_root, home, only);
    let (free, _) = space(home);
    if free > 0 && need > free {
        return Err(anyhow!("not enough room there: {} MB to copy, {} MB free", need >> 20, free >> 20));
    }
    Ok(())
}

// ---- the move, asked for by the window or an agent ----

/// Where a move stands, for the window to poll.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Move {
    pub running: bool,
    /// The folder at work.
    pub folder: Option<&'static str>,
    pub done_bytes: u64,
    pub total_bytes: u64,
    /// What each folder came to, once the move has ended.
    pub moved: Vec<Moved>,
    pub error: Option<String>,
    /// Why the engine did not start again afterwards.
    pub engine_error: Option<String>,
}

/// A move is running: an engine or a model is not installed meanwhile, as its folder may be on its way.
pub fn moving() -> bool {
    job_now().running
}

/// Held while a move starts and while an installation starts, so the two never begin together: each looks for the
/// other under it.
pub fn start_gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn job() -> &'static std::sync::Mutex<(Move, std::sync::Arc<AtomicBool>)> {
    static JOB: std::sync::OnceLock<std::sync::Mutex<(Move, std::sync::Arc<AtomicBool>)>> = std::sync::OnceLock::new();
    JOB.get_or_init(Default::default)
}

fn job_now() -> Move {
    job().lock().unwrap_or_else(std::sync::PoisonError::into_inner).0.clone()
}

fn data_root() -> Result<PathBuf, (StatusCode, Json<ApiError>)> {
    crate::studio_data_root().ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "the studio has no data folder".into()))
}

/// Where the downloads are, how much room is there, and a move in progress.
pub async fn status() -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let root = data_root()?;
    let storage = tokio::task::spawn_blocking(move || storage(&root)).await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(Json(json!({ "storage": storage, "move": job_now() })))
}

#[derive(Debug, Deserialize, Default)]
pub struct Named {
    /// A folder an agent names; the window leaves it out and the person picks one in a dialog.
    #[serde(default)]
    pub path: Option<String>,
    /// One download folder to move on its own (a name from the list); none moves them all.
    #[serde(default)]
    pub folder: Option<String>,
}

/// A folder for the downloads, picked in a dialog or named: where the studio's folders would go inside it, the room
/// on its disk and how much would be copied there. Nothing moves.
pub async fn pick(body: axum::body::Bytes) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let root = data_root()?;
    let request = serde_json::from_slice::<Named>(&body).unwrap_or_default();
    let only = selected(request.folder.as_ref().map(std::slice::from_ref)).map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    let named = request.path.map(PathBuf::from);
    let picked = match named {
        Some(folder) => Some(folder),
        None => {
            // the dialog opens where the folder is kept now, when that is outside the data folder
            let start = only.first().filter(|_| only.len() == 1).and_then(|name| {
                link_target(&root.join(name)).and_then(|target| chosen_of(name, &target)).map(|home| home.parent().map(Path::to_path_buf).unwrap_or(home))
            });
            let title = match &request.folder {
                Some(name) => format!("Folder for {name}"),
                None => "Folder for the studio's models".to_owned(),
            };
            let open = crate::save_locations::DialogOpen::take().ok_or_else(|| api_error(StatusCode::CONFLICT, "a folder dialog is open already".into()))?;
            tokio::task::spawn_blocking(move || {
                let _open = open;
                let mut dialog = rfd::FileDialog::new().set_title(title);
                if let Some(start) = start.filter(|start| start.is_dir()) {
                    dialog = dialog.set_directory(start);
                }
                dialog.pick_folder()
            })
            .await
            .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        }
    };
    let Some(picked) = picked else { return Ok(Json(json!({ "picked": false }))) };
    if !picked.is_absolute() {
        return Err(api_error(StatusCode::BAD_REQUEST, "the folder must be a full path".into()));
    }
    let home = home_in(&picked);
    let (free_bytes, total_bytes) = space(&home);
    let copy_bytes = { let (root, home, only) = (root.clone(), home.clone(), only.clone()); tokio::task::spawn_blocking(move || bytes_to_copy_of(&root, &home, &only)).await.unwrap_or(0) };
    Ok(Json(json!({ "picked": true, "path": picked.display().to_string(), "directory": home.display().to_string(), "free_bytes": free_bytes, "total_bytes": total_bytes, "copy_bytes": copy_bytes, "folder": request.folder })))
}

#[derive(Debug, Deserialize)]
pub struct Change {
    /// The folder to keep the downloads in; null brings them back into the studio's data folder.
    pub directory: Option<String>,
    /// The download folders to move; none moves them all.
    #[serde(default)]
    pub folders: Option<Vec<String>>,
}

/// Starts the move. The engine and the writing assistant are stopped for it, since they hold the files open, and the
/// engine starts again when it ends; nothing else may be at work.
pub async fn change(State(state): State<AppState>, Json(change): Json<Change>) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let root = data_root()?;
    if job_now().running {
        return Err(api_error(StatusCode::CONFLICT, "the models are being moved already".into()));
    }
    let at_work = crate::mcp::busy(&crate::mcp::status_summary().await);
    if !at_work.is_empty() {
        return Err(api_error(StatusCode::CONFLICT, format!("the studio is at work ({}); move the models when it is done", at_work.join(", "))));
    }
    if crate::chunked::downloading() || download_running(&root) || crate::engine_catalogue::progress::any_active() {
        return Err(api_error(StatusCode::CONFLICT, "a download is running; move the models when it is done or cancelled".into()));
    }
    let only = selected(change.folders.as_deref()).map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    let home = match change.directory.as_deref().map(str::trim).filter(|directory| !directory.is_empty()) {
        Some(directory) => {
            let home = check_place_for(&root, Path::new(directory), moves_programs(&only)).map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
            room_for_these(&root, &home, &only).map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
            Some(home)
        }
        None => None,
    };
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    // A submission may have started while the destination was being checked.
    let held = state.engine_use.clone().try_write_owned().map_err(|_| {
        api_error(StatusCode::CONFLICT, "the engine is in use; move the models when it is idle".into())
    })?;
    if crate::engine_at_work(&state).await {
        return Err(api_error(StatusCode::CONFLICT, "the engine is making music; move the models when it is done".into()));
    }
    {
        let _gate = start_gate();
        if crate::engine_catalogue::progress::any_active() {
            return Err(api_error(StatusCode::CONFLICT, "an engine or a model is being installed; move the models when it is done".into()));
        }
        let mut slot = job().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.0.running {
            return Err(api_error(StatusCode::CONFLICT, "the models are being moved already".into()));
        }
        *slot = (Move { running: true, ..Move::default() }, cancel.clone());
    }
    tokio::spawn(async move {
        let _held = held;
        let engine_was_up = match crate::stop_engine_and_assistant(&state).await {
            Ok(was_up) => was_up,
            Err(error) => {
                job().lock().unwrap_or_else(std::sync::PoisonError::into_inner).0 = Move { error: Some(error), ..Move::default() };
                return;
            }
        };
        let work = tokio::task::spawn_blocking(move || {
            relocate_only(&root, home.as_deref(), &only, &cancel, |folder, done, total| {
                let mut job = job().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                job.0.folder = Some(folder);
                job.0.done_bytes = done;
                job.0.total_bytes = total;
            })
        })
        .await;
        let engine_error = if engine_was_up { crate::restart_engine(&state).await.err() } else { None };
        let mut job = job().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        job.0 = match work {
            Ok(moved) => Move { moved, engine_error, ..Move::default() },
            Err(error) => Move { error: Some(format!("model relocation was interrupted: {error}")), engine_error, ..Move::default() },
        };
    });
    Ok(Json(json!({ "move": job_now() })))
}

/// Stops the move after the file being copied: the folder at work stays where it was, those already moved stay moved.
pub async fn cancel() -> Json<Value> {
    job().lock().unwrap_or_else(std::sync::PoisonError::into_inner).1.store(true, Ordering::Relaxed);
    Json(json!({ "move": job_now() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("model-storage-{label}-{}", uuid::Uuid::now_v7()));
        let data = root.join("data");
        fs::create_dir_all(data.join("models/yue2-cpp")).unwrap();
        fs::write(data.join("models/yue2-cpp/backbone.gguf"), b"weights").unwrap();
        fs::create_dir_all(data.join("adapters")).unwrap();
        fs::write(data.join("adapters/voice.bin"), b"lora").unwrap();
        fs::create_dir_all(data.join("training/models")).unwrap();
        fs::write(data.join("training/models/trainer.bin"), b"pack").unwrap();
        fs::create_dir_all(data.join("training/datasets/mine")).unwrap();
        fs::write(data.join("training/datasets/mine/song.wav"), b"the person's own").unwrap();
        fs::create_dir_all(data.join("media")).unwrap();
        fs::write(data.join("media/song.flac"), b"a song").unwrap();
        (root, data)
    }

    fn quiet() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// No link anywhere under `dir`.
    fn no_links(dir: &Path) -> bool {
        fs::read_dir(dir).map(|entries| entries.flatten().all(|entry| {
            let kind = entry.file_type().unwrap();
            !kind.is_symlink() && (!kind.is_dir() || no_links(&entry.path()))
        })).unwrap_or(true)
    }

    #[test]
    fn the_downloads_go_to_the_picked_folder_without_links_and_come_back() {
        let (root, data) = data("move");
        let home = check_place(&data, &root.join("big disk")).unwrap();
        assert_eq!(home, root.join("big disk").join(HOME));

        let moved = relocate(&data, Some(&home), &quiet(), |_, _, _| {});
        assert_eq!(moved, vec![Moved { name: "models", bytes: 7, error: None }, Moved { name: "adapters", bytes: 4, error: None }]);
        // the files are there, really, and nothing of them is left in the data folder: not even a link
        assert_eq!(fs::read(home.join("models/yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        assert!(fs::symlink_metadata(data.join("models")).is_err() && fs::symlink_metadata(data.join("adapters")).is_err());
        assert!(no_links(&data) && no_links(&home));
        // the studio asks where they are, and is told
        assert_eq!(folder(&data, "models"), home.join("models"));
        assert_eq!(under(&data, "models/yue2-cpp/backbone.gguf"), home.join("models/yue2-cpp/backbone.gguf"));
        assert_eq!(under(&data, "media/song.flac"), data.join("media/song.flac"), "the person's own stays");
        // written down in the data folder, and read again by a studio started afresh
        let written: Chosen = serde_json::from_slice(&fs::read(data.join(CHOSEN_FILE)).unwrap()).unwrap();
        assert_eq!(written.get("models"), Some(&home.join("models")));
        chosen_cache().lock().unwrap().remove(&data);
        assert_eq!(folder(&data, "adapters"), home.join("adapters"));
        // what the person made stayed, and so did the trainer, which lives with it
        assert!(data.join("training/datasets/mine/song.wav").is_file() && data.join("training/models/trainer.bin").is_file());
        assert!(data.join("media/song.flac").is_file());

        let now = storage(&data);
        assert_eq!(now.chosen.as_deref(), Some(home.display().to_string().as_str()));
        assert_eq!(now.used_bytes, 11);
        assert!(now.folders.iter().all(|folder| folder.elsewhere), "every folder lives there now, also an empty one: {:?}", now.folders);
        assert!(now.folders.iter().all(|folder| folder.name != MODEL_OBJECTS), "the shared weights are not shown on their own");
        assert!(relocate(&data, Some(&home), &quiet(), |_, _, _| {}).is_empty(), "already there: nothing to do");

        let back = relocate(&data, None, &quiet(), |_, _, _| {});
        assert!(back.iter().all(|folder| folder.error.is_none()), "{back:?}");
        assert_eq!(fs::read(data.join("models/yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        assert!(!home.join("models").exists());
        assert_eq!(folder(&data, "models"), data.join("models"));
        assert_eq!(storage(&data).chosen, None);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_downloader_follows_its_folder_at_once() {
        let (root, data) = data("follow");
        let downloader = crate::downloads::Downloader::in_folder(&data, "adapters");
        assert_eq!(downloader.root(), data.join("adapters"));
        let home = check_place(&data, &root.join("usb")).unwrap();
        relocate_only(&data, Some(&home), &["adapters"], &quiet(), |_, _, _| {});
        // the same downloader, made at the studio's start, now writes on the picked disk
        assert_eq!(downloader.root(), home.join("adapters"));
        assert_eq!(downloader.runtime_dir("x"), home.join("adapters/runtime/x"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_tree_copied_to_another_disk_is_whole_or_not_there() {
        let (root, data) = data("copy");
        let staging = root.join("staging");
        let (mut done, mut heard) = (0, Vec::new());
        copy_tree(&data.join("models"), &staging, &quiet(), &mut done, &mut |bytes| heard.push(bytes), &mut Copied::new()).unwrap();
        assert_eq!((done, heard.last().copied()), (7, Some(7)));
        assert_eq!(fs::read(staging.join("yue2-cpp/backbone.gguf")).unwrap(), b"weights");

        // a stop in the middle is an error, and the caller clears what was staged
        let stopped = AtomicBool::new(true);
        assert_eq!(copy_tree(&data.join("models"), &root.join("other"), &stopped, &mut 0, &mut |_| {}, &mut Copied::new()).unwrap_err().to_string(), "cancelled");

        // a link inside refuses the tree: its target is somebody else's to move
        make_link(&data.join("media"), &data.join("models/linked")).unwrap();
        let error = copy_tree(&data.join("models"), &root.join("third"), &quiet(), &mut 0, &mut |_| {}, &mut Copied::new()).unwrap_err().to_string();
        assert!(error.contains("is a link"), "{error}");
        remove_link(&data.join("models/linked")).unwrap();
        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn the_engines_models_and_their_shared_weights_stay_one_file_on_the_new_disk() {
        use std::os::unix::fs::MetadataExt;
        let (root, data) = data("hardlinks");
        fs::create_dir_all(data.join("music-models/ace/generation-1/files")).unwrap();
        fs::create_dir_all(data.join(MODEL_OBJECTS)).unwrap();
        fs::write(data.join(MODEL_OBJECTS).join("abc"), b"5.76 GiB of weights").unwrap();
        fs::hard_link(data.join(MODEL_OBJECTS).join("abc"), data.join("music-models/ace/generation-1/files/dit.gguf")).unwrap();
        // counted once, as it takes its room once
        assert_eq!(unique_bytes(&[data.join("music-models"), data.join(MODEL_OBJECTS)]), 19);
        assert_eq!(storage(&data).used_bytes, 7 + 4 + 19);

        // copied across disks (what carry does when a rename cannot), within one move
        let mut copied = Copied::new();
        let usb = root.join("usb");
        carry_copying(&data.join("music-models"), &usb.join("music-models"), &mut copied);
        carry_copying(&data.join(MODEL_OBJECTS), &usb.join(MODEL_OBJECTS), &mut copied);
        let (a, b) = (fs::metadata(usb.join("music-models/ace/generation-1/files/dit.gguf")).unwrap(), fs::metadata(usb.join(MODEL_OBJECTS).join("abc")).unwrap());
        assert_eq!((a.ino(), a.nlink()), (b.ino(), 2), "one file with two names, as before");

        // the models never move without their shared weights
        assert_eq!(selected(Some(&["music-models".to_owned()])).unwrap(), vec!["music-models", MODEL_OBJECTS]);
        assert!(selected(Some(&[MODEL_OBJECTS.to_owned()])).is_err(), "not on their own");
        let home = check_place(&data, &root.join("models disk")).unwrap();
        relocate_only(&data, Some(&home), &selected(Some(&["music-models".to_owned()])).unwrap(), &quiet(), |_, _, _| {});
        assert_eq!(folder(&data, MODEL_OBJECTS), home.join(MODEL_OBJECTS));
        let (a, b) = (fs::metadata(home.join("music-models/ace/generation-1/files/dit.gguf")).unwrap(), fs::metadata(home.join(MODEL_OBJECTS).join("abc")).unwrap());
        assert_eq!(a.ino(), b.ino());
        fs::remove_dir_all(root).ok();
    }

    /// `carry` on a copy: what it does between two disks.
    fn carry_copying(source: &Path, target: &Path, copied: &mut Copied) {
        let staging = sibling(target, STAGED);
        copy_tree(source, &staging, &quiet(), &mut 0, &mut |_| {}, copied).unwrap();
        fs::rename(&staging, target).unwrap();
        for path in copied.values_mut() {
            if let Ok(rest) = path.strip_prefix(&staging) {
                *path = target.join(rest);
            }
        }
    }

    #[test]
    fn a_folder_that_cannot_move_stays_and_the_rest_go() {
        let (root, data) = data("refused");
        let home = check_place(&data, &root.join("elsewhere")).unwrap();
        // something of another program's is already under that name
        fs::create_dir_all(home.join("models")).unwrap();
        fs::write(home.join("models/theirs.bin"), b"x").unwrap();
        let moved = relocate(&data, Some(&home), &quiet(), |_, _, _| {});
        assert!(moved[0].error.as_deref().is_some_and(|error| error.contains("already there")), "{moved:?}");
        assert_eq!(folder(&data, "models"), data.join("models"), "the folder stayed, in use");
        assert_eq!(fs::read(data.join("models/yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        assert_eq!(moved[1], Moved { name: "adapters", bytes: 4, error: None });
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_folder_goes_from_one_picked_place_straight_to_another() {
        let (root, data) = data("across");
        let first = check_place(&data, &root.join("disk one")).unwrap();
        let second = check_place(&data, &root.join("disk two")).unwrap();
        relocate(&data, Some(&first), &quiet(), |_, _, _| {});
        let moved = relocate(&data, Some(&second), &quiet(), |_, _, _| {});
        assert!(moved.iter().all(|folder| folder.error.is_none()), "{moved:?}");
        assert_eq!(folder(&data, "models"), second.join("models"));
        assert_eq!(fs::read(second.join("models/yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        assert!(!first.join("models").exists(), "the old copy is gone");
        assert!(fs::symlink_metadata(data.join("models")).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_folder_an_older_studio_left_behind_a_link_is_moved_and_the_link_goes() {
        let (root, data) = data("legacy");
        let old = root.join("old disk").join("YuE2 Studio models");
        fs::create_dir_all(&old).unwrap();
        fs::rename(data.join("models"), old.join("models")).unwrap();
        make_link(&old.join("models"), &data.join("models")).unwrap();
        assert_eq!(storage(&data).folders[0].location, old.join("models").display().to_string());
        let home = check_place(&data, &root.join("new disk")).unwrap();
        let moved = relocate_only(&data, Some(&home), &["models"], &quiet(), |_, _, _| {});
        assert!(moved.iter().all(|folder| folder.error.is_none()), "{moved:?}");
        assert!(fs::symlink_metadata(data.join("models")).is_err(), "the link is gone");
        assert_eq!(fs::read(home.join("models/yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        // and back home, from a link too
        fs::rename(home.join("models"), old.join("models")).unwrap();
        set_chosen(&data, "models", None).unwrap();
        make_link(&old.join("models"), &data.join("models")).unwrap();
        relocate_only(&data, None, &["models"], &quiet(), |_, _, _| {});
        assert!(link_target(&data.join("models")).is_none() && data.join("models/yue2-cpp/backbone.gguf").is_file());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_folder_with_nothing_yet_gets_its_place_before_the_download() {
        let (root, data) = data("ahead");
        let home = check_place_for(&data, &root.join("models disk"), false).unwrap();
        assert!(!data.join("music-models").exists(), "nothing downloaded yet");
        let moved = relocate_only(&data, Some(&home), &["music-models"], &quiet(), |_, _, _| {});
        assert!(moved.is_empty(), "nothing was moved, only made ready: {moved:?}");
        assert_eq!(folder(&data, "music-models"), home.join("music-models"));
        assert!(home.join("music-models").is_dir() && fs::symlink_metadata(data.join("music-models")).is_err());
        assert_eq!(storage(&data).folders.iter().find(|folder| folder.name == "music-models").unwrap().location, home.join("music-models").display().to_string());

        // a folder kept on a disk that is not there is not pointed elsewhere
        let gone = root.join("unplugged");
        set_chosen(&data, "music-engines", Some(&gone.join("music-engines"))).unwrap();
        let refused = relocate_only(&data, Some(&home), &["music-engines"], &quiet(), |_, _, _| {});
        assert!(refused[0].error.as_deref().is_some_and(|error| error.contains("not there now")), "{refused:?}");
        assert_eq!(folder(&data, "music-engines"), gone.join("music-engines"));

        // brought back, the downloads it got meanwhile come home
        fs::write(home.join("music-models/probe.bin"), b"w").unwrap();
        let back = relocate_only(&data, None, &["music-models"], &quiet(), |_, _, _| {});
        assert!(back.iter().all(|folder| folder.error.is_none()), "{back:?}");
        assert!(data.join("music-models/probe.bin").is_file() && folder(&data, "music-models") == data.join("music-models"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn engines_and_their_models_go_each_to_a_folder_of_its_own() {
        let (root, data) = data("apart");
        fs::create_dir_all(data.join("music-engines/ace-step/generation-1/files")).unwrap();
        fs::write(data.join("music-engines/ace-step/generation-1/files/ace-server"), b"program").unwrap();
        let engines = check_place_for(&data, &root.join("fast disk"), moves_programs(&["music-engines"])).unwrap();
        let models = check_place_for(&data, &root.join("big disk"), moves_programs(&["music-models"])).unwrap();
        relocate_only(&data, Some(&engines), &selected(Some(&["music-engines".to_owned()])).unwrap(), &quiet(), |_, _, _| {});
        relocate_only(&data, Some(&models), &selected(Some(&["music-models".to_owned()])).unwrap(), &quiet(), |_, _, _| {});
        assert_eq!(fs::read(folder(&data, "music-engines").join("ace-step/generation-1/files/ace-server")).unwrap(), b"program");
        assert_eq!(folder(&data, "music-engines"), engines.join("music-engines"));
        assert_eq!(folder(&data, "music-models"), models.join("music-models"));
        assert_eq!(folder(&data, "models"), data.join("models"), "the others stayed");
        assert!(selected(Some(&["media".to_owned()])).is_err(), "only download folders move");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn what_an_interrupted_move_left_is_cleared_at_the_next_start() {
        let (root, data) = data("sweep");
        let home = check_place(&data, &root.join("usb")).unwrap();
        relocate_only(&data, Some(&home), &["models"], &quiet(), |_, _, _| {});
        fs::create_dir_all(sibling(&data.join("models"), LEAVING)).unwrap();
        fs::create_dir_all(sibling(&home.join("adapters"), STAGED)).unwrap();
        fs::create_dir_all(sibling(&data.join("adapters"), STAGED)).unwrap();
        sweep(&data);
        assert!(!sibling(&data.join("models"), LEAVING).exists());
        assert!(!sibling(&data.join("adapters"), STAGED).exists());
        assert!(home.join("models/yue2-cpp/backbone.gguf").is_file(), "what is in use stays");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_place_is_checked_before_anything_moves() {
        let (root, data) = data("place");
        assert!(check_place(&data, Path::new("relative")).unwrap_err().to_string().contains("full path"));
        assert!(check_place(&data, &data.join("inside")).unwrap_err().to_string().contains("inside the studio"));
        // a folder already named for the studio is used as it is
        assert_eq!(check_place(&data, &root.join(HOME)).unwrap(), root.join(HOME));
        assert_eq!(chosen_of("models", &root.join("x/models")), Some(root.join("x")));
        // a broken or foreign line in the file is ignored, not trusted
        fs::write(data.join(CHOSEN_FILE), br#"{"models":"relative/path","media":"/elsewhere"}"#).unwrap();
        chosen_cache().lock().unwrap().remove(&data);
        assert_eq!(folder(&data, "models"), data.join("models"));
        assert_eq!(folder(&data, "media"), data.join("media"));
        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn engines_are_not_put_where_programs_cannot_run() {
        let (root, data) = data("noexec");
        assert!(moves_programs(&["music-engines"]) && moves_programs(&["native-music-runtimes"]));
        assert!(!moves_programs(&["music-models"]), "models are only read");
        // a disk that runs programs passes, and the probe leaves nothing behind
        let home = check_place_for(&data, &root.join("runs"), true).unwrap();
        assert!(fs::read_dir(&home).unwrap().next().is_none());
        // set STUDIO_NOEXEC_DIR to a folder on a noexec mount to see the refusal
        if let Some(noexec) = std::env::var_os("STUDIO_NOEXEC_DIR") {
            let refused = check_place_for(&data, Path::new(&noexec), true).unwrap_err().to_string();
            assert!(refused.contains("programs cannot run"), "{refused}");
            assert!(check_place_for(&data, Path::new(&noexec), false).is_ok(), "models may still go there");
        }
        fs::remove_dir_all(root).ok();
    }

    /// Set STUDIO_NOLINK_DIR to a folder on FAT, exFAT or NTFS (ntfs3), where no link can be made: the move works there.
    #[test]
    fn a_move_needs_no_link_on_any_disk() {
        let Some(nolink) = std::env::var_os("STUDIO_NOLINK_DIR") else { return };
        let (root, _) = data("nolink");
        let data = PathBuf::from(&nolink).join(format!("data-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(data.join("models/yue2-cpp")).unwrap();
        fs::write(data.join("models/yue2-cpp/backbone.gguf"), b"weights").unwrap();
        assert!(make_link(&data, &data.join("probe")).is_err(), "this disk cannot hold links");
        let home = check_place(&data, &PathBuf::from(&nolink).join(format!("usb-{}", uuid::Uuid::now_v7()))).unwrap();
        let moved = relocate_only(&data, Some(&home), &["models", "music-models"], &quiet(), |_, _, _| {});
        assert!(moved.iter().all(|folder| folder.error.is_none()), "{moved:?}");
        assert_eq!(fs::read(folder(&data, "models").join("yue2-cpp/backbone.gguf")).unwrap(), b"weights");
        assert_eq!(folder(&data, "music-models"), home.join("music-models"));
        fs::remove_dir_all(&data).ok();
        fs::remove_dir_all(home.parent().unwrap()).ok();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_download_in_progress_is_seen_by_its_part_file() {
        let (root, data) = data("download");
        assert!(!download_running(&data));
        fs::write(data.join("models/yue2-cpp/vae.gguf.part"), b"half").unwrap();
        assert!(download_running(&data));
        fs::remove_dir_all(root).ok();
    }
}
