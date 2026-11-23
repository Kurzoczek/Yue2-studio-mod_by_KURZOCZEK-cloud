//! Settings -> Save locations (Debian), ported from the Android app "AI Music Generator and Cover Maker"
//! (`OutputRouting`, `OutputCategory`): a folder of your own for every kind of file the studio writes.
//!
//! A folder chosen here is
//!  - where the Save dialog opens for a file of that kind (a song, a stem, a MIDI file, a score...),
//!  - where finished songs and the files made with them are copied when "Copy every finished song" is on:
//!    `<folder>/<song>/<file>`, so every song keeps its files together,
//!  - where the new generation pipeline (sections, loops and samples, used fragments, scores, metadata) writes.
//!
//! Without a folder of its own a kind goes to its default: audio to `~/Music/<studio>` (the XDG music folder,
//! `~/Muzyka` on a Polish desktop), MIDI, scores and metadata to `~/Downloads/<studio>` (`~/Pobrane`), logs and
//! state dumps to the studio's data folder. A folder that stops being writable falls back to the default, and the
//! studio's log says so.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The kinds of files, in the order the settings page lists them. (key, audio?).
pub const CATEGORIES: &[(&str, bool)] = &[
    ("final", true),
    ("parts", true),
    ("samples", true),
    ("stems", true),
    ("used_sounds", true),
    ("midi", false),
    ("notation", false),
    ("lyrics", false),
    ("video", true),
    ("metadata", false),
    ("logs", false),
    ("dumps", false),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// category -> folder chosen for it.
    pub folders: BTreeMap<String, String>,
    /// Copy every finished song (and the stems, MIDI and video made from it) to its folder.
    pub auto_export: bool,
}

fn store() -> &'static RwLock<Settings> {
    static STORE: OnceLock<RwLock<Settings>> = OnceLock::new();
    STORE.get_or_init(|| {
        RwLock::new(
            crate::studio_data_root()
                .and_then(|root| std::fs::read_to_string(root.join("save-locations.json")).ok())
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default(),
        )
    })
}

pub fn settings() -> Settings {
    store().read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

pub fn save(mut new: Settings) -> Result<(), String> {
    new.folders.retain(|key, folder| CATEGORIES.iter().any(|(known, _)| known == key) && !folder.trim().is_empty());
    for folder in new.folders.values_mut() {
        *folder = folder.trim().to_string();
        let path = Path::new(folder.as_str());
        if !path.is_absolute() {
            return Err(format!("{folder} is not a full path (it has to start with /)"));
        }
    }
    let root = crate::studio_data_root().ok_or("the studio has no data folder")?;
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    // on disk first, whole or not at all: a studio closed in the middle of a write never finds half a file (which
    // read as no settings at all), and the window never shows folders that were not kept
    let mut store = store().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    write_whole(&root.join("save-locations.json"), &serde_json::to_vec_pretty(&new).unwrap_or_default()).map_err(|error| error.to_string())?;
    *store = new;
    Ok(())
}

/// Writes `bytes` to `path` through a file of its own beside it and a rename: whoever reads `path` finds the old
/// content or the new one, never a mix, also when two writes meet.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let folder = path.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// One folder dialog at a time: a second "Browse…" while one is open is refused instead of opening a pile of them.
pub struct DialogOpen(());

impl DialogOpen {
    pub fn take() -> Option<Self> {
        DIALOG_OPEN.compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire).ok().map(|_| Self(()))
    }
}

impl Drop for DialogOpen {
    fn drop(&mut self) {
        DIALOG_OPEN.store(false, std::sync::atomic::Ordering::Release);
    }
}

static DIALOG_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a folder can be written to, without creating anything: the folder itself or, when it does not exist
/// yet, the nearest folder above it that does. For the settings page, which looks at folders as they are typed.
fn could_write(dir: &Path) -> bool {
    let mut probe = Some(dir);
    while let Some(path) = probe {
        if path.is_dir() {
            return access_writable(path);
        }
        if path.exists() {
            return false;
        }
        probe = path.parent();
    }
    false
}

