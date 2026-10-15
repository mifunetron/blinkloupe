#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod blink;
mod comfy;
mod feed;
mod hires;
mod http;
mod index;
mod library;
mod logx;
mod meta;
mod send;
mod settings;
mod sidecar;
mod update;
mod ws;

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use tauri::Manager;

struct AppState {
    index: Mutex<index::Index>,
    /// the LoRA index for the library's By LoRA view (built on demand in the background, cached in %APPDATA%\Loupe)
    loras: std::sync::Arc<Mutex<library::Loras>>,
    settings: Mutex<settings::Settings>,
    comfy: Mutex<comfy::Comfy>,
    /// prompt ids queued from each source frame this session (newest last), for undo
    queued: Mutex<HashMap<PathBuf, Vec<String>>>,
    /// every prompt id Loupe queued this session (re-rolls and hires), for the "yours" count
    mine: Mutex<HashSet<String>>,
    /// --root for this session only (testing); never saved
    cli_roots: Option<Vec<String>>,
    cli_windowed: bool,
    /// what the ComfyUI follower is doing (feed.rs)
    feed: Mutex<feed::Status>,
}

impl AppState {
    /// The watched folders, plus the feed folder while following a server.
    fn roots(&self, s: &settings::Settings) -> Vec<PathBuf> {
        let mut r: Vec<PathBuf> = self.cli_roots.clone().unwrap_or_else(|| s.roots.clone()).into_iter().map(PathBuf::from).collect();
        if s.follow_server {
            let f = s.feed_dir_path();
            let _ = fs::create_dir_all(&f); // a root that is missing at the start is never picked up later
            if !r.iter().any(|x| x.to_string_lossy().eq_ignore_ascii_case(&f.to_string_lossy())) {
                r.push(f);
            }
        }
        r
    }
}

#[derive(Serialize)]
struct Entry {
    path: String,
    rel: String,
    folder: String,
    name: String,
    mtime: f64,
    n: usize,
    total: usize,
    gap: Option<f64>,
    kind: String,
    marks: sidecar::Marks,
    queued: usize,
    index: usize,
    count: usize,
    /// frames marked skip in this frame's folder (the HUD's "showing skipped" tag appears only when there are some)
    skipped_here: usize,
    /// n / total come from the runner's plan (the frame's NN_ over the set's planned size) rather than a count of the files
    of_plan: bool,
    /// a re-roll of frame n (its name carries _reroll); only told apart when the folder has a plan
    reroll: bool,
    /// "pass" / "hires" in a finishing folder whose plan counts by kind (n / total = place among that kind / the picks), else ""
    finish: String,
}

fn entry_for(st: &AppState, ix: &index::Index, p: &Path) -> Option<Entry> {
    let m = *ix.files.get(p)?;
    let (n, total, gap) = ix.folder_progress(p);
    let suffix = st.settings.lock().unwrap().reroll_suffix.clone();
    let (n, total, of_plan, reroll, finish) = if let Some((pn, pt, rr)) = ix.set_position(p, &suffix) {
        (pn, pt, true, rr, String::new())
    } else if let Some((pn, pt, k)) = ix.finish_position(p) {
        (pn, pt, true, false, k)
    } else {
        (n, total, false, false, String::new())
    };
    let queued = st.queued.lock().unwrap().get(p).map(|v| v.len()).unwrap_or(0);
    Some(Entry {
        path: p.to_string_lossy().to_string(),
        rel: ix.rel(p),
        folder: p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
        name: p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        mtime: m as f64 / 1e9,
        n,
        total,
        gap,
        kind: index::kind_of(p).unwrap_or("image").to_string(),
        marks: sidecar::marks(p),
        queued,
        index: ix.position(p).map(|i| i + 1).unwrap_or(0),
        count: ix.order.len(),
        skipped_here: p.parent().map(|d| sidecar::rejected_in(d).len()).unwrap_or(0),
        of_plan,
        reroll,
        finish,
    })
}

// ----------------------------------------------------------------------------------------------- stream commands

#[derive(Serialize)]
struct StartInfo {
    newest: Option<Entry>,
    files: usize,
    folders: usize,
    roots: Vec<String>,
    missing: Vec<String>,
    first_run: bool,
    ms: u128,
}

#[tauri::command(async)]
fn start(state: tauri::State<'_, AppState>) -> StartInfo {
    let t0 = Instant::now();
    let s = state.settings.lock().unwrap().clone();
    let roots = state.roots(&s);
    let mut ix = state.index.lock().unwrap();
    *ix = index::Index::new(roots.clone(), s.depth).skipping(&[PathBuf::from(&s.picks_copy_dir)]);
    ix.full_scan();
    let newest = newest_visible(&state, &ix, s.show_rejected);
    let info = StartInfo {
        newest,
        files: ix.order.len(),
        folders: ix.folders(),
        roots: roots.iter().map(|r| r.to_string_lossy().to_string()).collect(),
        missing: roots.iter().filter(|r| !r.is_dir()).map(|r| r.to_string_lossy().to_string()).collect(),
        first_run: roots.is_empty(),
        ms: t0.elapsed().as_millis(),
    };
    logf!("start: {} roots, {} folders, {} files in {} ms", info.roots.len(), info.folders, info.files, info.ms);
    info
}

#[tauri::command(async)]
fn poll(state: tauri::State<'_, AppState>) -> Vec<Entry> {
    let mut ix = state.index.lock().unwrap();
    let new = ix.poll();
    if !new.is_empty() {
        logf!("new: {}", new.iter().map(|p| ix.rel(p)).collect::<Vec<_>>().join(", "));
    }
    new.iter().filter_map(|p| entry_for(&state, &ix, p)).collect()
}

fn is_rejected(cache: &mut HashMap<PathBuf, HashSet<String>>, p: &Path) -> bool {
    let d = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let set = cache.entry(d.clone()).or_insert_with(|| sidecar::rejected_in(&d));
    p.file_name().map(|n| set.contains(&n.to_string_lossy().to_string())).unwrap_or(false)
}

