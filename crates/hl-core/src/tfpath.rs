//! Locating and validating the TF2 `tf` directory.
//!
//! Auto-detection is best-effort and always confirmable by the user: this
//! machine has no TF2 install at the default Steam location, so the folder
//! picker is the primary path and detection is a convenience.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A directory holding `.dem` files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoDir {
    pub path: String,
    pub demo_count: usize,
}

/// What we found at a candidate `tf` directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TfPathInfo {
    pub path: String,
    /// True when this really looks like a TF2 `tf` directory.
    pub valid: bool,
    /// Every directory demos are read from. TF2 records into `tf/` by default
    /// while Demo Support writes to `tf/demos`, and real installs accumulate
    /// files in both — so both are scanned rather than picking a winner.
    pub demo_dirs: Vec<DemoDir>,
    pub cfg_dir: Option<String>,
    /// Total `.dem` files across every directory in `demo_dirs`.
    pub demo_count: usize,
    /// Human-readable findings, shown under the picker in the UI.
    pub notes: Vec<String>,
}

/// Markers that identify a TF2 `tf` directory. A live install has all of these;
/// we accept any one so a partially-verified or modded install still works.
const TF_MARKERS: [&str; 4] = ["gameinfo.txt", "tf2_misc_dir.vpk", "steam.inf", "cfg"];

/// Validate a user-supplied path, tolerating the two near-misses people
/// actually make: pointing at the `Team Fortress 2` folder, or at `tf/demos`.
pub fn inspect(path: impl AsRef<Path>) -> Result<TfPathInfo> {
    let given = path.as_ref();
    if !given.is_dir() {
        return Err(Error::InvalidTfPath(format!(
            "`{}` is not a directory",
            given.display()
        )));
    }

    let mut notes = Vec::new();
    let resolved = resolve(given, &mut notes);

    let markers: Vec<&str> = TF_MARKERS
        .iter()
        .copied()
        .filter(|m| resolved.join(m).exists())
        .collect();
    let valid = !markers.is_empty();
    if !valid {
        notes.push(
            "No TF2 marker files found here (expected gameinfo.txt, tf2_misc_dir.vpk or cfg/)."
                .to_string(),
        );
    }

    let demo_dirs = demo_dirs(&resolved);
    let demo_count = demo_dirs.iter().map(|d| d.demo_count).sum();
    let cfg_dir = Some(resolved.join("cfg")).filter(|p| p.is_dir());

    if demo_count == 0 {
        notes.push(
            "No .dem files found yet — new recordings will be picked up automatically.".to_string(),
        );
    } else {
        for d in &demo_dirs {
            if d.demo_count > 0 {
                notes.push(format!("{} demo file(s) in `{}`.", d.demo_count, d.path));
            }
        }
    }

    Ok(TfPathInfo {
        path: resolved.to_string_lossy().into_owned(),
        valid,
        demo_dirs,
        cfg_dir: cfg_dir.map(|p| p.to_string_lossy().into_owned()),
        demo_count,
        notes,
    })
}

/// Nudge a nearly-right path onto the actual `tf` directory.
fn resolve(given: &Path, notes: &mut Vec<String>) -> PathBuf {
    // `.../Team Fortress 2` -> `.../Team Fortress 2/tf`
    if given.join("tf").join("gameinfo.txt").exists() {
        notes.push("Adjusted to the `tf` subdirectory.".to_string());
        return given.join("tf");
    }
    // `.../tf/demos` -> `.../tf`
    if given.file_name().is_some_and(|n| n == "demos") {
        if let Some(parent) = given.parent() {
            if TF_MARKERS.iter().any(|m| parent.join(m).exists()) {
                notes.push("Adjusted up from `demos` to the `tf` directory.".to_string());
                return parent.to_path_buf();
            }
        }
    }
    given.to_path_buf()
}

