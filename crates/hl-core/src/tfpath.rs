//! Locating and validating the TF2 `tf` directory.
//!
//! Auto-detection is best-effort and always confirmable by the user: this
//! machine has no TF2 install at the default Steam location, so the folder
//! picker is the primary path and detection is a convenience.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What we found at a candidate `tf` directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TfPathInfo {
    pub path: String,
    /// True when this really looks like a TF2 `tf` directory.
    pub valid: bool,
    /// Directory demos are read from (`tf/demos` when it exists, else `tf`).
    pub demos_dir: Option<String>,
    pub cfg_dir: Option<String>,
    /// `.dem` files found directly in `demos_dir`.
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

    let demos_dir = pick_demos_dir(&resolved);
    let demo_count = demos_dir.as_deref().map(count_demos).unwrap_or(0);
    let cfg_dir = Some(resolved.join("cfg")).filter(|p| p.is_dir());

    match (&demos_dir, demo_count) {
        (Some(d), 0) => notes.push(format!(
            "No .dem files in `{}` yet — recordings will be picked up automatically.",
            d.display()
        )),
        (Some(d), n) => notes.push(format!("Found {n} demo file(s) in `{}`.", d.display())),
        (None, _) => notes.push("No demo directory found.".to_string()),
    }

    Ok(TfPathInfo {
        path: resolved.to_string_lossy().into_owned(),
        valid,
        demos_dir: demos_dir.map(|p| p.to_string_lossy().into_owned()),
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

/// TF2 writes demos into `tf/` by default; P-REC and the in-game recorder can
/// be configured to use `tf/demos`. Prefer the subdirectory when it exists.
fn pick_demos_dir(tf: &Path) -> Option<PathBuf> {
    let sub = tf.join("demos");
    if sub.is_dir() {
        if count_demos(&sub) > 0 || count_demos(tf) == 0 {
            return Some(sub);
        }
    }
    if tf.is_dir() {
        return Some(tf.to_path_buf());
    }
    None
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