fn newest_visible(st: &AppState, ix: &index::Index, show_rejected: bool) -> Option<Entry> {
    let mut cache = HashMap::new();
    ix.order.iter().rev().find(|(_, p)| show_rejected || !is_rejected(&mut cache, p)).and_then(|(_, p)| entry_for(st, ix, p))
}

#[derive(Serialize)]
struct StepResult {
    entry: Option<Entry>,
    past_newest: bool,
}

#[tauri::command(async)]
fn step(state: tauri::State<'_, AppState>, path: String, delta: i64) -> StepResult {
    let show_rejected = state.settings.lock().unwrap().show_rejected;
    let ix = state.index.lock().unwrap();
    let len = ix.order.len() as i64;
    let pos = match ix.position(Path::new(&path)) {
        Some(p) => p as i64,
        None => return StepResult { entry: newest_visible(&state, &ix, show_rejected), past_newest: true },
    };
    let dir = if delta >= 0 { 1 } else { -1 };
    let mut remaining = delta.abs().max(1);
    let mut j = pos;
    let mut cache = HashMap::new();
    loop {
        j += dir;
        if j >= len {
            return StepResult { entry: None, past_newest: true };
        }
        if j < 0 {
            return StepResult { entry: None, past_newest: false };
        }
        let p = &ix.order[j as usize].1;
        if !show_rejected && is_rejected(&mut cache, p) {
            continue;
        }
        remaining -= 1;
        if remaining == 0 {
            return StepResult { entry: entry_for(&state, &ix, p), past_newest: false };
        }
    }
}

#[tauri::command(async)]
fn edge(state: tauri::State<'_, AppState>, which: String) -> Option<Entry> {
    let show_rejected = state.settings.lock().unwrap().show_rejected;
    let ix = state.index.lock().unwrap();
    if which == "oldest" {
        let mut cache = HashMap::new();
        ix.order.iter().find(|(_, p)| show_rejected || !is_rejected(&mut cache, p)).and_then(|(_, p)| entry_for(&state, &ix, p))
    } else {
        newest_visible(&state, &ix, show_rejected)
    }
}

#[tauri::command(async)]
fn entry(state: tauri::State<'_, AppState>, path: String) -> Option<Entry> {
    let ix = state.index.lock().unwrap();
    entry_for(&state, &ix, Path::new(&path))
}

#[tauri::command(async)]
fn meta(path: String) -> meta::Meta {
    meta::summarize(Path::new(&path))
}

// ----------------------------------------------------------------------------------------------- marks

#[derive(Serialize)]
struct MarkResult {
    entry: Option<Entry>,
    /// what the picks copy did ("copied to …" / "copy removed"), or why it failed
    note: String,
    warn: bool,
}

#[tauri::command(async)]
fn mark(state: tauri::State<'_, AppState>, path: String, action: String, value: Option<u8>) -> Result<MarkResult, String> {
    let p = PathBuf::from(&path);
    let copy_dir = state.settings.lock().unwrap().picks_copy_dir.clone();
    let copy = |on: bool| -> (String, bool) {
        if copy_dir.is_empty() {
            return (String::new(), false);
        }
        match sidecar::copy_pick(&p, Path::new(&copy_dir), on) {
            Ok(Some(d)) if on => (format!("copied to {}", d.parent().map(|x| x.display().to_string()).unwrap_or_default()), false),
            Ok(Some(_)) => ("copy removed".into(), false),
            Ok(None) => (String::new(), false),
            Err(e) => (e, true),
        }
    };
    let (mut note, mut warn) = (String::new(), false);
    match action.as_str() {
        "pick" => {
            let on = sidecar::toggle_pick(&p)?.pick;
            (note, warn) = copy(on);
        }
        "reject" => {
            sidecar::update(&p, |m| m.rejected = !m.rejected)?;
        }
        "bookmark" => {
            sidecar::update(&p, |m| m.bookmark = !m.bookmark)?;
        }
        "stars" => {
            let v = value.unwrap_or(0).min(5);
            sidecar::update(&p, |m| m.stars = if m.stars == v { 0 } else { v })?;
        }
        "unflag" => {
            if sidecar::marks(&p).pick {
                (note, warn) = copy(false);
            }
            sidecar::set_pick(&p, false)?;
            sidecar::update(&p, |m| {
                m.rejected = false;
                m.stars = 0
            })?;
        }
        _ => return Err(format!("unknown mark {}", action)),
    }
    logf!("mark {} {}{}", action, path, if note.is_empty() { String::new() } else { format!(" ({note})") });
    let ix = state.index.lock().unwrap();
    Ok(MarkResult { entry: entry_for(&state, &ix, &p), note, warn })
}

#[derive(Serialize)]
struct RerollResult {
    action: String,
    note: String,
    entry: Option<Entry>,
}

fn queue_one(state: &AppState, url: &str, p: &Path, suffix: &str) -> Result<String, String> {
    let chunks = meta::read_chunks(p);
    let prompt = meta::chunk(&chunks, "prompt").ok_or("this file carries no ComfyUI graph")?;
    let mut g = match serde_json::from_str::<Value>(prompt) {
        Ok(Value::Object(g)) => g,
        _ => return Err("the graph in this file does not parse".into()),
    };
    if comfy::reroll_graph(&mut g, suffix) == 0 {
        return Err("no sampler seed found in this graph".into());
    }
    let wf = meta::chunk(&chunks, "workflow").and_then(|w| serde_json::from_str::<Value>(w).ok());
    let id = comfy::queue(url, &g, wf)?;
    state.queued.lock().unwrap().entry(p.to_path_buf()).or_default().push(id.clone());
    state.mine.lock().unwrap().insert(id.clone());
    Ok(id)
}

