//! ComfyUI: detection, re-roll graph surgery (new sampler seeds, a filename suffix), queueing and cancelling, and finding
//! output folders to watch.

use crate::http;
use crate::meta::link_id;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const PROBE: Duration = Duration::from_millis(700);
const CALL: Duration = Duration::from_secs(8);

pub fn random_seed() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish() & ((1u64 << 50) - 1) // safe for any frontend that holds seeds in a JS number
}

fn is_sampler_class(c: &str) -> bool {
    let l = c.to_ascii_lowercase();
    l.contains("sampler") && !l.contains("select")
}

/// The literal (node, input) slots a re-roll changes: every sampler's seed / noise_seed, or the seed node it links to.
/// Other seeds (wildcard processors, detailers, upscalers) stay, so the prompt and the finish stay the same.
fn seed_slots(g: &Map<String, Value>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for (id, n) in g {
        let n = match n.as_object() {
            Some(n) => n,
            None => continue,
        };
        let class = n.get("class_type").and_then(|v| v.as_str()).unwrap_or("");
        let ins = match n.get("inputs").and_then(|v| v.as_object()) {
            Some(i) => i,
            None => continue,
        };
        for key in ["noise_seed", "seed"] {
            if key == "seed" && !is_sampler_class(class) {
                continue;
            }
            match ins.get(key) {
                Some(Value::Number(_)) => {
                    if seen.insert((id.clone(), key.to_string())) {
                        out.push((id.clone(), key.to_string()));
                    }
                }
                Some(v) => {
                    if let Some(src) = link_id(v) {
                        if let Some(si) = g.get(&src).and_then(|s| s.get("inputs")).and_then(|v| v.as_object()) {
                            if let Some(k) = ["seed", "noise_seed", "value", "int", "Value", "number"]
                                .iter()
                                .find(|k| matches!(si.get(**k), Some(Value::Number(_))))
                            {
                                if seen.insert((src.clone(), k.to_string())) {
                                    out.push((src, k.to_string()));
                                }
                            }
                        }
                    }
                }
                None => {}
            }
        }
    }
    out
}

pub fn count_seeds(g: &Map<String, Value>) -> usize {
    seed_slots(g).len()
}

/// New seeds on every sampler slot and `suffix` on every save node's filename_prefix. Returns the seeds changed.
pub fn reroll_graph(g: &mut Map<String, Value>, suffix: &str) -> usize {
    let slots = seed_slots(g);
    for (id, key) in &slots {
        if let Some(ins) = g.get_mut(id).and_then(|n| n.get_mut("inputs")).and_then(|v| v.as_object_mut()) {
            ins.insert(key.clone(), json!(random_seed()));
        }
    }
    if !suffix.is_empty() {
        for n in g.values_mut() {
            let c = n.get("class_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(ins) = n.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                let Some(key) = crate::meta::save_name_key(&c, ins) else { continue };
                if let Some(Value::String(p)) = ins.get(key) {
                    if !p.ends_with(suffix) {
                        let np = format!("{}{}", p, suffix);
                        ins.insert(key.into(), Value::String(np));
                    }
                }
            }
        }
    }
    slots.len()
}

pub struct Comfy {
    pub url: Option<String>,
    checked: Option<Instant>,
    for_setting: String,
}

impl Comfy {
    pub fn new() -> Self {
        Comfy { url: None, checked: None, for_setting: String::new() }
    }

    /// The live ComfyUI URL, re-probed at most every 10 s (or at once when the setting changed or `force`).
    pub fn current(&mut self, setting: &str, force: bool) -> Option<String> {
        let fresh = self.checked.map(|t| t.elapsed() < Duration::from_secs(10)).unwrap_or(false);
        if fresh && !force && self.for_setting == setting {
            return self.url.clone();
        }
        let cands: Vec<String> = if setting.is_empty() {
            vec!["http://127.0.0.1:8188".into(), "http://127.0.0.1:8000".into()]
        } else {
            vec![setting.to_string()]
        };
        // keep the last good one first
        let mut order = Vec::new();
        if let Some(u) = &self.url {
            if cands.contains(u) {
                order.push(u.clone());
            }
        }
        order.extend(cands.into_iter().filter(|c| Some(c) != self.url.as_ref()));
        self.url = order.into_iter().find(|u| probe(u));
        self.checked = Some(Instant::now());
        self.for_setting = setting.to_string();
        self.url.clone()
    }
}

