//! Generation metadata: text chunks from PNG (tEXt / zTXt / iTXt) and EXIF from JPEG / WebP, summarised from a ComfyUI API
//! graph (walked from the save node back to the sampler that made the image), an A1111 / Forge `parameters` block, or
//! SwarmUI's `sui_image_params`.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

// ----------------------------------------------------------------------------------------------- chunk reading

pub fn read_chunks(path: &Path) -> Vec<(String, String)> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    match ext.as_str() {
        "png" => png_chunks(&mut f),
        "jpg" | "jpeg" => jpeg_chunks(&mut f),
        "webp" => webp_chunks(&mut f),
        "mp4" | "m4v" | "mov" => video_keys(mp4_tags(&mut f)),
        "webm" | "mkv" => video_keys(mkv_tags(&mut f)),
        _ => Vec::new(),
    }
}

// ---- video containers: the graph ComfyUI's SaveVideo / SaveWEBM and Video Helper Suite's Video Combine embed

/// Container tags as metadata pairs, names lowercased: ComfyUI's SaveVideo / SaveWEBM and Video Helper Suite's Video
/// Combine (save_metadata) all write `prompt` and `workflow` keys (mp4: QuickTime keys under `use_metadata_tags`;
/// Matroska: the uppercased tags PROMPT / WORKFLOW).
fn video_keys(tags: Vec<(String, String)>) -> Vec<(String, String)> {
    tags.into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect()
}

const TAG_LIMIT: u64 = 64 * 1024 * 1024;

/// MP4 / MOV tags: iTunes-style moov/udta/meta/ilst items (©cmt -> comment) and QuickTime moov/meta keys + ilst
/// (`movflags use_metadata_tags`, ffmpeg's way of keeping custom keys).
fn mp4_tags(f: &mut File) -> Vec<(String, String)> {
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut pos = 0u64;
    while pos + 8 <= len {
        let mut h = [0u8; 16];
        if f.seek(SeekFrom::Start(pos)).is_err() || f.read_exact(&mut h[..8]).is_err() {
            break;
        }
        let mut size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
        let mut head = 8;
        if size == 1 {
            if f.read_exact(&mut h[8..16]).is_err() {
                break;
            }
            size = u64::from_be_bytes([h[8], h[9], h[10], h[11], h[12], h[13], h[14], h[15]]);
            head = 16;
        } else if size == 0 {
            size = len - pos;
        }
        if size < head {
            break;
        }
        if &h[4..8] == b"moov" {
            if size - head > TAG_LIMIT {
                break;
            }
            let mut d = vec![0u8; (size - head) as usize];
            if f.read_exact(&mut d).is_err() {
                break;
            }
            let mut out = Vec::new();
            mp4_walk(&d, &mut out);
            return out;
        }
        pos += size;
    }
    Vec::new()
}

/// The boxes in `d`: (type, content).
fn mp4_boxes(d: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= d.len() {
        let mut size = u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize;
        let typ = [d[i + 4], d[i + 5], d[i + 6], d[i + 7]];
        let mut head = 8;
        if size == 1 && i + 16 <= d.len() {
            size = u64::from_be_bytes(d[i + 8..i + 16].try_into().unwrap()) as usize;
            head = 16;
        } else if size == 0 {
            size = d.len() - i;
        }
        if size < head || i + size > d.len() {
            break;
        }
        out.push((typ, &d[i + head..i + size]));
        i += size;
    }
    out
}

fn mp4_walk(moov: &[u8], out: &mut Vec<(String, String)>) {
    for (t, c) in mp4_boxes(moov) {
        match &t {
            b"udta" => {
                for (t2, c2) in mp4_boxes(c) {
                    if &t2 == b"meta" {
                        mp4_meta(c2, out);
                    }
                }
            }
            b"meta" => mp4_meta(c, out),
            _ => {}
        }
    }
}

/// A meta box: ISO (a version / flags word before its children) or QuickTime (children at once).
fn mp4_meta(c: &[u8], out: &mut Vec<(String, String)>) {
    let kids = if c.len() >= 12 && &c[8..12] == b"hdlr" && &c[4..8] != b"hdlr" { &c[4..] } else { c };
    let boxes = mp4_boxes(kids);
    let mut keys: Vec<String> = Vec::new();
    if let Some((_, k)) = boxes.iter().find(|(t, _)| t == b"keys") {
        // version / flags, entry count, then (size, namespace, name)
        let mut i = 8usize;
        while i + 8 <= k.len() {
            let size = u32::from_be_bytes([k[i], k[i + 1], k[i + 2], k[i + 3]]) as usize;
            if size < 8 || i + size > k.len() {
                break;
            }
            keys.push(String::from_utf8_lossy(&k[i + 8..i + size]).into_owned());
            i += size;
        }
    }
    let Some((_, ilst)) = boxes.iter().find(|(t, _)| t == b"ilst") else { return };
    for (t, item) in mp4_boxes(ilst) {
        let name = if t[0] == 0xA9 {
            match &t[1..] {
                b"cmt" => "comment".to_string(),
                b"nam" => "title".to_string(),
                b"des" => "description".to_string(),
                _ => continue,
            }
        } else {
            let n = u32::from_be_bytes(t) as usize;
            match keys.get(n.wrapping_sub(1)) {
                Some(k) => k.clone(),
                None if &t == b"desc" => "description".to_string(),
                None => continue,
            }
        };
        for (dt, data) in mp4_boxes(item) {
            // data: type indicator (1 = UTF-8), locale, value
            if &dt == b"data" && data.len() >= 8 && u32::from_be_bytes([data[0], data[1], data[2], data[3]]) & 0xFF_FFFF == 1 {
                out.push((name.clone(), String::from_utf8_lossy(&data[8..]).into_owned()));
            }
        }
    }
}

/// An EBML variable-length number at `d[i..]`: (value, length). `keep_marker` keeps the length bit (element ids).
fn ebml_vint(d: &[u8], i: usize, keep_marker: bool) -> Option<(u64, usize)> {
    let first = *d.get(i)?;
    let n = first.leading_zeros() as usize + 1;
    if n > 8 || i + n > d.len() {
        return None;
    }
    let mut v = if keep_marker { first as u64 } else { (first as u64) & ((1u64 << (8 - n)) - 1) };
    for b in &d[i + 1..i + n] {
        v = (v << 8) | *b as u64;
    }
    Some((v, n))
}

/// The elements in `d`: (id, content). An unknown size runs to the end.
fn ebml_elems(d: &[u8]) -> Vec<(u64, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < d.len() {
        let Some((id, a)) = ebml_vint(d, i, true) else { break };
        let Some((size, b)) = ebml_vint(d, i + a, false) else { break };
        let start = i + a + b;
        let unknown = size == (1u64 << (7 * b)) - 1;
        let end = if unknown { d.len() } else { start.saturating_add(size as usize).min(d.len()) };
        if start > d.len() {
            break;
        }
        out.push((id, &d[start..end]));
        i = end;
    }
    out
}

/// Matroska / WebM SimpleTags (TagName -> TagString), read from the Segment's Tags without loading the clusters.
fn mkv_tags(f: &mut File) -> Vec<(String, String)> {
    const SEGMENT: u64 = 0x1853_8067;
    const TAGS: u64 = 0x1254_C367;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut head = [0u8; 4];
    if f.read_exact(&mut head).is_err() || head != [0x1A, 0x45, 0xDF, 0xA3] {
        return Vec::new();
    }
    // the elements of one level, from `pos`, by seeking: (id, content start, content size)
    let read_head = |f: &mut File, pos: u64| -> Option<(u64, u64, u64)> {
        let mut b = [0u8; 16];
        f.seek(SeekFrom::Start(pos)).ok()?;
        let n = f.read(&mut b).ok()?;
        let (id, a) = ebml_vint(&b[..n], 0, true)?;
        let (size, s) = ebml_vint(&b[..n], a, false)?;
        let size = if size == (1u64 << (7 * s)) - 1 { u64::MAX } else { size };
        Some((id, pos + (a + s) as u64, size))
    };
    let mut pos = 0u64;
    let mut seg_end = len;
    let mut out = Vec::new();
    while pos < seg_end {
        let Some((id, start, size)) = read_head(f, pos) else { break };
        if id == SEGMENT {
            seg_end = if size == u64::MAX { len } else { (start + size).min(len) };
            pos = start; // step into the segment
            continue;
        }
        if id == TAGS && size <= TAG_LIMIT {
            let mut d = vec![0u8; size as usize];
            if f.seek(SeekFrom::Start(start)).is_ok() && f.read_exact(&mut d).is_ok() {
                for (tid, tag) in ebml_elems(&d) {
                    if tid != 0x7373 {
                        continue;
                    }
                    for (sid, st) in ebml_elems(tag) {
                        if sid != 0x67C8 {
                            continue;
                        }
                        let kids = ebml_elems(st);
                        let get = |want: u64| kids.iter().find(|(k, _)| *k == want).map(|(_, v)| String::from_utf8_lossy(v).trim_end_matches('\0').to_string());
                        if let (Some(n), Some(v)) = (get(0x45A3), get(0x4487)) {
                            out.push((n, v));
                        }
                    }
                }
            }
        }
        if size == u64::MAX {
            break; // an unknown-size cluster: tags, if any, came before it
        }
        pos = start + size;
    }
    out
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn inflate(b: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(b).read_to_end(&mut out).ok()?;
    Some(out)
}

fn png_chunks(f: &mut File) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut sig = [0u8; 8];
    if f.read_exact(&mut sig).is_err() || sig != [137, 80, 78, 71, 13, 10, 26, 10] {
        return out;
    }
    loop {
        let mut h = [0u8; 8];
        if f.read_exact(&mut h).is_err() {
            break;
        }
        let len = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
        let typ = [h[4], h[5], h[6], h[7]];
        match &typ {
            b"comf" if len <= 64 * 1024 * 1024 => {
                // ComfyUI's SaveAnimatedPNG: key\0json in its own chunk type, after the frames
                let mut d = vec![0u8; len as usize];
                if f.read_exact(&mut d).is_err() {
                    break;
                }
                let _ = f.seek(SeekFrom::Current(4));
                if let Some(z) = d.iter().position(|&c| c == 0) {
                    out.push((latin1(&d[..z]), String::from_utf8_lossy(&d[z + 1..]).into_owned()));
                }
            }
            b"tEXt" | b"zTXt" | b"iTXt" if len <= 64 * 1024 * 1024 => {
                let mut d = vec![0u8; len as usize];
                if f.read_exact(&mut d).is_err() {
                    break;
                }
                let _ = f.seek(SeekFrom::Current(4));
                if let Some(kv) = parse_text_chunk(&typ, &d) {
                    out.push(kv);
                }
            }
            b"IEND" => break,
            _ => {
                if f.seek(SeekFrom::Current(len as i64 + 4)).is_err() {
                    break;
                }
            }
        }
    }
    out
}