#[tauri::command(async)]
fn reroll(state: tauri::State<'_, AppState>, path: String) -> Result<RerollResult, String> {
    let p = PathBuf::from(&path);
    let s = state.settings.lock().unwrap().clone();
    let url = if s.reroll_mode == "mark" { None } else { state.comfy.lock().unwrap().current(&s.comfy_url, false) };
    let (action, note) = match (s.reroll_mode.as_str(), url) {
        ("mark", _) => {
            sidecar::mark_reroll(&p)?;
            ("marked".to_string(), String::new())
        }
        ("queue", None) => return Err("ComfyUI is not answering - start it, or set its address in Settings".into()),
        (_, Some(url)) => match queue_one(&state, &url, &p, &s.reroll_suffix) {
            Ok(id) => ("queued".to_string(), id),
            Err(e) if s.reroll_mode == "auto" && e.contains("no ComfyUI graph") => {
                sidecar::mark_reroll(&p)?;
                ("marked".to_string(), "no ComfyUI graph in this file - marked in REROLLS.txt instead".to_string())
            }
            Err(e) => return Err(e),
        },
        ("auto", None) => {
            sidecar::mark_reroll(&p)?;
            ("marked".to_string(), "ComfyUI is not answering - marked in REROLLS.txt".to_string())
        }
        _ => unreachable!(),
    };
    logf!("reroll {} {} {}", action, path, note);
    let ix = state.index.lock().unwrap();
    Ok(RerollResult { action, note, entry: entry_for(&state, &ix, &p) })
}

#[tauri::command(async)]
fn unreroll(state: tauri::State<'_, AppState>, path: String) -> Result<RerollResult, String> {
    let p = PathBuf::from(&path);
    let last = state.queued.lock().unwrap().get(&p).and_then(|v| v.last().cloned());
    let (action, note) = if let Some(id) = last {
        let s = state.settings.lock().unwrap().clone();
        let url = state.comfy.lock().unwrap().current(&s.comfy_url, false).ok_or("ComfyUI is not answering")?;
        if comfy::cancel(&url, &id)? {
            if let Some(v) = state.queued.lock().unwrap().get_mut(&p) {
                v.pop();
            }
            ("cancelled".to_string(), String::new())
        } else {
            ("running".to_string(), "that re-roll has already started".to_string())
        }
    } else if sidecar::unmark_reroll(&p)? {
        ("unmarked".to_string(), String::new())
    } else {
        ("none".to_string(), "no re-roll to undo".to_string())
    };
    logf!("unreroll {} {} {}", action, path, note);
    let ix = state.index.lock().unwrap();
    Ok(RerollResult { action, note, entry: entry_for(&state, &ix, &p) })
}

#[derive(Serialize)]
struct QueueMarked {
    queued: usize,
    skipped: Vec<String>,
    error: Option<String>,
}

/// Queues every REROLLS.txt request in this frame's folder and removes the lines it queued.
#[tauri::command(async)]
fn queue_marked(state: tauri::State<'_, AppState>, path: String) -> Result<QueueMarked, String> {
    let folder = Path::new(&path).parent().ok_or("no folder")?.to_path_buf();
    let s = state.settings.lock().unwrap().clone();
    let url = state.comfy.lock().unwrap().current(&s.comfy_url, true).ok_or("ComfyUI is not answering")?;
    let lines = sidecar::read_lines(&folder.join(sidecar::REROLLS));
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    let mut error = None;
    for name in lines {
        let p = folder.join(&name);
        if !p.is_file() {
            skipped.push(format!("{} (missing)", name));
            continue;
        }
        match queue_one(&state, &url, &p, &s.reroll_suffix) {
            Ok(_) => done.push(name),
            Err(e) => {
                skipped.push(format!("{} ({})", name, e));
                if error.is_none() {
                    error = Some(e);
                }
            }
        }
    }
    sidecar::consume_rerolls(&folder, &done)?;
    logf!("queue_marked {}: {} queued, {} skipped", folder.display(), done.len(), skipped.len());
    Ok(QueueMarked { queued: done.len(), skipped, error })
}

// ----------------------------------------------------------------------------------------------- hires on picks

fn hires_opts(s: &settings::Settings) -> hires::Opts {
    hires::Opts {
        scale: s.hires_scale,
        max_mp: s.hires_max_mp,
        denoise: s.hires_denoise,
        steps: s.hires_steps,
        upscale_model: s.upscale_model.clone(),
        suffix: s.hires_suffix.clone(),
        folder_suffix: s.hires_folder_suffix.clone(),
    }
}

/// Uploads the frame, builds its hires graph and queues it. Ok(note) describes the pass.
fn hires_one(
    state: &AppState,
    url: &str,
    p: &Path,
    o: &hires::Opts,
    schemas: &mut HashMap<String, Option<Value>>,
) -> Result<String, String> {
    let chunks = meta::read_chunks(p);
    let prompt = meta::chunk(&chunks, "prompt").ok_or("this file carries no ComfyUI graph - hires rebuilds it from that graph")?;
    let g = match serde_json::from_str::<Value>(prompt) {
        Ok(Value::Object(g)) => g,
        _ => return Err("the graph in this file does not parse".into()),
    };
    let src = meta::dims(p).ok_or("cannot read the frame's size")?;
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    // fail on the graph before uploading anything
    hires::build(&g, &stem, src, "check.png", o, &|_| None)?;
    for class in ["LoadImage", "ImageScale", "VAEEncodeTiled", "VAEDecodeTiled", "KSampler", "UpscaleModelLoader", "ConditioningZeroOut"] {
        if !schemas.contains_key(class) {
            schemas.insert(class.to_string(), comfy::object_info(url, class));
        }
    }
    let name = comfy::upload_image(url, p)?;
    let plan = hires::build(&g, &stem, src, &name, o, &|c| schemas.get(c).cloned().flatten())?;
    let id = comfy::queue(url, &plan.graph, None)?;
    state.mine.lock().unwrap().insert(id);
    sidecar::update(p, |m| m.hires += 1)?;
    Ok(plan.note)
}

#[derive(Serialize)]
struct HiresResult {
    note: String,
    entry: Option<Entry>,
}

