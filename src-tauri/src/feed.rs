//! Following a ComfyUI server (2026-10-14, phase 2 of making Loupe universal: follow any ComfyUI through its status
//! broadcast, /history and /view). Watching a folder only works when ComfyUI writes to this disk, and never sees Preview
//! Image outputs. ComfyUI sends `executed` only to the browser tab that queued a prompt, but it broadcasts `status` to every
//! socket whenever its queue changes (a job added or finished), and `/history?max_items=N` lists every job's outputs.
//! So: hear `status` on /ws (or poll every 2 s when the socket is refused), look at the newest history entry, and fetch
//! each new image through `/view` into `<feed dir>\<host_port>\<subfolder>\<file>` (previews under `previews\`). That folder
//! is one of the index roots, so the frames arrive LIVE like any other, and marks land on real files. Outputs of a server
//! on this machine that already sit in a watched folder are not copied.

use crate::{comfy, http, index, ws};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::Manager;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    /// following is switched on in Settings
    pub on: bool,
    /// the server being followed, once one answers
    pub url: String,
    /// the status socket is open (else history is polled every 2 s)
    pub socket: bool,
    /// images saved into the feed folder this session
    pub saved: u64,
    /// outputs already in a watched folder, so not copied
    pub local: u64,
    /// the last file saved
    pub last: String,
    pub error: String,
    pub dir: String,
}

/// One image a job produced, as /history names it.
#[derive(Debug, PartialEq, Clone)]
pub struct OutFile {
    pub filename: String,
    pub subfolder: String,
    /// "output", "temp" (Preview Image) or "input"
    pub kind: String,
}

/// The images of one /history record, in node order; only kinds of file Loupe shows.
pub fn outputs_of(rec: &Value) -> Vec<OutFile> {
    let mut out = Vec::new();
    let Some(nodes) = rec.get("outputs").and_then(|o| o.as_object()) else { return out };
    let mut ids: Vec<&String> = nodes.keys().collect();
    ids.sort_by_key(|k| (k.parse::<u64>().unwrap_or(u64::MAX), k.to_string()));
    for id in ids {
        for list in nodes[id].as_object().into_iter().flat_map(|o| o.values()) {
            for f in list.as_array().into_iter().flatten() {
                let Some(name) = f.get("filename").and_then(|x| x.as_str()) else { continue };
                if index::kind_of(Path::new(name)).is_none() {
                    continue;
                }
                out.push(OutFile {
                    filename: name.to_string(),
                    subfolder: f.get("subfolder").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    kind: f.get("type").and_then(|x| x.as_str()).unwrap_or("output").to_string(),
                });
            }
        }
    }
    out
}

/// /history?max_items=n as (prompt id, record), oldest first. The map's own order can't be trusted here (serde_json sorts
/// keys), so records go by their queue number, prompt[0].
pub fn history(url: &str, n: usize) -> Result<Vec<(String, Value)>, String> {
    let r = http::request(url, "GET", &format!("/history?max_items={}", n), None, Duration::from_secs(20))?;
    if r.status != 200 {
        return Err(format!("/history answered {}", r.status));
    }
    let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("/history: {}", e))?;
    Ok(order_history(v))
}

pub fn order_history(v: Value) -> Vec<(String, Value)> {
    let Value::Object(m) = v else { return Vec::new() };
    let mut recs: Vec<(String, Value)> = m.into_iter().collect();
    let num = |r: &Value| r.get("prompt").and_then(|p| p.get(0)).and_then(|n| n.as_f64()).unwrap_or(f64::MAX);
    recs.sort_by(|a, b| num(&a.1).partial_cmp(&num(&b.1)).unwrap_or(std::cmp::Ordering::Equal));
    recs
}

/// "127.0.0.1_8000" for http://127.0.0.1:8000 - the feed folder keeps each server apart.
pub fn host_tag(url: &str) -> String {
    match http::host_port(url) {
        Ok((h, p)) => format!("{}_{}", clean_part(&h.replace(':', "-")).unwrap_or_else(|| "server".into()), p),
        Err(_) => "server".into(),
    }
}

