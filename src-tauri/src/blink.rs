//! Blink inside BlinkLoupe (2026-10-14: a comparison window that works on anyone's ComfyUI setup, built into Loupe
//! rather than as a separate app). Blink's page (ui/blink/index.html: the seven comparison views, the numbers and the settings diff,
//! all computed in the page) is served at http://blink.localhost/ by a custom protocol, and its comparison groups are
//! found here, from the folders BlinkLoupe watches, by rules that assume nothing about anyone's pipeline:
//!   - versions: a frame and everything made from it, named after it: re-rolls (`<core>_reroll_*`), Send to a workflow's
//!     results (`<stem>_<key name>_*`), hires and passes, in its folder or in a sibling folder named after it
//!     (`<set>-hires`, `<set>_upscaled`) and that folder's subfolders;
//!   - A/B folders: sibling folders holding the same file names (a restyle of a set, bench runs, an A/B test);
//!   - folders: any folder, tick the images to compare (and any files dropped on the window).
//! The page asks for /api/groups, /api/list?dir= and /img?p= (the same root-checked file server the main window uses).

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const IMG_EXT: &[&str] = &["png", "jpg", "jpeg", "webp"];

pub fn is_image(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| IMG_EXT.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Member {
    pub label: String,
    pub path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Group {
    pub kind: &'static str,
    pub folder: String,
    pub title: String,
    pub mtime: i64,
    pub members: Vec<Member>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Folder {
    /// the folder's full path (the page sends it back to /api/list)
    pub name: String,
    /// shown: the root's own name, then the path under it
    pub label: String,
    pub mtime: i64,
    pub count: usize,
}

#[derive(Serialize, Debug)]
pub struct Data {
    pub roots: Vec<String>,
    pub groups: Vec<Group>,
    pub folders: Vec<Folder>,
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name)
}

/// The name without a trailing save counter (`_00001_`, `_00000`, 4+ digits): "fox_00001_" -> "fox".
pub fn core(stem: &str) -> &str {
    let t = stem.strip_suffix('_').unwrap_or(stem);
    if let Some((head, digits)) = t.rsplit_once('_') {
        if digits.len() >= 4 && digits.chars().all(|c| c.is_ascii_digit()) && !head.is_empty() {
            return head;
        }
    }
    stem
}

/// The tag that opens what follows an original's name: "reroll_00001_" -> "reroll", "upscale-4x_00001_" -> "upscale-4x".
/// None when it opens with a digit (the next frame of a batch is not a version of this one).
pub fn tag(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches('_');
    if !rest.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    Some(rest.split('_').next().unwrap_or(rest).to_string())
}

/// A shown name for a folder: the root's last part, then the path under it ("output/261013-set-fox").
pub fn label_of(dir: &Path, roots: &[PathBuf]) -> String {
    let d = dir.to_string_lossy().to_string();
    for r in roots {
        let rs = r.to_string_lossy().trim_end_matches(['\\', '/']).to_string();
        if d.len() >= rs.len() && d[..rs.len()].eq_ignore_ascii_case(&rs) && (d.len() == rs.len() || matches!(d.as_bytes()[rs.len()], b'\\' | b'/')) {
            let base = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| rs.clone());
            let rest = d[rs.len()..].trim_start_matches(['\\', '/']).replace('\\', "/");
            return if rest.is_empty() { base } else { format!("{}/{}", base, rest) };
        }
    }
    d.replace('\\', "/")
}

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

struct File {
    path: PathBuf,
    name: String,
    mtime: i64,
}

/// Every group, from the index's files (path -> mtime).
pub fn scan(files: &HashMap<PathBuf, i64>, roots: &[PathBuf]) -> Data {
    let mut by_dir: BTreeMap<PathBuf, Vec<File>> = BTreeMap::new();
    for (p, t) in files {
        if !is_image(p) {
            continue;
        }
        if let (Some(d), Some(n)) = (p.parent(), p.file_name()) {
            by_dir.entry(d.to_path_buf()).or_default().push(File { path: p.clone(), name: n.to_string_lossy().to_string(), mtime: *t });
        }
    }
    for v in by_dir.values_mut() {
        v.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let mut groups = versions(&by_dir, roots);
    groups.extend(parallel(&by_dir, roots));
    groups.sort_by(|a, b| b.mtime.cmp(&a.mtime).then(a.title.cmp(&b.title)));
    let mut folders: Vec<Folder> = by_dir
        .iter()
        .map(|(d, fs)| Folder { name: d.to_string_lossy().to_string(), label: label_of(d, roots), mtime: fs.iter().map(|f| f.mtime).max().unwrap_or(0), count: fs.len() })
        .collect();
    folders.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    Data { roots: roots.iter().map(|r| r.to_string_lossy().to_string()).collect(), groups, folders }
}

/// The sibling folders named after `dir` ("set" -> "set-hires", "set_upscaled"), with their subfolders.
fn finish_dirs<'a>(dir: &Path, by_dir: &'a BTreeMap<PathBuf, Vec<File>>) -> Vec<&'a PathBuf> {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().map(|n| n.to_string_lossy().to_lowercase())) else { return Vec::new() };
    let sib: Vec<&PathBuf> = by_dir
        .keys()
        .filter(|g| {
            g.parent() == Some(parent)
                && g.file_name()
                    .map(|n| {
                        let n = n.to_string_lossy().to_lowercase();
                        n.len() > name.len() + 1 && n.starts_with(&name) && matches!(n.as_bytes()[name.len()], b'-' | b'_' | b' ' | b'.')
                    })
                    .unwrap_or(false)
        })
        .collect();
    let mut out = sib.clone();
    for g in sib {
        out.extend(by_dir.keys().filter(|k| k.parent() == Some(g.as_path())));
    }
    out
}

