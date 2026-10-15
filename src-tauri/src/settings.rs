//! Settings in %APPDATA%\Loupe\settings.json. Every field has a default, so older files keep loading.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folders to watch (each with its subfolders down to `depth`).
    pub roots: Vec<String>,
    pub depth: u32,
    pub poll_ms: u32,
    /// Monitor name as Windows reports it (\\.\DISPLAY2); empty = the largest non-primary monitor.
    pub monitor: String,
    /// "auto" (full screen when a second monitor exists), "on", "off".
    pub fullscreen: String,
    /// "loupe" (culling-tool keys) or "classic" (the original Output Viewer keys).
    pub keymap: String,
    /// "auto" (queue in ComfyUI when it answers, else mark), "queue", "mark" (REROLLS.txt only).
    pub reroll_mode: String,
    /// Appended to every save node's filename_prefix on a re-roll; empty keeps the prefix.
    pub reroll_suffix: String,
    /// ComfyUI base URL; empty = try 127.0.0.1:8188 then 127.0.0.1:8000. http:// or https://, optionally user:password@
    /// (Basic auth) and a path the server lives under.
    pub comfy_url: String,
    /// Sent as `Authorization: Bearer <token>` to the server (ComfyUI-Login, token proxies); empty = none. Never logged.
    pub comfy_token: String,
    pub clock24: bool,
    pub hud_secs: u32,
    pub topmost: bool,
    pub show_rejected: bool,
    /// A frame whose brightest pixel is below this (0-255) is flagged BLANK.
    pub blank_threshold: u32,
    /// Hires: scale factor, capped at hires_max_mp megapixels (0 = no cap)
    pub hires_scale: f64,
    pub hires_max_mp: f64,
    pub hires_denoise: f64,
    /// 0 = the frame's own step count
    pub hires_steps: u32,
    /// An upscale model run before the lanczos resize; empty = lanczos only
    pub upscale_model: String,
    pub hires_suffix: String,
    /// "-hires" saves set/x as set-hires/x; empty = beside the original
    pub hires_folder_suffix: String,
    /// Esc / Q / Ctrl+Q ask "Quit Loupe?" first (the dialog's "Don't ask again" turns this off; Settings turns it back on)
    pub confirm_quit: bool,
    /// Keys the user binds to their own commands (see `Action`).
    pub actions: Vec<Action>,
    /// Keys that send the frame on screen into one of the user's ComfyUI workflows (see `WorkflowKey`, send.rs).
    pub workflows: Vec<WorkflowKey>,
    /// A pick also copies the file to <picks_copy_dir>\<its folder's name>\<file>; unpicking removes that copy. Empty = off.
    pub picks_copy_dir: String,
    /// Follow the ComfyUI server too (feed.rs): every image it finishes, read from its queue history and saved to `feed_dir`.
    pub follow_server: bool,
    /// Where server images are saved; empty = <Pictures>\Loupe. Watched while following.
    pub feed_dir: String,
    /// Include Preview Image outputs (they never reach ComfyUI's output folder).
    pub feed_previews: bool,
    /// Look for a newer Loupe at most once a day (update.rs); the request carries nothing about you.
    pub check_updates: bool,
    /// Where the update manifest is; empty = the public releases (update::FEED).
    pub update_url: String,
    /// Before 2026-10-14 the one Shift+H command. Read once and moved into `actions`; never written back.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub finish_cmd: String,
}

/// A key that runs the user's own command on the frame on screen (2026-10-14: Shift+H's finish command made general, so
/// Loupe carries no one's pipeline in its code). `cmd` is split into arguments first, then {file} {folder} {name} {stem} are
/// filled in, so paths with spaces stay one argument. It runs in the frame's folder, with no console window.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Action {
    /// As the key guide writes it: "Shift+H", "Ctrl+Alt+E", "F6", "Alt+1".
    pub key: String,
    pub label: String,
    pub cmd: String,
    /// true: wait, and show the last output line as the message (FINISHED <file> / ALREADY <file> / ERROR <why> are read
    /// specially); false: start it and carry on (an editor, a viewer).
    pub wait: bool,
}

