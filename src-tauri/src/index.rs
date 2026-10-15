//! Every image and video under the watched roots, discovered by mtime-gated folder scans (a port of the Output Viewer's
//! Index): one stat per known folder each poll, a listing only for folders whose mtime changed. A new file is committed
//! only once it is complete - its size has held since the last look and its format's trailer is present.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
pub const VIDEO_EXTS: &[&str] = &["mp4", "webm", "m4v", "mov"];

const SETTLE: Duration = Duration::from_millis(900);
const VIDEO_SETTLE: Duration = Duration::from_secs(3);
const GIVE_UP: Duration = Duration::from_secs(30);

pub fn kind_of(p: &Path) -> Option<&'static str> {
    let e = p.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXTS.contains(&e.as_str()) {
        Some("image")
    } else if VIDEO_EXTS.contains(&e.as_str()) {
        Some("video")
    } else {
        None
    }
}

fn mtime_ns(md: &fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Some(true) when the format's trailer is present, Some(false) when it is not (yet), None for formats without a cheap check.
pub fn looks_complete(path: &Path, size: u64) -> Option<bool> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mut f = fs::File::open(path).ok()?;
    let tail = |f: &mut fs::File, n: u64| -> Option<Vec<u8>> {
        let n = n.min(size);
        f.seek(SeekFrom::Start(size - n)).ok()?;
        let mut b = vec![0u8; n as usize];
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    let head = |f: &mut fs::File, n: usize| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(0)).ok()?;
        let mut b = vec![0u8; n];
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    match ext.as_str() {
        "png" => Some(tail(&mut f, 12)? == [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]),
        "jpg" | "jpeg" => Some(tail(&mut f, 32)?.windows(2).any(|w| w == [0xFF, 0xD9])),
        "gif" => Some(tail(&mut f, 1)? == [0x3B]),
        "webp" => {
            let h = head(&mut f, 12)?;
            let riff = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as u64;
            Some(&h[0..4] == b"RIFF" && &h[8..12] == b"WEBP" && riff + 8 <= size)
        }
        "bmp" => {
            let h = head(&mut f, 6)?;
            Some(u32::from_le_bytes([h[2], h[3], h[4], h[5]]) as u64 <= size)
        }
        _ => None,
    }
}

struct Pending {
    size: u64,
    changed: Instant,
}

pub struct Index {
    pub roots: Vec<PathBuf>,
    pub depth: u32,
    dirs: HashMap<PathBuf, (i64, u32)>,
    pub files: HashMap<PathBuf, i64>,
    pub order: Vec<(i64, PathBuf)>,
    pending: HashMap<PathBuf, Pending>,
    /// Folders never walked into, lower-cased (the picks copy folder, so copies don't come back as new frames).
    skip: Vec<String>,
    /// whether full_scan has run (Blink, opened on its own, scans on its first request)
    pub scanned: bool,
}

fn folded(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase()
}

impl Index {
    pub fn new(roots: Vec<PathBuf>, depth: u32) -> Self {
        Index { roots, depth, dirs: HashMap::new(), files: HashMap::new(), order: Vec::new(), pending: HashMap::new(), skip: Vec::new(), scanned: false }
    }

    pub fn skipping(mut self, dirs: &[PathBuf]) -> Self {
        self.skip = dirs.iter().filter(|d| !d.as_os_str().is_empty()).map(|d| folded(d)).collect();
        self
    }

    pub fn full_scan(&mut self) {
        self.scanned = true;
        self.dirs.clear();
        self.files.clear();
        self.order.clear();
        self.pending.clear();
        for r in self.roots.clone() {
            self.scan_dir(&r, 0, true);
        }
        self.order.sort();
    }