pub fn is_local(url: &str) -> bool {
    http::host_port(url).map(|(h, _)| h == "localhost" || h == "::1" || h.starts_with("127.")).unwrap_or(false)
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3",
    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// One path component from the server, made safe to use here: no "." or "..", no characters Windows refuses, no device name.
pub fn clean_part(s: &str) -> Option<String> {
    let t: String = s.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let t = t.trim().trim_end_matches(['.', ' ']).to_string();
    if t.is_empty() || t.chars().all(|c| c == '.') {
        return None;
    }
    let stem = t.split('.').next().unwrap_or("").to_ascii_uppercase();
    Some(if RESERVED.contains(&stem.as_str()) { format!("_{}", t) } else { t })
}

/// Where a server image is saved: <dir>\<host_tag>\[previews\]<subfolder parts>\<file>. None when the server's names try to
/// leave that folder ("..") or are empty.
pub fn mirror_path(dir: &Path, tag: &str, f: &OutFile) -> Option<PathBuf> {
    let mut p = dir.join(tag);
    if f.kind == "temp" {
        p.push("previews");
    }
    for part in f.subfolder.split(['/', '\\']).filter(|x| !x.is_empty()) {
        if part.trim() == ".." || part.trim() == "." {
            return None;
        }
        p.push(clean_part(part)?);
    }
    if f.filename.contains(['/', '\\']) || f.filename.trim() == ".." {
        return None;
    }
    p.push(clean_part(&f.filename)?);
    Some(p)
}

/// Percent-encodes a query value (RFC 3986 unreserved characters stay).
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{:02X}", b) })
        .collect()
}

fn under(p: &Path, roots: &[PathBuf]) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    roots.iter().any(|r| {
        let r = r.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
        !r.is_empty() && s.len() > r.len() && s.starts_with(&r) && matches!(s.as_bytes()[r.len()], b'\\' | b'/')
    })
}

/// What one follow session needs, fixed while it runs; any change restarts the session.
#[derive(Clone, PartialEq, Debug)]
pub struct Plan {
    pub url: String,
    pub dir: PathBuf,
    pub previews: bool,
    /// the folders the index watches (outputs found under them are not copied)
    pub roots: Vec<PathBuf>,
    /// the output folder of a server on this machine
    pub local_out: Option<PathBuf>,
}

/// Saves a job's images; returns (saved, already local).
pub fn mirror(plan: &Plan, rec: &Value, st: &mut Status) -> (u64, u64) {
    let tag = host_tag(&plan.url);
    let (mut saved, mut local) = (0, 0);
    for f in outputs_of(rec) {
        if f.kind == "input" || (f.kind == "temp" && !plan.previews) {
            continue;
        }
        if f.kind == "output" {
            if let Some(od) = &plan.local_out {
                let lp = f.subfolder.split(['/', '\\']).filter(|x| !x.is_empty()).fold(od.clone(), |p, x| p.join(x)).join(&f.filename);
                if lp.is_file() && under(&lp, &plan.roots) {
                    local += 1;
                    continue;
                }
            }
        }
        let Some(dest) = mirror_path(&plan.dir, &tag, &f) else {
            st.error = format!("skipped an unsafe name from the server: {}/{}", f.subfolder, f.filename);
            continue;
        };
        if dest.exists() {
            continue;
        }
        let q = format!("/view?filename={}&subfolder={}&type={}", enc(&f.filename), enc(&f.subfolder), enc(&f.kind));
        match http::request(&plan.url, "GET", &q, None, Duration::from_secs(60)) {
            Ok(r) if r.status == 200 && !r.body.is_empty() => {
                let part = dest.with_file_name(format!("{}.part", dest.file_name().unwrap().to_string_lossy()));
                let wrote = fs::create_dir_all(dest.parent().unwrap())
                    .and_then(|_| fs::write(&part, &r.body))
                    .and_then(|_| fs::rename(&part, &dest));
                match wrote {
                    Ok(()) => {
                        saved += 1;
                        st.last = dest.file_name().unwrap().to_string_lossy().to_string();
                    }
                    Err(e) => {
                        let _ = fs::remove_file(&part);
                        st.error = format!("{}: {}", dest.display(), e);
                    }
                }
            }
            Ok(r) => st.error = format!("/view answered {} for {}", r.status, f.filename),
            Err(e) => st.error = e,
        }
    }
    st.saved += saved;
    st.local += local;
    (saved, local)
}

