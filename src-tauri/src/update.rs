//! Updates (2026-10-14, phase 4 of making Loupe, now BlinkLoupe, universal). Each release carries a small `latest.json`: the version, a
//! line of notes, the installer's address, its size and SHA-256. Loupe reads it at most once a day (Settings > Updates
//! can turn that off), sends nothing but the request, and an installed copy can download the installer, check its size and
//! checksum, and run it silently (`/S /R`: install over this copy, then start Loupe again).

use crate::http;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The newest release's manifest: GitHub sends /releases/latest/download/<asset> to the newest release's copy.
pub const FEED: &str = "https://github.com/mifunetron/blinkloupe-releases/releases/latest/download/latest.json";
/// Where a person downloads Loupe by hand.
pub const PAGE: &str = "https://github.com/mifunetron/blinkloupe-releases/releases/latest";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How old a check may be before Loupe checks again.
const DAY: u64 = 20 * 3600;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Manifest {
    pub version: String,
    pub notes: String,
    /// the installer
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// the release's page, for a copy that can't update itself
    pub page: String,
}

/// What the last check found, kept in %APPDATA%\Loupe\update.json (not in settings.json, so a check never races a save).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct State {
    /// unix seconds of the last check
    pub checked: u64,
    pub latest: Option<Manifest>,
    pub error: String,
}

fn state_path() -> PathBuf {
    crate::settings::dir().join("update.json")
}

pub fn load_state() -> State {
    std::fs::read_to_string(state_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_state(s: &State) {
    let _ = std::fs::create_dir_all(crate::settings::dir());
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(state_path(), t);
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn due(s: &State) -> bool {
    now().saturating_sub(s.checked) >= DAY
}

/// "v0.9.1" / "0.9.1-beta" -> (0, 9, 1).
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    let a = it.next()?.ok()?;
    let b = it.next().unwrap_or(Ok(0)).ok()?;
    let c = it.next().unwrap_or(Ok(0)).ok()?;
    Some((a, b, c))
}

pub fn newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The manifest, checked for sense: a version, an https:// installer, a 64-digit SHA-256.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let m: Manifest = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("the update feed is not a manifest: {}", e))?;
    if parse_version(&m.version).is_none() {
        return Err(format!("the update feed names no version ({:?})", m.version));
    }
    if !m.url.starts_with("https://") && !m.url.starts_with("http://127.0.0.1") && !m.url.starts_with("http://localhost") {
        return Err("the update feed's installer address is not https://".into());
    }
    if m.sha256.len() != 64 || !m.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("the update feed has no SHA-256 for its installer".into());
    }
    Ok(m)
}

/// Reads the feed now and records what it says.
pub fn check(feed: &str) -> State {
    let mut s = load_state();
    s.checked = now();
    match http::fetch(feed, Duration::from_secs(15)).and_then(|r| {
        if r.status == 200 {
            parse_manifest(&r.text())
        } else if r.status == 404 {
            Err("no release is published yet".into())
        } else {
            Err(format!("the update feed answered {}", r.status))
        }
    }) {
        Ok(m) => {
            s.latest = Some(m);
            s.error.clear();
        }
        Err(e) => s.error = e,
    }
    save_state(&s);
    crate::logf!("update check: {}", if s.error.is_empty() { format!("latest {}", s.latest.as_ref().map(|m| m.version.as_str()).unwrap_or("?")) } else { s.error.clone() });
    s
}

/// The folder of an installed Loupe (its uninstaller sits beside it); None for a copy run from anywhere else, which can
/// only point at the download page.
pub fn installed_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    dir.join("uninstall.exe").is_file().then_some(dir)
}

/// Downloads the installer, checks its size and SHA-256, and saves it in the temp folder.
pub fn download(m: &Manifest) -> Result<PathBuf, String> {
    let r = http::fetch(&m.url, Duration::from_secs(60))?;
    if r.status != 200 {
        return Err(format!("the download answered {}", r.status));
    }
    if m.size > 0 && r.body.len() as u64 != m.size {
        return Err(format!("the download is {} bytes, the release says {} - not installed", r.body.len(), m.size));
    }
    if !hex(&sha256(&r.body)).eq_ignore_ascii_case(&m.sha256) {
        return Err("the download does not match its checksum - not installed".into());
    }
    let ver = m.version.trim_start_matches(['v', 'V']);
    let p = std::env::temp_dir().join(format!("BlinkLoupe-{}-setup.exe", ver));
    std::fs::write(&p, &r.body).map_err(|e| format!("{}: {}", p.display(), e))?;
    Ok(p)
}

/// Starts the installer silently over this copy; it closes Loupe, installs, and starts Loupe again.
pub fn run_installer(p: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(p).args(["/S", "/R"]).spawn().map(|_| ()).map_err(|e| format!("could not start the installer: {}", e))
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

/// SHA-256 (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_answers() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(hex(&sha256(&vec![b'a'; 1_000_000])), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.9.1"), Some((0, 9, 1)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.0.0-beta.2"), Some((1, 0, 0)));
        assert_eq!(parse_version("x"), None);
        assert!(newer("0.10.0", "0.9.9") && newer("v1.0.0", "0.9.0") && !newer("0.9.0", "0.9.0") && !newer("0.8.5", "0.9.0"));
        assert!(!newer("junk", "0.9.0"));
    }

    /// The real chain: the fixed feed address through GitHub's redirects, then the installer and its checksum. Needs the
    /// network: cargo test live_release -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_release() {
        let r = http::fetch(FEED, Duration::from_secs(20)).unwrap();
        assert_eq!(r.status, 200, "{}", r.text());
        let m = parse_manifest(&r.text()).unwrap();
        let p = download(&m).unwrap();
        println!("latest {} -> {} ({} bytes, checksum ok)", m.version, p.display(), std::fs::metadata(&p).unwrap().len());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn manifests() {
        let ok = format!(r#"{{"version": "0.9.1", "url": "https://github.com/x/y/releases/download/v0.9.1/Loupe-0.9.1-setup.exe", "sha256": "{}", "size": 3}}"#, "ab".repeat(32));
        assert_eq!(parse_manifest(&ok).unwrap().version, "0.9.1");
        assert!(parse_manifest(&ok.replace("https://", "http://")).unwrap_err().contains("https"));
        assert!(parse_manifest(&ok.replace(&"ab".repeat(32), "abc")).unwrap_err().contains("SHA-256"));
        assert!(parse_manifest(r#"{"version": "?"}"#).is_err());
        assert!(parse_manifest("<html>").is_err());
    }
}