fn parse_text_chunk(typ: &[u8; 4], d: &[u8]) -> Option<(String, String)> {
    let z = d.iter().position(|&c| c == 0)?;
    let key = latin1(&d[..z]);
    let rest = &d[z + 1..];
    let text = match typ {
        b"tEXt" => latin1(rest),
        b"zTXt" => latin1(&inflate(rest.get(1..)?)?),
        _ => {
            // iTXt: compression flag, method, language\0, translated keyword\0, text (UTF-8)
            let comp = *rest.first()?;
            let r = rest.get(2..)?;
            let a = r.iter().position(|&c| c == 0)?;
            let r = &r[a + 1..];
            let b = r.iter().position(|&c| c == 0)?;
            let t = &r[b + 1..];
            if comp == 1 {
                String::from_utf8_lossy(&inflate(t)?).into_owned()
            } else {
                String::from_utf8_lossy(t).into_owned()
            }
        }
    };
    Some((key, text))
}

fn jpeg_chunks(f: &mut File) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut b = [0u8; 2];
    if f.read_exact(&mut b).is_err() || b != [0xFF, 0xD8] {
        return out;
    }
    loop {
        let mut m = [0u8; 4];
        if f.read_exact(&mut m).is_err() || m[0] != 0xFF {
            break;
        }
        let marker = m[1];
        let len = u16::from_be_bytes([m[2], m[3]]) as usize;
        if marker == 0xDA || marker == 0xD9 || len < 2 {
            break; // start of scan: no more metadata
        }
        let mut d = vec![0u8; len - 2];
        if f.read_exact(&mut d).is_err() {
            break;
        }
        if marker == 0xE1 && d.starts_with(b"Exif\0\0") {
            out.extend(exif_fields(&d[6..]));
        } else if marker == 0xFE {
            out.push(("comment".into(), String::from_utf8_lossy(&d).into_owned()));
        }
    }
    out
}

fn webp_chunks(f: &mut File) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut h = [0u8; 12];
    if f.read_exact(&mut h).is_err() || &h[0..4] != b"RIFF" || &h[8..12] != b"WEBP" {
        return out;
    }
    loop {
        let mut c = [0u8; 8];
        if f.read_exact(&mut c).is_err() {
            break;
        }
        let size = u32::from_le_bytes([c[4], c[5], c[6], c[7]]) as u64;
        if &c[0..4] == b"EXIF" && size < 64 * 1024 * 1024 {
            let mut d = vec![0u8; size as usize];
            if f.read_exact(&mut d).is_err() {
                break;
            }
            let t = if d.starts_with(b"Exif\0\0") { &d[6..] } else { &d[..] };
            out.extend(exif_fields(t));
            if size % 2 == 1 {
                let _ = f.seek(SeekFrom::Current(1));
            }
        } else if f.seek(SeekFrom::Current((size + (size & 1)) as i64)).is_err() {
            break;
        }
    }
    out
}

/// The EXIF fields generators write metadata into: UserComment (A1111 / Forge), Make "workflow:" and Model "prompt:"
/// (ComfyUI's WebP savers), ImageDescription, XPComment.
pub fn exif_fields(t: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if t.len() < 8 {
        return out;
    }
    let le = &t[0..2] == b"II";
    let u16_at = |o: usize| -> Option<u16> {
        let b = t.get(o..o + 2)?;
        Some(if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let b = t.get(o..o + 4)?;
        Some(if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };
    let mut ifds = vec![u32_at(4).unwrap_or(0) as usize];
    let mut seen = HashSet::new();
    while let Some(off) = ifds.pop() {
        if off == 0 || !seen.insert(off) {
            continue;
        }
        let n = match u16_at(off) {
            Some(n) => n as usize,
            None => continue,
        };
        for i in 0..n.min(512) {
            let e = off + 2 + i * 12;
            let (tag, typ, count) = match (u16_at(e), u16_at(e + 2), u32_at(e + 4)) {
                (Some(a), Some(b), Some(c)) => (a, b, c as usize),
                _ => break,
            };
            let unit = match typ {
                1 | 2 | 6 | 7 => 1,
                3 | 8 => 2,
                4 | 9 | 11 => 4,
                _ => 8,
            };
            let total = unit * count;
            let data_off = if total <= 4 { e + 8 } else { u32_at(e + 8).unwrap_or(0) as usize };
            let data = match t.get(data_off..data_off.saturating_add(total)) {
                Some(d) => d,
                None => continue,
            };
            let ascii = || latin1(data).trim_end_matches('\0').to_string();
            match tag {
                0x8769 => ifds.push(u32_at(e + 8).unwrap_or(0) as usize),
                0x010E => out.push(("description".into(), ascii())),
                0x010F | 0x0110 => {
                    let s = ascii();
                    let low = s.to_ascii_lowercase();
                    if let Some(i) = low.find("workflow:") {
                        out.push(("workflow".into(), s[i + 9..].to_string()));
                    } else if let Some(i) = low.find("prompt:") {
                        out.push(("prompt".into(), s[i + 7..].to_string()));
                    }
                }
                0x9286 => {
                    let s = user_comment(data);
                    if !s.trim().is_empty() {
                        out.push(("parameters".into(), s));
                    }
                }
                0x9C9C => {
                    let u: Vec<u16> = data.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    out.push(("comment".into(), String::from_utf16_lossy(&u).trim_end_matches('\0').to_string()));
                }
                _ => {}
            }
        }
    }
    out
}

fn user_comment(d: &[u8]) -> String {
    if d.len() < 8 {
        return String::new();
    }
    let (head, body) = d.split_at(8);
    if head.starts_with(b"UNICODE") {
        // piexif writes UTF-16 BE; some tools write LE - pick the order whose high bytes are mostly zero
        let even_zero = body.iter().step_by(2).filter(|&&b| b == 0).count();
        let odd_zero = body.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
        let be = even_zero >= odd_zero;
        let u: Vec<u16> = body
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&u).trim_end_matches('\0').to_string()
    } else {
        String::from_utf8_lossy(body).trim_end_matches('\0').to_string()
    }
}

/// Pixel size from the file header (PNG IHDR, JPEG SOFn, WebP VP8 / VP8L / VP8X), without decoding.
pub fn dims(path: &Path) -> Option<(u32, u32)> {
    let mut f = File::open(path).ok()?;
    let mut h = vec![0u8; 64];
    let n = f.read(&mut h).ok()?;
    h.truncate(n);
    if h.starts_with(&[137, 80, 78, 71]) && h.len() >= 24 {
        return Some((u32::from_be_bytes([h[16], h[17], h[18], h[19]]), u32::from_be_bytes([h[20], h[21], h[22], h[23]])));
    }
    if h.starts_with(b"RIFF") && h.len() >= 30 && &h[8..12] == b"WEBP" {
        return match &h[12..16] {
            b"VP8X" => Some((
                1 + (h[24] as u32 | (h[25] as u32) << 8 | (h[26] as u32) << 16),
                1 + (h[27] as u32 | (h[28] as u32) << 8 | (h[29] as u32) << 16),
            )),
            b"VP8 " => Some(((u16::from_le_bytes([h[26], h[27]]) & 0x3FFF) as u32, (u16::from_le_bytes([h[28], h[29]]) & 0x3FFF) as u32)),
            b"VP8L" => {
                let b = u32::from_le_bytes([h[21], h[22], h[23], h[24]]);
                Some(((b & 0x3FFF) + 1, ((b >> 14) & 0x3FFF) + 1))
            }
            _ => None,
        };
    }
    if h.starts_with(&[0xFF, 0xD8]) {
        f.seek(SeekFrom::Start(2)).ok()?;
        loop {
            let mut m = [0u8; 4];
            f.read_exact(&mut m).ok()?;
            if m[0] != 0xFF {
                return None;
            }
            let marker = m[1];
            let len = u16::from_be_bytes([m[2], m[3]]) as i64;
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                let mut s = [0u8; 5];
                f.read_exact(&mut s).ok()?;
                return Some((u16::from_be_bytes([s[3], s[4]]) as u32, u16::from_be_bytes([s[1], s[2]]) as u32));
            }
            f.seek(SeekFrom::Current(len - 2)).ok()?;
        }
    }
    None
}

// ----------------------------------------------------------------------------------------------- the summary

#[derive(Serialize, Default, Debug, Clone)]
pub struct Meta {
    pub source: String,
    pub model: Option<String>,
    pub loras: Vec<(String, String)>,
    pub sampler: Option<String>,
    pub scheduler: Option<String>,
    pub steps: Option<String>,
    pub cfg: Option<String>,
    pub seed: Option<String>,
    pub denoise: Option<String>,
    pub size: Option<String>,
    pub positive: String,
    pub negative: String,
    pub passes: usize,
    pub keys: Vec<String>,
    pub can_reroll: bool,
}