/// Looks for finished jobs not seen yet and saves their images. The newest entry alone is fetched first, so a quiet server
/// costs one small request; a backlog fetches more.
pub fn sync(plan: &Plan, seen: &mut HashSet<String>, st: &mut Status) -> Result<usize, String> {
    let newest = history(&plan.url, 1)?;
    if newest.iter().all(|(id, _)| seen.contains(id)) {
        return Ok(0);
    }
    let mut recs = history(&plan.url, 16)?;
    if recs.iter().all(|(id, _)| !seen.contains(id)) {
        recs = history(&plan.url, 64)?;
    }
    let mut n = 0;
    for (id, rec) in recs {
        if seen.insert(id.clone()) {
            let (saved, local) = mirror(plan, &rec, st);
            crate::logf!("feed: job {} finished - {} saved, {} already local", id.chars().take(8).collect::<String>(), saved, local);
            n += 1;
        }
    }
    Ok(n)
}

fn plan_now(app: &tauri::AppHandle) -> Option<Plan> {
    let st = app.state::<crate::AppState>();
    let s = st.settings.lock().unwrap().clone();
    if !s.follow_server {
        return None;
    }
    let url = st.comfy.lock().unwrap().current(&s.comfy_url, false)?;
    let roots = st.index.lock().unwrap().roots.clone();
    Some(Plan { local_out: None, dir: s.feed_dir_path(), previews: s.feed_previews, roots, url })
}

fn set(app: &tauri::AppHandle, f: impl FnOnce(&mut Status)) {
    let st = app.state::<crate::AppState>();
    let mut g = st.feed.lock().unwrap();
    f(&mut g);
}

