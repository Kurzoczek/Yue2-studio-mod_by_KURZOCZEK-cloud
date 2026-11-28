//! The studio's data folder - the library, the songs, the settings, and the downloads not kept elsewhere - in a
//! folder the person picks (Debian build).
//!
//! The library is a database the studio has open while it runs, so the folder is not moved under it: the choice is
//! written down (`~/.config/yue2-studio/move-data-to`) and the program that starts the studio (`yue2-studio`) moves
//! the folder before the next start, showing how far it has come, and then writes the new place into
//! `~/.config/yue2-studio/data-folder`, where the studio reads it from then on. A song is found again in the moved
//! folder by its file's name, so the library survives the move.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

/// The folder the studio's own name for its data is, inside one a person picks.
pub const NAME: &str = "yue2-studio";

/// `~/.config/yue2-studio` (or `$XDG_CONFIG_HOME/yue2-studio`): where the choice is kept, outside the data folder.
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join(NAME))
}

/// The data folder picked in an earlier run and since moved to: none until one was.
pub fn chosen() -> Option<PathBuf> {
    read_line(&config_dir()?.join("data-folder"))
}

/// The folder the data goes to at the next start, when a move is waiting.
pub fn pending() -> Option<PathBuf> {
    read_line(&config_dir()?.join("move-data-to"))
}

fn read_line(path: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(path).ok()?;
    let line = text.lines().next()?.trim();
    let path = PathBuf::from(line);
    path.is_absolute().then_some(path)
}

/// Where the data goes inside the folder picked: the folder itself when it is the studio's own already, else the
/// studio's own folder made inside it.
pub fn target_in(picked: &Path) -> PathBuf {
    if picked.file_name().is_some_and(|name| name == NAME) { picked.to_path_buf() } else { picked.join(NAME) }
}

/// Checks a target before the move is written down: a full path, not the data folder or inside it (nor it inside
/// the target), empty if it is there already, writable, and with room for what there is to move.
pub fn check(current: &Path, target: &Path, size: u64) -> Result<()> {
    if !target.is_absolute() {
        bail!("give a full path, not {}", target.display());
    }
    let current_real = fs::canonicalize(current).unwrap_or_else(|_| current.to_path_buf());
    let existing = target.ancestors().find(|path| path.exists()).ok_or_else(|| anyhow!("{} does not exist", target.display()))?;
    let existing_real = fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    let target_real = existing_real.join(target.strip_prefix(existing).unwrap_or(Path::new("")));
    if target_real == current_real {
        bail!("this is the data folder already");
    }
    if target_real.starts_with(&current_real) || current_real.starts_with(&target_real) {
        bail!("pick a folder outside the studio's data folder, and not one it is inside of");
    }
    if target.is_dir() && fs::read_dir(target).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
        bail!("{} is not empty: pick an empty folder, or one where the studio can make its own", target.display());
    }
    // writable: a probe made and removed where the folder will be
    let probe = existing.join(format!(".yue2-studio-probe-{}", std::process::id()));
    fs::write(&probe, b"probe").with_context(|| format!("the studio cannot write into {}", existing.display()))?;
    let _ = fs::remove_file(&probe);
    let free = crate::model_storage::space(existing).0;
    let same_disk = same_disk(&current_real, &existing_real);
    if !same_disk && free > 0 && free < size {
        bail!("{} has {} free, and {} has to be copied there", existing.display(), gigabytes(free), gigabytes(size));
    }
    Ok(())
}

#[cfg(unix)]
fn same_disk(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    matches!((fs::metadata(a), fs::metadata(b)), (Ok(a), Ok(b)) if a.dev() == b.dev())
}

#[cfg(not(unix))]
fn same_disk(_a: &Path, _b: &Path) -> bool {
    false
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// Writes the move down for the next start (`Some`), or takes a waiting one back (`None`).
pub fn plan(target: Option<&Path>) -> Result<()> {
    let config = config_dir().context("no home folder to keep the choice in")?;
    let file = config.join("move-data-to");
    match target {
        Some(target) => {
            fs::create_dir_all(&config).with_context(|| format!("create {}", config.display()))?;
            let mut temporary = tempfile::NamedTempFile::new_in(&config)?;
            std::io::Write::write_all(&mut temporary, format!("{}\n", target.display()).as_bytes())?;
            temporary.persist(&file).map_err(|error| error.error)?;
        }
        None => {
            if file.exists() {
                fs::remove_file(&file).with_context(|| format!("remove {}", file.display()))?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub current: String,
    /// Where the data goes at the next start, if a move is waiting.
    pub pending: Option<String>,
    /// What the move copies: the data folder, without the download folders kept elsewhere (they stay where they are).
    pub size_bytes: u64,
    /// Started by the program that moves it (`yue2-studio`): only then does a planned move happen.
    pub launcher: bool,
}

pub fn status(current: &Path) -> Status {
    Status {
        current: current.display().to_string(),
        pending: pending().map(|path| path.display().to_string()),
        size_bytes: folder_bytes(current),
        launcher: std::env::var_os("YUE_STUDIO_LAUNCHER").is_some(),
    }
}

/// The bytes under a folder, links not followed (a download folder an older studio linked out is not inside it).
pub fn folder_bytes(root: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(root) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => folder_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_checked_before_the_move_is_written_down() {
        let root = std::env::temp_dir().join(format!("data-folder-{}", uuid::Uuid::now_v7()));
        let current = root.join("data");
        fs::create_dir_all(current.join("media")).unwrap();
        fs::write(current.join("library.sqlite"), b"db").unwrap();
        assert_eq!(folder_bytes(&current), 2);
        assert_eq!(target_in(&root.join("usb")), root.join("usb").join(NAME));
        assert_eq!(target_in(&root.join(NAME)), root.join(NAME));

        assert!(check(&current, &root.join("usb").join(NAME), 2).is_ok(), "a new folder");
        assert!(check(&current, Path::new("relative"), 2).unwrap_err().to_string().contains("full path"));
        assert!(check(&current, &current, 2).unwrap_err().to_string().contains("already"));
        assert!(check(&current, &current.join("media/x"), 2).unwrap_err().to_string().contains("outside"));
        assert!(check(&current.join("media"), &current, 2).unwrap_err().to_string().contains("outside"), "nor one the data folder is inside of");
        fs::create_dir_all(root.join("full")).unwrap();
        fs::write(root.join("full/other.txt"), b"x").unwrap();
        assert!(check(&current, &root.join("full"), 2).unwrap_err().to_string().contains("not empty"));
        fs::create_dir_all(root.join("empty")).unwrap();
        assert!(check(&current, &root.join("empty"), 2).is_ok(), "an empty folder is used as it is");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_line_is_read_only_when_it_is_a_full_path() {
        let root = std::env::temp_dir().join(format!("data-line-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&root).unwrap();
        #[cfg(windows)]
        let full = r"D:\usb\yue2-studio";
        #[cfg(not(windows))]
        let full = "/media/usb/yue2-studio";
        fs::write(root.join("a"), format!("{full}\nanything\n")).unwrap();
        fs::write(root.join("b"), "relative/path\n").unwrap();
        assert_eq!(read_line(&root.join("a")), Some(PathBuf::from(full)));
        assert_eq!(read_line(&root.join("b")), None);
        assert_eq!(read_line(&root.join("missing")), None);
        fs::remove_dir_all(root).ok();
    }
}