#[cfg(unix)]
fn access_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(name) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    unsafe { libc::access(name.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn access_writable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| !meta.permissions().readonly())
}

// ------------------------------------------------------------------------------------------------ last folders

/// The folder the Save dialog last saved each kind of file in, kept across restarts (save-last-folders.json): a
/// kind without a folder of its own opens the next Save dialog there, also after the studio was closed.
fn last_store() -> &'static RwLock<BTreeMap<String, String>> {
    static STORE: OnceLock<RwLock<BTreeMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| {
        RwLock::new(
            crate::studio_data_root()
                .and_then(|root| std::fs::read_to_string(root.join("save-last-folders.json")).ok())
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default(),
        )
    })
}

/// Where the last file of the kind of `name` was saved, if that folder is still there.
pub fn last_folder_for(name: &str) -> Option<PathBuf> {
    let category = category_of(name);
    let folder = last_store().read().unwrap_or_else(|poisoned| poisoned.into_inner()).get(category).cloned()?;
    let path = PathBuf::from(folder);
    path.is_dir().then_some(path)
}

/// The Save dialog saved `name` in `folder`: remembered for its kind, on disk.
pub fn remember_last_folder(name: &str, folder: &Path) {
    let category = category_of(name);
    // held while the file is written, so two saves at once cannot put the older list on disk last
    let mut store = last_store().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    store.insert(category.to_string(), folder.display().to_string());
    if let Some(root) = crate::studio_data_root() {
        let _ = std::fs::create_dir_all(&root);
        if let Err(error) = write_whole(&root.join("save-last-folders.json"), &serde_json::to_vec_pretty(&*store).unwrap_or_default()) {
            crate::studio_log!("[SAVE] the last save folder was not kept: {error}");
        }
    }
}

fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".studio-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

/// The folder chosen for a kind, when there is one and it can be written to.
pub fn folder_for(category: &str) -> Option<PathBuf> {
    let folder = settings().folders.get(category).cloned()?;
    let path = PathBuf::from(&folder);
    if writable(&path) {
        Some(path)
    } else {
        crate::studio_log!("[SAVE] the folder chosen for {category} ({folder}) cannot be written to - the default location is used");
        None
    }
}

/// An XDG user folder (`MUSIC`, `DOWNLOAD`) as `user-dirs.dirs` names it, else `~/Music`, `~/Downloads`.
fn xdg_user_dir(kind: &str, fallback: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let wanted = format!("XDG_{kind}_DIR=");
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some(value) = line.trim().strip_prefix(&wanted) {
                let value = value.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                if !value.is_empty() {
                    return Some(PathBuf::from(value));
                }
            }
        }
    }
    Some(home.join(fallback))
}

fn studio_folder_name() -> &'static str {
    match crate::governor::kind() {
        crate::governor::Engine::Ace => "ACE-Step Studio",
        crate::governor::Engine::Yue => "YuE2 Studio",
    }
}

/// Where a kind goes without a folder of its own.
pub fn default_for(category: &str) -> Option<PathBuf> {
    let audio = CATEGORIES.iter().find(|(key, _)| *key == category).is_some_and(|(_, audio)| *audio);
    match category {
        "logs" => crate::studio_data_root().map(|root| root.join("diagnostics").join("logs")),
        "dumps" => crate::studio_data_root().map(|root| root.join("diagnostics").join("dumps")),
        _ if audio => xdg_user_dir("MUSIC", "Music").map(|dir| dir.join(studio_folder_name())),
        _ => xdg_user_dir("DOWNLOAD", "Downloads").map(|dir| dir.join(studio_folder_name())),
    }
}

/// The folder for a kind: chosen, else the default.
pub fn resolved(category: &str) -> Option<PathBuf> {
    folder_for(category).or_else(|| default_for(category))
}