pub fn chunk<'a>(chunks: &'a [(String, String)], key: &str) -> Option<&'a str> {
    chunks.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

pub fn summarize(path: &Path) -> Meta {
    let chunks = read_chunks(path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    summarize_chunks(&chunks, &stem)
}

pub fn summarize_chunks(chunks: &[(String, String)], stem: &str) -> Meta {
    let mut m = Meta { keys: chunks.iter().map(|(k, _)| k.clone()).collect(), ..Default::default() };
    if let Some(p) = chunk(chunks, "prompt") {
        if let Ok(Value::Object(g)) = serde_json::from_str::<Value>(p) {
            comfy_summary(&g, stem, &mut m);
            m.can_reroll = crate::comfy::count_seeds(&g) > 0;
        }
    }
    let params = chunk(chunks, "parameters").or_else(|| chunk(chunks, "a1111_parameters"));
    if let Some(params) = params {
        let t = params.trim_start();
        if t.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<Value>(t) {
                if let Some(Value::Object(sp)) = v.get("sui_image_params") {
                    swarm_summary(sp, &mut m);
                }
            }
        } else {
            a1111_summary(params, &mut m);
        }
    }
    m
}

// ---- ComfyUI graph walk

pub(crate) type G = Map<String, Value>;

pub(crate) fn node<'a>(g: &'a G, id: &str) -> Option<&'a Map<String, Value>> {
    g.get(id)?.as_object()
}

pub(crate) fn class<'a>(n: &'a Map<String, Value>) -> &'a str {
    n.get("class_type").and_then(|v| v.as_str()).unwrap_or("")
}

pub(crate) fn inputs<'a>(n: &'a Map<String, Value>) -> Option<&'a Map<String, Value>> {
    n.get("inputs")?.as_object()
}

/// A link input is [source_id, output_slot].
pub fn link_id(v: &Value) -> Option<String> {
    let a = v.as_array()?;
    if a.len() != 2 || !a[1].is_number() {
        return None;
    }
    match &a[0] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub(crate) fn lit(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn base_name(s: &str) -> String {
    let file = s.rsplit(['\\', '/']).next().unwrap_or(s);
    match file.rsplit_once('.') {
        Some((a, ext)) if ext.len() <= 12 && !a.is_empty() => a.to_string(),
        _ => file.to_string(),
    }
}

pub(crate) fn is_sampler(n: &Map<String, Value>) -> bool {
    match inputs(n) {
        Some(i) => (i.contains_key("latent_image") || i.contains_key("latent"))
            && BUNDLE_KEYS.iter().chain(["model", "guider", "positive", "base_model", "base_positive"].iter()).any(|k| i.contains_key(*k)),
        None => false,
    }
}

/// Inputs that carry a model and its prompts together: Efficiency's SDXL tuple, Impact's basic pipe, rgthree's context.
pub(crate) const BUNDLE_KEYS: &[&str] = &["sdxl_tuple", "basic_pipe", "base_ctx", "ctx", "context", "pipe"];

/// The sampler's model source: its model input, its guider's, or the bundle it takes.
pub(crate) fn model_source(g: &G, ins: &Map<String, Value>) -> Option<String> {
    ins.get("model")
        .or_else(|| ins.get("base_model"))
        .and_then(link_id)
        .or_else(|| via(g, ins, "guider").and_then(|gi| gi.get("model").and_then(link_id)))
        .or_else(|| BUNDLE_KEYS.iter().find_map(|k| ins.get(*k).and_then(link_id)))
}

/// The sampler's positive (or negative) conditioning source: its own input, its guider's, or the bundle it takes.
pub(crate) fn cond_source(g: &G, ins: &Map<String, Value>, neg: bool) -> Option<String> {
    let key = if neg { "negative" } else { "positive" };
    let base = if neg { "base_negative" } else { "base_positive" };
    ins.get(key).or_else(|| ins.get(base)).and_then(link_id).or_else(|| {
        let gi = via(g, ins, "guider")?;
        gi.get(key).or_else(|| if neg { None } else { gi.get("conditioning") }).and_then(link_id)
    })
    .or_else(|| if via(g, ins, "guider").is_some() { None } else { BUNDLE_KEYS.iter().find_map(|k| ins.get(*k).and_then(link_id)) })
}

/// A literal input, or the first of `names` on the node a link points at (four hops at most, through primitives,
/// reroutes, switches and display nodes such as easy showAnything).
pub(crate) fn value_of(g: &G, ins: &Map<String, Value>, key: &str, names: &[&str]) -> Option<String> {
    let v = ins.get(key)?;
    if let Some(s) = lit(v) {
        return Some(s);
    }
    let mut id = link_id(v)?;
    for _ in 0..4 {
        let n = node(g, &id)?;
        let ni = inputs(n)?;
        for k in names {
            if let Some(s) = ni.get(*k).and_then(lit) {
                return Some(s);
            }
        }
        id = names
            .iter()
            .find_map(|k| ni.get(*k).and_then(link_id))
            .or_else(|| ["anything", "any", "input", "input1", "value", "in"].iter().find_map(|k| ni.get(*k).and_then(link_id)))
            .or_else(|| {
                let links: Vec<String> = ni.values().filter_map(link_id).collect();
                if links.len() == 1 { links.into_iter().next() } else { None }
            })?;
    }
    None
}

/// The model file a loader names, literally or through a linked name node (easy ckptNames, Image Saver's literals).
fn model_name(g: &G, ins: &Map<String, Value>) -> Option<String> {
    MODEL_NAME_KEYS.iter().find_map(|k| {
        let v = ins.get(*k)?;
        match v {
            Value::String(s) => Some(s.clone()),
            _ if link_id(v).is_some() => value_of(g, ins, k, &[k, "ckpt_name", "unet_name", "model_name", "value", "string", "text"]),
            _ => None,
        }
    })
}

pub(crate) fn via<'a>(g: &'a G, ins: &Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    inputs(node(g, &link_id(ins.get(key)?)?)?)
}

/// A node that writes the picture out: SaveImage and every "...Save..." node (WAS Image Save, Image Saver, SaveVideo,
/// SaveWEBM, SaveAnimatedWEBP ...), Video Helper Suite's Video Combine, and Preview Image.
pub(crate) fn is_output(class: &str, ins: &Map<String, Value>) -> bool {
    (class.contains("Save") || class == "VHS_VideoCombine" || class == "PreviewImage")
        && ["images", "image", "video"].iter().any(|k| ins.contains_key(*k))
}

/// The input that names a save node's file: `filename_prefix`, or `filename` on Image Saver (with its folder in `path`).
pub(crate) fn save_name_key(class: &str, ins: &Map<String, Value>) -> Option<&'static str> {
    if !is_output(class, ins) {
        return None;
    }
    let is_str = |k: &str| ins.get(k).map(|v| v.is_string()).unwrap_or(false);
    if is_str("filename_prefix") {
        Some("filename_prefix")
    } else if class.starts_with("Image Saver") && is_str("filename") {
        Some("filename")
    } else {
        None
    }
}

pub(crate) fn find_save(g: &G, stem: &str) -> Option<String> {
    let mut saves: Vec<(&String, &Map<String, Value>)> = g
        .iter()
        .filter_map(|(id, v)| v.as_object().map(|n| (id, n)))
        .filter(|(_, n)| inputs(n).map(|i| is_output(class(n), i)).unwrap_or(false))
        .collect();
    if saves.is_empty() {
        return None;
    }
    // the save node whose prefix names this file, else a real Save over a Preview, else the highest id
    if let Some((id, _)) = saves.iter().find(|(_, n)| {
        inputs(n)
            .and_then(|i| save_name_key(class(n), i).and_then(|k| i.get(k)))
            .and_then(|v| v.as_str())
            .map(|p| {
                let last = p.rsplit(['/', '\\']).next().unwrap_or(p);
                !last.is_empty() && stem.starts_with(last)
            })
            .unwrap_or(false)
    }) {
        return Some((*id).clone());
    }
    saves.sort_by_key(|(id, n)| (class(n).contains("Save"), id.parse::<i64>().unwrap_or(0)));
    saves.last().map(|(id, _)| (*id).clone())
}

/// Every node upstream of `start`, breadth first (nearest first).
pub(crate) fn upstream(g: &G, start: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut q = VecDeque::from([start.to_string()]);
    while let Some(id) = q.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(ins) = node(g, &id).and_then(inputs) {
            for v in ins.values() {
                if let Some(src) = link_id(v) {
                    q.push_back(src);
                }
            }
        }
        out.push(id);
    }
    out
}

fn lora_named(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty() && !t.eq_ignore_ascii_case("none")
}

/// `<lora:name:weight>` tags in a prompt (Impact's Wildcard Encode and rgthree's Power Prompt apply them).
pub(crate) fn lora_tags(text: &str, out: &mut Vec<(String, String)>) {
    let mut rest = text;
    while let Some(i) = rest.find("<lora:") {
        let tail = &rest[i + 6..];
        let Some(end) = tail.find('>') else { break };
        let mut parts = tail[..end].split(':');
        let name = parts.next().unwrap_or("").trim();
        let w = parts.next().map(|w| w.trim()).filter(|w| w.parse::<f64>().is_ok()).unwrap_or("1");
        if lora_named(name) && w.parse::<f64>().map(|x| x != 0.0).unwrap_or(true) {
            out.push((base_name(name), w.to_string()));
        }
        rest = &tail[end + 1..];
    }
}

