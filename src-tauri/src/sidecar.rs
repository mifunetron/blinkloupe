//! Marks live beside the images and never touch the image files:
//! - PICKS.txt    one picked file name per line (the format pipelines already read)
//! - REROLLS.txt  one line per requested re-roll (a name twice = two rolls)
//! - .loupe.json  stars, bookmarks and skips (hidden; a skip is stored as `rejected`)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PICKS: &str = "PICKS.txt";
pub const REROLLS: &str = "REROLLS.txt";
pub const STORE: &str = ".loupe.json";

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub pick: bool,
    pub stars: u8,
    pub bookmark: bool,
    pub rejected: bool,
    pub rerolls_marked: u32,
    /// hires passes queued for this frame
    pub hires: u32,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct FileMarks {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub stars: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bookmark: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rejected: bool,
    #[serde(default, skip_serializing_if = "is_zero32")]
    pub hires: u32,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}
fn is_zero32(v: &u32) -> bool {
    *v == 0
}
fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    version: u32,
    files: HashMap<String, FileMarks>,
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn folder_of(p: &Path) -> PathBuf {
    p.parent().map(|d| d.to_path_buf()).unwrap_or_default()
}

pub fn read_lines(p: &Path) -> Vec<String> {
    fs::read_to_string(p)
        .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn write_lines(p: &Path, lines: &[String]) -> Result<(), String> {
    let mut text = lines.join("\n");
    if !lines.is_empty() {
        text.push('\n');
    }
    write_file(p, text.as_bytes(), false)
}

/// Overwrites in place when the file exists (a hidden file cannot be replaced by CREATE_ALWAYS), else creates it.
fn write_file(p: &Path, bytes: &[u8], hidden: bool) -> Result<(), String> {
    let existed = p.exists();
    let mut f = if existed {
        OpenOptions::new().write(true).truncate(true).open(p)
    } else {
        OpenOptions::new().write(true).create_new(true).open(p)
    }
    .map_err(|e| format!("{}: {}", p.display(), e))?;
    f.write_all(bytes).map_err(|e| format!("{}: {}", p.display(), e))?;
    drop(f);
    if hidden && !existed {
        hide(p);
    }
    Ok(())
}

#[cfg(windows)]
fn hide(p: &Path) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("attrib").arg("+h").arg(p).creation_flags(0x0800_0000).status();
}

#[cfg(not(windows))]
fn hide(_p: &Path) {}

