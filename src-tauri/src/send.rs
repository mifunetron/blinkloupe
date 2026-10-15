//! Send the frame on screen into the user's own workflow (2026-10-14, phase 3 of making Loupe universal). A workflow is an
//! API-format graph: ComfyUI's Workflow > Export (API) file, or any image that workflow made (its `prompt` chunk). Loupe
//! fills its Load Image node with the frame, optionally gives its samplers new seeds, fills `{positive}`-style
//! placeholders in its text, optionally points its save nodes beside the frame, and queues it. Nothing of the frame's own
//! graph is used, so any workflow with a Load Image node works: an upscaler, a face fix, img2img, a video.

use crate::meta::{self, class, inputs, Meta, G};
use serde_json::{Map, Value};
use std::path::Path;

pub struct Loaded {
    pub graph: G,
    /// the UI workflow, when the source carried one (an image's `workflow` chunk): sent as extra_pnginfo so outputs keep it
    pub workflow: Option<Value>,
}

/// Reads a workflow file: an API-format JSON (bare, or a /prompt body {"prompt": {...}}) or an image with a `prompt` chunk.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ["png", "webp", "jpg", "jpeg"].contains(&ext.as_str()) {
        if !path.is_file() {
            return Err(format!("{} is not there", path.display()));
        }
        let chunks = meta::read_chunks(path);
        let p = meta::chunk(&chunks, "prompt").ok_or("that image carries no ComfyUI graph - choose one ComfyUI saved, or an Export (API) file")?;
        let graph = parse_api(p)?;
        let workflow = meta::chunk(&chunks, "workflow").and_then(|w| serde_json::from_str(w).ok());
        return Ok(Loaded { graph, workflow });
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("not JSON: {}", e))?;
    if v.get("nodes").map(|n| n.is_array()).unwrap_or(false) {
        return Err("this is a UI workflow - in ComfyUI use Workflow > Export (API) and choose that file".into());
    }
    if let Some(p) = v.get("prompt").filter(|p| p.is_object()) {
        let workflow = v.pointer("/extra_data/extra_pnginfo/workflow").cloned();
        return Ok(Loaded { graph: api_graph(p.clone())?, workflow });
    }
    Ok(Loaded { graph: api_graph(v)?, workflow: None })
}

fn parse_api(s: &str) -> Result<G, String> {
    api_graph(serde_json::from_str(s).map_err(|_| "the graph does not parse".to_string())?)
}

fn api_graph(v: Value) -> Result<G, String> {
    let Value::Object(g) = v else { return Err("not a ComfyUI API workflow".into()) };
    if g.is_empty() || !g.values().all(|n| n.get("class_type").map(|c| c.is_string()).unwrap_or(false)) {
        return Err("not a ComfyUI API workflow (every node needs a class_type) - use Workflow > Export (API)".into());
    }
    Ok(g)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Media {
    Image,
    Video,
}

/// How a loader node takes the frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Takes {
    /// a name in ComfyUI's input folder: the frame is uploaded first
    Upload,
    /// a path on the server's own disk: only for a server on this computer
    Path,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: String,
    pub input: String,
    pub class: String,
    pub title: String,
    pub media: Media,
    pub takes: Takes,
    pub mask: bool,
}

fn title(n: &Map<String, Value>) -> String {
    n.get("_meta").and_then(|m| m.get("title")).and_then(|t| t.as_str()).unwrap_or("").to_string()
}