/// Every directory that can hold demos: `tf` itself (where the `record` command
/// writes) and `tf/demos` (where Demo Support and P-REC write). Both are real
/// on a used install, so both are reported.
fn demo_dirs(tf: &Path) -> Vec<DemoDir> {
    [tf.to_path_buf(), tf.join("demos")]
        .into_iter()
        .filter(|p| p.is_dir())
        .map(|p| DemoDir {
            demo_count: count_demos(&p),
            path: p.to_string_lossy().into_owned(),
        })
        .collect()
}

fn count_demos(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("dem"))
        })
        .count()
}

/// Best-effort scan of the usual Steam locations. Returns the first `tf`
/// directory that inspects as valid.
pub fn detect() -> Option<TfPathInfo> {
    for library in steam_libraries() {
        let candidate = library
            .join("steamapps")
            .join("common")
            .join("Team Fortress 2")
            .join("tf");
        if candidate.is_dir() {
            if let Ok(info) = inspect(&candidate) {
                if info.valid {
                    return Some(info);
                }
            }
        }
    }
    None
}

/// Steam roots worth checking: the standard install locations, any libraries
/// declared in `libraryfolders.vdf`, and the common manual layouts on other drives.
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(base) = std::env::var(var) {
            roots.push(PathBuf::from(base).join("Steam"));
        }
    }

    // Manual installs sitting at the root of a secondary drive.
    for letter in 'C'..='Z' {
        let drive = PathBuf::from(format!("{letter}:\\"));
        if !drive.is_dir() {
            continue;
        }
        roots.push(drive.join("Steam"));
        roots.push(drive.join("SteamLibrary"));
        roots.push(drive.join("Games").join("Steam"));
    }

    // Libraries declared by Steam itself.
    let declared: Vec<PathBuf> = roots.iter().flat_map(|r| parse_library_folders(r)).collect();
    roots.extend(declared);

    roots.retain(|p| p.is_dir());
    roots.sort();
    roots.dedup();
    roots
}

/// Pull `"path"  "D:\\SteamLibrary"` entries out of `libraryfolders.vdf`.
///
/// A line scan rather than a real VDF parser: the format is stable, this is a
/// convenience path, and the user can always pick the folder by hand.
fn parse_library_folders(steam_root: &Path) -> Vec<PathBuf> {
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(&vdf) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("\"path\"")?;
            let start = rest.find('"')? + 1;
            let end = rest[start..].find('"')? + start;
            Some(PathBuf::from(rest[start..end].replace("\\\\", "\\")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_path_that_is_not_a_directory() {
        assert!(inspect("this-path-does-not-exist-12345").is_err());
    }

    /// Mirrors a real install: Demo Support files in `tf/demos`, plus a few
    /// left over from the `record` command in `tf` itself. Both must count.
    #[test]
    fn counts_demos_in_both_directories() {
        let tf = std::env::temp_dir().join("hl-test-tf/tf");
        let demos = tf.join("demos");
        std::fs::create_dir_all(&demos).unwrap();
        std::fs::create_dir_all(tf.join("cfg")).unwrap();
        std::fs::write(tf.join("gameinfo.txt"), "").unwrap();
        std::fs::write(tf.join("root_one.dem"), "").unwrap();
        std::fs::write(demos.join("ds_one.dem"), "").unwrap();
        std::fs::write(demos.join("ds_two.dem"), "").unwrap();
        // Sidecars and unrelated files must not be counted as demos.
        std::fs::write(demos.join("ds_one.json"), "{}").unwrap();
        std::fs::write(demos.join("_events.txt"), "").unwrap();

        let info = inspect(&tf).unwrap();
        assert!(info.valid);
        assert_eq!(info.demo_count, 3);
        assert_eq!(info.demo_dirs.len(), 2);

        // Pointing at the parent resolves down to `tf`.
        let info = inspect(tf.parent().unwrap()).unwrap();
        assert_eq!(info.demo_count, 3);
    }

    #[test]
    fn extracts_library_paths_from_vdf_lines() {
        let dir = std::env::temp_dir().join("hl-test-vdf/steamapps");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n",
        )
        .unwrap();
        let found = parse_library_folders(dir.parent().unwrap());
        assert_eq!(found, vec![PathBuf::from("D:\\SteamLibrary")]);
    }
}