/// The follower thread: idle while following is off, else one session per server / settings, re-planned on any change.
pub fn spawn(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("feed".into())
        .spawn(move || loop {
            let on = app.state::<crate::AppState>().settings.lock().unwrap().follow_server;
            if !on {
                set(&app, |s| *s = Status::default());
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            match plan_now(&app) {
                None => {
                    let setting = app.state::<crate::AppState>().settings.lock().unwrap().comfy_url.clone();
                    let why = if setting.is_empty() {
                        "No ComfyUI answers on 127.0.0.1:8188 or :8000 - set its address in Settings".to_string()
                    } else {
                        format!("No ComfyUI at {}: {}", http::redact(&setting), comfy::why_not(&setting))
                    };
                    set(&app, |s| *s = Status { on: true, error: why, ..Default::default() });
                    std::thread::sleep(Duration::from_secs(3));
                }
                Some(mut plan) => {
                    if is_local(&plan.url) {
                        plan.local_out = comfy::system_stats(&plan.url).and_then(|v| comfy::output_dir_from_stats(&v));
                    }
                    if let Err(e) = follow(&app, &plan) {
                        crate::logf!("feed: {} ({})", e, http::redact(&plan.url));
                        set(&app, |s| {
                            s.socket = false;
                            s.error = e;
                        });
                        std::thread::sleep(Duration::from_secs(3));
                    }
                }
            }
        })
        .expect("feed thread");
}

/// One session with one server: returns Ok when the settings change, Err when the server stops answering.
fn follow(app: &tauri::AppHandle, plan: &Plan) -> Result<(), String> {
    let mut st = Status { on: true, url: http::redact(&plan.url), dir: plan.dir.to_string_lossy().to_string(), ..Default::default() };
    // what the server already finished counts as seen; its newest job is shown at once (skipped if already saved)
    let first = history(&plan.url, 64)?;
    let mut seen: HashSet<String> = first.iter().map(|(id, _)| id.clone()).collect();
    if let Some((_, rec)) = first.last() {
        mirror(plan, rec, &mut st);
    }
    let sid = format!("loupe-feed-{:08x}", comfy::random_seed() as u32);
    let open = || ws::Ws::connect(&plan.url, &format!("/ws?clientId={}", sid), Duration::from_secs(3));
    let mut sock = open().map_err(|e| crate::logf!("feed: no status socket ({}); polling", e)).ok();
    crate::logf!("feed: following {} into {} ({})", http::redact(&plan.url), plan.dir.display(), if sock.is_some() { "socket" } else { "polling" });
    let (mut last_check, mut last_try) = (Instant::now(), Instant::now());
    loop {
        st.socket = sock.is_some();
        set(app, |s| *s = st.clone());
        if plan_now(app).map(|p| p.url != plan.url || p.dir != plan.dir || p.previews != plan.previews || p.roots != plan.roots) != Some(false) {
            return Ok(());
        }
        let mut check = false;
        match sock.as_mut() {
            Some(w) => match w.next(Duration::from_secs(1)) {
                Ok(Some(ws::Msg::Text(t))) => check = t.contains("\"status\""),
                Ok(_) => {}
                Err(e) => {
                    crate::logf!("feed: socket lost ({}); polling", e);
                    sock = None;
                    last_try = Instant::now();
                }
            },
            None => {
                std::thread::sleep(Duration::from_secs(1));
                if last_try.elapsed() > Duration::from_secs(15) {
                    last_try = Instant::now();
                    sock = open().ok();
                }
            }
        }
        let every = if sock.is_some() { Duration::from_secs(15) } else { Duration::from_secs(2) };
        if check || last_check.elapsed() >= every {
            last_check = Instant::now();
            st.error.clear();
            sync(plan, &mut seen, &mut st)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn of(kind: &str, sub: &str, name: &str) -> OutFile {
        OutFile { filename: name.into(), subfolder: sub.into(), kind: kind.into() }
    }

    #[test]
    fn outputs_from_a_history_record() {
        let rec = json!({
            "prompt": [7, "id", {}, {}, ["9"]],
            "outputs": {
                "9": {"images": [{"filename": "fox_00001_.png", "subfolder": "set", "type": "output"}]},
                "12": {"images": [{"filename": "ComfyUI_temp_abcde_00001_.png", "subfolder": "", "type": "temp"}],
                       "text": ["not a file"]},
                "15": {"gifs": [{"filename": "clip_00001.mp4", "subfolder": "vid", "type": "output", "format": "video/h264-mp4"}]},
                "3": {"images": [{"filename": "load.png", "subfolder": "", "type": "input"}]},
                "20": {"audio": [{"filename": "song.flac", "subfolder": "", "type": "output"}]}
            }
        });
        assert_eq!(
            outputs_of(&rec),
            vec![
                of("input", "", "load.png"),
                of("output", "set", "fox_00001_.png"),
                of("temp", "", "ComfyUI_temp_abcde_00001_.png"),
                of("output", "vid", "clip_00001.mp4"),
            ],
            "node order; audio and text are not frames"
        );
        assert!(outputs_of(&json!({"status": {}})).is_empty());
    }

    #[test]
    fn history_goes_by_queue_number() {
        let v = json!({
            "zzz": {"prompt": [3, "zzz"]},
            "aaa": {"prompt": [12, "aaa"]},
            "mmm": {"prompt": [5, "mmm"]}
        });
        let ids: Vec<String> = order_history(v).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec!["zzz", "mmm", "aaa"]);
    }

    #[test]
    fn server_names_cannot_leave_the_feed_folder() {
        let d = Path::new(r"C:\feed");
        let t = "10.0.0.5_8188";
        assert_eq!(mirror_path(d, t, &of("output", "set/sub", "a.png")), Some(PathBuf::from(r"C:\feed\10.0.0.5_8188\set\sub\a.png")));
        assert_eq!(mirror_path(d, t, &of("temp", "", "p.png")), Some(PathBuf::from(r"C:\feed\10.0.0.5_8188\previews\p.png")));
        assert_eq!(mirror_path(d, t, &of("output", "../../Windows", "a.png")), None);
        assert_eq!(mirror_path(d, t, &of("output", r"set\..", "a.png")), None);
        assert_eq!(mirror_path(d, t, &of("output", "", "../a.png")), None);
        assert_eq!(mirror_path(d, t, &of("output", "", "..")), None);
        assert_eq!(mirror_path(d, t, &of("output", "", "")), None);
        assert_eq!(mirror_path(d, t, &of("output", r"C:\Windows", "a.png")), Some(PathBuf::from(r"C:\feed\10.0.0.5_8188\C_\Windows\a.png")));
        assert_eq!(mirror_path(d, t, &of("output", "/abs", "con.png")), Some(PathBuf::from(r"C:\feed\10.0.0.5_8188\abs\_con.png")));
        assert_eq!(mirror_path(d, t, &of("output", "", "a?b*.png")), Some(PathBuf::from(r"C:\feed\10.0.0.5_8188\a_b_.png")));
    }

    #[test]
    fn small_helpers() {
        assert_eq!(host_tag("http://127.0.0.1:8000"), "127.0.0.1_8000");
        assert_eq!(host_tag("http://[::1]:8188/"), "--1_8188");
        assert!(is_local("http://localhost:8188") && is_local("http://127.0.0.1:8000") && !is_local("http://192.168.1.9:8188"));
        assert_eq!(enc("a b/c&d=é.png"), "a%20b%2Fc%26d%3D%C3%A9.png");
        let roots = vec![PathBuf::from(r"D:\ComfyUI\output")];
        assert!(under(Path::new(r"d:\comfyui\output\set\a.png"), &roots));
        assert!(!under(Path::new(r"D:\ComfyUI\output-old\a.png"), &roots));
        assert!(!under(Path::new(r"D:\ComfyUI\output"), &roots));
    }

    /// A tiny stand-in for ComfyUI: /history and /view over plain HTTP, one request per connection.
    fn fake_server(history: Value, png: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut c = c;
                let mut b = [0u8; 4096];
                let n = c.read(&mut b).unwrap_or(0);
                let req = String::from_utf8_lossy(&b[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                let (status, body): (&str, Vec<u8>) = if path.starts_with("/history") {
                    let max: usize = path.rsplit('=').next().and_then(|x| x.parse().ok()).unwrap_or(99);
                    let recs = order_history(history.clone());
                    let keep: serde_json::Map<String, Value> = recs.into_iter().rev().take(max).collect();
                    ("200 OK", serde_json::to_vec(&Value::Object(keep)).unwrap())
                } else if path.starts_with("/view?filename=missing") {
                    ("404 Not Found", Vec::new())
                } else if path.starts_with("/view") {
                    ("200 OK", png.clone())
                } else {
                    ("404 Not Found", Vec::new())
                };
                let _ = c.write_all(format!("HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", status, body.len()).as_bytes());
                let _ = c.write_all(&body);
            }
        });
        url
    }

    #[test]
    fn sync_saves_new_jobs_once_and_skips_local_outputs() {
        let d = std::env::temp_dir().join(format!("loupe-feed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let local = d.join("out");
        fs::create_dir_all(local.join("set")).unwrap();
        fs::write(local.join("set").join("here_00001_.png"), b"on disk").unwrap();
        let hist = json!({
            "old": {"prompt": [1, "old"], "outputs": {"9": {"images": [{"filename": "old.png", "subfolder": "", "type": "output"}]}}},
            "new": {"prompt": [2, "new"], "outputs": {
                "9": {"images": [{"filename": "here_00001_.png", "subfolder": "set", "type": "output"}]},
                "10": {"images": [{"filename": "remote_00001_.png", "subfolder": "set", "type": "output"}]},
                "11": {"images": [{"filename": "ComfyUI_temp_x_00001_.png", "subfolder": "", "type": "temp"}]},
                "12": {"images": [{"filename": "missing.png", "subfolder": "", "type": "output"}]}
            }}
        });
        let url = fake_server(hist, b"\x89PNG fake".to_vec());
        let plan = Plan { url: url.clone(), dir: d.join("feed"), previews: true, roots: vec![local.clone()], local_out: Some(local.clone()) };
        let mut seen: HashSet<String> = ["old".to_string()].into();
        let mut st = Status::default();
        assert_eq!(sync(&plan, &mut seen, &mut st).unwrap(), 1, "only the new job");
        let tag = host_tag(&url);
        assert_eq!(fs::read(d.join("feed").join(&tag).join("set").join("remote_00001_.png")).unwrap(), b"\x89PNG fake");
        assert!(d.join("feed").join(&tag).join("previews").join("ComfyUI_temp_x_00001_.png").is_file());
        assert!(!d.join("feed").join(&tag).join("set").join("here_00001_.png").exists(), "already in a watched folder");
        assert!(!d.join("feed").join(&tag).join("old.png").exists(), "seen before");
        assert_eq!((st.saved, st.local), (2, 1));
        assert!(st.error.contains("404"), "{}", st.error);
        assert_eq!(sync(&plan, &mut seen, &mut st).unwrap(), 0, "nothing new the second time");
        // previews off
        let plan2 = Plan { previews: false, dir: d.join("feed2"), ..plan };
        let mut seen2 = HashSet::new();
        sync(&plan2, &mut seen2, &mut Status::default()).unwrap();
        assert!(!d.join("feed2").join(&tag).join("previews").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