    fn scan_dir(&mut self, d: &Path, depth: u32, initial: bool) {
        let md = match fs::metadata(d) {
            Ok(m) => m,
            Err(_) => {
                self.dirs.remove(d);
                return;
            }
        };
        self.dirs.insert(d.to_path_buf(), (mtime_ns(&md), depth));
        let rd = match fs::read_dir(d) {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut present: HashSet<PathBuf> = HashSet::new();
        for e in rd.flatten() {
            let ft = match e.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let p = e.path();
            if ft.is_dir() {
                let name = e.file_name().to_string_lossy().to_string();
                if depth < self.depth && !name.starts_with('.') && !name.starts_with("__") && !self.dirs.contains_key(&p)
                    && !self.skip.contains(&folded(&p))
                {
                    self.scan_dir(&p, depth + 1, initial);
                }
            } else if ft.is_file() && kind_of(&p).is_some() {
                present.insert(p.clone());
                if self.files.contains_key(&p) || self.pending.contains_key(&p) {
                    continue;
                }
                let m = match e.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if initial {
                    self.files.insert(p.clone(), mtime_ns(&m));
                    self.order.push((mtime_ns(&m), p));
                } else {
                    self.pending.insert(p, Pending { size: m.len(), changed: Instant::now() });
                }
            }
        }
        if !initial {
            // Files that left this folder (deleted or moved away by someone else) leave the index too.
            let gone: Vec<PathBuf> =
                self.files.keys().filter(|p| p.parent() == Some(d) && !present.contains(*p)).cloned().collect();
            for p in gone {
                self.remove(&p);
            }
        }
    }

    pub fn remove(&mut self, p: &Path) {
        if let Some(m) = self.files.remove(p) {
            if let Ok(i) = self.order.binary_search(&(m, p.to_path_buf())) {
                self.order.remove(i);
            }
        }
        self.pending.remove(p);
    }

    fn commit(&mut self, p: PathBuf, m: i64) {
        let key = (m, p.clone());
        let i = self.order.binary_search(&key).unwrap_or_else(|i| i);
        self.order.insert(i, key);
        self.files.insert(p, m);
    }

    /// One tick: rescans folders that changed, then commits pending files that are complete. Returns them oldest first.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        let dirs: Vec<(PathBuf, i64, u32)> = self.dirs.iter().map(|(p, (m, d))| (p.clone(), *m, *d)).collect();
        for (d, m, depth) in dirs {
            match fs::metadata(&d) {
                Ok(md) => {
                    if mtime_ns(&md) != m {
                        self.scan_dir(&d, depth, false);
                    }
                }
                Err(_) => {
                    self.dirs.remove(&d);
                }
            }
        }
        // a root that did not exist at start may appear later
        for r in self.roots.clone() {
            if !self.dirs.contains_key(&r) && r.is_dir() {
                self.scan_dir(&r, 0, false);
            }
        }
        let now = Instant::now();
        let mut new: Vec<(i64, PathBuf)> = Vec::new();
        let pend: Vec<PathBuf> = self.pending.keys().cloned().collect();
        for p in pend {
            let (size0, changed) = {
                let x = &self.pending[&p];
                (x.size, x.changed)
            };
            if now.duration_since(changed) < SETTLE {
                continue; // settle on a later tick, never the one it was found on
            }
            let md = match fs::metadata(&p) {
                Ok(m) => m,
                Err(_) => {
                    self.pending.remove(&p);
                    continue;
                }
            };
            if md.len() != size0 || md.len() == 0 {
                let x = self.pending.get_mut(&p).unwrap();
                x.size = md.len();
                x.changed = now;
                continue;
            }
            let quiet = now.duration_since(changed);
            let ready = match looks_complete(&p, md.len()) {
                Some(true) => true,
                Some(false) => {
                    if quiet >= GIVE_UP {
                        crate::logf!("committing {} without its trailer after {}s of no growth", p.display(), quiet.as_secs());
                        true
                    } else {
                        false
                    }
                }
                None => quiet >= VIDEO_SETTLE,
            };
            if ready {
                self.pending.remove(&p);
                let m = mtime_ns(&md);
                self.commit(p.clone(), m);
                new.push((m, p));
            }
        }
        new.sort();
        new.into_iter().map(|(_, p)| p).collect()
    }

    pub fn position(&self, p: &Path) -> Option<usize> {
        let m = *self.files.get(p)?;
        self.order.binary_search(&(m, p.to_path_buf())).ok()
    }

    /// (n, total, seconds since the previous frame in the same folder)
    pub fn folder_progress(&self, p: &Path) -> (usize, usize, Option<f64>) {
        let d = p.parent();
        let same: Vec<&(i64, PathBuf)> = self.order.iter().filter(|(_, q)| q.parent() == d).collect();
        let n = same.iter().position(|(_, q)| q == p).map(|i| i + 1).unwrap_or(0);
        let gap = if n >= 2 { Some((same[n - 1].0 - same[n - 2].0) as f64 / 1e9) } else { None };
        (n, same.len(), gap)
    }

