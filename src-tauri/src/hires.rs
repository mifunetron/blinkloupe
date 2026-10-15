//! Hires on picks. The frame's own model chain (every LoRA), prompts and VAE are rebuilt around an uploaded copy of the
//! frame: optional upscale model, lanczos to the target size, tiled VAE encode, one low-denoise KSampler pass, tiled VAE
//! decode, saved through the frame's own save node with a suffix. The base pass is never re-run, so the finish starts
//! from exactly the picked pixels.

use crate::meta::{self, G};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct Opts {
    pub scale: f64,
    pub max_mp: f64,
    pub denoise: f64,
    /// 0 = the frame's own step count
    pub steps: u32,
    /// empty = lanczos only
    pub upscale_model: String,
    pub suffix: String,
    /// appended to the save prefix's folder ("-hires" puts set/x into set-hires/x); empty = beside the original
    pub folder_suffix: String,
}

pub struct Plan {
    pub graph: G,
    pub note: String,
}

/// `scale` x the source, capped at `max_mp` megapixels (0 = no cap), never smaller than the source, multiples of 8.
pub fn target(w: u32, h: u32, scale: f64, max_mp: f64) -> (u32, u32) {
    let mp = w as f64 * h as f64 / 1e6;
    let mut s = scale.max(1.0);
    if max_mp > 0.0 && mp * s * s > max_mp {
        s = (max_mp / mp).sqrt();
    }
    let s = s.max(1.0);
    let r8 = |x: f64| (((x / 8.0).round() as u32) * 8).max(8);
    (r8(w as f64 * s), r8(h as f64 * s))
}

fn link(id: &str, slot: u64) -> Value {
    json!([id, slot])
}

fn is_link(v: &Value) -> bool {
    meta::link_id(v).is_some()
}

/// The options of a combo input, in either object_info format: [["a","b"]] or ["COMBO", {"options": [...]}].
pub fn combo_options(spec: &Value) -> Option<Vec<String>> {
    let a = spec.as_array()?;
    let list = match a.first()? {
        Value::Array(l) => l.clone(),
        Value::String(t) if t == "COMBO" => a.get(1)?.get("options")?.as_array()?.clone(),
        _ => return None,
    };
    Some(list.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
}

/// Keeps only the inputs the server's node declares and fills its missing required ones with their defaults.
fn fill(inputs: Map<String, Value>, schema: Option<&Value>) -> Map<String, Value> {
    let ins = match schema.and_then(|s| s.get("input")) {
        Some(i) => i,
        None => return inputs,
    };
    let req = ins.get("required").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    let opt = ins.get("optional").and_then(|v| v.as_object()).cloned().unwrap_or_default();
    let mut out: Map<String, Value> = inputs.into_iter().filter(|(k, _)| req.contains_key(k) || opt.contains_key(k)).collect();
    for (k, spec) in &req {
        if out.contains_key(k) {
            continue;
        }
        if let Some(d) = spec.as_array().and_then(|a| a.get(1)).and_then(|o| o.get("default")) {
            out.insert(k.clone(), d.clone());
        } else if let Some(first) = combo_options(spec).and_then(|o| o.into_iter().next()) {
            out.insert(k.clone(), Value::String(first));
        }
    }
    out
}

/// A value for a combo input, or the fallback when the server's list does not have it.
fn pick_option(schema: Option<&Value>, key: &str, want: &str, fallback: &str) -> String {
    let opts = schema.and_then(|s| s.get("input")).and_then(|i| i.get("required")).and_then(|r| r.get(key)).and_then(combo_options);
    match opts {
        Some(o) if !o.iter().any(|x| x == want) => {
            if o.iter().any(|x| x == fallback) {
                fallback.to_string()
            } else {
                o.first().cloned().unwrap_or_else(|| fallback.to_string())
            }
        }
        _ => want.to_string(),
    }
}

pub fn new_prefix(p: &str, suffix: &str, folder_suffix: &str) -> String {
    let (folder, name) = match p.rfind(['/', '\\']) {
        Some(i) => (Some(&p[..i]), &p[i + 1..]),
        None => (None, p),
    };
    let name = if suffix.is_empty() || name.ends_with(suffix) { name.to_string() } else { format!("{}{}", name, suffix) };
    match folder {
        Some(f) if !folder_suffix.is_empty() && !f.ends_with(folder_suffix) => format!("{}{}/{}", f, folder_suffix, name),
        Some(f) => format!("{}/{}", f, name),
        None => name,
    }
}

enum Src {
    Direct { model: Value, positive: Value, negative: Option<Value> },
    /// Efficiency's SDXL_TUPLE
    Tuple(Value),
    /// Impact's BASIC_PIPE
    Pipe(Value),
}

/// A loader's VAE output upstream: VAELoader, a checkpoint loader (slot 2), Efficiency's loaders.
fn loader_vae(g: &G, ups: &[String]) -> Option<Value> {
    let slot = |c: &str| -> Option<u64> {
        match c {
            "VAELoader" => Some(0),
            "Efficient Loader" => Some(4),
            "Eff. Loader SDXL" => Some(2),
            c if c.starts_with("CheckpointLoader") || c.starts_with("Checkpoint Loader") => Some(2),
            _ => None,
        }
    };
    ups.iter().find_map(|id| slot(meta::class(meta::node(g, id)?)).map(|s| link(id, s)))
}

/// Link inputs on the save node (Image Saver's metadata fields, a WAS save's path) that point outside the new graph:
/// kept when their chain is only literals and strings (copied in), dropped when it would re-run images or models.
fn settle_links(g: &G, sv: &mut Value, out: &mut G) {
    let Some(ins) = sv.get_mut("inputs").and_then(|v| v.as_object_mut()) else { return };
    let keys: Vec<String> = ins.iter().filter(|(k, v)| *k != "images" && is_link(v)).map(|(k, _)| k.clone()).collect();
    for k in keys {
        let src = meta::link_id(&ins[&k]).unwrap();
        if out.contains_key(&src) {
            continue;
        }
        let chain = meta::upstream(g, &src);
        let cheap = chain.iter().all(|id| {
            meta::node(g, id)
                .and_then(meta::inputs)
                .map(|i| !meta::is_sampler(meta::node(g, id).unwrap()) && !["image", "images", "pixels", "samples", "model", "clip", "vae", "latent"].iter().any(|k| i.contains_key(*k)))
                .unwrap_or(false)
        });
        if cheap {
            for id in chain {
                if let Some(n) = g.get(&id) {
                    out.entry(id).or_insert_with(|| n.clone());
                }
            }
        } else {
            ins.remove(&k);
        }
    }
}

struct Builder<'s> {
    out: G,
    next: u64,
    schema: &'s dyn Fn(&str) -> Option<Value>,
}

