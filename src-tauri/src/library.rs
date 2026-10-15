//! The library panel (L): bookmarked and rated frames across the watched folders, and frames grouped by the LoRAs named in their
//! metadata (2026-10-13), sortable by date and by rating.
//!
//! Marks come from each folder's .loupe.json (sidecar). The LoRA index reads every watched image's text chunks once (meta::summarize
//! - headers only, the pixels are never read) in a background thread, and keeps the result in %APPDATA%\Loupe\lora-cache.json keyed
//! by path and modification time, so later opens only read new or changed files.

use crate::{index, meta, settings, sidecar, AppState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Clone)]
pub struct Item {
    path: String,
    rel: String,
    name: String,
    mtime: f64,
    stars: u8,
    bookmark: bool,
}

/// path -> (the index's modification time, the LoRA names in the file's metadata)
#[derive(Default)]
pub struct Loras {
    loaded: bool,
    building: bool,
    done: usize,
    total: usize,
    files: HashMap<PathBuf, (i64, Vec<String>)>,
}

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    v: u32,
    files: HashMap<String, (i64, Vec<String>)>,
}

fn cache_path() -> PathBuf {
    settings::dir().join("lora-cache.json")
}

/// "Anima\People'sWorks_v10.5.safetensors" -> "People'sWorks_v10.5"
fn lora_name(raw: &str) -> String {
    let n = raw.rsplit(['\\', '/']).next().unwrap_or(raw).trim();
    n.strip_suffix(".safetensors").unwrap_or(n).to_string()
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn make_item(ix: &index::Index, p: &Path, mtime: i64, stars: u8, bookmark: bool) -> Item {
    Item { path: p.to_string_lossy().to_string(), rel: ix.rel(p), name: name_of(p), mtime: mtime as f64 / 1e9, stars, bookmark }
}

/// Every rated or bookmarked frame that is still in the index.
#[tauri::command(async)]
pub fn marked_list(state: tauri::State<'_, AppState>) -> Vec<Item> {
    let ix = state.index.lock().unwrap();
    let folders: HashSet<PathBuf> = ix.files.keys().filter_map(|p| p.parent().map(|d| d.to_path_buf())).collect();
    let mut out = Vec::new();
    for f in folders {
        for (name, stars, bookmark) in sidecar::marked_in(&f) {
            let p = f.join(&name);
            if let Some(&m) = ix.files.get(&p) {
                out.push(make_item(&ix, &p, m, stars, bookmark));
            }
        }
    }
    out
}

#[derive(Serialize)]
pub struct LoraStatus {
    building: bool,
    done: usize,
    total: usize,
    /// (LoRA, frames using it), most used first
    loras: Vec<(String, usize)>,
}

/// The LoRA counts over the frames in the index; starts (or continues) a background read of any frame not yet indexed.
/// The panel polls this while `building` is true.
#[tauri::command(async)]
pub fn lora_index(state: tauri::State<'_, AppState>) -> LoraStatus {
    let snapshot: Vec<(PathBuf, i64)> = {
        let ix = state.index.lock().unwrap();
        ix.files.iter().filter(|(p, _)| index::kind_of(p) == Some("image")).map(|(p, m)| (p.clone(), *m)).collect()
    };
    let arc = state.loras.clone();
    {
        let mut l = arc.lock().unwrap();
        if !l.loaded {
            if let Some(c) = fs::read_to_string(cache_path()).ok().and_then(|t| serde_json::from_str::<Cache>(&t).ok()) {
                l.files = c.files.into_iter().map(|(k, v)| (PathBuf::from(k), v)).collect();
            }
            l.loaded = true;
        }
        if !l.building {
            let todo: Vec<(PathBuf, i64)> =
                snapshot.iter().filter(|(p, m)| l.files.get(p).map(|(mm, _)| mm != m).unwrap_or(true)).cloned().collect();
            if !todo.is_empty() {
                l.building = true;
                l.done = 0;
                l.total = todo.len();
                let arc2 = arc.clone();
                std::thread::spawn(move || build(arc2, todo));
            }
        }
    }
    let l = arc.lock().unwrap();
    let live: HashSet<&PathBuf> = snapshot.iter().map(|(p, _)| p).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (p, (_, names)) in l.files.iter() {
        if live.contains(p) {
            for n in names {
                *counts.entry(n.clone()).or_default() += 1;
            }
        }
    }
    let mut loras: Vec<(String, usize)> = counts.into_iter().collect();
    loras.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    LoraStatus { building: l.building, done: l.done, total: l.total, loras }
}

fn build(arc: Arc<Mutex<Loras>>, todo: Vec<(PathBuf, i64)>) {
    let mut batch: Vec<(PathBuf, (i64, Vec<String>))> = Vec::new();
    let total = todo.len();
    for (i, (p, m)) in todo.into_iter().enumerate() {
        let mut names: Vec<String> =
            meta::summarize(&p).loras.into_iter().map(|(n, _)| lora_name(&n)).filter(|n| !n.is_empty()).collect();
        names.sort();
        names.dedup();
        batch.push((p, (m, names)));
        if batch.len() >= 100 || i + 1 == total {
            let mut l = arc.lock().unwrap();
            for (p, v) in batch.drain(..) {
                l.files.insert(p, v);
            }
            l.done = i + 1;
        }
    }
    let mut l = arc.lock().unwrap();
    let cache = Cache { v: 1, files: l.files.iter().map(|(p, v)| (p.to_string_lossy().to_string(), v.clone())).collect() };
    if fs::create_dir_all(settings::dir()).is_ok() {
        if let Ok(text) = serde_json::to_string(&cache) {
            let tmp = settings::dir().join("lora-cache.json.tmp");
            if fs::write(&tmp, text).is_ok() {
                let _ = fs::rename(&tmp, cache_path());
            }
        }
    }
    l.done = l.total;
    l.building = false;
    crate::logf!("lora index: {} files read, {} in the cache", total, l.files.len());
}

/// The frames whose metadata names this LoRA, with their stars and bookmarks.
#[tauri::command(async)]
pub fn lora_images(state: tauri::State<'_, AppState>, name: String) -> Vec<Item> {
    let paths: Vec<PathBuf> = {
        let l = state.loras.lock().unwrap();
        l.files.iter().filter(|(_, (_, n))| n.iter().any(|x| *x == name)).map(|(p, _)| p.clone()).collect()
    };
    let ix = state.index.lock().unwrap();
    let mut marks: HashMap<PathBuf, HashMap<String, (u8, bool)>> = HashMap::new();
    let mut out = Vec::new();
    for p in paths {
        if let Some(&m) = ix.files.get(&p) {
            let folder = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let mk = marks.entry(folder.clone()).or_insert_with(|| sidecar::star_map(&folder));
            let (stars, bookmark) = mk.get(&name_of(&p)).cloned().unwrap_or((0, false));
            out.push(make_item(&ix, &p, m, stars, bookmark));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(lora_name("Anima\\People'sWorks_v10.5_Animabasev1.0.safetensors"), "People'sWorks_v10.5_Animabasev1.0");
        assert_eq!(lora_name("kegantV5"), "kegantV5");
        assert_eq!(lora_name("sub/dir/x.safetensors"), "x");
    }
}