fn load_store(folder: &Path) -> Store {
    fs::read_to_string(folder.join(STORE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_store(folder: &Path, mut s: Store) -> Result<(), String> {
    s.version = 1;
    s.files.retain(|_, m| m.stars > 0 || m.bookmark || m.rejected || m.hires > 0);
    let text = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    write_file(&folder.join(STORE), text.as_bytes(), true)
}

pub fn marks(p: &Path) -> Marks {
    let folder = folder_of(p);
    let name = name_of(p);
    let fm = load_store(&folder).files.get(&name).cloned().unwrap_or_default();
    Marks {
        pick: read_lines(&folder.join(PICKS)).contains(&name),
        stars: fm.stars,
        bookmark: fm.bookmark,
        rejected: fm.rejected,
        rerolls_marked: read_lines(&folder.join(REROLLS)).iter().filter(|l| **l == name).count() as u32,
        hires: fm.hires,
    }
}

pub fn toggle_pick(p: &Path) -> Result<Marks, String> {
    let pp = folder_of(p).join(PICKS);
    let name = name_of(p);
    let mut cur = read_lines(&pp);
    if cur.contains(&name) {
        cur.retain(|l| *l != name);
    } else {
        cur.push(name);
    }
    write_lines(&pp, &cur)?;
    Ok(marks(p))
}

pub fn set_pick(p: &Path, on: bool) -> Result<Marks, String> {
    let pp = folder_of(p).join(PICKS);
    let name = name_of(p);
    let mut cur = read_lines(&pp);
    let has = cur.contains(&name);
    if on && !has {
        cur.push(name);
        write_lines(&pp, &cur)?;
    } else if !on && has {
        cur.retain(|l| *l != name);
        write_lines(&pp, &cur)?;
    }
    Ok(marks(p))
}

/// Settings > Picks, "Also copy picks to" (2026-10-14, for pipelines that don't read PICKS.txt): a pick copies the frame to
/// <dir>\<its folder's name>\<file>, so sets that reuse file names stay apart. Unpicking removes that copy only while it is still
/// the same size as the frame, so a file someone has since edited or replaced stays. The frame itself is never touched, and a
/// destination that resolves to the frame is refused. Returns the copy's path when one was made or removed.
pub fn copy_pick(p: &Path, dir: &Path, on: bool) -> Result<Option<PathBuf>, String> {
    let set = p.parent().and_then(|d| d.file_name()).ok_or("the frame has no folder")?;
    let dest = dir.join(set).join(p.file_name().ok_or("no file name")?);
    let same = |a: &Path, b: &Path| matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y);
    if same(p, &dest) || dest.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy()) {
        return Err("the picks copy folder leads back to the frame itself - choose another folder".into());
    }
    let len = fs::metadata(p).map_err(|e| format!("{}: {}", p.display(), e))?.len();
    let there = fs::metadata(&dest).ok().map(|m| m.len());
    if on {
        match there {
            Some(l) if l == len => Ok(None),
            Some(_) => Err(format!("{} already exists and is a different file - not copied", dest.display())),
            None => {
                fs::create_dir_all(dest.parent().unwrap()).map_err(|e| format!("{}: {}", dir.display(), e))?;
                fs::copy(p, &dest).map_err(|e| format!("copying to {}: {}", dest.display(), e))?;
                Ok(Some(dest))
            }
        }
    } else if there == Some(len) {
        fs::remove_file(&dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
        let _ = fs::remove_dir(dest.parent().unwrap()); // only when that leaves it empty
        Ok(Some(dest))
    } else {
        Ok(None)
    }
}

pub fn update(p: &Path, f: impl FnOnce(&mut FileMarks)) -> Result<Marks, String> {
    let folder = folder_of(p);
    let mut s = load_store(&folder);
    let e = s.files.entry(name_of(p)).or_default();
    f(e);
    save_store(&folder, s)?;
    Ok(marks(p))
}

pub fn mark_reroll(p: &Path) -> Result<Marks, String> {
    let rp = folder_of(p).join(REROLLS);
    let mut f = OpenOptions::new().create(true).append(true).open(&rp).map_err(|e| format!("{}: {}", rp.display(), e))?;
    writeln!(f, "{}", name_of(p)).map_err(|e| e.to_string())?;
    drop(f);
    Ok(marks(p))
}

/// Removes the last REROLLS.txt line for this file; Ok(false) when none was marked.
pub fn unmark_reroll(p: &Path) -> Result<bool, String> {
    let rp = folder_of(p).join(REROLLS);
    let name = name_of(p);
    let mut cur = read_lines(&rp);
    match cur.iter().rposition(|l| *l == name) {
        Some(i) => {
            cur.remove(i);
            write_lines(&rp, &cur)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Removes the given lines (one occurrence each) from REROLLS.txt in `folder`.
pub fn consume_rerolls(folder: &Path, done: &[String]) -> Result<(), String> {
    let rp = folder.join(REROLLS);
    let mut cur = read_lines(&rp);
    for d in done {
        if let Some(i) = cur.iter().position(|l| l == d) {
            cur.remove(i);
        }
    }
    write_lines(&rp, &cur)
}

/// Files in a folder that are rated or bookmarked: (name, stars, bookmark), for the library list.
pub fn marked_in(folder: &Path) -> Vec<(String, u8, bool)> {
    load_store(folder).files.into_iter().filter(|(_, m)| m.stars > 0 || m.bookmark).map(|(k, m)| (k, m.stars, m.bookmark)).collect()
}

/// (stars, bookmark) for every marked file in a folder.
pub fn star_map(folder: &Path) -> HashMap<String, (u8, bool)> {
    load_store(folder).files.into_iter().map(|(k, m)| (k, (m.stars, m.bookmark))).collect()
}

/// Names rejected in a folder (for skipping them while stepping).
pub fn rejected_in(folder: &Path) -> HashSet<String> {
    load_store(folder).files.into_iter().filter(|(_, m)| m.rejected).map(|(k, _)| k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_round_trip() {
        let d = std::env::temp_dir().join(format!("loupe-side-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let a = d.join("a_00001_.png");
        let b = d.join("b_00002_.png");
        assert_eq!(marks(&a), Marks::default());
        assert!(toggle_pick(&a).unwrap().pick);
        assert!(toggle_pick(&b).unwrap().pick);
        assert!(!toggle_pick(&a).unwrap().pick);
        assert_eq!(read_lines(&d.join(PICKS)), vec!["b_00002_.png"]);
        assert_eq!(mark_reroll(&a).unwrap().rerolls_marked, 1);
        assert_eq!(mark_reroll(&a).unwrap().rerolls_marked, 2);
        mark_reroll(&b).unwrap();
        assert!(unmark_reroll(&a).unwrap());
        assert_eq!(marks(&a).rerolls_marked, 1);
        consume_rerolls(&d, &["a_00001_.png".into()]).unwrap();
        assert_eq!(read_lines(&d.join(REROLLS)), vec!["b_00002_.png"]);
        assert_eq!(update(&a, |m| m.stars = 4).unwrap().stars, 4);
        assert!(update(&a, |m| m.rejected = true).unwrap().rejected);
        // the store is hidden after creation; rewriting it in place must still work
        assert_eq!(update(&a, |m| m.stars = 2).unwrap().stars, 2);
        assert!(rejected_in(&d).contains("a_00001_.png"));
        assert!(!update(&a, |m| m.rejected = false).unwrap().rejected);
        assert!(rejected_in(&d).is_empty());
    }

    #[test]
    fn pick_copies_land_by_set_and_only_loupes_own_copy_is_removed() {
        let d = std::env::temp_dir().join(format!("loupe-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let set = d.join("261014-set-x");
        fs::create_dir_all(&set).unwrap();
        let f = set.join("01_a_00000.png");
        fs::write(&f, b"frame bytes").unwrap();
        let out = d.join("picked");
        let copy = out.join("261014-set-x").join("01_a_00000.png");
        assert_eq!(copy_pick(&f, &out, true).unwrap(), Some(copy.clone()));
        assert_eq!(fs::read(&copy).unwrap(), b"frame bytes");
        assert_eq!(copy_pick(&f, &out, true).unwrap(), None, "already there");
        assert_eq!(copy_pick(&f, &out, false).unwrap(), Some(copy.clone()));
        assert!(!copy.exists() && !copy.parent().unwrap().exists(), "the copy and its emptied folder go");
        assert!(f.exists(), "the frame stays");
        // a copy someone changed is left alone, and is never overwritten
        copy_pick(&f, &out, true).unwrap();
        fs::write(&copy, b"edited, longer bytes").unwrap();
        assert_eq!(copy_pick(&f, &out, false).unwrap(), None);
        assert!(copy.exists());
        assert!(copy_pick(&f, &out, true).is_err());
        // a copy folder that leads back to the frame itself is refused, and the frame survives an unpick
        assert!(copy_pick(&f, &d, false).is_err());
        assert!(copy_pick(&f, &d, true).is_err());
        assert_eq!(fs::read(&f).unwrap(), b"frame bytes");
        let _ = fs::remove_dir_all(&d);
    }
}