#[tauri::command(async)]
fn hires(state: tauri::State<'_, AppState>, path: String) -> Result<HiresResult, String> {
    let p = PathBuf::from(&path);
    let s = state.settings.lock().unwrap().clone();
    let url = state
        .comfy
        .lock()
        .unwrap()
        .current(&s.comfy_url, false)
        .ok_or("ComfyUI is not answering - hires needs it running")?;
    let note = hires_one(&state, &url, &p, &hires_opts(&s), &mut HashMap::new())?;
    logf!("hires queued {} ({})", path, note);
    let ix = state.index.lock().unwrap();
    Ok(HiresResult { note, entry: entry_for(&state, &ix, &p) })
}

#[derive(Serialize)]
struct ActionResult {
    ok: bool,
    /// false: started and left running (the action does not wait)
    waited: bool,
    line: String,
    /// the file named by a FINISHED / ALREADY line
    output: Option<String>,
}

/// Splits a command line on spaces outside double quotes (the quotes themselves are dropped).
fn split_cmd(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for ch in s.chars() {
        match ch {
            '"' => { quoted = !quoted; any = true; }
            c if c.is_whitespace() && !quoted => { if any { out.push(std::mem::take(&mut cur)); any = false; } }
            c => { cur.push(c); any = true; }
        }
    }
    if any { out.push(cur); }
    out
}

/// An action's command line for one frame: split into arguments first, then {file} {folder} {name} {stem} filled in, so a
/// path with spaces stays one argument whether or not the template quotes it.
fn action_args(cmd: &str, p: &Path) -> Vec<String> {
    let file = p.to_string_lossy().to_string();
    let folder = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    split_cmd(cmd)
        .into_iter()
        .map(|a| a.replace("{file}", &file).replace("{folder}", &folder).replace("{name}", &name).replace("{stem}", &stem))
        .collect()
}

/// Runs the user's action number `index` (Settings > Actions) on this frame. A waiting action reports its verdict line.
#[tauri::command(async)]
fn run_action(state: tauri::State<'_, AppState>, index: usize, path: String) -> Result<ActionResult, String> {
    let a = state.settings.lock().unwrap().actions.get(index).cloned().ok_or("That action is gone - Settings > Actions")?;
    let p = PathBuf::from(&path);
    let parts = action_args(&a.cmd, &p);
    if parts.is_empty() {
        return Err(format!("{}: no command set", a.label));
    }
    let mut c = std::process::Command::new(&parts[0]);
    c.args(&parts[1..]);
    if let Some(d) = p.parent() {
        c.current_dir(d);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashes up
    }
    if !a.wait {
        c.spawn().map_err(|e| format!("Could not start {}: {e}", parts[0]))?;
        logf!("action {} started on {}", a.key, path);
        return Ok(ActionResult { ok: true, waited: false, line: String::new(), output: None });
    }
    let out = c.output().map_err(|e| format!("Could not run {}: {e}", parts[0]))?;
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let line = text
        .lines()
        .rev()
        .find(|l| l.starts_with("FINISHED") || l.starts_with("ALREADY") || l.starts_with("ERROR"))
        .or_else(|| text.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("")
        .trim()
        .to_string();
    let output = ["FINISHED ", "ALREADY "].iter().find_map(|p| line.strip_prefix(p)).map(|rest| rest.split(" (").next().unwrap_or(rest).to_string());
    let ok = out.status.success() && !line.starts_with("ERROR");
    logf!("action {} on {} -> {}", a.key, path, line);
    Ok(ActionResult { ok, waited: true, line, output })
}

/// `folder` relative to `root` with / separators ("" when they are the same folder); None when it is not inside.
fn rel_under(folder: &Path, root: &Path) -> Option<String> {
    let f = folder.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_string();
    let r = root.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_string();
    if r.is_empty() {
        return None;
    }
    if f.eq_ignore_ascii_case(&r) {
        return Some(String::new());
    }
    let head = f.get(..r.len() + 1)?;
    if head.eq_ignore_ascii_case(&format!("{}\\", r)) {
        Some(f[r.len() + 1..].replace('\\', "/"))
    } else {
        None
    }
}

/// Where "beside the frame" is in the server's output folder: the frame's folder relative to a local server's output
/// folder, or to a followed server's mirror; else a folder named like the frame's own.
fn beside_folder(url: &str, frame: &Path, s: &settings::Settings) -> String {
    let folder = frame.parent().unwrap_or(Path::new(""));
    if feed::is_local(url) {
        if let Some(rel) = comfy::system_stats(url).and_then(|v| comfy::output_dir_from_stats(&v)).and_then(|out| rel_under(folder, &out)) {
            return rel;
        }
    }
    if let Some(rel) = rel_under(folder, &s.feed_dir_path().join(feed::host_tag(url))) {
        return rel.strip_prefix("previews").map(|r| r.trim_start_matches('/').to_string()).unwrap_or(rel);
    }
    folder.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
}

#[derive(Serialize)]
struct SendResult {
    id: String,
    note: String,
}

/// Sends the frame on screen into the workflow bound to `index` (Settings > Send to a workflow) and queues it.
#[tauri::command(async)]
fn send_to_workflow(state: tauri::State<'_, AppState>, index: usize, path: String) -> Result<SendResult, String> {
    let s = state.settings.lock().unwrap().clone();
    let w = s.workflows.get(index).cloned().ok_or("That workflow key is gone - Settings > Send to a workflow")?;
    let p = PathBuf::from(&path);
    let mut l = send::load(Path::new(&w.file)).map_err(|e| format!("{}: {}", w.label, e))?;
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let media = if index::VIDEO_EXTS.contains(&ext.as_str()) { send::Media::Video } else { send::Media::Image };
    let ts = send::targets(&l.graph, media).map_err(|e| format!("{}: {}", w.label, e))?;
    let url = state.comfy.lock().unwrap().current(&s.comfy_url, false).ok_or("ComfyUI is not answering - sending to a workflow needs it running")?;
    let local = feed::is_local(&url);
    if let Some(t) = ts.iter().find(|t| t.takes == send::Takes::Path && !local) {
        return Err(format!(
            "{}: #{} {} reads a path on the server's own disk, and this ComfyUI is on another computer - use a Load Image node",
            w.label, t.id, t.class
        ));
    }
    let uploaded = if ts.iter().any(|t| t.takes == send::Takes::Upload) { Some(comfy::upload_image(&url, &p)?) } else { None };
    let fill = send::Fill::of(&p, &meta::summarize(&p));
    let slug = send::slug(&w.label);
    let sub = if w.beside { Some(beside_folder(&url, &p, &s)) } else { None };
    let o = send::Options { new_seed: w.new_seed, beside: sub.as_deref(), slug: &slug };
    let value = |t: &send::Target| match t.takes {
        send::Takes::Upload => uploaded.clone().unwrap_or_default(),
        send::Takes::Path => p.to_string_lossy().to_string(),
    };
    let r = send::prepare(&mut l.graph, &ts, &value, &fill, &o);
    let id = comfy::queue(&url, &l.graph, l.workflow).map_err(|e| format!("{}: {}", w.label, e))?;
    state.mine.lock().unwrap().insert(id.clone());
    let mut note = vec![format!("into #{}", ts.iter().map(|t| t.id.as_str()).collect::<Vec<_>>().join(" + #"))];
    if r.seeds > 0 {
        note.push(if r.seeds == 1 { "new seed".into() } else { format!("{} new seeds", r.seeds) });
    }
    if let (Some(sub), true) = (&sub, r.saves > 0) {
        note.push(format!("saves to {}{}_{}", if sub.is_empty() { String::new() } else { format!("{}/", sub) }, fill.stem, slug));
    }
    logf!("send {} ({}) {} -> prompt {} ({}; {} placeholders)", w.key, w.file, path, id, note.join(", "), r.placeholders);
    Ok(SendResult { id, note: note.join(" · ") })
}

/// One line about a workflow file for Settings: what the frame goes into, or why it can't be used.
#[tauri::command(async)]
fn workflow_summary(file: String) -> Result<String, String> {
    send::summary(Path::new(file.trim().trim_matches('"')))
}

#[tauri::command]
fn pick_workflow() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Choose a workflow: an Export (API) file, or an image it made")
        .add_filter("ComfyUI API workflow or image", &["json", "png", "webp", "jpg", "jpeg"])
        .pick_file()
        .map(|p| p.to_string_lossy().to_string())
}