/// A frame and its versions, wherever they are; versions of versions join the first frame's group.
fn versions(by_dir: &BTreeMap<PathBuf, Vec<File>>, roots: &[PathBuf]) -> Vec<Group> {
    let mut out = Vec::new();
    for (dir, originals) in by_dir {
        // the candidates: this folder's own files, then those in the folders named after it
        let mut cands: Vec<(&File, String)> = originals.iter().map(|f| (f, String::new())).collect();
        for g in finish_dirs(dir, by_dir) {
            let sub = g.strip_prefix(dir.parent().unwrap_or(dir)).ok().and_then(|r| r.iter().nth(1)).map(|s| s.to_string_lossy().to_string());
            for f in &by_dir[g] {
                cands.push((f, sub.clone().unwrap_or_default()));
            }
        }
        let stems: Vec<&str> = originals.iter().map(|f| stem(&f.name)).collect();
        let mut by_core: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, s) in stems.iter().enumerate() {
            by_core.entry(core(s)).or_default().push(i);
        }
        // parent of each candidate: the longest original whose full name it extends with a tag, else the one original
        // whose name without its counter it extends with a tag
        let mut parent: HashMap<String, (usize, String)> = HashMap::new(); // candidate path key -> (original index, tag)
        for (f, _) in &cands {
            let ds = stem(&f.name);
            let own = key(&f.path);
            let mut best: Option<(usize, usize, String)> = None;
            for (i, os) in stems.iter().enumerate() {
                if key(&originals[i].path) == own || ds.len() <= os.len() + 1 || !ds.starts_with(os) || ds.as_bytes()[os.len()] != b'_' {
                    continue;
                }
                if let Some(t) = tag(&ds[os.len() + 1..]) {
                    if best.as_ref().map(|b| os.len() > b.1).unwrap_or(true) {
                        best = Some((i, os.len(), t));
                    }
                }
            }
            if best.is_none() {
                for (c, idx) in &by_core {
                    if idx.len() != 1 || *c == ds || ds.len() <= c.len() + 1 || !ds.starts_with(c) || ds.as_bytes()[c.len()] != b'_' {
                        continue;
                    }
                    let i = idx[0];
                    if key(&originals[i].path) == own || core(ds) == *c {
                        continue;
                    }
                    if let Some(t) = tag(&ds[c.len() + 1..]) {
                        if best.as_ref().map(|b| c.len() > b.1).unwrap_or(true) {
                            best = Some((i, c.len(), t));
                        }
                    }
                }
            }
            if let Some((i, _, t)) = best {
                parent.insert(own, (i, t));
            }
        }
        // follow parents to the first frame, collecting the tag path
        let idx_of: HashMap<String, usize> = originals.iter().enumerate().map(|(i, f)| (key(&f.path), i)).collect();
        let mut members: BTreeMap<usize, Vec<(i64, String, String)>> = BTreeMap::new(); // root original -> (mtime, label, path)
        for (f, sub) in &cands {
            let own = key(&f.path);
            let Some((mut p, t)) = parent.get(&own).cloned() else { continue };
            let mut tags = vec![t];
            let mut guard = 0;
            while let Some((pp, pt)) = parent.get(&key(&originals[p].path)) {
                guard += 1;
                if guard > 16 || *pp == p {
                    break;
                }
                tags.push(pt.clone());
                p = *pp;
            }
            tags.reverse();
            let mut label = tags.join(" › ");
            if !sub.is_empty() {
                label = format!("{} · {}", sub, label);
            }
            members.entry(p).or_default().push((f.mtime, label, f.path.to_string_lossy().to_string()));
        }
        for (root, mut ms) in members {
            // a frame that is itself a version of another is shown in that one's group, not as its own
            if parent.contains_key(&key(&originals[root].path)) {
                continue;
            }
            ms.sort();
            ms.dedup_by(|a, b| a.2 == b.2);
            let mut seen: HashMap<String, usize> = HashMap::new();
            let mut list = vec![Member { label: "original".into(), path: originals[root].path.to_string_lossy().to_string() }];
            let mut mtime = originals[root].mtime;
            for (t, l, p) in ms {
                let n = seen.entry(l.clone()).or_insert(0);
                *n += 1;
                list.push(Member { label: if *n > 1 { format!("{} {}", l, n) } else { l }, path: p });
                mtime = mtime.max(t);
            }
            let _ = idx_of.len();
            out.push(Group { kind: "versions", folder: label_of(dir, roots), title: stem(&originals[root].name).to_string(), mtime, members: list });
        }
    }
    out
}