pub fn probe(url: &str) -> bool {
    match http::request(url, "GET", "/system_stats", None, PROBE) {
        Ok(r) => r.status == 200 && r.text().contains("\"system\""),
        Err(_) => false,
    }
}

/// Why the server at `url` doesn't answer as ComfyUI, in a sentence for Settings (refused token, certificate, nothing there).
pub fn why_not(url: &str) -> String {
    match http::request(url, "GET", "/system_stats", None, Duration::from_secs(5)) {
        Err(e) => e,
        Ok(r) if r.status == 401 || r.status == 403 => {
            format!("the server refused BlinkLoupe ({}) - check the access token, or user:password in the address", r.status)
        }
        Ok(r) if r.status != 200 => format!("{} answered {} for /system_stats - is ComfyUI at that address?", http::redact(url), r.status),
        Ok(r) if !r.text().contains("\"system\"") => format!("{} answers, but not as ComfyUI", http::redact(url)),
        Ok(_) => String::new(),
    }
}

pub fn system_stats(url: &str) -> Option<Value> {
    let r = http::request(url, "GET", "/system_stats", None, PROBE).ok()?;
    serde_json::from_slice(&r.body).ok()
}

/// POST /prompt. Ok(prompt_id) or the server's reason.
pub fn queue(url: &str, graph: &Map<String, Value>, workflow: Option<Value>) -> Result<String, String> {
    let mut body = json!({ "prompt": graph, "client_id": "loupe" });
    if let Some(wf) = workflow {
        body["extra_data"] = json!({ "extra_pnginfo": { "workflow": wf } });
    }
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    let r = http::request(url, "POST", "/prompt", Some(&bytes), CALL)?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
    if r.status == 200 {
        if let Some(id) = v.get("prompt_id").and_then(|x| x.as_str()) {
            return Ok(id.to_string());
        }
    }
    Err(refusal(&v, r.status))
}

fn refusal(v: &Value, status: u16) -> String {
    let mut msg = v
        .get("error")
        .and_then(|e| e.get("message").and_then(|m| m.as_str()).or_else(|| e.as_str()))
        .unwrap_or("")
        .to_string();
    if let Some(Value::Object(ne)) = v.get("node_errors") {
        if let Some((_, e)) = ne.iter().next() {
            let class = e.get("class_type").and_then(|c| c.as_str()).unwrap_or("");
            let first = e.get("errors").and_then(|x| x.as_array()).and_then(|a| a.first());
            let m = first.and_then(|f| f.get("message")).and_then(|m| m.as_str()).unwrap_or("");
            let d = first.and_then(|f| f.get("details")).and_then(|m| m.as_str()).unwrap_or("");
            msg = format!("{} - {}: {} {}", msg, class, m, d).trim().to_string();
        }
    }
    if msg.is_empty() {
        format!("ComfyUI answered {}", status)
    } else {
        msg
    }
}

#[derive(Serialize, Default, Clone)]
pub struct QueueState {
    pub running: Vec<String>,
    pub pending: Vec<String>,
}

pub fn queue_state(url: &str) -> Result<QueueState, String> {
    let r = http::request(url, "GET", "/queue", None, PROBE)?;
    let v: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
    let ids = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|it| it.get(1).and_then(|x| x.as_str()).map(|s| s.to_string())).collect())
            .unwrap_or_default()
    };
    Ok(QueueState { running: ids("queue_running"), pending: ids("queue_pending") })
}

/// Removes a prompt that has not started. Ok(false) when it is already running or done.
pub fn cancel(url: &str, id: &str) -> Result<bool, String> {
    let st = queue_state(url)?;
    if !st.pending.iter().any(|p| p == id) {
        return Ok(false);
    }
    let body = serde_json::to_vec(&json!({ "delete": [id] })).unwrap();
    let r = http::request(url, "POST", "/queue", Some(&body), CALL)?;
    Ok(r.status == 200)
}

/// A node's object_info entry ({"input": {...}, ...}), None when the server does not know the node.
pub fn object_info(url: &str, class: &str) -> Option<Value> {
    let path = format!("/object_info/{}", class.replace(' ', "%20"));
    let r = http::request(url, "GET", &path, None, CALL).ok()?;
    let v: Value = serde_json::from_slice(&r.body).ok()?;
    v.get(class).cloned()
}