#[derive(Serialize)]
struct UpdateView {
    version: String,
    /// installed by the installer, so it can update itself; otherwise only the download page is offered
    installed: bool,
    state: update::State,
    /// the release that is newer than this copy, if any
    newer: Option<update::Manifest>,
    page: String,
}

fn update_view() -> UpdateView {
    let state = update::load_state();
    let newer = state.latest.clone().filter(|m| update::newer(&m.version, update::VERSION));
    let page = newer.as_ref().map(|m| m.page.clone()).filter(|p| p.starts_with("https://")).unwrap_or_else(|| update::PAGE.to_string());
    UpdateView { version: update::VERSION.to_string(), installed: update::installed_dir().is_some(), state, newer, page }
}

fn feed_url(s: &settings::Settings) -> String {
    if s.update_url.is_empty() { update::FEED.to_string() } else { s.update_url.clone() }
}

#[tauri::command]
fn update_info() -> UpdateView {
    update_view()
}

#[tauri::command(async)]
fn check_updates_now(state: tauri::State<'_, AppState>) -> UpdateView {
    let url = feed_url(&state.settings.lock().unwrap());
    update::check(&url);
    update_view()
}

/// Downloads the newer release's installer, checks it, starts it silently and quits; the installer starts Loupe again.
#[tauri::command(async)]
fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    if update::installed_dir().is_none() {
        return Err("This copy of BlinkLoupe wasn't installed by its installer - download the new version from the release page".into());
    }
    let m = update_view().newer.ok_or("BlinkLoupe is up to date")?;
    let p = update::download(&m)?;
    logf!("update: installing {} from {}", m.version, p.display());
    update::run_installer(&p)?;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        app.exit(0);
    });
    Ok(())
}

// ----------------------------------------------------------------------------------------------- Blink and the windows