fn collect_loras(n: &Map<String, Value>, out: &mut Vec<(String, String)>) {
    let ins = match inputs(n) {
        Some(i) => i,
        None => return,
    };
    if let Some(name) = ins.get("lora_name").and_then(|v| v.as_str()).filter(|n| lora_named(n)) {
        let s = ["strength_model", "strength", "lora_strength", "lora_model_strength", "model_weight"]
            .iter()
            .find_map(|k| ins.get(*k).and_then(lit))
            .unwrap_or_else(|| "1".into());
        out.push((base_name(name), s));
    }
    // numbered slots: rgthree Power Lora Loader (lora_1 = {on, lora, strength}), rgthree Lora Loader Stack (lora_01 +
    // strength_01), Efficiency LoRA Stacker (lora_name_1 + lora_wt_1 or model_str_1, lora_count), Comfyroll LoRA Stack
    // (switch_1 + lora_name_1 + model_weight_1)
    let count = ins.get("lora_count").and_then(|v| v.as_u64());
    let simple = ins.get("input_mode").and_then(|v| v.as_str()) == Some("simple");
    let mut slots: Vec<(u32, &String, &str)> = ins
        .keys()
        .filter_map(|k| {
            let (num, kind) = if let Some(n) = k.strip_prefix("lora_name_") { (n, "name") } else if let Some(n) = k.strip_prefix("lora_") { (n, "lora") } else { return None };
            n_parse(num).map(|n| (n, k, kind))
        })
        .collect();
    slots.sort_by_key(|(n, _, _)| *n);
    for (num, k, kind) in slots {
        if count.map(|c| num as u64 > c).unwrap_or(false) {
            continue;
        }
        let digits = &k[k.len() - k.chars().rev().take_while(|c| c.is_ascii_digit()).count()..];
        match ins.get(k) {
            Some(Value::Object(o)) => {
                if o.get("on").and_then(|v| v.as_bool()) == Some(false) {
                    continue;
                }
                let num = |k: &str| o.get(k).and_then(|v| v.as_f64());
                if num("strength") == Some(0.0) && num("strengthTwo").unwrap_or(0.0) == 0.0 {
                    continue;
                }
                if let Some(name) = o.get("lora").and_then(|v| v.as_str()).filter(|n| lora_named(n)) {
                    let s = o.get("strength").and_then(lit).unwrap_or_else(|| "1".into());
                    out.push((base_name(name), s));
                }
            }
            Some(Value::String(name)) if lora_named(name) => {
                if ins.get(&format!("switch_{}", digits)).and_then(|v| v.as_str()).map(|s| s.eq_ignore_ascii_case("off")).unwrap_or(false) {
                    continue;
                }
                let order: &[&str] = if simple { &["lora_wt_", "model_str_"] } else { &["model_str_", "lora_wt_"] };
                let s = order
                    .iter()
                    .chain(["model_weight_", "strength_model_", "strength_", "lora_strength_", "weight_"].iter())
                    .find_map(|pre| ins.get(&format!("{}{}", pre, digits)).and_then(lit))
                    .unwrap_or_else(|| "1".into());
                let _ = kind;
                if s.parse::<f64>().map(|x| x == 0.0).unwrap_or(false) {
                    continue;
                }
                out.push((base_name(name), s));
            }
            _ => {}
        }
    }
}

fn n_parse(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

const MODEL_NAME_KEYS: &[&str] = &["ckpt_name", "unet_name", "gguf_name", "model_name", "model_path", "base_ckpt_name"];
/// Where a model chain continues upstream, nearest first.
const MODEL_LINK_KEYS: &[&str] = &["model", "opt_model", "base_model", "sdxl_tuple", "basic_pipe", "base_ctx", "ctx", "context", "pipe"];

fn model_chain(g: &G, mut id: Option<String>, m: &mut Meta) {
    let mut loras = Vec::new();
    let mut seen = HashSet::new();
    while let Some(cur) = id.take() {
        if !seen.insert(cur.clone()) {
            break;
        }
        let n = match node(g, &cur) {
            Some(n) => n,
            None => break,
        };
        let ins = match inputs(n) {
            Some(i) => i,
            None => break,
        };
        // this node's LoRAs in the order it applies them: its own (Efficient Loader's lora_name), then a lora_stack input
        let mut here = Vec::new();
        collect_loras(n, &mut here);
        lora_stack(g, ins, &mut here, 0);
        if let Some(t) = tag_text(n) {
            lora_tags(t, &mut here);
        }
        here.reverse();
        loras.extend(here);
        if let Some(name) = model_name(g, ins) {
            m.model = Some(base_name(&name));
            break;
        }
        id = MODEL_LINK_KEYS.iter().find_map(|k| ins.get(*k).and_then(link_id));
    }
    loras.reverse(); // loader first, as the chain runs
    m.loras = loras;
}

/// The LoRAs a `lora_stack` input brings, in the order they are applied: Efficiency's LoRA Stacker puts its own before the
/// stack it extends, Comfyroll's CR LoRA Stack after it.
fn lora_stack(g: &G, ins: &Map<String, Value>, out: &mut Vec<(String, String)>, depth: u32) {
    if depth > 8 {
        return;
    }
    let Some(src) = ins.get("lora_stack").and_then(link_id) else { return };
    let Some(n) = node(g, &src) else { return };
    let Some(si) = inputs(n) else { return };
    if class(n).starts_with("CR ") {
        lora_stack(g, si, out, depth + 1);
        collect_loras(n, out);
    } else {
        collect_loras(n, out);
        lora_stack(g, si, out, depth + 1);
    }
}

/// The prompt whose `<lora:...>` tags a node on the model chain applies: Impact's Wildcard Encode (populated_text, filled
/// in before queueing), rgthree's Power Prompt (only with both opt_model and opt_clip linked, and LoRAs not disabled).
fn tag_text(n: &Map<String, Value>) -> Option<&str> {
    let ins = inputs(n)?;
    let c = class(n);
    let t = if c.starts_with("ImpactWildcardEncode") {
        ins.get("populated_text").and_then(|v| v.as_str()).filter(|t| !t.trim().is_empty()).or_else(|| ins.get("wildcard_text").and_then(|v| v.as_str()))
    } else if c.contains("Power Prompt") {
        let linked = |k: &str| ins.get(k).and_then(link_id).is_some();
        let off = ins.get("insert_lora").and_then(|v| v.as_str()) == Some("DISABLE LORAS");
        if linked("opt_model") && linked("opt_clip") && !off { ins.get("prompt").and_then(|v| v.as_str()) } else { None }
    } else {
        None
    }?;
    Some(t).filter(|t| t.contains("<lora:"))
}

const TEXT_KEYS: &[&str] =
    &["text", "text_g", "prompt", "string", "value", "String", "positive", "populated_text", "wildcard_text", "text_positive", "texts"];
const NEG_TEXT_KEYS: &[&str] = &["negative", "text_negative"];
const COND_KEYS: &[&str] = &["conditioning", "conditioning_1", "conditioning_to", "conditioning_from", "cond", "positive", "base_positive"];
const NEG_COND_KEYS: &[&str] = &["negative", "base_negative"];
/// String joins: text_a .. text_d (WAS Text Concatenate), text1 / text2 (Comfyroll), string_1 / text_1 ...
fn join_part(k: &str) -> Option<u32> {
    for pre in ["string_", "text_", "text"] {
        if let Some(rest) = k.strip_prefix(pre) {
            if let Some(n) = n_parse(rest) {
                return Some(n);
            }
            if pre == "text_" && rest.len() == 1 && rest.as_bytes()[0].is_ascii_lowercase() {
                return Some((rest.as_bytes()[0] - b'a') as u32 + 1);
            }
        }
    }
    None
}

/// A prefix / suffix node's text around `s` (a space between them unless one is already there).
fn affix(ins: &Map<String, Value>, s: String) -> String {
    let join = |a: &str, b: &str| {
        if a.is_empty() || b.is_empty() || a.ends_with(char::is_whitespace) || b.starts_with(char::is_whitespace) { format!("{}{}", a, b) } else { format!("{} {}", a, b) }
    };
    let pre = ins.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
    let suf = ins.get("suffix").and_then(|v| v.as_str()).unwrap_or("");
    join(&join(pre, &s), suf)
}

/// The prompt text behind a conditioning (or string) link. `neg` asks for the negative side of a node that carries
/// both (Efficiency's loaders, rgthree's context, Impact's basic pipe, ControlNet apply).
fn text_from(g: &G, id: &str, neg: bool, depth: u32) -> Option<String> {
    text_at(g, id, 0, neg, depth)
}

/// The text behind a link to output `slot` of node `id`.
fn text_link(g: &G, v: &Value, neg: bool, depth: u32) -> Option<String> {
    let src = link_id(v)?;
    let slot = v.as_array().and_then(|a| a.get(1)).and_then(|s| s.as_u64()).unwrap_or(0);
    text_at(g, &src, slot, neg, depth)
}

fn text_at(g: &G, id: &str, slot: u64, neg: bool, depth: u32) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let n = node(g, id)?;
    if class(n).contains("ZeroOut") {
        return Some("(zeroed out)".into()); // a zeroed negative carries no text, whatever fed it
    }
    let ins = inputs(n)?;
    let carries = COND_KEYS.iter().chain(NEG_COND_KEYS).chain(BUNDLE_KEYS).chain(["clip", "model"].iter()).any(|k| ins.get(*k).and_then(link_id).is_some());
    if !carries && ["image", "images", "pixels"].iter().any(|k| ins.get(*k).and_then(link_id).is_some()) {
        return None; // a tagger or captioner: its text is made while the graph runs, and its own inputs are instructions
    }
    // a multi-output string node: output k is input text_<b + k - 1>
    if slot > 0 && slot < 24 && ins.contains_key("text") {
        let k = format!("text_{}", (b'a' + slot as u8) as char);
        if let Some(v) = ins.get(&k) {
            return v.as_str().map(|s| s.to_string()).or_else(|| text_link(g, v, neg, depth + 1));
        }
    }
    let has_neg = NEG_TEXT_KEYS.iter().chain(NEG_COND_KEYS.iter()).any(|k| ins.contains_key(*k));
    let polar = |k: &&str| -> bool { !(neg && has_neg && ["positive", "base_positive", "text_positive"].contains(k)) };
    // text on this node: a literal, or a linked string node
    let text_keys: Vec<&str> = if neg && has_neg { NEG_TEXT_KEYS.iter().chain(TEXT_KEYS.iter()).copied().filter(|k| polar(k)).collect() } else { TEXT_KEYS.to_vec() };
    for k in &text_keys {
        if let Some(v) = ins.get(*k) {
            if let Some(s) = v.as_str() {
                if !s.trim().is_empty() {
                    let mut s = s.to_string();
                    if *k == "texts" {
                        s = affix(ins, s);
                    }
                    if *k == "text_g" {
                        if let Some(l) = ins.get("text_l").and_then(|v| v.as_str()) {
                            if !l.trim().is_empty() && l != s {
                                s = format!("{}\n{}", s, l);
                            }
                        }
                    }
                    return Some(s);
                }
            } else if let Some(s) = text_link(g, v, neg, depth + 1) {
                return Some(if *k == "texts" { affix(ins, s) } else { s });
            }
        }
    }
    // a string join node
    let mut parts: Vec<(u32, String)> = Vec::new();
    for (k, v) in ins {
        if let Some(num) = join_part(k) {
            let s = v.as_str().map(|s| s.to_string()).or_else(|| text_link(g, v, false, depth + 1));
            if let Some(s) = s {
                if !s.trim().is_empty() {
                    parts.push((num, s));
                }
            }
        }
    }
    if !parts.is_empty() {
        parts.sort();
        let delim = ["delimiter", "separator"].iter().find_map(|k| ins.get(*k).and_then(|v| v.as_str())).unwrap_or(" ");
        return Some(parts.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join(delim));
    }
    // conditioning passed through (ControlNet apply, combine, area, a bundle ...)
    let cond_keys: Vec<&str> = if neg && has_neg {
        NEG_COND_KEYS.iter().chain(COND_KEYS.iter()).copied().filter(|k| polar(k)).collect()
    } else {
        COND_KEYS.to_vec()
    };
    for k in cond_keys.iter().copied().chain(BUNDLE_KEYS.iter().copied()) {
        if let Some(s) = ins.get(k).and_then(|v| text_link(g, v, neg, depth + 1)) {
            return Some(s);
        }
    }
    None
}