/// Sibling folders that hold the same file names: one group per shared name, one member per folder (oldest first).
fn parallel(by_dir: &BTreeMap<PathBuf, Vec<File>>, roots: &[PathBuf]) -> Vec<Group> {
    let mut out = Vec::new();
    let mut by_parent: BTreeMap<&Path, Vec<&PathBuf>> = BTreeMap::new();
    for d in by_dir.keys() {
        if let Some(p) = d.parent() {
            by_parent.entry(p).or_default().push(d);
        }
    }
    for (parent, dirs) in by_parent {
        if dirs.len() < 2 {
            continue;
        }
        // names that could pair folders: not ComfyUI's default prefix, which every folder of plain runs shares
        let mut where_: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, d) in dirs.iter().enumerate() {
            for f in &by_dir[*d] {
                if core(stem(&f.name)).eq_ignore_ascii_case("ComfyUI") {
                    continue;
                }
                where_.entry(f.name.to_lowercase()).or_default().push(i);
            }
        }
        let mut pairs: HashMap<(usize, usize), usize> = HashMap::new();
        for v in where_.values() {
            if v.len() < 2 || v.len() > 16 {
                continue;
            }
            for a in 0..v.len() {
                for b in a + 1..v.len() {
                    *pairs.entry((v[a], v[b])).or_insert(0) += 1;
                }
            }
        }
        // union the folders that share at least two names and half of the smaller folder
        let mut uf: Vec<usize> = (0..dirs.len()).collect();
        fn find(uf: &mut Vec<usize>, x: usize) -> usize {
            let mut r = x;
            while uf[r] != r {
                r = uf[r];
            }
            let mut c = x;
            while uf[c] != r {
                let n = uf[c];
                uf[c] = r;
                c = n;
            }
            r
        }
        let mut linked = HashSet::new();
        for ((a, b), n) in &pairs {
            let small = by_dir[dirs[*a]].len().min(by_dir[dirs[*b]].len());
            // a one-image folder joins on its one name (a bench case run on one setting only)
            if n * 2 >= small && (*n >= 2 || small == 1) {
                let (ra, rb) = (find(&mut uf, *a), find(&mut uf, *b));
                uf[ra] = rb;
                linked.insert(*a);
                linked.insert(*b);
            }
        }
        let mut comps: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in linked {
            let r = find(&mut uf, i);
            comps.entry(r).or_default().push(i);
        }
        for (_, mut comp) in comps {
            if comp.len() < 2 || comp.len() > 16 {
                continue;
            }
            let first = |i: usize| by_dir[dirs[i]].iter().map(|f| f.mtime).min().unwrap_or(0);
            comp.sort_by_key(|i| (first(*i), dirs[*i].clone()));
            let names: Vec<String> = comp.iter().map(|i| dirs[*i].file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect();
            let labels = short_labels(&names);
            let folder = format!("{}: {}", label_of(parent, roots), names.join(" ↔ "));
            let mut shared: BTreeMap<String, Vec<(usize, &File)>> = BTreeMap::new();
            for (k, i) in comp.iter().enumerate() {
                for f in &by_dir[dirs[*i]] {
                    shared.entry(f.name.to_lowercase()).or_default().push((k, f));
                }
            }
            for (_, fs) in shared {
                if fs.len() < 2 || core(stem(&fs[0].1.name)).eq_ignore_ascii_case("ComfyUI") {
                    continue;
                }
                out.push(Group {
                    kind: "parallel",
                    folder: folder.clone(),
                    title: stem(&fs[0].1.name).to_string(),
                    mtime: fs.iter().map(|(_, f)| f.mtime).max().unwrap_or(0),
                    members: fs.iter().map(|(k, f)| Member { label: labels[*k].clone(), path: f.path.to_string_lossy().to_string() }).collect(),
                });
            }
        }
    }
    out
}