/// Opens Blink (blink.rs), or brings it forward; with a frame, Blink shows that frame's versions or its folder.
fn open_blink(app: &tauri::AppHandle, frame: Option<String>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("blink") {
        if let Some(f) = &frame {
            let _ = w.eval(&format!("window.blinkOpen && window.blinkOpen({})", serde_json::to_string(f).unwrap_or_default()));
        }
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    let q = frame.map(|f| format!("?open={}", feed::enc(&f))).unwrap_or_default();
    // the query is percent-encoded, so this always parses
    let url = tauri::Url::parse(&format!("blink://localhost/{}", q)).unwrap_or_else(|_| tauri::Url::parse("blink://localhost/").unwrap());
    let w = tauri::WebviewWindowBuilder::new(app, "blink", tauri::WebviewUrl::CustomProtocol(url))
        .title("Blink · BlinkLoupe")
        .inner_size(1600.0, 1000.0)
        .min_inner_size(800.0, 500.0)
        .theme(Some(tauri::Theme::Dark))
        .background_color(tauri::window::Color(17, 18, 20, 255)) // the page's own background: no white flash while it loads
        .disable_drag_drop_handler() // the page takes files dropped from Explorer itself
        .build()?;
    let _ = w.set_focus();
    Ok(())
}

/// The main window (the live view); brought forward when it exists.
fn open_main(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let st = app.state::<AppState>();
    let s = st.settings.lock().unwrap().clone();
    let win = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("BlinkLoupe")
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .inner_size(1280.0, 800.0)
        .min_inner_size(480.0, 360.0)
        .visible(false)
        .theme(Some(tauri::Theme::Dark))
        .build()?;
    place(&win, &s, st.cli_windowed);
    let _ = win.set_always_on_top(s.topmost);
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

/// V in the live view: Blink on the frame on screen.
#[tauri::command(async)]
fn blink(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    open_blink(&app, path).map_err(|e| e.to_string())
}

/// The second launch's request to the running copy ("main" or "blink"), in the log folder beside the lock.
fn request_path() -> PathBuf {
    logx::dir().join("request")
}

/// What blink:// serves: the page, its icon, the groups, a folder listing, and the images (root-checked).
fn serve_blink(app: &tauri::AppHandle, uri: &str, range: Option<String>) -> tauri::http::Response<Vec<u8>> {
    let st = app.state::<AppState>();
    let after_host = uri.splitn(4, '/').nth(3).unwrap_or("");
    let (path, query) = after_host.split_once('?').unwrap_or((after_host, ""));
    let param = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{}=", k))).map(percent_decode).unwrap_or_default();
    let json = |v: String| respond(200, "application/json; charset=utf-8", v.into_bytes(), &[("Cache-Control", "no-store".into())]);
    match path {
        "" | "index.html" => respond(200, "text/html; charset=utf-8", include_str!("../../ui/blink/index.html").as_bytes().to_vec(), &[("Cache-Control", "no-store".into())]),
        "icon-256.png" | "favicon.ico" => respond(200, "image/png", include_bytes!("../icons/icon.png").to_vec(), &[]),
        "api/groups" => {
            let (files, roots) = {
                let mut ix = st.index.lock().unwrap();
                if !ix.scanned {
                    ix.full_scan(); // Blink opened on its own: nothing has scanned yet
                } else if !param("fresh").is_empty() {
                    ix.poll();
                }
                (ix.files.clone(), ix.roots.clone())
            };
            json(serde_json::to_string(&blink::scan(&files, &roots)).unwrap_or_default())
        }
        "api/list" => {
            let roots = st.index.lock().unwrap().roots.clone();
            match blink::list(&param("dir"), &roots) {
                Some(l) => json(serde_json::to_string(&l).unwrap_or_default()),
                None => respond(404, "application/json", br#"{"error":"no such folder"}"#.to_vec(), &[]),
            }
        }
        "img" => {
            let roots = st.index.lock().unwrap().roots.clone();
            serve(&roots, uri, range)
        }
        "api/ping" | "api/bye" => respond(204, "text/plain", Vec::new(), &[]),
        _ => respond(404, "text/plain", Vec::new(), &[]),
    }
}

/// Opens an https:// address in the default browser.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https:// addresses open".into());
    }
    std::process::Command::new("explorer").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Opens the folder that holds loupe.log, for a bug report.
#[tauri::command]
fn open_log_folder() -> Result<(), String> {
    let d = logx::dir();
    std::process::Command::new("explorer").arg(&d).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct HiresPicks {
    queued: usize,
    already: usize,
    skipped: Vec<String>,
    error: Option<String>,
}

/// Queues a hires pass for every pick in this frame's folder that has not had one.
#[tauri::command(async)]
fn hires_picks(state: tauri::State<'_, AppState>, path: String) -> Result<HiresPicks, String> {
    let folder = Path::new(&path).parent().ok_or("no folder")?.to_path_buf();
    let s = state.settings.lock().unwrap().clone();
    let url = state.comfy.lock().unwrap().current(&s.comfy_url, true).ok_or("ComfyUI is not answering - hires needs it running")?;
    let o = hires_opts(&s);
    let mut schemas = HashMap::new();
    let (mut queued, mut already, mut skipped, mut error) = (0, 0, Vec::new(), None);
    for name in sidecar::read_lines(&folder.join(sidecar::PICKS)) {
        let p = folder.join(&name);
        if !p.is_file() {
            skipped.push(format!("{} (missing)", name));
            continue;
        }
        if sidecar::marks(&p).hires > 0 {
            already += 1;
            continue;
        }
        match hires_one(&state, &url, &p, &o, &mut schemas) {
            Ok(_) => queued += 1,
            Err(e) => {
                skipped.push(format!("{} ({})", name, e));
                if error.is_none() {
                    error = Some(e);
                }
            }
        }
    }
    logf!("hires_picks {}: {} queued, {} already hired, {} skipped", folder.display(), queued, already, skipped.len());
    Ok(HiresPicks { queued, already, skipped, error })
}

#[tauri::command(async)]
fn upscale_models(state: tauri::State<'_, AppState>) -> Vec<String> {
    let s = state.settings.lock().unwrap().clone();
    match state.comfy.lock().unwrap().current(&s.comfy_url, false) {
        Some(url) => comfy::upscale_models(&url),
        None => Vec::new(),
    }
}

#[derive(Serialize)]
struct ComfyStatus {
    url: Option<String>,
    running: usize,
    pending: usize,
    mine: usize,
    feed: feed::Status,
    /// why no server answered, when a forced check of the address in Settings failed
    error: String,
}

#[tauri::command(async)]
fn comfy_status(state: tauri::State<'_, AppState>, force: bool) -> ComfyStatus {
    let s = state.settings.lock().unwrap().clone();
    let url = state.comfy.lock().unwrap().current(&s.comfy_url, force);
    let (mut running, mut pending, mut mine) = (0, 0, 0);
    if let Some(u) = &url {
        if let Ok(q) = comfy::queue_state(u) {
            running = q.running.len();
            pending = q.pending.len();
            let ours = state.mine.lock().unwrap();
            mine = q.running.iter().chain(q.pending.iter()).filter(|id| ours.contains(*id)).count();
        }
    }
    let error = if url.is_none() && force && !s.comfy_url.is_empty() { comfy::why_not(&s.comfy_url) } else { String::new() };
    // the address goes to the screen without any user:password in it
    ComfyStatus { url: url.map(|u| http::redact(&u)), running, pending, mine, feed: state.feed.lock().unwrap().clone(), error }
}

// ----------------------------------------------------------------------------------------------- settings, folders, window

#[derive(Serialize)]
struct SettingsView {
    settings: settings::Settings,
    session_roots: Option<Vec<String>>,
    /// where a followed server's images go when Settings names no folder
    feed_default: String,
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> SettingsView {
    SettingsView {
        settings: state.settings.lock().unwrap().clone(),
        session_roots: state.cli_roots.clone(),
        feed_default: settings::default_feed_dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn save_settings(window: tauri::WebviewWindow, state: tauri::State<'_, AppState>, s: settings::Settings) -> Result<settings::Settings, String> {
    let mut s = s;
    s.sanitize();
    settings::save(&s)?;
    http::set_token(&s.comfy_token);
    let old = std::mem::replace(&mut *state.settings.lock().unwrap(), s.clone());
    if old.monitor != s.monitor || old.fullscreen != s.fullscreen {
        place(&window, &s, state.cli_windowed);
    }
    let _ = window.set_always_on_top(s.topmost);
    logf!("settings saved: {} roots, keymap {}, reroll {}", s.roots.len(), s.keymap, s.reroll_mode);
    Ok(s)
}

#[tauri::command(async)]
fn detect_folders(state: tauri::State<'_, AppState>) -> Vec<comfy::Candidate> {
    let s = state.settings.lock().unwrap().clone();
    let url = state.comfy.lock().unwrap().current(&s.comfy_url, true);
    comfy::candidate_folders(url.as_deref())
}

#[tauri::command]
fn pick_folder() -> Option<String> {
    rfd::FileDialog::new().set_title("Choose a folder for BlinkLoupe to watch").pick_folder().map(|p| p.to_string_lossy().to_string())
}

#[derive(Serialize)]
struct MonitorInfo {
    name: String,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    primary: bool,
}

#[tauri::command]
fn monitors(window: tauri::WebviewWindow) -> Vec<MonitorInfo> {
    let primary = window.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let name = m.name().cloned().unwrap_or_default();
            MonitorInfo {
                primary: Some(&name) == primary.as_ref(),
                name,
                width: m.size().width,
                height: m.size().height,
                x: m.position().x,
                y: m.position().y,
            }
        })
        .collect()
}

fn place(win: &tauri::WebviewWindow, s: &settings::Settings, force_windowed: bool) {
    let mons = win.available_monitors().unwrap_or_default();
    let primary = win.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    let target = mons
        .iter()
        .find(|m| !s.monitor.is_empty() && m.name().map(|n| n == &s.monitor).unwrap_or(false))
        .or_else(|| {
            mons.iter()
                .filter(|m| m.name() != primary.as_ref())
                .max_by_key(|m| m.size().width as u64 * m.size().height as u64)
        })
        .or_else(|| mons.first());
    let full = !force_windowed
        && match s.fullscreen.as_str() {
            "on" => true,
            "off" => false,
            _ => mons.len() > 1,
        };
    let _ = win.set_fullscreen(false);
    if let Some(m) = target {
        let (pos, size) = (m.position(), m.size());
        if full {
            let _ = win.set_position(tauri::PhysicalPosition::new(pos.x + 10, pos.y + 10));
            let _ = win.set_fullscreen(true);
        } else {
            let (w, h) = ((size.width as f64 * 0.8) as u32, (size.height as f64 * 0.8) as u32);
            let _ = win.set_size(tauri::PhysicalSize::new(w, h));
            let _ = win.set_position(tauri::PhysicalPosition::new(
                pos.x + (size.width - w) as i32 / 2,
                pos.y + (size.height - h) as i32 / 2,
            ));
        }
        logf!(
            "window on {} {}x{} at {},{} full={}",
            m.name().cloned().unwrap_or_default(),
            size.width,
            size.height,
            pos.x,
            pos.y,
            full
        );
    }
}

#[tauri::command]
fn set_topmost(window: tauri::WebviewWindow, on: bool) {
    let _ = window.set_always_on_top(on);
}

#[tauri::command]
fn toggle_fullscreen(window: tauri::WebviewWindow) -> bool {
    let on = !window.is_fullscreen().unwrap_or(false);
    let _ = window.set_fullscreen(on);
    on
}

#[tauri::command]
fn reveal(path: String) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path)).spawn();
}

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    logf!("quit");
    app.exit(0);
}