/// A key that sends the frame on screen into a workflow (2026-10-14, phase 3): an Export (API) JSON file, or an image
/// that workflow made. Its Load Image node (the one titled "Loupe" when there are several) gets the frame.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WorkflowKey {
    pub key: String,
    pub label: String,
    /// the workflow file
    pub file: String,
    /// new seeds on its samplers each time (else sending the same frame twice is cached and does nothing)
    pub new_seed: bool,
    /// its save nodes write `<the frame's folder>/<stem>_<label>`, so the result lands beside the frame
    pub beside: bool,
}

impl Default for WorkflowKey {
    fn default() -> Self {
        WorkflowKey { key: String::new(), label: String::new(), file: String::new(), new_seed: true, beside: true }
    }
}

impl Default for Action {
    fn default() -> Self {
        Action { key: String::new(), label: String::new(), cmd: String::new(), wait: true }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            roots: Vec::new(),
            depth: 4,
            poll_ms: 1000,
            monitor: String::new(),
            fullscreen: "auto".into(),
            keymap: "loupe".into(),
            reroll_mode: "auto".into(),
            reroll_suffix: "_reroll".into(),
            comfy_url: String::new(),
            comfy_token: String::new(),
            clock24: false,
            hud_secs: 6,
            topmost: false,
            show_rejected: false,
            blank_threshold: 8,
            hires_scale: 2.0,
            hires_max_mp: 4.2,
            hires_denoise: 0.30,
            hires_steps: 0,
            upscale_model: String::new(),
            hires_suffix: "_hires".into(),
            hires_folder_suffix: String::new(),
            confirm_quit: true,
            actions: Vec::new(),
            workflows: Vec::new(),
            picks_copy_dir: String::new(),
            follow_server: false,
            feed_dir: String::new(),
            feed_previews: true,
            check_updates: true,
            update_url: String::new(),
            finish_cmd: String::new(),
        }
    }
}

/// <Pictures>\Loupe: where a followed server's images go unless Settings names another folder.
pub fn default_feed_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Pictures").join("BlinkLoupe")
}

impl Settings {
    pub fn feed_dir_path(&self) -> PathBuf {
        if self.feed_dir.is_empty() { default_feed_dir() } else { PathBuf::from(&self.feed_dir) }
    }
}

/// %APPDATA%\BlinkLoupe. Loupe was renamed BlinkLoupe on 2026-10-14, when Blink joined it: the
/// first time the new folder is missing, the old %APPDATA%\Loupe files are copied over; the old folder stays as it was.
pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("BlinkLoupe");
    if !d.exists() {
        let old = base.join("Loupe");
        if old.is_dir() && fs::create_dir_all(&d).is_ok() {
            for f in ["settings.json", "update.json", "lora-cache.json"] {
                let _ = fs::copy(old.join(f), d.join(f));
            }
        }
    }
    d
}

pub fn load() -> Settings {
    let p = dir().join("settings.json");
    let mut s: Settings = fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    s.sanitize();
    s
}