/// Folder names without what they all share at the start ("vram-bench-baseline", "vram-bench-fp16" -> "baseline", "fp16");
/// the whole name when that would leave nothing.
fn short_labels(names: &[String]) -> Vec<String> {
    let first = names.first().cloned().unwrap_or_default();
    let mut n = first.len();
    for s in names {
        n = n.min(first.chars().zip(s.chars()).take_while(|(a, b)| a.eq_ignore_ascii_case(b)).count());
    }
    if names.iter().any(|s| s.len() <= n) {
        return names.to_vec(); // one name is the others' start ("set", "set-restyle"): keep them whole
    }
    // cut at a separator so a shared first letter doesn't eat a word
    let cut = first[..n.min(first.len())].rfind(['-', '_', ' ', '.']).map(|i| i + 1).unwrap_or(0);
    names.iter().map(|s| if s.len() > cut && cut > 0 { s[cut..].to_string() } else { s.clone() }).collect()
}

/// What /api/list shows for a folder inside a root: its subfolders and images, and the folder above (none at a root).
#[derive(Serialize)]
pub struct Listing {
    pub dir: String,
    pub label: String,
    pub parent: String,
    pub subdirs: Vec<String>,
    pub images: Vec<String>,
}

pub fn inside(p: &Path, roots: &[PathBuf]) -> bool {
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    let lower = PathBuf::from(p.to_string_lossy().to_lowercase());
    roots.iter().any(|r| lower.starts_with(PathBuf::from(r.to_string_lossy().to_lowercase())))
}