    pub fn root_of(&self, p: &Path) -> Option<&PathBuf> {
        self.roots.iter().filter(|r| p.starts_with(r)).max_by_key(|r| r.as_os_str().len())
    }

    pub fn rel(&self, p: &Path) -> String {
        match self.root_of(p) {
            Some(r) => {
                let rel = p.strip_prefix(r).map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                if self.roots.len() > 1 {
                    let name = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    format!("{}\\{}", name, rel)
                } else {
                    rel
                }
            }
            None => p.to_string_lossy().to_string(),
        }
    }

    pub fn folders(&self) -> usize {
        self.dirs.len()
    }
}

/// A folder's plan: how many frames a run means to make there. Counting the files on disk makes the newest frame of a set
/// still rendering n/n, so any pipeline can write this file before it queues (2026-10-13: show the frame number out of the
/// set's total, not x/x; opened to any pipeline 2026-10-14). The format (README, "Pipeline hooks"):
///   {"total": N}                                        a set: "frame n/N"
///   {"total": N, "by": "kind", "kinds": ["pass", ...]}  a finishing folder: "<kind> n/N" ("kinds" defaults to pass, hires)
/// Other keys are ignored.
pub const SET_PLAN: &str = ".loupe-set.json";

pub struct Plan {
    pub total: usize,
    /// Some: a finishing folder, counted by these kinds of file
    pub kinds: Option<Vec<String>>,
}