#[tauri::command]
fn log_line(msg: String) {
    logf!("ui: {}", msg);
}

// ----------------------------------------------------------------------------------------------- the loupe:// file protocol

/// Decodes encodeURIComponent output (%XX escapes; '+' stays '+').
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime_for(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "application/octet-stream",
    }
}

fn respond(status: u16, mime: &str, body: Vec<u8>, extra: &[(&str, String)]) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-cache");
    for (k, v) in extra {
        b = b.header(*k, v.as_str());
    }
    b.body(body).unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

fn serve(roots: &[PathBuf], uri: &str, range: Option<String>) -> tauri::http::Response<Vec<u8>> {
    let q = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let raw = q.split('&').find_map(|kv| kv.strip_prefix("p=")).unwrap_or("");
    let p = PathBuf::from(percent_decode(raw));
    // only media files inside a watched root are served (component-wise, case-insensitive)
    let lower = PathBuf::from(p.to_string_lossy().to_lowercase());
    let inside = roots.iter().any(|r| lower.starts_with(PathBuf::from(r.to_string_lossy().to_lowercase())));
    if !inside || index::kind_of(&p).is_none() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return respond(403, "text/plain", Vec::new(), &[]);
    }
    let mut f = match fs::File::open(&p) {
        Ok(f) => f,
        Err(_) => return respond(404, "text/plain", Vec::new(), &[]),
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = mime_for(&p);
    if let Some(r) = range.and_then(|r| r.strip_prefix("bytes=").map(|s| s.to_string())) {
        let (a, b) = r.split_once('-').unwrap_or((r.as_str(), ""));
        let start: u64 = a.trim().parse().unwrap_or(0);
        let end: u64 = b.trim().parse().unwrap_or(len.saturating_sub(1)).min(len.saturating_sub(1));
        let end = end.min(start + 8 * 1024 * 1024 - 1); // serve big videos in 8 MB slices
        if start >= len || end < start {
            return respond(416, mime, Vec::new(), &[("Content-Range", format!("bytes */{}", len))]);
        }
        let mut buf = vec![0u8; (end - start + 1) as usize];
        if f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut buf).is_err() {
            return respond(500, "text/plain", Vec::new(), &[]);
        }
        return respond(
            206,
            mime,
            buf,
            &[("Content-Range", format!("bytes {}-{}/{}", start, end, len)), ("Accept-Ranges", "bytes".into())],
        );
    }
    let mut buf = Vec::with_capacity(len as usize);
    if f.read_to_end(&mut buf).is_err() {
        return respond(500, "text/plain", Vec::new(), &[]);
    }
    respond(200, mime, buf, &[("Accept-Ranges", "bytes".into())])
}