/// The kind of a file the window saves, from its name.
pub fn category_of(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit('.').next().unwrap_or_default();
    match extension {
        "mid" | "midi" => "midi",
        "musicxml" | "mxl" | "xml" | "ly" | "pdf" | "abc" | "svg" => "notation",
        "lrc" | "srt" | "vtt" | "txt" => "lyrics",
        "mp4" | "webm" | "mkv" | "mov" => "video",
        "json" => "metadata",
        "wav" | "flac" | "mp3" | "ogg" | "opus" | "m4a" | "aac" => {
            if ["stem", "vocals", "drums", "bass", "guitar", "piano", "other", "instrumental"].iter().any(|word| lower.contains(word)) {
                "stems"
            } else {
                "final"
            }
        }
        _ => "final",
    }
}

/// The folder a Save dialog for `name` opens in: the one chosen for its kind, if any.
pub fn start_folder(name: &str) -> Option<PathBuf> {
    folder_for(category_of(name))
}

/// A name a file or folder can have on any disk (no slashes, no control characters, not empty).
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() { "song".into() } else { cleaned.chars().take(120).collect() }
}

/// Copies a file of a song to `<folder of its kind>/<song>/`, never replacing a file that is there (a number is
/// added). Returns where it went.
pub fn export(category: &str, song: &str, source: &Path, file_name: &str) -> std::io::Result<PathBuf> {
    let dir = resolved(category).ok_or_else(|| std::io::Error::other("no folder to save into"))?.join(safe_name(song));
    std::fs::create_dir_all(&dir)?;
    let file_name = safe_name(file_name);
    let mut target = dir.join(&file_name);
    let (stem, extension) = match file_name.rsplit_once('.') {
        Some((stem, extension)) => (stem.to_string(), format!(".{extension}")),
        None => (file_name.clone(), String::new()),
    };
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{stem} ({n}){extension}"));
        n += 1;
    }
    // a name of its own for the copy in progress: two files of one song ("x.wav", "x.flac") never share it
    let part = dir.join(format!(".{}.{}.part", target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), uuid::Uuid::now_v7()));
    if let Err(error) = std::fs::copy(source, &part).and_then(|_| std::fs::rename(&part, &target)) {
        let _ = std::fs::remove_file(&part);
        return Err(error);
    }
    Ok(target)
}

/// A finished song, when "Copy every finished song" is on: the audio file of the song to its folder.
pub fn song_finished(title: &str, audio: &Path) {
    if !settings().auto_export {
        return;
    }
    let name = audio.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "song.wav".into());
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_string()).unwrap_or_else(|| "wav".into());
    match export("final", title, audio, &format!("{}.{extension}", safe_name(title))) {
        Ok(target) => crate::studio_log!("[SAVE] {title}: copied to {}", target.display()),
        Err(error) => crate::studio_log!("[ERROR] [SAVE] {title}: not copied to its folder: {error}"),
    }
}