fn read_plan(dir: &Path) -> Option<Plan> {
    let txt = fs::read_to_string(dir.join(SET_PLAN)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let total = v.get("total")?.as_u64()? as usize;
    if total == 0 {
        return None;
    }
    let kinds = (v.get("by").and_then(|b| b.as_str()) == Some("kind")).then(|| {
        let k: Vec<String> = v
            .get("kinds")
            .and_then(|k| k.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        if k.is_empty() { vec!["pass".to_string(), "hires".to_string()] } else { k }
    });
    Some(Plan { total, kinds })
}

/// A frame's own number from a leading "NN_" ("05_ramen-stall_00000.png" -> 5). None when the name does not start with 1-4
/// digits and an underscore, or the number is 0.
pub fn frame_number(name: &str) -> Option<usize> {
    let digits = name.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 4 || !name[digits..].starts_with('_') {
        return None;
    }
    name[..digits].parse().ok().filter(|n| *n > 0)
}

/// A re-roll carries the re-roll suffix (Settings > Re-rolls) in its name; an empty suffix means none can be told apart.
pub fn is_reroll(name: &str, suffix: &str) -> bool {
    !suffix.is_empty() && name.to_lowercase().contains(&suffix.to_lowercase())
}

/// Which of the plan's kinds a file in a finishing folder is: "<stem>_pass_00000.png" is a pass, "<stem>_hires_00001_.png" a
/// hires. The first kind that appears as "_<kind>_" or "_<kind>." wins.
pub fn finish_kind(name: &str, kinds: &[String]) -> Option<String> {
    let l = name.to_ascii_lowercase();
    kinds.iter().find(|k| l.contains(&format!("_{k}_")) || l.contains(&format!("_{k}."))).cloned()
}

impl Index {
    /// (frame number, the set's planned total, re-roll) in a folder whose plan is a set. The number is the name's "NN_"; a
    /// name without one (ComfyUI's own "ComfyUI_00012_.png") counts by its place among the folder's frames that are not
    /// re-rolls. None without a plan: the HUD counts files.
    pub fn set_position(&self, p: &Path, reroll_suffix: &str) -> Option<(usize, usize, bool)> {
        let d = p.parent()?;
        let plan = read_plan(d)?;
        if plan.kinds.is_some() {
            return None;
        }
        let name = p.file_name()?.to_string_lossy().to_string();
        let reroll = is_reroll(&name, reroll_suffix);
        let n = match frame_number(&name) {
            Some(n) => n,
            None if reroll => return None,
            None => {
                self.order
                    .iter()
                    .filter(|(_, q)| q.parent() == Some(d) && !q.file_name().map(|f| is_reroll(&f.to_string_lossy(), reroll_suffix)).unwrap_or(false))
                    .position(|(_, q)| q == p)?
                    + 1
            }
        };
        Some((n, plan.total.max(n), reroll))
    }

    /// (place among the folder's files of its kind, by time; the planned total; the kind) in a folder whose plan counts by
    /// kind. Its files carry the SOURCE set's numbers (a -hires folder's HUD would read 16/16), so the place is by time.
    pub fn finish_position(&self, p: &Path) -> Option<(usize, usize, String)> {
        let d = p.parent()?;
        let plan = read_plan(d)?;
        let kinds = plan.kinds?;
        let kind = finish_kind(&p.file_name()?.to_string_lossy(), &kinds)?;
        let n = self
            .order
            .iter()
            .filter(|(_, q)| q.parent() == Some(d) && q.file_name().and_then(|f| finish_kind(&f.to_string_lossy(), &kinds)).as_ref() == Some(&kind))
            .position(|(_, q)| q == p)?
            + 1;
        Some((n, plan.total.max(n), kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn frame_numbers_from_names() {
        assert_eq!(frame_number("05_ramen-stall-rain-anime_00000.png"), Some(5));
        assert_eq!(frame_number("11_conductor-murmuration-myw4y_reroll_00000.png"), Some(11));
        assert_eq!(frame_number("Muse35_00001_.png"), None);
        assert_eq!(frame_number("00_x.png"), None);
        assert_eq!(frame_number("20261013_x.png"), None);
        assert_eq!(frame_number("7.png"), None);
        assert!(is_reroll("11_conductor_reroll_00000.png", "_reroll"));
        assert!(is_reroll("fox_RR_00001_.png", "_rr"), "the user's own suffix");
        assert!(!is_reroll("11_conductor_reroll_00000.png", ""));
    }

    fn put(ix: &mut Index, d: &Path, names: &[&str]) {
        for (i, nm) in names.iter().enumerate() {
            let p = d.join(nm);
            ix.files.insert(p.clone(), i as i64);
            ix.order.push((i as i64, p));
        }
    }

    #[test]
    fn set_position_reads_the_plan() {
        let d = tmpdir("setplan");
        let f = d.join("03_falconer_00000.png");
        fs::write(&f, b"x").unwrap();
        let mut ix = Index::new(vec![d.clone()], 3);
        put(&mut ix, &d, &["ComfyUI_00001_.png", "03_falconer_00000.png", "ComfyUI_00002_reroll_00001_.png", "ComfyUI_00003_.png"]);
        assert_eq!(ix.set_position(&f, "_reroll"), None, "no plan: the HUD counts files");
        fs::write(d.join(SET_PLAN), br#"{"total": 14, "written": "2026-10-13 20:37:05"}"#).unwrap();
        assert_eq!(ix.set_position(&f, "_reroll"), Some((3, 14, false)));
        assert_eq!(ix.set_position(&d.join("03_falconer_reroll_00000.png"), "_reroll"), Some((3, 14, true)));
        assert_eq!(ix.set_position(&d.join("16_extra_00000.png"), "_reroll"), Some((16, 16, false)), "a frame past the plan stretches it");
        assert_eq!(ix.set_position(&d.join("ComfyUI_00003_.png"), "_reroll"), Some((3, 14, false)), "no NN_: its place among non-re-rolls");
        assert_eq!(ix.set_position(&d.join("ComfyUI_00002_reroll_00001_.png"), "_reroll"), None, "an unnumbered re-roll has no place");
        fs::write(d.join(SET_PLAN), b"not json").unwrap();
        assert_eq!(ix.set_position(&f, "_reroll"), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn finish_folder_counts_by_kind() {
        let ph = ["pass".to_string(), "hires".to_string()];
        assert_eq!(finish_kind("40_40-stobe-carcass_00000_pass_00000.png", &ph).as_deref(), Some("pass"));
        assert_eq!(finish_kind("03_falconer_00000_hires_00001_.png", &ph).as_deref(), Some("hires"));
        assert_eq!(finish_kind("03_falconer_00000.png", &ph), None);
        let d = tmpdir("kindplan");
        fs::write(d.join(SET_PLAN), br#"{"total": 21, "by": "kind"}"#).unwrap();
        let mut ix = Index::new(vec![d.clone()], 3);
        let names = ["01_a_00000_pass_00000.png", "03_b_00000_pass_00000.png", "01_a_00000_hires_00001_.png", "08_c_00000_pass_00000.png"];
        put(&mut ix, &d, &names);
        let at = |i: usize| ix.finish_position(&d.join(names[i])).map(|(n, t, k)| (n, t, k.to_string()));
        assert_eq!(at(1), Some((2, 21, "pass".into())));
        assert_eq!(at(3), Some((3, 21, "pass".into())), "the hires between them is not counted");
        assert_eq!(at(2), Some((1, 21, "hires".into())));
        assert_eq!(ix.set_position(&d.join(names[1]), "_reroll"), None, "a kind plan never reads the source numbers");
        // anyone's own kinds
        fs::write(d.join(SET_PLAN), br#"{"total": 5, "by": "kind", "kinds": ["Upscaled", "facefix"]}"#).unwrap();
        let mut ix = Index::new(vec![d.clone()], 3);
        put(&mut ix, &d, &["a_upscaled_00001_.png", "a_facefix_00001_.png", "b_upscaled_00001_.png", "c_pass_00000.png"]);
        assert_eq!(ix.finish_position(&d.join("b_upscaled_00001_.png")), Some((2, 5, "upscaled".into())));
        assert_eq!(ix.finish_position(&d.join("a_facefix_00001_.png")), Some((1, 5, "facefix".into())));
        assert_eq!(ix.finish_position(&d.join("c_pass_00000.png")), None, "pass is not one of these kinds");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn skipped_folders_are_never_walked() {
        let d = tmpdir("skip");
        for sub in ["set", "picks"] {
            fs::create_dir_all(d.join(sub)).unwrap();
            fs::write(d.join(sub).join("a.png"), [b"\x89PNG".as_slice(), &PNG_END].concat()).unwrap();
        }
        let mut ix = Index::new(vec![d.clone()], 3).skipping(&[d.join("PICKS"), PathBuf::new()]);
        ix.full_scan();
        assert_eq!(ix.order.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>(), vec![d.join("set").join("a.png")]);
        let _ = fs::remove_dir_all(&d);
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("loupe-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    const PNG_END: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];

    #[test]
    fn completeness() {
        let d = tmpdir("complete");
        let p = d.join("a.png");
        fs::write(&p, [b"\x89PNG\r\n\x1a\nxxxx".as_slice(), &PNG_END].concat()).unwrap();
        assert_eq!(looks_complete(&p, fs::metadata(&p).unwrap().len()), Some(true));
        fs::write(&p, b"\x89PNG\r\n\x1a\nxxxxxxxxxxxxxx").unwrap();
        assert_eq!(looks_complete(&p, fs::metadata(&p).unwrap().len()), Some(false));
        let j = d.join("b.jpg");
        fs::write(&j, [0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9]).unwrap();
        assert_eq!(looks_complete(&j, 7), Some(true));
        let v = d.join("c.mp4");
        fs::write(&v, b"....").unwrap();
        assert_eq!(looks_complete(&v, 4), None);
    }

    #[test]
    fn scan_settle_and_order() {
        let d = tmpdir("scan");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("old.png"), [b"\x89PNG".as_slice(), &PNG_END].concat()).unwrap();
        fs::write(d.join("notes.txt"), b"x").unwrap();
        let mut ix = Index::new(vec![d.clone()], 3);
        ix.full_scan();
        assert_eq!(ix.order.len(), 1);
        // a half-written file in a subfolder: found, but not committed until complete and settled
        let p = d.join("sub").join("new.png");
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(b"\x89PNG partial").unwrap();
        drop(f);
        std::thread::sleep(Duration::from_millis(30));
        assert!(ix.poll().is_empty());
        std::thread::sleep(Duration::from_millis(950));
        assert!(ix.poll().is_empty(), "no trailer yet");
        let mut f = fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(&PNG_END).unwrap();
        drop(f);
        assert!(ix.poll().is_empty(), "size just changed");
        std::thread::sleep(Duration::from_millis(950));
        let got = ix.poll();
        assert_eq!(got, vec![p.clone()]);
        assert_eq!(ix.order.last().unwrap().1, p);
        assert_eq!(ix.folder_progress(&p), (1, 1, None));
        assert_eq!(ix.rel(&p), "sub\\new.png");
        // deletion leaves the index
        fs::remove_file(&p).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        ix.poll();
        assert!(!ix.files.contains_key(&p));
    }
}