impl Builder<'_> {
    fn add(&mut self, class: &str, inputs: Value) -> String {
        let id = self.next.to_string();
        self.next += 1;
        let s = (self.schema)(class);
        let ins = fill(inputs.as_object().cloned().unwrap_or_default(), s.as_ref());
        self.out.insert(id.clone(), json!({ "class_type": class, "inputs": ins }));
        id
    }
}

/// Builds the hires graph for one frame. `src` is the frame's pixel size, `image` the name LoadImage reads (an upload),
/// `schema` returns a node's object_info entry (None offline: then the inputs go as written).
pub fn build(g: &G, stem: &str, src: (u32, u32), image: &str, o: &Opts, schema: &dyn Fn(&str) -> Option<Value>) -> Result<Plan, String> {
    let save = meta::find_save(g, stem).ok_or("no save node in this frame's graph")?;
    let sc = meta::class(meta::node(g, &save).unwrap());
    if sc == "VHS_VideoCombine" || sc.contains("Video") || sc.contains("WEBM") || sc.contains("Animated") {
        return Err(format!("this frame's graph saves a video ({}) - hires finishes stills", sc));
    }
    let ups = meta::upstream(g, &save);
    let sid = ups
        .iter()
        .find(|id| meta::node(g, id).map(meta::is_sampler).unwrap_or(false))
        .cloned()
        .ok_or("no sampler upstream of the save node")?;
    let ins = meta::inputs(meta::node(g, &sid).unwrap()).unwrap();
    let guider = meta::via(g, ins, "guider");

    // where the model and prompts come from: the sampler's own inputs (or its guider's, or Searge's base_*), or a bundle
    // that is unpacked in the new graph (Efficiency's SDXL tuple, Impact's basic pipe)
    let linked = |v: Option<&Value>| v.filter(|v| is_link(v)).cloned();
    let from = if let Some(t) = linked(ins.get("sdxl_tuple")) {
        Src::Tuple(t)
    } else if let Some(p) = linked(ins.get("basic_pipe")) {
        Src::Pipe(p)
    } else {
        let model = linked(ins.get("model"))
            .or_else(|| linked(ins.get("base_model")))
            .or_else(|| guider.and_then(|gi| linked(gi.get("model"))))
            .ok_or("cannot find the model chain that made this frame")?;
        let positive = linked(ins.get("positive"))
            .or_else(|| linked(ins.get("base_positive")))
            .or_else(|| guider.and_then(|gi| linked(gi.get("positive").or_else(|| gi.get("conditioning")))))
            .ok_or("cannot find the prompt that made this frame")?;
        let negative = linked(ins.get("negative")).or_else(|| linked(ins.get("base_negative"))).or_else(|| guider.and_then(|gi| linked(gi.get("negative"))));
        Src::Direct { model, positive, negative }
    };
    let cfg = meta::value_of(g, ins, "cfg", &["cfg", "value", "float"])
        .or_else(|| guider.and_then(|gi| gi.get("cfg").and_then(meta::lit)))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0);
    let sampler_name = meta::value_of(g, ins, "sampler_name", &["sampler_name"])
        .or_else(|| meta::value_of(g, ins, "sampler", &["sampler_name"]))
        .unwrap_or_else(|| "euler".into());
    let scheduler = meta::value_of(g, ins, "scheduler", &["scheduler"])
        .or_else(|| meta::value_of(g, ins, "sigmas", &["scheduler"]))
        .unwrap_or_else(|| "simple".into());
    let steps = if o.steps > 0 {
        o.steps
    } else {
        meta::value_of(g, ins, "steps", &["steps", "value", "int"])
            .or_else(|| meta::value_of(g, ins, "sigmas", &["steps"]))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(8)
    };
    let seed = meta::value_of(g, ins, "seed", &["seed", "noise_seed", "value", "int"])
        .or_else(|| meta::value_of(g, ins, "noise_seed", &["noise_seed", "seed", "value", "int"]))
        .or_else(|| meta::value_of(g, ins, "noise", &["noise_seed", "seed"]))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|s| s.wrapping_add(500_000) & ((1u64 << 50) - 1))
        .unwrap_or_else(crate::comfy::random_seed);
    let vae = ups
        .iter()
        .filter_map(|id| meta::node(g, id))
        .filter(|n| meta::class(n).contains("VAEDecode"))
        .find_map(|n| meta::inputs(n).and_then(|i| i.get("vae")).filter(|v| is_link(v)).cloned())
        .or_else(|| ins.get("optional_vae").filter(|v| is_link(v)).cloned())
        .or_else(|| loader_vae(g, &ups));
    if vae.is_none() && !matches!(from, Src::Pipe(_)) {
        return Err("cannot find the VAE that decoded this frame".into());
    }

    // keep only what the model, the prompts and the VAE need
    let mut keep: HashSet<String> = HashSet::new();
    let sources: Vec<&Value> = match &from {
        Src::Direct { model, positive, negative } => [Some(model), Some(positive), negative.as_ref()].into_iter().flatten().collect(),
        Src::Tuple(t) => vec![t],
        Src::Pipe(p) => vec![p],
    };
    for v in sources.into_iter().chain(vae.as_ref()) {
        if let Some(src) = meta::link_id(v) {
            keep.extend(meta::upstream(g, &src));
        }
    }
    let mut out = G::new();
    for id in &keep {
        if let Some(n) = g.get(id) {
            out.insert(id.clone(), n.clone());
        }
    }
    let next = g.keys().filter_map(|k| k.parse::<u64>().ok()).max().unwrap_or(0) + 1000;
    let mut b = Builder { out, next, schema };
    let (model, positive, negative, vae) = match from {
        Src::Direct { model, positive, negative } => (model, positive, negative, vae.unwrap()),
        Src::Tuple(t) => {
            // (base_model, base_clip, base_positive, base_negative, refiner ...)
            let u = b.add("Unpack SDXL Tuple", json!({ "sdxl_tuple": t }));
            (link(&u, 0), link(&u, 2), Some(link(&u, 3)), vae.unwrap())
        }
        Src::Pipe(p) => {
            // (model, clip, vae, positive, negative)
            let f = b.add("FromBasicPipe", json!({ "basic_pipe": p }));
            (link(&f, 0), link(&f, 3), Some(link(&f, 4)), vae.unwrap_or_else(|| link(&f, 2)))
        }
    };

    let load = b.add("LoadImage", json!({ "image": image }));
    let mut img = link(&load, 0);
    if !o.upscale_model.is_empty() {
        let um = b.add("UpscaleModelLoader", json!({ "model_name": o.upscale_model }));
        let up = b.add("ImageUpscaleWithModel", json!({ "upscale_model": link(&um, 0), "image": img }));
        img = link(&up, 0);
    }
    let (w, h) = target(src.0, src.1, o.scale, o.max_mp);
    let sc = b.add("ImageScale", json!({ "image": img, "upscale_method": "lanczos", "width": w, "height": h, "crop": "disabled" }));
    let enc = b.add("VAEEncodeTiled", json!({ "pixels": link(&sc, 0), "vae": vae, "tile_size": 512, "overlap": 64 }));
    let neg = match negative {
        Some(n) => n,
        None => {
            let z = b.add("ConditioningZeroOut", json!({ "conditioning": positive }));
            link(&z, 0)
        }
    };
    let ks_schema = schema("KSampler");
    let sampler_name = pick_option(ks_schema.as_ref(), "sampler_name", &sampler_name, "euler");
    let scheduler = pick_option(ks_schema.as_ref(), "scheduler", &scheduler, "simple");
    let ks = b.add(
        "KSampler",
        json!({ "model": model, "seed": seed, "steps": steps, "cfg": cfg, "sampler_name": sampler_name, "scheduler": scheduler,
                "positive": positive, "negative": neg, "latent_image": link(&enc, 0), "denoise": o.denoise }),
    );
    let dec = b.add("VAEDecodeTiled", json!({ "samples": link(&ks, 0), "vae": vae, "tile_size": 512, "overlap": 64 }));

    let mut sv = g[&save].clone();
    sv["inputs"]["images"] = link(&dec, 0);
    settle_links(g, &mut sv, &mut b.out);
    let key = sv.get("inputs").and_then(|v| v.as_object()).and_then(|i| meta::save_name_key(sv["class_type"].as_str().unwrap_or(""), i));
    if let Some(key) = key {
        if let Some(Value::String(p)) = sv["inputs"].get(key).cloned() {
            let named = if key == "filename" {
                // Image Saver: the file name pattern takes the suffix, its `path` input the folder suffix
                if let Some(Value::String(f)) = sv["inputs"].get("path").cloned() {
                    if !o.folder_suffix.is_empty() && !f.is_empty() && !f.ends_with(&o.folder_suffix) {
                        sv["inputs"]["path"] = Value::String(format!("{}{}", f.trim_end_matches(['/', '\\']), o.folder_suffix));
                    }
                }
                if p.ends_with(&o.suffix) { p } else { format!("{}{}", p, o.suffix) }
            } else {
                new_prefix(&p, &o.suffix, &o.folder_suffix)
            };
            sv["inputs"][key] = Value::String(named);
        }
    }
    b.out.insert(save, sv);
    let note = format!(
        "{}x{} · denoise {} · {} steps{}",
        w,
        h,
        o.denoise,
        steps,
        if o.upscale_model.is_empty() { String::new() } else { format!(" · {}", meta::base_name(&o.upscale_model)) }
    );
    Ok(Plan { graph: b.out, note })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Opts {
        Opts { scale: 2.0, max_mp: 4.2, denoise: 0.3, steps: 0, upscale_model: String::new(), suffix: "_hires".into(), folder_suffix: String::new() }
    }

    fn schema(class: &str) -> Option<Value> {
        match class {
            "VAEEncodeTiled" => Some(json!({"input": {"required": {"pixels": ["IMAGE"], "vae": ["VAE"], "tile_size": ["INT", {"default": 512}], "overlap": ["INT", {"default": 64}], "temporal_size": ["INT", {"default": 64}], "temporal_overlap": ["INT", {"default": 8}]}}})),
            "KSampler" => Some(json!({"input": {"required": {"model": ["MODEL"], "seed": ["INT", {}], "steps": ["INT", {}], "cfg": ["FLOAT", {}], "sampler_name": [["euler", "er_sde"]], "scheduler": ["COMBO", {"options": ["simple", "beta"]}], "positive": ["CONDITIONING"], "negative": ["CONDITIONING"], "latent_image": ["LATENT"], "denoise": ["FLOAT", {}]}}})),
            _ => None,
        }
    }

    #[test]
    fn sizes() {
        assert_eq!(target(1024, 1024, 2.0, 4.2), (2048, 2048));
        assert_eq!(target(896, 1152, 2.0, 4.2), (1792, 2304));
        assert_eq!(target(1088, 1920, 2.0, 4.2), (1544, 2720));
        assert_eq!(target(2048, 2048, 2.0, 4.2), (2048, 2048), "never smaller than the source");
        assert_eq!(target(512, 512, 1.5, 0.0), (768, 768));
    }

    #[test]
    fn prefixes() {
        assert_eq!(new_prefix("set/fox", "_hires", ""), "set/fox_hires");
        assert_eq!(new_prefix("set/fox", "_hires", "-hires"), "set-hires/fox_hires");
        assert_eq!(new_prefix("fox", "_hires", "-hires"), "fox_hires");
        assert_eq!(new_prefix("set/fox_hires", "_hires", ""), "set/fox_hires");
    }

    #[test]
    fn hires_from_ksampler_graph() {
        let g: G = serde_json::from_str(crate::meta::tests::BASIC).unwrap();
        let p = build(&g, "fox_00001_", (896, 1152), "loupe/fox.png", &opts(), &schema).unwrap();
        let gr = &p.graph;
        for gone in ["3", "5", "8"] {
            assert!(!gr.contains_key(gone), "node {} should be pruned", gone);
        }
        for kept in ["4", "10", "11", "6", "7", "9"] {
            assert!(gr.contains_key(kept), "node {} should be kept", kept);
        }
        let by = |c: &str| gr.values().find(|n| n["class_type"] == c).unwrap().clone();
        let ks = by("KSampler");
        assert_eq!(ks["inputs"]["model"], json!(["11", 0]));
        assert_eq!((ks["inputs"]["positive"].clone(), ks["inputs"]["negative"].clone()), (json!(["6", 0]), json!(["7", 0])));
        assert_eq!(ks["inputs"]["seed"], json!(1234 + 500_000));
        assert_eq!((ks["inputs"]["steps"].clone(), ks["inputs"]["denoise"].clone()), (json!(8), json!(0.3)));
        let enc = by("VAEEncodeTiled");
        assert_eq!(enc["inputs"]["vae"], json!(["4", 2]));
        assert_eq!(enc["inputs"]["temporal_overlap"], json!(8), "required inputs filled from the schema");
        let sc = by("ImageScale");
        assert_eq!((sc["inputs"]["width"].clone(), sc["inputs"]["height"].clone()), (json!(1792), json!(2304)));
        assert_eq!(by("LoadImage")["inputs"]["image"], json!("loupe/fox.png"));
        let save = gr["9"].clone();
        assert_eq!(save["inputs"]["filename_prefix"], json!("tests/fox_hires"));
        let dec = by("VAEDecodeTiled");
        let dec_id = gr.iter().find(|(_, n)| n["class_type"] == "VAEDecodeTiled").unwrap().0.clone();
        assert_eq!(save["inputs"]["images"], json!([dec_id, 0]));
        assert_eq!(dec["inputs"]["vae"], json!(["4", 2]));
    }

    #[test]
    fn hires_from_custom_advanced_graph() {
        let mut g: G = serde_json::from_str(crate::meta::tests::CUSTOM).unwrap();
        g.insert("11".into(), json!({"class_type": "VAELoader", "inputs": {"vae_name": "ae.safetensors"}}));
        let mut o = opts();
        o.upscale_model = "4x_foolhardy_Remacri.pth".into();
        let p = build(&g, "city_00001_", (1088, 1920), "loupe/city.png", &o, &schema).unwrap();
        let ks = p.graph.values().find(|n| n["class_type"] == "KSampler").unwrap().clone();
        assert_eq!(ks["inputs"]["model"], json!(["1", 0]));
        assert_eq!(ks["inputs"]["cfg"], json!(3.5));
        assert_eq!((ks["inputs"]["sampler_name"].clone(), ks["inputs"]["scheduler"].clone()), (json!("er_sde"), json!("beta")));
        assert_eq!(ks["inputs"]["steps"], json!(10));
        assert_eq!(ks["inputs"]["seed"], json!(77 + 500_000));
        assert!(p.graph.values().any(|n| n["class_type"] == "ImageUpscaleWithModel"));
        assert!(!p.graph.values().any(|n| n["class_type"] == "SamplerCustomAdvanced"), "the old sampler is pruned");
        let sc = p.graph.values().find(|n| n["class_type"] == "ImageScale").unwrap();
        assert_eq!((sc["inputs"]["width"].clone(), sc["inputs"]["height"].clone()), (json!(1544), json!(2720)));
    }

    #[test]
    fn unknown_sampler_falls_back() {
        let s = schema("KSampler");
        assert_eq!(pick_option(s.as_ref(), "sampler_name", "res_2m", "euler"), "euler");
        assert_eq!(pick_option(s.as_ref(), "scheduler", "beta", "simple"), "beta");
        assert_eq!(pick_option(None, "scheduler", "beta57", "simple"), "beta57");
    }
}