fn comfy_summary(g: &G, stem: &str, m: &mut Meta) {
    m.source = "comfyui".into();
    let save = find_save(g, stem);
    let ups: Vec<String> = match &save {
        Some(s) => upstream(g, s),
        None => g.keys().cloned().collect(),
    };
    let samplers: Vec<&String> = ups.iter().filter(|id| node(g, id).map(is_sampler).unwrap_or(false)).collect();
    m.passes = samplers.len();
    let sid = match samplers.first() {
        Some(s) => (*s).clone(),
        None => {
            fallback_summary(g, m);
            return;
        }
    };
    let s = node(g, &sid).unwrap();
    let ins = inputs(s).unwrap();
    let guider = via(g, ins, "guider");
    m.seed = value_of(g, ins, "seed", &["seed", "noise_seed", "value", "int"])
        .or_else(|| value_of(g, ins, "noise_seed", &["noise_seed", "seed", "value", "int"]))
        .or_else(|| value_of(g, ins, "noise", &["noise_seed", "seed"]));
    m.steps = value_of(g, ins, "steps", &["steps", "value", "int"]).or_else(|| value_of(g, ins, "sigmas", &["steps"]));
    m.cfg = value_of(g, ins, "cfg", &["cfg", "value", "float"]).or_else(|| guider.and_then(|gi| gi.get("cfg").and_then(lit)));
    m.sampler = value_of(g, ins, "sampler_name", &["sampler_name", "sampler"]).or_else(|| value_of(g, ins, "sampler", &["sampler_name", "sampler"]));
    m.scheduler = value_of(g, ins, "scheduler", &["scheduler"]).or_else(|| value_of(g, ins, "sigmas", &["scheduler"]));
    m.denoise = value_of(g, ins, "denoise", &["denoise", "value", "float"])
        .or_else(|| value_of(g, ins, "sigmas", &["denoise"]))
        .filter(|d| d.parse::<f64>().map(|x| x < 0.999).unwrap_or(false));
    let _ = guider;
    model_chain(g, model_source(g, ins), m);
    m.positive = cond_source(g, ins, false).and_then(|p| text_from(g, &p, false, 0)).unwrap_or_default();
    m.negative = cond_source(g, ins, true).and_then(|p| text_from(g, &p, true, 0)).unwrap_or_default();
    // the latent size: follow the first pass's latent input back to an empty latent with a width and height
    let first = samplers.last().map(|s| (*s).clone()).unwrap_or(sid);
    let mut cur = node(g, &first).and_then(inputs).and_then(|i| i.get("latent_image").or_else(|| i.get("latent")).and_then(link_id));
    for _ in 0..6 {
        let i = match cur.as_deref().and_then(|id| node(g, id)).and_then(inputs) {
            Some(i) => i,
            None => break,
        };
        let num = |k: &str| value_of(g, i, k, &[k, "value", "int", "size"]).and_then(|x| x.parse::<u64>().ok());
        let wh = [("width", "height"), ("empty_latent_width", "empty_latent_height")].iter().find_map(|(a, b)| Some((num(a)?, num(b)?)));
        if let Some((w, h)) = wh {
            m.size = Some(format!("{}x{}", w, h));
            break;
        }
        cur = ["samples", "latent", "latent_image", "samples_from"].iter().find_map(|k| i.get(*k).and_then(link_id));
    }
}

/// Graphs without a recognisable save -> sampler path: the old viewer's heuristics.
fn fallback_summary(g: &G, m: &mut Meta) {
    let mut texts: Vec<String> = Vec::new();
    let mut ids: Vec<&String> = g.keys().collect();
    ids.sort_by_key(|id| id.parse::<i64>().unwrap_or(0));
    let mut loras = Vec::new();
    for id in ids {
        let n = match node(g, id) {
            Some(n) => n,
            None => continue,
        };
        let ins = match inputs(n) {
            Some(i) => i,
            None => continue,
        };
        let c = class(n);
        collect_loras(n, &mut loras);
        if c.starts_with("KSampler") || is_sampler(n) {
            m.seed = m.seed.take().or_else(|| ins.get("seed").or_else(|| ins.get("noise_seed")).and_then(lit));
            m.steps = m.steps.take().or_else(|| ins.get("steps").and_then(lit));
            m.cfg = m.cfg.take().or_else(|| ins.get("cfg").and_then(lit));
            m.sampler = m.sampler.take().or_else(|| ins.get("sampler_name").and_then(lit));
            m.scheduler = m.scheduler.take().or_else(|| ins.get("scheduler").and_then(lit));
        }
        if m.model.is_none() {
            // checkpoint-style names only: an upscaler's or detector's model_name is not the model that made the picture
            if let Some(name) = ["ckpt_name", "unet_name", "gguf_name", "base_ckpt_name"].iter().find_map(|k| ins.get(*k).and_then(|v| v.as_str())) {
                m.model = Some(base_name(name));
            }
        }
        if c.starts_with("CLIPTextEncode") {
            if let Some(t) = ins.get("text").and_then(|v| v.as_str()) {
                texts.push(t.to_string());
            }
        }
    }
    m.loras = loras;
    m.positive = texts.into_iter().max_by_key(|t| t.len()).unwrap_or_default();
}

// ---- A1111 / Forge and SwarmUI

/// "Key: value, Key: "quoted, value", ..." -> pairs.
pub fn parse_settings_line(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut parts = Vec::new();
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                cur.push(ch)
            }
            ',' if !quoted => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    for p in parts {
        if let Some((k, v)) = p.split_once(':') {
            out.push((k.trim().to_string(), v.trim().trim_matches('"').to_string()));
        }
    }
    out
}

fn a1111_summary(params: &str, m: &mut Meta) {
    if m.source.is_empty() {
        m.source = "a1111".into();
    }
    let lines: Vec<&str> = params.lines().collect();
    let steps_at = lines.iter().rposition(|l| l.starts_with("Steps:"));
    let neg_at = lines.iter().position(|l| l.starts_with("Negative prompt:"));
    let body_end = neg_at.or(steps_at).unwrap_or(lines.len());
    let positive = lines[..body_end].join("\n");
    let mut loras = Vec::new();
    let mut rest = positive.as_str();
    while let Some(i) = rest.find("<lora:") {
        let tail = &rest[i + 6..];
        if let Some(j) = tail.find('>') {
            let mut it = tail[..j].splitn(2, ':');
            let name = it.next().unwrap_or("").to_string();
            let w = it.next().unwrap_or("1").split(':').next().unwrap_or("1").to_string();
            loras.push((name, w));
            rest = &tail[j + 1..];
        } else {
            break;
        }
    }
    let stripped = {
        let mut s = String::new();
        let mut r = positive.as_str();
        while let Some(i) = r.find("<lora:") {
            s.push_str(&r[..i]);
            match r[i..].find('>') {
                Some(j) => r = &r[i + j + 1..],
                None => {
                    r = "";
                }
            }
        }
        s.push_str(r);
        s.trim().to_string()
    };
    if m.positive.is_empty() {
        m.positive = stripped;
    }
    if m.negative.is_empty() {
        if let Some(n) = neg_at {
            let end = steps_at.filter(|s| *s > n).unwrap_or(lines.len());
            m.negative = lines[n..end].join("\n").trim_start_matches("Negative prompt:").trim().to_string();
        }
    }
    if m.loras.is_empty() {
        m.loras = loras;
    }
    if let Some(s) = steps_at {
        let kv = parse_settings_line(lines[s]);
        let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone());
        m.steps = m.steps.take().or_else(|| get("Steps"));
        m.sampler = m.sampler.take().or_else(|| get("Sampler"));
        m.scheduler = m.scheduler.take().or_else(|| get("Schedule type"));
        m.cfg = m.cfg.take().or_else(|| get("CFG scale"));
        m.seed = m.seed.take().or_else(|| get("Seed"));
        m.size = m.size.take().or_else(|| get("Size"));
        m.model = m.model.take().or_else(|| get("Model"));
        m.denoise = m.denoise.take().or_else(|| get("Denoising strength"));
    }
}