/// The loader a node is, if Loupe knows how to fill it: (input, media, takes, is a mask loader).
fn loader(class: &str, ins: &Map<String, Value>) -> Option<(&'static str, Media, Takes, bool)> {
    let has = |k: &str| ins.get(k).map(|v| v.is_string()).unwrap_or(false);
    match class {
        "LoadVideo" if has("file") => Some(("file", Media::Video, Takes::Upload, false)),
        "VHS_LoadVideo" | "VHS_LoadVideoFFmpeg" if has("video") => Some(("video", Media::Video, Takes::Upload, false)),
        "VHS_LoadVideoPath" | "VHS_LoadVideoFFmpegPath" if has("video") => Some(("video", Media::Video, Takes::Path, false)),
        "VHS_LoadImagePath" if has("image") => Some(("image", Media::Image, Takes::Path, false)),
        "Image Load" if has("image_path") => Some(("image_path", Media::Image, Takes::Path, false)),
        "Image Load" if has("image") => Some(("image", Media::Image, Takes::Upload, false)), // WAS v3: an upload combo
        "LoadImageOutput" => None, // reads the output folder through its own picker; an upload is not in its list
        c if c.contains("LoadImage") && has("image") => {
            let path = c.contains("Path") || c.contains("FromPath");
            Some(("image", Media::Image, if path { Takes::Path } else { Takes::Upload }, c.contains("Mask")))
        }
        _ => None,
    }
}

/// Every node Loupe could fill, lowest id first.
pub fn loaders(g: &G) -> Vec<Target> {
    let mut out: Vec<Target> = g
        .iter()
        .filter_map(|(id, n)| {
            let n = n.as_object()?;
            let c = class(n);
            let (input, media, takes, mask) = loader(c, inputs(n)?)?;
            Some(Target { id: id.clone(), input: input.into(), class: c.into(), title: title(n), media, takes, mask })
        })
        .collect();
    out.sort_by_key(|t| (t.id.parse::<i64>().unwrap_or(i64::MAX), t.id.clone()));
    out
}

fn describe(t: &Target) -> String {
    if t.title.is_empty() || t.title == t.class { format!("#{} {}", t.id, t.class) } else { format!("#{} \"{}\"", t.id, t.title) }
}