/// The upscale models the server can load (UpscaleModelLoader's list).
pub fn upscale_models(url: &str) -> Vec<String> {
    object_info(url, "UpscaleModelLoader")
        .and_then(|s| s.get("input").and_then(|i| i.get("required")).and_then(|r| r.get("model_name")).cloned())
        .and_then(|spec| crate::hires::combo_options(&spec))
        .unwrap_or_default()
}

/// Uploads a frame into ComfyUI's input folder (subfolder "loupe") and returns the name LoadImage takes.
pub fn upload_image(url: &str, path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png").to_ascii_lowercase();
    // the same file name can exist in many folders: tag the upload with a hash of the full path
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().to_lowercase().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    let name = format!("{}-{:08x}.{}", stem, (h & 0xffff_ffff), ext);
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => "image/png",
    };
    let (ct, body) = http::multipart(&[("type", "input"), ("subfolder", "loupe"), ("overwrite", "true")], "image", &name, mime, &bytes);
    let r = http::request_typed(url, "POST", "/upload/image", Some(&body), &ct, Duration::from_secs(60))?;
    let v: Value = serde_json::from_slice(&r.body).map_err(|_| format!("upload failed: ComfyUI answered {}", r.status))?;
    let n = v.get("name").and_then(|x| x.as_str()).ok_or_else(|| format!("upload failed: {}", r.text()))?;
    let sub = v.get("subfolder").and_then(|x| x.as_str()).unwrap_or("");
    Ok(if sub.is_empty() { n.to_string() } else { format!("{}/{}", sub, n) })
}

// ----------------------------------------------------------------------------------------------- output folders

#[derive(Serialize, Clone, Debug)]
pub struct Candidate {
    pub path: String,
    pub source: String,
}

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from)
}

/// A running server's output folder from its /system_stats argv, and how it was found: --output-directory, else
/// --base-directory\output, else the folder of main.py\output. None when the argv says nothing usable.
pub fn output_dir_with_source(v: &Value) -> Option<(PathBuf, &'static str)> {
    let argv: Vec<String> = v
        .get("system")
        .and_then(|s| s.get("argv"))
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let arg = |name: &str| argv.iter().position(|a| a == name).and_then(|i| argv.get(i + 1)).cloned();
    if let Some(o) = arg("--output-directory") {
        return Some((PathBuf::from(o), "the running ComfyUI (--output-directory)"));
    }
    if let Some(b) = arg("--base-directory") {
        return Some((PathBuf::from(b).join("output"), "the running ComfyUI (--base-directory)"));
    }
    let mp = PathBuf::from(argv.first()?);
    if mp.is_absolute() {
        return mp.parent().map(|d| (d.join("output"), "the running ComfyUI"));
    }
    None
}

pub fn output_dir_from_stats(v: &Value) -> Option<PathBuf> {
    output_dir_with_source(v).map(|(p, _)| p)
}