fn swarm_summary(sp: &Map<String, Value>, m: &mut Meta) {
    m.source = "swarmui".into();
    let s = |k: &str| sp.get(k).and_then(lit);
    m.positive = s("prompt").unwrap_or_default();
    m.negative = s("negativeprompt").unwrap_or_default();
    m.model = s("model").map(|x| base_name(&x));
    m.seed = s("seed");
    m.steps = s("steps");
    m.cfg = s("cfgscale");
    m.sampler = s("sampler");
    m.scheduler = s("scheduler");
    if let (Some(w), Some(h)) = (s("width"), s("height")) {
        m.size = Some(format!("{}x{}", w, h));
    }
    if let Some(Value::Array(ls)) = sp.get("loras") {
        let ws = sp.get("loraweights").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        m.loras = ls
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_str().map(|n| (base_name(n), ws.get(i).and_then(lit).unwrap_or_else(|| "1".into()))))
            .collect();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const BASIC: &str = r#"{
      "4": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "models\\sdxl_base.safetensors"}},
      "10": {"class_type": "LoraLoader", "inputs": {"lora_name": "styleA.safetensors", "strength_model": 0.8, "strength_clip": 1.0, "model": ["4", 0], "clip": ["4", 1]}},
      "11": {"class_type": "LoraLoaderModelOnly", "inputs": {"lora_name": "detail.safetensors", "strength_model": 2.0, "model": ["10", 0]}},
      "6": {"class_type": "CLIPTextEncode", "inputs": {"text": "a red fox in snow", "clip": ["10", 1]}},
      "7": {"class_type": "CLIPTextEncode", "inputs": {"text": "blurry", "clip": ["10", 1]}},
      "17": {"class_type": "ConditioningZeroOut", "inputs": {"conditioning": ["6", 0]}},
      "5": {"class_type": "EmptyLatentImage", "inputs": {"width": 896, "height": 1152, "batch_size": 1}},
      "3": {"class_type": "KSampler", "inputs": {"seed": 1234, "steps": 8, "cfg": 1.0, "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0, "model": ["11", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["5", 0]}},
      "8": {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["4", 2]}},
      "9": {"class_type": "SaveImage", "inputs": {"filename_prefix": "tests/fox", "images": ["8", 0]}}
    }"#;

    pub const CUSTOM: &str = r#"{
      "1": {"class_type": "UnetLoaderGGUF", "inputs": {"unet_name": "flux-dev-Q4.gguf"}},
      "2": {"class_type": "RandomNoise", "inputs": {"noise_seed": 77}},
      "3": {"class_type": "KSamplerSelect", "inputs": {"sampler_name": "er_sde"}},
      "4": {"class_type": "BasicScheduler", "inputs": {"scheduler": "beta", "steps": 10, "denoise": 1.0, "model": ["1", 0]}},
      "5": {"class_type": "CLIPTextEncode", "inputs": {"text": ["12", 0], "clip": ["9", 0]}},
      "12": {"class_type": "LC Join Strings", "inputs": {"string_1": "a city", "string_2": "at dusk", "delimiter": ", "}},
      "6": {"class_type": "CFGGuider", "inputs": {"cfg": 3.5, "model": ["1", 0], "positive": ["5", 0], "negative": ["5", 0]}},
      "7": {"class_type": "EmptySD3LatentImage", "inputs": {"width": 1088, "height": 1920, "batch_size": 1}},
      "8": {"class_type": "SamplerCustomAdvanced", "inputs": {"noise": ["2", 0], "guider": ["6", 0], "sampler": ["3", 0], "sigmas": ["4", 0], "latent_image": ["7", 0]}},
      "9": {"class_type": "DualCLIPLoader", "inputs": {"clip_name1": "a", "clip_name2": "b"}},
      "10": {"class_type": "VAEDecode", "inputs": {"samples": ["8", 0], "vae": ["11", 0]}},
      "13": {"class_type": "SaveImage", "inputs": {"filename_prefix": "city", "images": ["10", 0]}}
    }"#;

    #[test]
    fn comfy_basic_walk() {
        let g: G = serde_json::from_str(BASIC).unwrap();
        let mut m = Meta::default();
        comfy_summary(&g, "fox_00001_", &mut m);
        assert_eq!(m.model.as_deref(), Some("sdxl_base"));
        assert_eq!(m.loras, vec![("styleA".into(), "0.8".into()), ("detail".into(), "2.0".into())]);
        assert_eq!((m.seed.as_deref(), m.steps.as_deref(), m.cfg.as_deref()), (Some("1234"), Some("8"), Some("1.0")));
        assert_eq!((m.sampler.as_deref(), m.scheduler.as_deref()), (Some("euler"), Some("simple")));
        assert_eq!(m.denoise, None);
        assert_eq!(m.size.as_deref(), Some("896x1152"));
        assert_eq!((m.positive.as_str(), m.negative.as_str()), ("a red fox in snow", "blurry"));
        assert_eq!(m.passes, 1);
        assert_eq!(text_from(&g, "17", true, 0).as_deref(), Some("(zeroed out)"), "a zeroed negative is not the positive");
    }

    #[test]
    fn comfy_custom_advanced() {
        let g: G = serde_json::from_str(CUSTOM).unwrap();
        let mut m = Meta::default();
        comfy_summary(&g, "city_00001_", &mut m);
        assert_eq!(m.model.as_deref(), Some("flux-dev-Q4"));
        assert_eq!(m.seed.as_deref(), Some("77"));
        assert_eq!((m.steps.as_deref(), m.cfg.as_deref()), (Some("10"), Some("3.5")));
        assert_eq!((m.sampler.as_deref(), m.scheduler.as_deref()), (Some("er_sde"), Some("beta")));
        assert_eq!(m.positive, "a city, at dusk");
        assert_eq!(m.size.as_deref(), Some("1088x1920"));
    }

    #[test]
    fn a1111_block() {
        let p = "a castle <lora:mossy:0.7> at night\nNegative prompt: lowres, bad\nSteps: 20, Sampler: DPM++ 2M, Schedule type: Karras, CFG scale: 5, Seed: 42, Size: 832x1216, Model: juggernaut, Lora hashes: \"mossy: abc, x: y\"";
        let mut m = Meta::default();
        a1111_summary(p, &mut m);
        assert_eq!(m.positive, "a castle  at night");
        assert_eq!(m.negative, "lowres, bad");
        assert_eq!(m.loras, vec![("mossy".into(), "0.7".into())]);
        assert_eq!((m.steps.as_deref(), m.sampler.as_deref(), m.scheduler.as_deref()), (Some("20"), Some("DPM++ 2M"), Some("Karras")));
        assert_eq!((m.seed.as_deref(), m.size.as_deref(), m.model.as_deref()), (Some("42"), Some("832x1216"), Some("juggernaut")));
    }

    fn png_with(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut b = vec![137, 80, 78, 71, 13, 10, 26, 10];
        for (t, d) in chunks {
            b.extend((d.len() as u32).to_be_bytes());
            b.extend(t.iter());
            b.extend(d);
            b.extend([0, 0, 0, 0]);
        }
        b.extend([0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
        b
    }

    #[test]
    fn png_text_chunks() {
        let d = std::env::temp_dir().join(format!("loupe-meta-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("fox_00001_.png");
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut z, "caf\u{e9} \u{1F98A}".as_bytes()).unwrap();
        let mut itxt = b"note\0\x01\0en\0\0".to_vec();
        itxt.extend(z.finish().unwrap());
        let bytes = png_with(&[
            (b"IHDR", vec![0; 13]),
            (b"tEXt", [b"prompt\0".as_slice(), BASIC.as_bytes()].concat()),
            (b"IDAT", vec![1, 2, 3]),
            (b"iTXt", itxt),
        ]);
        std::fs::write(&p, bytes).unwrap();
        let c = read_chunks(&p);
        assert_eq!(c.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), vec!["prompt", "note"]);
        assert_eq!(chunk(&c, "note"), Some("caf\u{e9} \u{1F98A}"));
        let m = summarize(&p);
        assert_eq!(m.source, "comfyui");
        assert!(m.can_reroll);
        assert_eq!(m.positive, "a red fox in snow");
    }

    #[test]
    fn exif_user_comment_and_comfy_webp() {
        // big-endian TIFF: IFD0 with Model "prompt:{...}" and an Exif IFD with a UNICODE UserComment
        let model = b"prompt:{\"1\":{}}\0".to_vec();
        let comment: Vec<u8> = [b"UNICODE\0".as_slice(), &"hi there".encode_utf16().flat_map(|u| u.to_be_bytes()).collect::<Vec<u8>>()].concat();
        let mut t = b"MM\0\x2a\0\0\0\x08".to_vec();
        // IFD0 at 8: 2 entries
        let ifd0_len = 2 + 2 * 12 + 4;
        let exif_ifd_off = 8 + ifd0_len;
        let exif_ifd_len = 2 + 12 + 4;
        let model_off = exif_ifd_off + exif_ifd_len;
        let comment_off = model_off + model.len();
        t.extend(2u16.to_be_bytes());
        t.extend(0x0110u16.to_be_bytes());
        t.extend(2u16.to_be_bytes());
        t.extend((model.len() as u32).to_be_bytes());
        t.extend((model_off as u32).to_be_bytes());
        t.extend(0x8769u16.to_be_bytes());
        t.extend(4u16.to_be_bytes());
        t.extend(1u32.to_be_bytes());
        t.extend((exif_ifd_off as u32).to_be_bytes());
        t.extend(0u32.to_be_bytes());
        t.extend(1u16.to_be_bytes());
        t.extend(0x9286u16.to_be_bytes());
        t.extend(7u16.to_be_bytes());
        t.extend((comment.len() as u32).to_be_bytes());
        t.extend((comment_off as u32).to_be_bytes());
        t.extend(0u32.to_be_bytes());
        t.extend(&model);
        t.extend(&comment);
        let f = exif_fields(&t);
        assert_eq!(chunk(&f, "prompt"), Some("{\"1\":{}}"));
        assert_eq!(chunk(&f, "parameters"), Some("hi there"));
    }

    #[test]
    fn graphs_inside_videos_and_animated_pngs() {
        // made with ffmpeg the way ComfyUI's SaveVideo / SaveWEBM and Video Helper Suite write them: prompt / workflow keys
        // (mp4 QuickTime keys + ilst under use_metadata_tags; Matroska PROMPT / WORKFLOW), and SaveAnimatedPNG's comf chunks
        let d = std::env::temp_dir().join(format!("loupe-video-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for (name, bytes) in [
            ("keys.mp4", &include_bytes!("../tests/fixtures/keys.mp4")[..]),
            ("tags.webm", &include_bytes!("../tests/fixtures/tags.webm")[..]),
            ("comf-chunks.png", &include_bytes!("../tests/fixtures/comf-chunks.png")[..]),
        ] {
            let p = d.join(name);
            std::fs::write(&p, bytes).unwrap();
            let c = read_chunks(&p);
            assert!(chunk(&c, "workflow").map(|w| w.contains("last_node_id")).unwrap_or(false), "{}: workflow in {:?}", name, c.iter().map(|x| &x.0).collect::<Vec<_>>());
            let m = summarize(&p);
            assert_eq!((m.model.as_deref(), m.seed.as_deref(), m.positive.as_str()), (Some("wan"), Some("4242"), "a red kite over dunes"), "{}", name);
            assert!(m.can_reroll, "{}", name);
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
pub(crate) mod public {
    /// The API-format test workflows in tests/fixtures/public (see SOURCES.md): five from MIT / Apache-2.0 projects, seven written
    /// for BlinkLoupe, by short name.
    pub fn all() -> Vec<(&'static str, super::G)> {
        let raw: [(&str, &str); 12] = [
            ("eff-sdxl", include_str!("../tests/fixtures/public/efficiency-sdxl+imagesaver+was_curiousjp-img2tag2img.json")),
            ("eff-adv", include_str!("../tests/fixtures/public/efficiency-adv+lorastacker+was-imagesave_own.json")),
            ("eff-ks", include_str!("../tests/fixtures/public/efficiency-ksampler+instantid+ipadapter_own.json")),
            ("impact-pipe", include_str!("../tests/fixtures/public/impact-basicpipe+efficiency_charlweed-gimp-comfyui.json")),
            ("imagesaver-wildcard", include_str!("../tests/fixtures/public/imagesaver+impact-wildcard+rgthree-seed_own.json")),
            ("imagesaver-simple", include_str!("../tests/fixtures/public/imagesaver-simple+rgthree-powerlora+subgraph_own.json")),
            ("powerlora", include_str!("../tests/fixtures/public/rgthree-powerlora-lora1+addtextprefix_own.json")),
            ("cr-stack", include_str!("../tests/fixtures/public/comfyroll-lorastack+searge-sdxl_own.json")),
            ("vhs-mochi", include_str!("../tests/fixtures/public/vhs-videocombine+core-saveanimatedwebp_fofr-cog-comfyui-mochi.json")),
            ("vhs-load", include_str!("../tests/fixtures/public/vhs-loadvideo+videocombine_fofr-cog-comfyui-flashvsr.json")),
            ("savevideo-old", include_str!("../tests/fixtures/public/core-savevideo-oldformat+samplercustom_own.json")),
            ("savevideo-new", include_str!("../tests/fixtures/public/core-savevideo-dynamiccombo_eliovp-paiton-wan22.json")),
        ];
        raw.iter()
            .map(|(n, t)| {
                let v: serde_json::Value = serde_json::from_str(t).unwrap();
                let v = v.get("prompt").filter(|p| p.is_object()).cloned().unwrap_or(v);
                (*n, v.as_object().unwrap().clone())
            })
            .collect()
    }

    /// Links in `g` that point at no node.
    pub fn dangling(g: &super::G) -> Vec<String> {
        let mut bad = Vec::new();
        for (id, n) in g {
            for (k, v) in n.get("inputs").and_then(|i| i.as_object()).into_iter().flatten() {
                if let Some(src) = super::link_id(v) {
                    if !g.contains_key(&src) {
                        bad.push(format!("{}.{} -> {}", id, k, src));
                    }
                }
            }
        }
        bad
    }

    fn read(name: &str) -> super::Meta {
        let g = all().into_iter().find(|(n, _)| *n == name).unwrap().1;
        super::summarize_chunks(&[("prompt".into(), serde_json::to_string(&g).unwrap())], "")
    }

    #[test]
    fn public_workflows_read() {
        // (name, model, loras, seed, steps, cfg, sampler, scheduler, size, positive starts, negative starts)
        type Row = (&'static str, Option<&'static str>, Vec<(&'static str, &'static str)>, Option<&'static str>, Option<&'static str>, Option<&'static str>, Option<&'static str>, Option<&'static str>, Option<&'static str>, &'static str, &'static str);
        let rows: Vec<Row> = vec![
            // Eff. Loader SDXL -> Unpack -> FreeU -> Pack SDXL Tuple -> KSampler SDXL (Eff.); seed from WAS Seed slot 3; the
            // prompt is written by a tagger and a LLaVA captioner while the graph runs, so the graph holds none of it
            ("eff-sdxl", Some("checkpoint"), vec![], Some("1"), Some("25"), Some("7"), Some("euler_ancestral"), Some("normal"), Some("1024x1024"), "", ""),
            // Efficient Loader (lora_name None) + LoRA Stacker (simple, lora_count 5, one named) -> KSampler Adv. (Efficient);
            // prompts joined by WAS Text Concatenate from ttN text pieces
            ("eff-adv", Some("mountainScapeMix_v3"), vec![("inkWashTexture_v2", "1")], Some("512093847710245"), Some("6"), Some("2"), Some("lcm"), Some("sgm_uniform"), Some("640x896"), "misty pine ridge", "blurry, oversaturated"),
            // KSampler (Efficient) through InstantID / IPAdapter conditioning
            ("eff-ks", Some("crispSketchXL_v2"), vec![("linocutPrint_v1", "1")], Some("41720394857712"), Some("24"), Some("6"), Some("dpmpp_2m"), Some("karras"), Some("1024x1024"), "Linocut portrait", "photo, soft focus"),
            // ImpactKSamplerAdvancedBasicPipe <- ToBasicPipe / FromBasicPipe_v2
            ("impact-pipe", Some("flux1-dev"), vec![], Some("197"), Some("20"), Some("8"), Some("euler"), Some("normal"), Some("1024x1024"), "photograph of a street where women ride scooters", "car, cars, autos, bicycles"),
            // ImpactWildcardEncode x2, easy ckptNames, easy showAnything -> Image Saver selectors, easy int sizes
            ("imagesaver-wildcard", Some("studioPortraitXL_v3"), vec![], Some("-1"), Some("30"), Some("5.5"), Some("dpmpp_2m"), Some("karras"), Some("896x1152"), "high detail, natural window light", "lowres, jpeg artifacts"),
            // subgraph ids ("40:6"), an empty Power Lora Loader, an empty prompt, ConditioningZeroOut
            ("imagesaver-simple", Some("openFieldKrea2_Q4_0"), vec![], Some("-1"), Some("9"), Some("1"), Some("euler"), Some("beta"), Some("1024x1536"), "", "(zeroed out)"),
            // Power Lora Loader lora_1, AddTextPrefix(prefix + texts <- String Literal)
            ("powerlora", Some("pixelForge-aio"), vec![("retroPixelIcons_v2", "1")], Some("908172635401928"), Some("20"), Some("3"), Some("euler"), Some("simple"), Some("512x512"), "pixicon style: clean game icon, flat colours, white background. A brass key", "(((text, watermark)))"),
            // CR LoRA Stack (third slot Off) -> CR Apply LoRA Stack; SeargeSDXLSampler's base_model / base_positive
            ("cr-stack", Some("harborMix_v2"), vec![("watercolorWash_v1", "1"), ("paperGrain_v3", "1")], Some("230948571230498"), Some("40"), Some("8"), Some("dpmpp_2m"), Some("karras"), None, "watercolour harbour at noon", "photo, hard edges"),
            ("vhs-mochi", Some("mochi_preview_bf16"), vec![], Some("828710630039845"), Some("30"), Some("4.5"), Some("euler"), Some("simple"), Some("848x480"), "a fox moving quickly", ""),
            // SamplerCustomAdvanced + BasicGuider; an empty Power Lora Loader after LoraLoaderModelOnly; SaveVideo
            ("savevideo-old", Some("tideMotion_i2v_bf16"), vec![("tideMotion_fast_6step_bf16", "0.8")], Some("1234"), Some("6"), None, Some("euler"), Some("simple"), Some("640x640"), "", ""),
        ];
        for (name, model, loras, seed, steps, cfg, sampler, sched, size, pos, neg) in rows {
            let m = read(name);
            assert_eq!(m.model.as_deref(), model, "{name} model");
            let l: Vec<(String, String)> = loras.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
            assert_eq!(m.loras, l, "{name} loras");
            assert_eq!((m.seed.as_deref(), m.steps.as_deref(), m.cfg.as_deref()), (seed, steps, cfg), "{name} seed / steps / cfg");
            assert_eq!((m.sampler.as_deref(), m.scheduler.as_deref(), m.size.as_deref()), (sampler, sched, size), "{name} sampler / scheduler / size");
            assert!(if pos.is_empty() { m.positive.is_empty() } else { m.positive.starts_with(pos) }, "{name} positive: {:?}", m.positive);
            assert!(if neg.is_empty() { m.negative.is_empty() } else { m.negative.starts_with(neg) }, "{name} negative: {:?}", m.negative);
            assert!(m.can_reroll, "{name} re-rolls");
        }
        assert_eq!(read("cr-stack").denoise.as_deref(), Some("0.6"));
        assert_eq!(read("impact-pipe").passes, 2);
    }

    #[test]
    fn public_workflows_reroll_names_every_saver() {
        // (name, seeds changed, node id, the input that takes the suffix)
        for (name, seeds, save, key) in [
            ("eff-sdxl", 1, "save_image", "filename"),             // Image Saver
            ("eff-adv", 1, "51", "filename_prefix"),               // WAS Image Save
            ("imagesaver-simple", 1, "212", "filename"),           // Image Saver Simple
            ("vhs-mochi", 1, "28", "filename_prefix"),             // SaveAnimatedWEBP
            ("vhs-load", 0, "40", "filename_prefix"),              // VHS_VideoCombine
            ("savevideo-new", 1, "10", "filename_prefix"),         // SaveVideo (dotted format keys)
        ] {
            let mut g = all().into_iter().find(|(n, _)| *n == name).unwrap().1;
            let before = g[save]["inputs"][key].as_str().unwrap().to_string();
            assert_eq!(crate::comfy::reroll_graph(&mut g, "_reroll"), seeds, "{name}");
            assert_eq!(g[save]["inputs"][key].as_str().unwrap(), format!("{}_reroll", before), "{name}");
        }
        // the VHS combine that only previews (save_output false) is a saver too; both of its names take the suffix
        let mut g = all().into_iter().find(|(n, _)| *n == "vhs-mochi").unwrap().1;
        crate::comfy::reroll_graph(&mut g, "_reroll");
        let vhs = g.values().find(|n| n["class_type"] == "VHS_VideoCombine").unwrap();
        assert!(vhs["inputs"]["filename_prefix"].as_str().unwrap().ends_with("_reroll"));
    }

    #[test]
    fn public_workflows_hires_and_send() {
        let o = crate::hires::Opts { scale: 2.0, max_mp: 4.2, denoise: 0.3, steps: 0, upscale_model: String::new(), suffix: "_hires".into(), folder_suffix: "-hires".into() };
        for (name, g) in all() {
            let r = crate::hires::build(&g, "", (1024, 1024), "loupe/x.png", &o, &|_| None);
            let video = ["vhs-mochi", "vhs-load", "savevideo-old", "savevideo-new"].contains(&name);
            if video {
                assert!(r.err().unwrap().contains("video"), "{name}: hires refuses a video graph");
                continue;
            }
            let p = r.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(dangling(&p.graph).is_empty(), "{name}: {:?}", dangling(&p.graph));
            let ks = p.graph.values().filter(|n| n["class_type"] == "KSampler" && n["inputs"]["denoise"] == serde_json::json!(0.3)).count();
            assert_eq!(ks, 1, "{name}: one finishing pass");
            let added = |c: &str| p.graph.iter().any(|(id, n)| !g.contains_key(id) && n["class_type"] == c);
            assert_eq!(added("Unpack SDXL Tuple"), name == "eff-sdxl", "{name}");
            assert_eq!(added("FromBasicPipe"), name == "impact-pipe", "{name}");
        }
        // Image Saver: the file name pattern takes the suffix and its path the folder suffix; a path linked to a string
        // node is copied in, metadata linked to the base image (width / height from an image-info node) is dropped
        let g = all().into_iter().find(|(n, _)| *n == "eff-sdxl").unwrap().1;
        let p = crate::hires::build(&g, "", (1024, 1024), "loupe/x.png", &o, &|_| None).unwrap();
        let sv = &p.graph["save_image"]["inputs"];
        assert_eq!(sv["filename"], serde_json::json!("%time_%basemodelname_%seed_hires"));
        assert!(sv.get("width").is_none() && sv.get("path").is_some() && sv.get("seed_value").is_some(), "{sv}");

        let pick = |name: &str, m| crate::send::targets(&all().into_iter().find(|(n, _)| *n == name).unwrap().1, m).map(|t| t.into_iter().map(|t| t.class).collect::<Vec<_>>());
        use crate::send::Media::{Image, Video};
        assert_eq!(pick("eff-sdxl", Image).unwrap(), vec!["LoadImage //Inspire"]);
        assert_eq!(pick("eff-ks", Image).unwrap(), vec!["LoadImage"]);
        assert_eq!(pick("vhs-load", Video).unwrap(), vec!["VHS_LoadVideo"]);
        assert!(pick("vhs-load", Image).unwrap_err().contains("loads only videos"));
        assert!(pick("powerlora", Image).unwrap_err().contains("no Load Image"));
    }

    #[test]
    #[ignore]
    fn dump_hires_send() {
        let o = crate::hires::Opts { scale: 2.0, max_mp: 4.2, denoise: 0.3, steps: 0, upscale_model: String::new(), suffix: "_hires".into(), folder_suffix: String::new() };
        for (name, g) in all() {
            match crate::hires::build(&g, "", (1024, 1024), "loupe/x.png", &o, &|_| None) {
                Ok(p) => {
                    let added: Vec<String> = p.graph.iter().filter(|(id, _)| !g.contains_key(*id)).map(|(_, n)| n["class_type"].as_str().unwrap_or("").to_string()).collect();
                    println!("== {name}: hires ok, {} nodes, added {:?}, dangling {:?}", p.graph.len(), added, dangling(&p.graph));
                }
                Err(e) => println!("== {name}: hires ERR {e}"),
            }
            let t = |m| crate::send::targets(&g, m).map(|t| t.iter().map(|t| format!("#{} {}", t.id, t.class)).collect::<Vec<_>>());
            println!("   send image {:?} | video {:?}", t(crate::send::Media::Image), t(crate::send::Media::Video));
        }
    }

    #[test]
    #[ignore]
    fn dump() {
        for (name, g) in all() {
            let m = super::summarize_chunks(&[("prompt".into(), serde_json::to_string(&g).unwrap())], "");
            let save = super::find_save(&g, "");
            let cut = |s: &str| s.chars().take(70).collect::<String>().replace('\n', " ");
            println!("== {name}: save {:?} {:?}", save, save.as_ref().and_then(|s| g.get(s)).and_then(|n| n.get("class_type")));
            println!("   model {:?} loras {:?}", m.model, m.loras);
            println!("   seed {:?} steps {:?} cfg {:?} sampler {:?} sched {:?} denoise {:?} size {:?} passes {} seeds {}", m.seed, m.steps, m.cfg, m.sampler, m.scheduler, m.denoise, m.size, m.passes, crate::comfy::count_seeds(&g));
            println!("   + {}", cut(&m.positive));
            println!("   - {}", cut(&m.negative));
        }
    }
}

#[cfg(test)]
mod regress {
    /// Prints one line per image under $LOUPE_DUMP_DIR (model, LoRAs, sampler fields, size, prompt hashes, seed slots), for
    /// diffing two versions of the walk over the same files without reading the prompts.
    #[test]
    #[ignore]
    fn dump_dir() {
        use std::hash::{Hash, Hasher};
        let Some(dir) = std::env::var_os("LOUPE_DUMP_DIR") else { return };
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().map(|x| x.eq_ignore_ascii_case("png")).unwrap_or(false) {
                    files.push(p);
                }
            }
        }
        files.sort();
        let h = |s: &str| {
            let mut x = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut x);
            format!("{:08x}", x.finish() as u32)
        };
        let o = crate::hires::Opts { scale: 2.0, max_mp: 4.2, denoise: 0.3, steps: 0, upscale_model: String::new(), suffix: "_hires".into(), folder_suffix: "-hires".into() };
        for p in files {
            let chunks = super::read_chunks(&p);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let hires = match super::chunk(&chunks, "prompt").and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()) {
                Some(serde_json::Value::Object(g)) => match crate::hires::build(&g, &stem, (1024, 1024), "loupe/x.png", &o, &|_| None) {
                    Ok(plan) => {
                        if std::env::var("LOUPE_DUMP_GRAPH").map(|f| p.to_string_lossy().contains(&f)).unwrap_or(false) {
                            println!("GRAPH {}", serde_json::to_string_pretty(&plan.graph).unwrap());
                        }
                        format!("hires {} dangling {}", h(&serde_json::to_string(&plan.graph).unwrap()), super::public::dangling(&plan.graph).len())
                    }
                    Err(e) => format!("hires ERR {}", e),
                },
                _ => "no graph".into(),
            };
            println!("{} | {}", p.display(), hires);
            let m = super::summarize(&p);
            println!(
                "{} | {:?} {:?} | {:?} {:?} {:?} {:?} {:?} {:?} {:?} | {} | +{} -{} | reroll {}",
                p.display(), m.model, m.loras, m.seed, m.steps, m.cfg, m.sampler, m.scheduler, m.denoise, m.size, m.passes, h(&m.positive), h(&m.negative), m.can_reroll
            );
        }
    }
}