// ----------------------------------------------------------------------------------------------- main

fn single_instance() -> Option<fs::File> {
    let d = logx::dir();
    let _ = fs::create_dir_all(&d);
    let mut o = fs::OpenOptions::new();
    o.write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(0);
    }
    o.open(d.join("blinkloupe.lock")).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let cli_roots = arg("--root").map(|r| vec![r]);
    let cli_windowed = args.iter().any(|a| a == "--windowed");
    // --blink: Blink on its own (the Start menu's Blink entry)
    let blink_only = args.iter().any(|a| a == "--blink");
    let _lock = match single_instance() {
        Some(f) => f,
        None => {
            // already running: ask that copy to bring up the window this launch was for
            let _ = fs::write(request_path(), if blink_only { "blink" } else { "main" });
            logf!("already running; asked it for its {} window", if blink_only { "Blink" } else { "main" });
            return;
        }
    };
    let _ = fs::remove_file(request_path());
    let s = settings::load();
    http::set_token(&s.comfy_token);
    logf!("blinkloupe {} start{}", env!("CARGO_PKG_VERSION"), cli_roots.as_ref().map(|r| format!(" --root {}", r[0])).unwrap_or_default());
    let roots: Vec<PathBuf> = cli_roots.clone().unwrap_or_else(|| s.roots.clone()).into_iter().map(PathBuf::from).collect();
    let state = AppState {
        index: Mutex::new(index::Index::new(roots, s.depth).skipping(&[PathBuf::from(&s.picks_copy_dir)])),
        settings: Mutex::new(s),
        comfy: Mutex::new(comfy::Comfy::new()),
        queued: Mutex::new(HashMap::new()),
        mine: Mutex::new(HashSet::new()),
        loras: std::sync::Arc::new(Mutex::new(library::Loras::default())),
        cli_roots,
        cli_windowed,
        feed: Mutex::new(feed::Status::default()),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            start,
            poll,
            step,
            edge,
            entry,
            meta,
            mark,
            reroll,
            unreroll,
            queue_marked,
            hires,
            hires_picks,
            run_action,
            send_to_workflow,
            workflow_summary,
            pick_workflow,
            update_info,
            blink,
            check_updates_now,
            install_update,
            open_url,
            open_log_folder,
            library::marked_list,
            library::lora_index,
            library::lora_images,
            upscale_models,
            comfy_status,
            get_settings,
            save_settings,
            detect_folders,
            pick_folder,
            monitors,
            set_topmost,
            toggle_fullscreen,
            reveal,
            quit,
            log_line
        ])
        .register_asynchronous_uri_scheme_protocol("blink", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().to_string();
            let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
            std::thread::spawn(move || responder.respond(serve_blink(&app, &uri, range)));
        })
        .register_asynchronous_uri_scheme_protocol("loupe", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().to_string();
            let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
            std::thread::spawn(move || {
                let st = app.state::<AppState>();
                let roots = st.index.lock().unwrap().roots.clone();
                responder.respond(serve(&roots, &uri, range));
            });
        })
        .setup(move |app| {
            let st = app.state::<AppState>();
            if blink_only {
                open_blink(app.handle(), None)?;
            } else {
                open_main(app.handle())?;
            }
            // a later launch writes what it wanted beside the lock; this copy opens that window
            let h = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(400));
                if let Ok(what) = fs::read_to_string(request_path()) {
                    let _ = fs::remove_file(request_path());
                    let r = if what.trim() == "blink" { open_blink(&h, None) } else { open_main(&h) };
                    if let Err(e) = r {
                        logf!("could not open the {} window: {}", what.trim(), e);
                    }
                }
            });
            feed::spawn(app.handle().clone());
            if st.cli_roots.is_none() {
                // the daily update check (never in a --root test session)
                let h = app.handle().clone();
                std::thread::spawn(move || loop {
                    let s = h.state::<AppState>().settings.lock().unwrap().clone();
                    if s.check_updates && update::due(&update::load_state()) {
                        update::check(&feed_url(&s));
                    }
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running BlinkLoupe");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        assert_eq!(percent_decode("D%3A%5Cout%5Ca%20b.png"), "D:\\out\\a b.png");
        assert_eq!(percent_decode("caf%C3%A9.png"), "caf\u{e9}.png");
    }

    #[test]
    fn serve_guards() {
        let d = std::env::temp_dir().join(format!("loupe-serve-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let p = d.join("x.png");
        fs::write(&p, b"0123456789").unwrap();
        let enc = |p: &Path| p.to_string_lossy().replace('\\', "%5C").replace(':', "%3A");
        let ok = serve(&[d.clone()], &format!("http://loupe.localhost/f?p={}", enc(&p)), None);
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.body().len(), 10);
        let part = serve(&[d.clone()], &format!("http://loupe.localhost/f?p={}", enc(&p)), Some("bytes=2-4".into()));
        assert_eq!((part.status().as_u16(), part.body().as_slice()), (206, b"234".as_slice()));
        let outside = serve(&[d.join("sub")], &format!("http://loupe.localhost/f?p={}", enc(&p)), None);
        assert_eq!(outside.status(), 403);
        let txt = d.join("notes.txt");
        fs::write(&txt, b"x").unwrap();
        assert_eq!(serve(&[d.clone()], &format!("http://loupe.localhost/f?p={}", enc(&txt)), None).status(), 403);
    }
}