pub fn list(dir: &str, roots: &[PathBuf]) -> Option<Listing> {
    let d = PathBuf::from(dir.replace('/', "\\"));
    if !inside(&d, roots) || !d.is_dir() {
        return None;
    }
    let mut subdirs = Vec::new();
    let mut images = Vec::new();
    for e in std::fs::read_dir(&d).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            subdirs.push(n);
        } else if is_image(&p) {
            images.push(n);
        }
    }
    subdirs.sort();
    images.sort();
    let at_root = roots.iter().any(|r| r.to_string_lossy().trim_end_matches(['\\', '/']).eq_ignore_ascii_case(d.to_string_lossy().trim_end_matches(['\\', '/'])));
    let parent = if at_root { String::new() } else { d.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default() };
    Some(Listing { dir: d.to_string_lossy().to_string(), label: label_of(&d, roots), parent, subdirs, images })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> HashMap<PathBuf, i64> {
        list.iter().enumerate().map(|(i, p)| (PathBuf::from(p), i as i64)).collect()
    }

    fn find<'a>(d: &'a Data, title: &str) -> &'a Group {
        d.groups.iter().find(|g| g.title == title).unwrap_or_else(|| panic!("no group {title}: {:?}", d.groups.iter().map(|g| &g.title).collect::<Vec<_>>()))
    }

    fn labels(g: &Group) -> Vec<&str> {
        g.members.iter().map(|m| m.label.as_str()).collect()
    }

    #[test]
    fn counters_and_tags() {
        assert_eq!(core("fox_00001_"), "fox");
        assert_eq!(core("01_border_00000"), "01_border");
        assert_eq!(core("v2_10"), "v2_10", "short numbers are part of the name");
        assert_eq!(tag("reroll_00001_"), Some("reroll".into()));
        assert_eq!(tag("_upscale-4x_00001_"), Some("upscale-4x".into()));
        assert_eq!(tag("00002_"), None);
    }

    #[test]
    fn versions_in_a_folder_and_beside_it() {
        let r = vec![PathBuf::from(r"D:\out")];
        let d = scan(
            &files(&[
                r"D:\out\set\01_fox_00000.png",
                r"D:\out\set\01_fox_reroll_00000.png",
                r"D:\out\set\01_fox_reroll_00001.png",
                r"D:\out\set\02_owl_00000.png",
                r"D:\out\set\02_owl_00000_upscale-4x_00001_.png",
                r"D:\out\set\02_owl_00000_upscale-4x_00001__flip_00001_.png",
                r"D:\out\set-hires\detail\01_fox_00000_base_00000.png",
                r"D:\out\set-hires\01_fox_00000_pass_00000.png",
                r"D:\out\set-hires\01_fox_00000_hires_00001_.png",
                r"D:\out\batch\ComfyUI_00001_.png",
                r"D:\out\batch\ComfyUI_00002_.png",
                r"D:\out\batch\ComfyUI_reroll_00001_.png",
            ]),
            &r,
        );
        let fox = find(&d, "01_fox_00000");
        assert_eq!(labels(fox), vec!["original", "reroll", "reroll 2", "detail · base", "pass", "hires"]);
        assert_eq!(fox.folder, "out/set");
        let owl = find(&d, "02_owl_00000");
        assert_eq!(labels(owl), vec!["original", "upscale-4x", "upscale-4x › flip"]);
        assert!(!d.groups.iter().any(|g| g.title.starts_with("02_owl_00000_upscale")), "a version is not a group of its own");
        assert!(!d.groups.iter().any(|g| g.folder == "out/batch"), "a re-roll of one frame of a batch can't be told apart");
        assert!(d.folders.iter().any(|f| f.label == "out/set-hires/detail" && f.count == 1));
    }

    #[test]
    fn parallel_folders() {
        let r = vec![PathBuf::from(r"D:\out")];
        let d = scan(
            &files(&[
                r"D:\out\tests\vram-bench-baseline\fox_00001_.png",
                r"D:\out\tests\vram-bench-baseline\owl_00001_.png",
                r"D:\out\tests\vram-bench-fp16\fox_00001_.png",
                r"D:\out\tests\vram-bench-fp16\owl_00001_.png",
                r"D:\out\tests\vram-bench-sage\fox_00001_.png",
                r"D:\out\set-a\01_x_00000.png",
                r"D:\out\set-a\02_y_00000.png",
                r"D:\out\set-a-restyle\01_x_00000.png",
                r"D:\out\set-a-restyle\02_y_00000.png",
                r"D:\out\runs1\ComfyUI_00001_.png",
                r"D:\out\runs1\ComfyUI_00002_.png",
                r"D:\out\runs2\ComfyUI_00001_.png",
                r"D:\out\runs2\ComfyUI_00002_.png",
            ]),
            &r,
        );
        let fox: Vec<&Group> = d.groups.iter().filter(|g| g.kind == "parallel" && g.title == "fox_00001_").collect();
        assert_eq!(fox.len(), 1);
        assert_eq!(labels(fox[0]), vec!["baseline", "fp16", "sage"]);
        let x = d.groups.iter().find(|g| g.kind == "parallel" && g.title == "01_x_00000").unwrap();
        assert_eq!(labels(x), vec!["set-a", "set-a-restyle"]);
        assert!(!d.groups.iter().any(|g| g.title.starts_with("ComfyUI")), "default ComfyUI names never pair folders");
    }

    #[test]
    fn labels_and_listing_bounds() {
        let r = vec![PathBuf::from(r"D:\out"), PathBuf::from(r"C:\Users\me\Pictures\BlinkLoupe")];
        assert_eq!(label_of(Path::new(r"D:\out\a\b"), &r), "out/a/b");
        assert_eq!(label_of(Path::new(r"C:\Users\me\Pictures\BlinkLoupe"), &r), "BlinkLoupe");
        assert!(inside(Path::new(r"D:\OUT\x.png"), &r));
        assert!(!inside(Path::new(r"D:\out\..\secret.png"), &r));
        assert!(!inside(Path::new(r"D:\outside\x.png"), &r));
        assert!(list(r"C:\Windows", &r).is_none());
    }
}