/// Where the folder dialog of "Browse…" opens: the folder in the field (or the nearest one above it that exists),
/// else the kind's default, else the home folder.
pub fn browse_start(category: &str, typed: Option<&str>) -> Option<PathBuf> {
    let existing = |path: PathBuf| path.ancestors().find(|folder| folder.is_dir()).map(Path::to_path_buf);
    typed
        .map(str::trim)
        .filter(|folder| folder.starts_with('/'))
        .and_then(|folder| existing(PathBuf::from(folder)))
        .or_else(|| default_for(category).and_then(existing))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

/// The dialog's title for a kind, so the person sees what the folder is for.
pub fn browse_title(category: &str) -> String {
    let kind = match category {
        "final" => "finished songs",
        "parts" => "sections",
        "samples" => "loops and samples",
        "stems" => "stems",
        "used_sounds" => "used fragments of the input",
        "midi" => "MIDI",
        "notation" => "scores",
        "lyrics" => "lyrics and karaoke",
        "video" => "videos",
        "metadata" => "metadata",
        "logs" => "logs",
        "dumps" => "state dumps",
        _ => "every kind of file",
    };
    format!("Folder for {kind}")
}

/// Settings -> Save locations.
pub fn page() -> Value {
    let settings = settings();
    let kinds: Vec<Value> = CATEGORIES
        .iter()
        .map(|(key, audio)| {
            let chosen = settings.folders.get(*key).cloned();
            // looked at without creating anything: the page is read every few seconds, also while a path is typed
            let usable = chosen.as_ref().map(|folder| could_write(Path::new(folder)));
            json!({
                "key": key,
                "audio": audio,
                "folder": chosen,
                "writable": usable,
                "default": default_for(key),
            })
        })
        .collect();
    let last = last_store().read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    json!({ "settings": settings, "kinds": kinds, "last": last })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_says_its_kind() {
        assert_eq!(category_of("Song.wav"), "final");
        assert_eq!(category_of("Song - vocals.flac"), "stems");
        assert_eq!(category_of("Song.mid"), "midi");
        assert_eq!(category_of("Song.musicxml"), "notation");
        assert_eq!(category_of("Song.lrc"), "lyrics");
        assert_eq!(category_of("Song.mp4"), "video");
    }

    #[test]
    fn a_folder_being_typed_is_never_created() {
        let dir = std::env::temp_dir().join(format!("studio-typed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let typed = dir.join("Muz");
        assert!(could_write(&typed), "the folder above can be written to");
        assert!(!typed.exists(), "looking at it created nothing");
        let file = dir.join("plik");
        std::fs::write(&file, b"").unwrap();
        assert!(!could_write(&file.join("pod")), "a file is not a folder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn browsing_opens_at_the_nearest_folder_that_exists() {
        let dir = std::env::temp_dir().join(format!("studio-browse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let typed = dir.join("not").join("yet");
        assert_eq!(browse_start("final", Some(&typed.display().to_string())), Some(dir.clone()));
        assert!(!typed.exists(), "nothing is created");
        assert!(browse_start("final", Some("relative/path")).is_some(), "a path that is not a full one falls back");
        assert_eq!(browse_title("midi"), "Folder for MIDI");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_safe_everywhere() {
        assert_eq!(safe_name("a/b:c?"), "a_b_c_");
        assert_eq!(safe_name("  . "), "song");
    }

    #[test]
    fn an_export_never_replaces_a_file() {
        let dir = std::env::temp_dir().join(format!("studio-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("in.wav");
        std::fs::write(&source, b"RIFF").unwrap();
        // the folder is given through a chosen one, so no XDG folder is touched
        let target_root = dir.join("out");
        *store().write().unwrap() = Settings { folders: [("final".to_string(), target_root.display().to_string())].into(), auto_export: true };
        let first = export("final", "My song", &source, "My song.wav").unwrap();
        let second = export("final", "My song", &source, "My song.wav").unwrap();
        assert_eq!(first, target_root.join("My song").join("My song.wav"));
        assert_eq!(second, target_root.join("My song").join("My song (2).wav"));
        // no copy in progress is left behind
        let left: Vec<_> = std::fs::read_dir(target_root.join("My song")).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(left.len(), 2, "{left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_settings_file_is_replaced_whole() {
        let dir = std::env::temp_dir().join(format!("studio-whole-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("save-locations.json");
        write_whole(&path, b"{\"auto_export\":true}").unwrap();
        write_whole(&path, b"{\"auto_export\":false}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"auto_export\":false}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no file of the write is left beside it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_folder_dialog_at_a_time() {
        let open = DialogOpen::take().expect("none open");
        assert!(DialogOpen::take().is_none(), "a second one is refused");
        drop(open);
        assert!(DialogOpen::take().is_some(), "and allowed again once the first closed");
    }
}