pub fn save(s: &Settings) -> Result<(), String> {
    fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let p = dir().join("settings.json");
    let tmp = dir().join("settings.json.tmp");
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.depth = self.depth.clamp(0, 12);
        self.poll_ms = self.poll_ms.clamp(250, 10_000);
        self.hud_secs = self.hud_secs.clamp(1, 120);
        self.blank_threshold = self.blank_threshold.min(255);
        if !["auto", "on", "off"].contains(&self.fullscreen.as_str()) {
            self.fullscreen = "auto".into();
        }
        if !["loupe", "classic"].contains(&self.keymap.as_str()) {
            self.keymap = "loupe".into();
        }
        if !["auto", "queue", "mark"].contains(&self.reroll_mode.as_str()) {
            self.reroll_mode = "auto".into();
        }
        if !self.hires_scale.is_finite() || self.hires_scale < 1.0 {
            self.hires_scale = 2.0;
        }
        self.hires_scale = self.hires_scale.min(8.0);
        if !self.hires_max_mp.is_finite() || self.hires_max_mp < 0.0 {
            self.hires_max_mp = 4.2;
        }
        if !self.hires_denoise.is_finite() || self.hires_denoise <= 0.0 || self.hires_denoise > 1.0 {
            self.hires_denoise = 0.30;
        }
        self.hires_steps = self.hires_steps.min(100);
        let mut seen = std::collections::HashSet::new();
        self.roots.retain(|r| !r.trim().is_empty() && seen.insert(r.trim().to_lowercase()));
        self.comfy_url = self.comfy_url.trim().trim_end_matches('/').to_string();
        self.comfy_token = self.comfy_token.trim().to_string();
        self.picks_copy_dir = self.picks_copy_dir.trim().to_string();
        self.feed_dir = self.feed_dir.trim().to_string();
        self.update_url = self.update_url.trim().to_string();
        let old = std::mem::take(&mut self.finish_cmd);
        if !old.trim().is_empty() && !self.actions.iter().any(|a| a.key.eq_ignore_ascii_case("Shift+H")) {
            self.actions.insert(0, Action { key: "Shift+H".into(), label: "Hires".into(), cmd: old.trim().into(), wait: true });
        }
        let mut keys = std::collections::HashSet::new();
        for a in self.actions.iter_mut() {
            a.key = a.key.trim().to_string();
            a.cmd = a.cmd.trim().to_string();
            a.label = a.label.trim().to_string();
            if a.label.is_empty() {
                a.label = a.key.clone();
            }
        }
        self.actions.retain(|a| !a.key.is_empty() && !a.cmd.is_empty() && keys.insert(a.key.to_lowercase()));
        for w in self.workflows.iter_mut() {
            w.key = w.key.trim().to_string();
            w.file = w.file.trim().trim_matches('"').to_string();
            w.label = w.label.trim().to_string();
            if w.label.is_empty() {
                w.label = std::path::Path::new(&w.file).file_stem().and_then(|s| s.to_str()).unwrap_or(&w.key).to_string();
            }
        }
        // one key, one job: an action keeps a key a workflow also names
        self.workflows.retain(|w| !w.key.is_empty() && !w.file.is_empty() && keys.insert(w.key.to_lowercase()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_finish_cmd_becomes_the_shift_h_action() {
        let mut s: Settings = serde_json::from_str(r#"{"finish_cmd": "py fin.py \"{file}\""}"#).unwrap();
        s.sanitize();
        assert_eq!(s.actions, vec![Action { key: "Shift+H".into(), label: "Hires".into(), cmd: "py fin.py \"{file}\"".into(), wait: true }]);
        assert!(s.finish_cmd.is_empty());
        let text = serde_json::to_string(&s).unwrap();
        assert!(!text.contains("finish_cmd"), "the old field is not written back");
        let mut again: Settings = serde_json::from_str(&text).unwrap();
        again.sanitize();
        assert_eq!(again.actions.len(), 1, "migrates once");
    }

    #[test]
    fn actions_need_a_key_and_a_command_and_keys_are_unique() {
        let mut s = Settings::default();
        s.actions = vec![
            Action { key: " F6 ".into(), label: "".into(), cmd: " a ".into(), wait: false },
            Action { key: "f6".into(), label: "dup".into(), cmd: "b".into(), wait: true },
            Action { key: "F7".into(), label: "x".into(), cmd: "".into(), wait: true },
        ];
        s.sanitize();
        assert_eq!(s.actions, vec![Action { key: "F6".into(), label: "F6".into(), cmd: "a".into(), wait: false }]);
        assert!(Settings::default().actions.is_empty(), "no one's pipeline is built in");
    }

    #[test]
    fn workflow_keys_are_trimmed_named_and_never_share_a_key() {
        let mut s = Settings::default();
        s.actions = vec![Action { key: "F6".into(), label: "a".into(), cmd: "a".into(), wait: true }];
        s.workflows = vec![
            WorkflowKey { key: "f6".into(), file: "x.json".into(), ..Default::default() },
            WorkflowKey { key: " Alt+U ".into(), file: " \"D:\\wf\\Upscale 4x.json\" ".into(), ..Default::default() },
            WorkflowKey { key: "Alt+V".into(), ..Default::default() },
        ];
        s.sanitize();
        assert_eq!(s.workflows.len(), 1);
        let w = &s.workflows[0];
        assert_eq!((w.key.as_str(), w.file.as_str(), w.label.as_str(), w.new_seed, w.beside), ("Alt+U", "D:\\wf\\Upscale 4x.json", "Upscale 4x", true, true));
        let back: Settings = serde_json::from_str(r#"{"workflows": [{"key": "Alt+U", "file": "a.json"}]}"#).unwrap();
        assert!(back.workflows[0].new_seed && back.workflows[0].beside, "missing fields take the defaults");
    }
}