/// The loaders that get the frame: the ones titled "Loupe" (any case, anywhere in the title), else the one image (or
/// video) loader that is not a mask loader. More than one untitled candidate is an error naming them.
pub fn targets(g: &G, media: Media) -> Result<Vec<Target>, String> {
    let all = loaders(g);
    let titled: Vec<Target> = all.iter().filter(|t| t.title.to_lowercase().contains("loupe")).cloned().collect();
    let what = if media == Media::Video { "video" } else { "image" };
    if !titled.is_empty() {
        if let Some(t) = titled.iter().find(|t| t.media != media) {
            return Err(format!("the frame is {} {}, but {} loads {}s", if media == Media::Video { "a" } else { "an" }, what, describe(t), if t.media == Media::Video { "video" } else { "image" }));
        }
        return Ok(titled);
    }
    let cands: Vec<Target> = all.iter().filter(|t| t.media == media && !t.mask).cloned().collect();
    match cands.len() {
        1 => Ok(cands),
        0 if all.iter().any(|t| t.media != media && !t.mask) => {
            Err(format!("the frame is {} {}, and this workflow loads only {}", if media == Media::Video { "a" } else { "an" }, what, if media == Media::Video { "images" } else { "videos" }))
        }
        0 => Err(format!("this workflow has no Load {} node for the frame", if media == Media::Video { "Video" } else { "Image" })),
        n => Err(format!(
            "this workflow has {} {} loaders ({}) - in ComfyUI, title the one BlinkLoupe fills \"Loupe\" and export it again",
            n,
            what,
            cands.iter().map(describe).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The values placeholders take, from the frame.
pub struct Fill {
    pub positive: String,
    pub negative: String,
    pub name: String,
    pub stem: String,
    /// the frame's folder name
    pub folder: String,
}

impl Fill {
    pub fn of(path: &Path, m: &Meta) -> Fill {
        let s = |o: Option<&std::ffi::OsStr>| o.and_then(|x| x.to_str()).unwrap_or("").to_string();
        Fill {
            positive: m.positive.clone(),
            negative: if m.negative == "(zeroed out)" { String::new() } else { m.negative.clone() },
            name: s(path.file_name()),
            stem: s(path.file_stem()),
            folder: s(path.parent().and_then(|p| p.file_name())),
        }
    }
}

pub const PLACEHOLDERS: &[&str] = &["{positive}", "{negative}", "{name}", "{stem}", "{folder}"];

/// Fills `{positive}` `{negative}` `{name}` `{stem}` `{folder}` in every text input. Returns how many inputs changed.
pub fn fill_placeholders(g: &mut G, f: &Fill) -> usize {
    let mut n = 0;
    for node in g.values_mut() {
        let Some(ins) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else { continue };
        for v in ins.values_mut() {
            let Value::String(s) = v else { continue };
            if !PLACEHOLDERS.iter().any(|p| s.contains(p)) {
                continue;
            }
            *s = s
                .replace("{positive}", &f.positive)
                .replace("{negative}", &f.negative)
                .replace("{name}", &f.name)
                .replace("{stem}", &f.stem)
                .replace("{folder}", &f.folder);
            n += 1;
        }
    }
    n
}

/// "Upscale 4x" -> "upscale-4x": the label as a file-name suffix.
pub fn slug(label: &str) -> String {
    let mut out = String::new();
    for c in label.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "sent".into() } else { out }
}

/// Points every save node at `<subfolder>/<stem>_<slug>`, so the result lands beside the frame. Returns the nodes changed.
pub fn save_beside(g: &mut G, subfolder: &str, stem: &str, slug: &str) -> usize {
    let name = format!("{}_{}", stem, slug);
    let sub = subfolder.trim_matches(['/', '\\']).replace('\\', "/");
    let mut n = 0;
    for node in g.values_mut() {
        let Some(c) = node.get("class_type").and_then(|v| v.as_str()).map(|s| s.to_string()) else { continue };
        let Some(ins) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) else { continue };
        let Some(key) = meta::save_name_key(&c, ins) else { continue };
        if key == "filename" {
            // Image Saver: a folder input and a file name pattern
            ins.insert("filename".into(), Value::String(name.clone()));
            if ins.get("path").map(|v| v.is_string()).unwrap_or(false) {
                ins.insert("path".into(), Value::String(sub.clone()));
            }
        } else {
            let p = if sub.is_empty() { name.clone() } else { format!("{}/{}", sub, name) };
            ins.insert(key.into(), Value::String(p));
        }
        n += 1;
    }
    n
}

pub struct Options<'a> {
    pub new_seed: bool,
    /// Some(subfolder) points the save nodes beside the frame ("" = the output folder itself)
    pub beside: Option<&'a str>,
    pub slug: &'a str,
}

pub struct Prepared {
    pub seeds: usize,
    pub placeholders: usize,
    pub saves: usize,
}

/// Puts the frame into the targets (`value` per target: the uploaded name or the path) and applies the options.
pub fn prepare(g: &mut G, ts: &[Target], value: &dyn Fn(&Target) -> String, fill: &Fill, o: &Options) -> Prepared {
    for t in ts {
        if let Some(ins) = g.get_mut(&t.id).and_then(|n| n.get_mut("inputs")).and_then(|v| v.as_object_mut()) {
            ins.insert(t.input.clone(), Value::String(value(t)));
        }
    }
    let seeds = if o.new_seed { crate::comfy::reroll_graph(g, "") } else { 0 };
    let placeholders = fill_placeholders(g, fill);
    let saves = match o.beside {
        Some(sub) => save_beside(g, sub, &fill.stem, o.slug),
        None => 0,
    };
    Prepared { seeds, placeholders, saves }
}

/// One line for Settings about a workflow file: what Loupe would fill and change. Err = why it can't be used.
pub fn summary(path: &Path) -> Result<String, String> {
    let l = load(path)?;
    let g = &l.graph;
    let all = loaders(g);
    let pick = targets(g, Media::Image).or_else(|e| targets(g, Media::Video).map_err(|_| e))?;
    let samplers = crate::comfy::count_seeds(g);
    let saves = g.values().filter_map(|n| n.as_object()).filter(|n| inputs(n).and_then(|i| meta::save_name_key(class(n), i)).is_some()).count();
    let mut parts = vec![format!("{} nodes", g.len()), format!("the frame goes into {}", pick.iter().map(describe).collect::<Vec<_>>().join(" + "))];
    if pick.iter().any(|t| t.takes == Takes::Path) {
        parts.push("(a path: a ComfyUI on this computer only)".into());
    }
    if all.len() > pick.len() {
        parts.push(format!("{} other loader{} left as saved", all.len() - pick.len(), if all.len() - pick.len() == 1 { "" } else { "s" }));
    }
    parts.push(match samplers {
        0 => "no sampler seeds".into(),
        1 => "1 seed".into(),
        n => format!("{} seeds", n),
    });
    parts.push(match saves {
        0 => "no save node - add one, or follow the server for previews".into(),
        1 => "1 save node".into(),
        n => format!("{} save nodes", n),
    });
    let ph: Vec<&str> = PLACEHOLDERS
        .iter()
        .copied()
        .filter(|p| g.values().any(|n| n.get("inputs").and_then(|i| i.as_object()).map(|i| i.values().any(|v| v.as_str().map(|s| s.contains(p)).unwrap_or(false))).unwrap_or(false)))
        .collect();
    if !ph.is_empty() {
        parts.push(format!("fills {}", ph.join(" ")));
    }
    Ok(parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn g(v: Value) -> G {
        v.as_object().unwrap().clone()
    }

    fn upscale() -> G {
        g(json!({
            "1": {"class_type": "LoadImage", "inputs": {"image": "example.png"}, "_meta": {"title": "Load Image"}},
            "2": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": "4x.pth"}},
            "3": {"class_type": "ImageUpscaleWithModel", "inputs": {"upscale_model": ["2", 0], "image": ["1", 0]}},
            "4": {"class_type": "SaveImage", "inputs": {"filename_prefix": "ComfyUI", "images": ["3", 0]}}
        }))
    }

    #[test]
    fn the_one_load_image_gets_the_frame() {
        let t = targets(&upscale(), Media::Image).unwrap();
        assert_eq!((t.len(), t[0].id.as_str(), t[0].input.as_str(), t[0].takes), (1, "1", "image", Takes::Upload));
        let e = targets(&upscale(), Media::Video).unwrap_err();
        assert!(e.contains("loads only images"), "{}", e);
    }

    #[test]
    fn several_loaders_need_a_loupe_title() {
        let mut w = upscale();
        w.insert("5".into(), json!({"class_type": "LoadImage", "inputs": {"image": "style.png"}, "_meta": {"title": "Style ref"}}));
        w.insert("6".into(), json!({"class_type": "LoadImageMask", "inputs": {"image": "m.png", "channel": "alpha"}}));
        let e = targets(&w, Media::Image).unwrap_err();
        assert!(e.contains("2 image loaders") && e.contains("#1") && e.contains("\"Style ref\"") && !e.contains("#6"), "{}", e);
        w["1"]["_meta"]["title"] = json!("Loupe frame");
        let t = targets(&w, Media::Image).unwrap();
        assert_eq!(t.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["1"]);
        // a titled mask loader is filled too: the user said so
        w["6"]["_meta"] = json!({"title": "loupe"});
        assert_eq!(targets(&w, Media::Image).unwrap().len(), 2);
    }

    #[test]
    fn video_and_path_loaders() {
        let w = g(json!({
            "1": {"class_type": "VHS_LoadVideo", "inputs": {"video": "a.mp4", "force_rate": 0}},
            "2": {"class_type": "Image Load", "inputs": {"image_path": "C:/x.png", "RGBA": "false"}},
            "3": {"class_type": "LoadVideo", "inputs": {"file": "b.mp4"}}
        }));
        let v: Vec<_> = loaders(&w).into_iter().map(|t| (t.id, t.input, t.media, t.takes)).collect();
        assert_eq!(
            v,
            vec![
                ("1".into(), "video".into(), Media::Video, Takes::Upload),
                ("2".into(), "image_path".into(), Media::Image, Takes::Path),
                ("3".into(), "file".into(), Media::Video, Takes::Upload)
            ]
        );
        assert_eq!(targets(&w, Media::Image).unwrap()[0].id, "2");
        assert!(targets(&w, Media::Video).unwrap_err().contains("2 video loaders"));
    }

    #[test]
    fn prepare_fills_the_frame_seeds_placeholders_and_saves() {
        let mut w = upscale();
        w.insert("7".into(), json!({"class_type": "CLIPTextEncode", "inputs": {"text": "{positive}, sharp", "clip": ["8", 1]}}));
        w.insert("9".into(), json!({"class_type": "KSampler", "inputs": {"seed": 1, "model": ["8", 0], "latent_image": ["10", 0], "positive": ["7", 0]}}));
        let ts = targets(&w, Media::Image).unwrap();
        let fill = Fill { positive: "a fox".into(), negative: String::new(), name: "01_fox_00001_.png".into(), stem: "01_fox_00001_".into(), folder: "set".into() };
        let o = Options { new_seed: true, beside: Some("261014-set-fox"), slug: &slug("Upscale 4x") };
        let r = prepare(&mut w, &ts, &|_| "loupe/01_fox-1a2b3c4d.png".into(), &fill, &o);
        assert_eq!((r.seeds, r.placeholders, r.saves), (1, 1, 1));
        assert_eq!(w["1"]["inputs"]["image"], json!("loupe/01_fox-1a2b3c4d.png"));
        assert_eq!(w["7"]["inputs"]["text"], json!("a fox, sharp"));
        assert_ne!(w["9"]["inputs"]["seed"], json!(1));
        assert_eq!(w["4"]["inputs"]["filename_prefix"], json!("261014-set-fox/01_fox_00001__upscale-4x"));
        let o = Options { new_seed: false, beside: None, slug: "x" };
        let mut w2 = upscale();
        let r = prepare(&mut w2, &ts, &|_| "f.png".into(), &fill, &o);
        assert_eq!((r.seeds, r.saves), (0, 0));
        assert_eq!(w2["4"]["inputs"]["filename_prefix"], json!("ComfyUI"));
    }

    #[test]
    fn image_saver_gets_its_path_and_file_name() {
        let mut w = g(json!({"5": {"class_type": "Image Saver", "inputs": {"images": ["3", 0], "filename": "%time_%seed", "path": "%date", "extension": "png"}}}));
        assert_eq!(save_beside(&mut w, "sub\\set", "fox", "fix"), 1);
        assert_eq!((w["5"]["inputs"]["filename"].clone(), w["5"]["inputs"]["path"].clone()), (json!("fox_fix"), json!("sub/set")));
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Upscale 4x"), "upscale-4x");
        assert_eq!(slug("  Face fix!! "), "face-fix");
        assert_eq!(slug("--"), "sent");
    }

    #[test]
    fn loads_api_json_ui_json_and_body() {
        let d = std::env::temp_dir().join(format!("loupe-send-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let api = d.join("api.json");
        std::fs::write(&api, serde_json::to_string(&Value::Object(upscale())).unwrap()).unwrap();
        assert_eq!(load(&api).unwrap().graph.len(), 4);
        let ui = d.join("ui.json");
        std::fs::write(&ui, r#"{"last_node_id": 4, "nodes": [], "links": []}"#).unwrap();
        assert!(load(&ui).err().unwrap().contains("Export (API)"));
        let body = d.join("body.json");
        std::fs::write(&body, serde_json::to_string(&json!({"prompt": upscale(), "extra_data": {"extra_pnginfo": {"workflow": {"nodes": []}}}})).unwrap()).unwrap();
        let l = load(&body).unwrap();
        assert!(l.workflow.is_some() && l.graph.len() == 4);
        let s = summary(&api).unwrap();
        assert!(s.contains("#1 \"Load Image\"") && s.contains("1 save node") && s.contains("no sampler seeds"), "{}", s);
        let _ = std::fs::remove_dir_all(&d);
    }
}