/// Output folders worth offering on first run, most specific first; only folders that exist.
pub fn candidate_folders(live_url: Option<&str>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut add = |p: PathBuf, src: &str| {
        if p.is_dir() {
            let s = p.to_string_lossy().to_string();
            if !out.iter().any(|c| c.path.eq_ignore_ascii_case(&s)) {
                out.push(Candidate { path: s, source: src.to_string() });
            }
        }
    };
    if let Some((p, src)) = live_url.and_then(system_stats).and_then(|v| output_dir_with_source(&v)) {
        add(p, src);
    }
    if let Some(app) = env_path("APPDATA") {
        if let Ok(t) = std::fs::read_to_string(app.join("ComfyUI").join("config.json")) {
            if let Some(b) = serde_json::from_str::<Value>(&t).ok().and_then(|v| v.get("basePath").and_then(|x| x.as_str()).map(|s| s.to_string())) {
                add(PathBuf::from(b).join("output"), "ComfyUI Desktop");
            }
        }
        if let Ok(t) = std::fs::read_to_string(app.join("Comfy Desktop").join("installations.json")) {
            if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&t) {
                for inst in a {
                    for k in ["adoptedBaseDir", "basePath", "installPath"] {
                        if let Some(b) = inst.get(k).and_then(|x| x.as_str()) {
                            add(PathBuf::from(b).join("output"), "Comfy Desktop installation");
                        }
                    }
                }
            }
        }
    }
    let home = env_path("USERPROFILE").unwrap_or_default();
    for (p, src) in [
        (PathBuf::from(r"C:\ComfyUI_windows_portable\ComfyUI\output"), "ComfyUI portable"),
        (PathBuf::from(r"D:\ComfyUI_windows_portable\ComfyUI\output"), "ComfyUI portable"),
        (home.join(r"ComfyUI_windows_portable\ComfyUI\output"), "ComfyUI portable"),
        (home.join(r"Desktop\ComfyUI_windows_portable\ComfyUI\output"), "ComfyUI portable"),
        (home.join(r"ComfyUI\output"), "ComfyUI"),
        (home.join(r"Documents\ComfyUI\output"), "ComfyUI"),
        (PathBuf::from(r"C:\ComfyUI\output"), "ComfyUI"),
        (PathBuf::from(r"D:\ComfyUI\output"), "ComfyUI"),
        (home.join(r"stable-diffusion-webui\outputs"), "A1111"),
        (home.join(r"stable-diffusion-webui-forge\outputs"), "Forge"),
        (home.join(r"SwarmUI\Output"), "SwarmUI"),
    ] {
        add(p, src);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reroll_changes_only_sampler_seeds() {
        let mut g: Map<String, Value> = serde_json::from_str(
            r#"{
              "3": {"class_type": "KSampler", "inputs": {"seed": 5, "steps": 8, "model": ["4", 0], "positive": ["6", 0], "latent_image": ["5", 0]}},
              "12": {"class_type": "KSamplerAdvanced", "inputs": {"noise_seed": ["20", 0], "model": ["4", 0], "latent_image": ["3", 0]}},
              "20": {"class_type": "Seed (rgthree)", "inputs": {"seed": 99}},
              "21": {"class_type": "ImpactWildcardProcessor", "inputs": {"seed": 7, "wildcard_text": "x"}},
              "22": {"class_type": "FaceDetailer", "inputs": {"seed": 8, "model": ["4", 0]}},
              "23": {"class_type": "RandomNoise", "inputs": {"noise_seed": 9}},
              "9": {"class_type": "SaveImage", "inputs": {"filename_prefix": "set/fox", "images": ["8", 0]}},
              "10": {"class_type": "PreviewImage", "inputs": {"images": ["8", 0]}}
            }"#,
        )
        .unwrap();
        assert_eq!(count_seeds(&g), 3);
        let n = reroll_graph(&mut g, "_reroll");
        assert_eq!(n, 3);
        let seed = |id: &str, k: &str| g[id]["inputs"][k].clone();
        assert_ne!(seed("3", "seed"), json!(5));
        assert_ne!(seed("20", "seed"), json!(99));
        assert_ne!(seed("23", "noise_seed"), json!(9));
        assert_eq!(seed("12", "noise_seed"), json!(["20", 0]));
        assert_eq!(seed("21", "seed"), json!(7));
        assert_eq!(seed("22", "seed"), json!(8));
        assert_eq!(g["9"]["inputs"]["filename_prefix"], json!("set/fox_reroll"));
        reroll_graph(&mut g, "_reroll");
        assert_eq!(g["9"]["inputs"]["filename_prefix"], json!("set/fox_reroll"), "suffix is added once");
    }

    #[test]
    fn output_folder_from_the_server_argv() {
        let at = |argv: Value| output_dir_from_stats(&json!({"system": {"argv": argv}}));
        assert_eq!(at(json!(["ComfyUI\\main.py", "--base-directory", "D:\\C", "--output-directory", "D:\\C\\out"])), Some(PathBuf::from("D:\\C\\out")));
        assert_eq!(at(json!(["main.py", "--base-directory", "D:\\C"])), Some(PathBuf::from("D:\\C").join("output")));
        assert_eq!(at(json!(["C:\\ComfyUI\\main.py", "--port", "8188"])), Some(PathBuf::from("C:\\ComfyUI\\output")));
        assert_eq!(at(json!(["main.py"])), None, "a relative main.py says nothing about where it is");
    }

    #[test]
    fn refusal_message() {
        let v: Value = serde_json::from_str(
            r#"{"error": {"type": "prompt_outputs_failed_validation", "message": "Prompt outputs failed validation"},
                "node_errors": {"4": {"errors": [{"message": "Value not in list", "details": "ckpt_name: 'x' not in []"}], "class_type": "CheckpointLoaderSimple"}}}"#,
        )
        .unwrap();
        let m = refusal(&v, 400);
        assert!(m.contains("CheckpointLoaderSimple") && m.contains("Value not in list"), "{}", m);
    }
}
