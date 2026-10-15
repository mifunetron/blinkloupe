// BlinkLoupe UI: shows the newest frame (LIVE), steps through history, marks frames and sends re-rolls.
// All file access goes through the Rust side (invoke) and the loupe:// protocol.

const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);

const $ = (id) => document.getElementById(id);
const stage = $("stage"), img = $("img"), vid = $("vid"), msg = $("msg");
const hud = $("hud"), statusEl = $("status"), whereEl = $("where"), comfyEl = $("comfy"), hud2 = $("hud2");
const infoEl = $("info"), toastEl = $("toast"), veil = $("veil"), panel = $("panel"), probe = $("probe");

const S = {
  settings: null,
  sessionRoots: null,
  cur: null,          // the Entry on screen
  live: true,
  paused: false,
  unseen: 0,          // frames that landed while not LIVE
  fit: "fit",
  rot: 0,
  hudPinned: false,
  info: false,
  meta: null,
  blank: false,
  dims: null,
  comfy: { url: null, mine: 0, pending: 0, running: 0 },
  hudTimer: null,
  toastTimer: null,
  panel: null,        // "settings" | "first" | "keys"
  busy: false,
};

const fileUrl = (p) => "http://loupe.localhost/f?p=" + encodeURIComponent(p);
const log = (m) => invoke("log_line", { msg: String(m) }).catch(() => {});
// a script error anywhere in the page lands in loupe.log, so a report from someone else's machine carries it
window.addEventListener("error", (e) => log(`script error: ${e.message} (${(e.filename || "").split("/").pop()}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => log(`script error: ${e.reason}`));

// ------------------------------------------------------------------------------------------------ start and poll

async function boot() {
  const sv = await invoke("get_settings");
  S.settings = sv.settings;
  S.sessionRoots = sv.session_roots;
  S.feedDefault = sv.feed_default;
  const info = await invoke("start");
  if (info.first_run) {
    message("");
    openFirstRun();
  } else if (info.newest) {
    show(info.newest);
  } else {
    message("Waiting for images\n" + info.roots.join("\n") + "\n\nF1 shows the keys");
  }
  if (info.missing && info.missing.length) toast("Folder not found: " + info.missing.join(", "), true);
  setTimeout(tick, S.settings.poll_ms);
  refreshComfy(false);
  setInterval(() => refreshComfy(false), 5000);
  setTimeout(announceUpdate, 20000);
  setInterval(announceUpdate, 3600 * 1000);
}

async function tick() {
  try {
    const fresh = await invoke("poll");
    if (fresh.length) onNew(fresh);
  } catch (e) {
    log("poll failed: " + e);
  }
  setTimeout(tick, S.settings.poll_ms);
}

function onNew(list) {
  if (S.live && !S.paused) {
    show(list[list.length - 1]);
  } else {
    // never steal the frame being studied - say loudly that frames are waiting instead
    S.unseen += list.length;
    showHud();
  }
}

async function refreshComfy(force) {
  if (S.settings && S.settings.reroll_mode === "mark" && !force) {
    S.comfy = { url: null, mine: 0, pending: 0, running: 0 };
  } else {
    try {
      S.comfy = await invoke("comfy_status", { force: !!force });
    } catch (e) {
      S.comfy = { url: null, mine: 0, pending: 0, running: 0 };
    }
  }
  drawComfy();
}

// ------------------------------------------------------------------------------------------------ showing a frame

function message(text) {
  img.removeAttribute("src");
  img.hidden = true;
  vid.pause();
  vid.removeAttribute("src");
  vid.hidden = true;
  msg.textContent = text;
}

function show(entry) {
  if (!entry) return;
  S.cur = entry;
  S.meta = null;
  S.blank = false;
  S.dims = null;
  msg.textContent = "";
  const url = fileUrl(entry.path) + "&t=" + Math.round(entry.mtime * 1000);
  if (entry.kind === "video") {
    img.hidden = true;
    img.removeAttribute("src");
    vid.hidden = false;
    vid.src = url;
    vid.play().catch(() => {});
  } else {
    vid.pause();
    vid.removeAttribute("src");
    vid.hidden = true;
    img.hidden = false;
    img.src = url;
  }
  showHud();
  if (S.info) loadInfo();
}

img.addEventListener("load", () => {
  S.dims = [img.naturalWidth, img.naturalHeight];
  try {
    const ctx = probe.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(img, 0, 0, 48, 48);
    const d = ctx.getImageData(0, 0, 48, 48).data;
    let max = 0;
    const sum = [0, 0, 0];
    for (let i = 0; i < d.length; i += 4) {
      const m = Math.max(d[i], d[i + 1], d[i + 2]);
      if (m > max) max = m;
      sum[0] += d[i]; sum[1] += d[i + 1]; sum[2] += d[i + 2];
    }
    S.blank = max < S.settings.blank_threshold;
    const n = d.length / 4;
    log(`show ${S.cur && S.cur.rel} ${img.naturalWidth}x${img.naturalHeight} mean ${sum.map((v) => Math.round(v / n)).join(",")}${S.blank ? " BLANK" : ""}`);
  } catch (e) {
    S.blank = false;
  }
  showHud();
});
img.addEventListener("error", () => {
  if (S.cur && S.cur.kind !== "video") message("Could not load\n" + S.cur.rel);
});
vid.addEventListener("loadedmetadata", () => {
  S.dims = [vid.videoWidth, vid.videoHeight];
  showHud();
});

function applyView() {
  stage.className = { fit: "fit", fill: "fill", "1:1": "one" }[S.fit] + (S.rot ? " rot" + S.rot : "");
}

// ------------------------------------------------------------------------------------------------ navigation

async function step(delta) {
  if (!S.cur) return;
  const r = await invoke("step", { path: S.cur.path, delta });
  if (r.entry) {
    S.live = false;
    show(r.entry);
  } else if (r.past_newest) {
    goLive();
  }
}

async function goLive() {
  S.live = true;
  S.paused = false;
  S.unseen = 0;
  const e = await invoke("edge", { which: "newest" });
  if (e) show(e);
  else showHud();
}

async function oldest() {
  const e = await invoke("edge", { which: "oldest" });
  if (e) {
    S.live = false;
    show(e);
  }
}

function togglePause() {
  S.paused = !S.paused;
  if (!S.paused && S.live) goLive();
  else showHud();
}

async function rescan() {
  const info = await invoke("start");
  toast(`Rescanned ${info.files} files in ${info.folders} folders`);
  goLive();
}

async function refreshCur() {
  if (!S.cur) return;
  const e = await invoke("entry", { path: S.cur.path });
  if (e) S.cur = e;
  showHud();
}

// ------------------------------------------------------------------------------------------------ marks and re-rolls

async function mark(action, value) {
  if (!S.cur) return;
  try {
    const r = await invoke("mark", { path: S.cur.path, action, value: value ?? null });
    if (r.entry) S.cur = r.entry;
    const m = S.cur.marks;
    const said = {
      pick: m.pick ? "Picked" : "Pick removed",
      bookmark: m.bookmark ? "Bookmarked" : "Bookmark removed",
      reject: m.rejected ? "Skipped - stepping passes over it (K shows skipped frames)" : "Skip removed",
      stars: m.stars ? "★".repeat(m.stars) : "Stars cleared",
      unflag: "Marks cleared",
    }[action];
    toast(r.note ? `${said} · ${r.note}` : said, r.warn);
    showHud();
    if (S.info) loadInfo();
  } catch (e) {
    toast(String(e), true);
  }
}

async function reroll() {
  if (!S.cur || S.busy) return;
  S.busy = true;
  try {
    const r = await invoke("reroll", { path: S.cur.path });
    if (r.entry) S.cur = r.entry;
    if (r.action === "queued") toast("Re-roll queued in ComfyUI");
    else toast(`Re-roll marked in REROLLS.txt (×${S.cur.marks.rerolls_marked})` + (r.note ? " - " + r.note : ""));
    refreshComfy(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
    showHud();
  }
}

async function unreroll() {
  if (!S.cur || S.busy) return;
  S.busy = true;
  try {
    const r = await invoke("unreroll", { path: S.cur.path });
    if (r.entry) S.cur = r.entry;
    toast({ cancelled: "Re-roll cancelled", unmarked: "Re-roll mark removed", running: "Too late - " + r.note, none: r.note }[r.action] || r.action);
    refreshComfy(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
    showHud();
  }
}

async function queueMarked() {
  if (!S.cur || S.busy) return;
  S.busy = true;
  try {
    const r = await invoke("queue_marked", { path: S.cur.path });
    let t = `Queued ${r.queued} marked re-roll${r.queued === 1 ? "" : "s"} from this folder`;
    if (r.skipped.length) t += ` · ${r.skipped.length} skipped` + (r.error ? ` (${r.error})` : "");
    toast(t, r.queued === 0 && r.skipped.length > 0);
    refreshCur();
    refreshComfy(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
  }
}

async function hiresThis() {
  if (!S.cur || S.busy) return;
  if (S.cur.kind === "video") return toast("Hires works on images", true);
  S.busy = true;
  toast("Uploading and queueing the hires pass…");
  try {
    const r = await invoke("hires", { path: S.cur.path });
    if (r.entry) S.cur = r.entry;
    toast("Hires queued in ComfyUI · " + r.note);
    refreshComfy(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
    showHud();
  }
}

async function runAction(i) {
  // the user's own key (Settings > Actions) on this frame; a waiting action's last output line becomes the message
  const a = S.settings.actions[i];
  if (!S.cur || !a) return;
  if (!a.wait) {
    try {
      await invoke("run_action", { index: i, path: S.cur.path });
      toast(`${a.label} · started`);
    } catch (e) {
      toast(String(e), true);
    }
    return;
  }
  if (S.busy) return;
  S.busy = true;
  toast(`${a.label}…`);
  try {
    const r = await invoke("run_action", { index: i, path: S.cur.path });
    const name = r.output ? r.output.split(/[\\/]/).pop() : "";
    const extra = (r.line.match(/\((.*)\)\s*$/) || [])[1];
    const t = r.line.startsWith("FINISHED") ? `${a.label} finished · ${name}${extra ? " · " + extra : ""}`
      : r.line.startsWith("ALREADY") ? `${a.label} · already done · ${name}`
      : r.line.replace(/^ERROR\s*/, "") || (r.ok ? `${a.label} · done` : `${a.label} failed`);
    toast(t, !r.ok);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
    showHud();
  }
}

async function sendToWorkflow(i) {
  // Settings > Send to a workflow: the frame goes into the user's own workflow and is queued; the result arrives like any
  // other new frame
  const w = S.settings.workflows[i];
  if (!S.cur || !w || S.busy) return;
  S.busy = true;
  toast(`${w.label}: sending…`);
  try {
    const r = await invoke("send_to_workflow", { index: i, path: S.cur.path });
    toast(`${w.label} · queued · ${r.note}`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
    showHud();
  }
}

async function installUpdate() {
  const u = await invoke("update_info");
  if (!u.newer) return toast("BlinkLoupe is up to date");
  toast(`Downloading BlinkLoupe ${u.newer.version}…`);
  try {
    await invoke("install_update");
    toast(`Installing BlinkLoupe ${u.newer.version} - it starts again in a moment`);
  } catch (e) {
    toast(String(e), true);
  }
}

// A newer release is mentioned once per version (Settings > Updates has the button).
async function announceUpdate() {
  try {
    const u = await invoke("update_info");
    if (!u.newer) return;
    let told = "";
    try { told = localStorage.getItem("loupe.updateTold") || ""; } catch (e) {}
    if (told === u.newer.version) return;
    toast(`BlinkLoupe ${u.newer.version} is out - Settings (S) > Updates`);
    try { localStorage.setItem("loupe.updateTold", u.newer.version); } catch (e) {}
  } catch (e) {
    log("update info: " + e);
  }
}

async function hiresPicks() {
  if (!S.cur || S.busy) return;
  S.busy = true;
  toast("Queueing hires for this folder's picks…");
  try {
    const r = await invoke("hires_picks", { path: S.cur.path });
    let t = r.queued ? `Hires queued for ${r.queued} pick${r.queued === 1 ? "" : "s"}` : "No picks left to hires in this folder";
    if (r.already) t += ` · ${r.already} already hired`;
    if (r.skipped.length) t += ` · ${r.skipped.length} skipped` + (r.error ? ` (${r.error})` : "");
    toast(t, r.queued === 0 && r.skipped.length > 0);
    refreshCur();
    refreshComfy(false);
  } catch (e) {
    toast(String(e), true);
  } finally {
    S.busy = false;
  }
}

// ------------------------------------------------------------------------------------------------ HUD

function clock(sec) {
  const d = new Date(sec * 1000);
  if (S.settings.clock24) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });
  return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit", hour12: true }).replace(/\s?([AP])M$/i, (m, a) => " " + a.toLowerCase() + "m");
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}

function drawHud() {
  const e = S.cur;
  let status = S.paused ? "PAUSED" : S.live ? "LIVE" : "HISTORY";
  let cls = S.paused || !S.live ? "hold" : "";
  if (S.unseen) {
    status += `  ·  ${S.unseen} NEW - press End`;
    cls = "new";
  }
  if (S.blank) cls = "bad";
  statusEl.textContent = status;
  statusEl.className = cls;
  whereEl.innerHTML = e ? `<bdi>${esc(e.rel)}</bdi>` : "";
  if (!e) {
    hud2.innerHTML = "";
    drawComfy();
    return;
  }
  const parts = [];
  if (S.dims) parts.push(`${S.dims[0]}×${S.dims[1]} · ${((S.dims[0] * S.dims[1]) / 1e6).toFixed(1)} MP`);
  parts.push(clock(e.mtime));
  // n / total: the frame's number over the set's planned size when the runner left a .loupe-set.json, else a count of the files
  // in a hires folder the plan counts by kind: "pass 7/21" = the 7th pass of the set's 21 picks
  parts.push(`${e.finish || "frame"} ${e.n}/${e.total}${e.reroll ? " · reroll" : ""}`);
  if (e.gap != null) parts.push(`${Math.round(e.gap)} s after the previous`);
  const tags = [];
  const m = e.marks;
  if (m.pick) tags.push('<span class="tag pick">PICK</span>');
  if (m.bookmark) tags.push('<span class="tag bookmark">BOOKMARK</span>');
  if (m.stars) tags.push(`<span class="tag star">${"★".repeat(m.stars)}</span>`);
  if (m.rejected) tags.push('<span class="tag reject">SKIPPED</span>');
  if (m.rerolls_marked) tags.push(`<span class="tag reroll">REROLL ×${m.rerolls_marked}</span>`);
  if (e.queued) tags.push(`<span class="tag reroll">SENT ×${e.queued}</span>`);
  if (m.hires) tags.push(`<span class="tag hires">HIRES${m.hires > 1 ? " ×" + m.hires : ""}</span>`);
  if (S.blank) tags.push('<span class="tag blank">BLANK</span>');
  if (S.fit !== "fit") tags.push(`<span class="tag soft">${S.fit}</span>`);
  if (S.rot) tags.push(`<span class="tag soft">rot ${S.rot}</span>`);
  if (S.settings.topmost) tags.push('<span class="tag soft">top</span>');
  // only when this folder has skipped frames - otherwise the tag says nothing (2026-10-13)
  if (S.settings.show_rejected && e.skipped_here) tags.push(`<span class="tag soft">showing ${e.skipped_here} skipped</span>`);
  hud2.innerHTML = parts.map((p) => `<span>${esc(p)}</span>`).join("") + tags.join("");
  drawComfy();
}

function drawComfy() {
  if (!S.settings) return;
  if (S.settings.reroll_mode === "mark") {
    comfyEl.textContent = "re-rolls: mark only";
    comfyEl.className = "";
  } else if (S.comfy.url) {
    const q = S.comfy.running + S.comfy.pending;
    comfyEl.textContent = "ComfyUI" + (S.comfy.feed && S.comfy.feed.on && S.comfy.feed.url ? " · following" : "") + (q ? ` · ${q} in queue` + (S.comfy.mine ? ` (${S.comfy.mine} yours)` : "") : "");
    comfyEl.className = "on";
  } else {
    comfyEl.textContent = "ComfyUI not found";
    comfyEl.className = "";
  }
}

function showHud() {
  applyView();
  drawHud();
  hud.hidden = false;
  clearTimeout(S.hudTimer);
  if (!S.hudPinned) S.hudTimer = setTimeout(hideHud, S.settings.hud_secs * 1000);
}

function hideHud() {
  if (S.hudPinned || S.unseen || S.info) return;
  hud.hidden = true;
}

// ------------------------------------------------------------------------------------------------ info panel

async function loadInfo() {
  if (!S.cur) return;
  const path = S.cur.path;
  infoEl.hidden = false;
  if (!S.meta) {
    infoEl.innerHTML = '<span class="none">reading…</span>';
    try {
      S.meta = await invoke("meta", { path });
    } catch (e) {
      S.meta = { keys: [] };
    }
    if (!S.cur || S.cur.path !== path) return;
  }
  const m = S.meta;
  const rows = [];
  const mk = S.cur.marks || {};
  rows.push(["Rating", mk.stars ? "★".repeat(mk.stars) + "☆".repeat(5 - mk.stars) + "  " + mk.stars + " of 5" : "not rated - 1 to 5 rate it"]);
  rows.push(["Bookmark", mk.bookmark ? "bookmarked" : "no - B bookmarks it"]);
  const marksOnly = rows.length;
  if (m.model) rows.push(["Model", m.model]);
  if (m.loras && m.loras.length) rows.push(["LoRAs", m.loras.map(([n, s]) => `${n} @ ${s}`).join("   ·   ")]);
  const samp = [];
  if (m.steps) samp.push(`${m.steps} steps`);
  if (m.sampler) samp.push(`${m.sampler} / ${m.scheduler || "?"}`);
  if (m.cfg) samp.push(`CFG ${m.cfg}`);
  if (m.seed) samp.push(`seed ${m.seed}`);
  if (m.denoise) samp.push(`denoise ${m.denoise}`);
  if (m.size) samp.push(`latent ${m.size}`);
  if (m.passes > 1) samp.push(`${m.passes} sampler passes (the last shown)`);
  if (samp.length) rows.push(["Sampler", samp.join("   ·   ")]);
  if (m.positive) rows.push(["Prompt", m.positive.length > 2400 ? m.positive.slice(0, 2400) + " …" : m.positive]);
  if (m.negative) rows.push(["Negative", m.negative]);
  const src = { comfyui: "ComfyUI graph", a1111: "A1111 / Forge parameters", swarmui: "SwarmUI" }[m.source];
  if (src) rows.push(["Source", src + (m.source === "comfyui" ? (m.can_reroll ? " · can re-roll" : " · no sampler seed found") : "")]);
  const none = rows.length === marksOnly
    ? `<span class="none">No generation metadata in this file${m.keys && m.keys.length ? " (chunks: " + esc(m.keys.join(", ")) + ")" : ""}.</span>` : "";
  infoEl.innerHTML = "<dl>" + rows.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join("") + "</dl>" + none;
}

function toggleInfo() {
  S.info = !S.info;
  if (S.info) loadInfo();
  else infoEl.hidden = true;
  showHud();
}

// ------------------------------------------------------------------------------------------------ library (L)
// 2026-10-13: a list of bookmarked and rated images, sortable by date, by rating and by LoRA

const LIB = { tab: "bookmarks", sort: "newest", q: "", marked: null, loras: null, lora: null, items: null, show: 240, timer: null };

function libSort(list) {
  const by = {
    newest: (a, b) => b.mtime - a.mtime,
    oldest: (a, b) => a.mtime - b.mtime,
    rating: (a, b) => b.stars - a.stars || b.mtime - a.mtime,
  }[LIB.sort];
  return list.slice().sort(by);
}

function libCards(list) {
  const q = LIB.q.toLowerCase();
  const shown = libSort(list.filter((it) => !q || it.rel.toLowerCase().includes(q)));
  if (!shown.length) return '<div class="libempty">Nothing here yet.</div>';
  const cards = shown.slice(0, LIB.show).map((it) => {
    const folder = it.rel.includes("\\") ? it.rel.slice(0, it.rel.lastIndexOf("\\")) : "";
    const marks = (it.stars ? "★".repeat(it.stars) : "") + (it.bookmark ? (it.stars ? "  " : "") + "BOOKMARK" : "");
    return `<button class="card" data-p="${esc(it.path)}" title="${esc(it.rel)}"><img loading="lazy" decoding="async" src="${fileUrl(it.path)}&t=${Math.round(it.mtime * 1000)}">` +
      `<span class="cn">${esc(it.name)}</span><span class="cm">${marks || "&nbsp;"}</span><span class="cf">${esc(folder)} · ${esc(clock(it.mtime))}</span></button>`;
  }).join("");
  const more = shown.length > LIB.show ? `<button class="more" id="libmore">Show ${Math.min(240, shown.length - LIB.show)} more of ${shown.length - LIB.show}</button>` : "";
  return `<div class="count">${shown.length} frame${shown.length === 1 ? "" : "s"}</div><div class="cards">${cards}</div>${more}`;
}

async function jumpTo(path) {
  const e = await invoke("entry", { path });
  if (!e) return toast("That frame is no longer in the watched folders", true);
  closePanel();
  S.live = false;
  show(e);
}

async function renderLib() {
  if (S.panel !== "library") return;
  const body = $("libbody");
  clearTimeout(LIB.timer);
  $("panel").querySelectorAll("button.tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === LIB.tab));
  if (LIB.tab !== "loras") {
    if (!LIB.marked) LIB.marked = await invoke("marked_list");
    const list = LIB.marked.filter((it) => (LIB.tab === "bookmarks" ? it.bookmark : it.stars > 0));
    body.innerHTML = `<div class="grid">${libCards(list)}</div>`;
  } else {
    const st = await invoke("lora_index");
    LIB.loras = st.loras;
    const q = LIB.q.toLowerCase();
    const names = st.loras.filter(([n]) => !LIB.lora || !q || n.toLowerCase().includes(q));
    const progress = st.building ? `<div class="libprog">Reading LoRAs from the frames' metadata… ${st.done} of ${st.total}</div>` : "";
    const left = `<div class="loras">${progress}${names.map(([n, c]) =>
      `<button class="${n === LIB.lora ? "on" : ""}" data-l="${esc(n)}"><span>${esc(n)}</span><span class="n">${c}</span></button>`).join("") ||
      (st.building ? "" : '<div class="libempty">No LoRAs found in the metadata.</div>')}</div>`;
    let right = '<div class="grid"><div class="libempty">Choose a LoRA on the left.</div></div>';
    if (LIB.lora) {
      LIB.items = await invoke("lora_images", { name: LIB.lora });
      right = `<div class="grid"><h2>${esc(LIB.lora)}</h2>${libCards(LIB.items)}</div>`;
    }
    body.innerHTML = left + right;
    body.querySelectorAll("[data-l]").forEach((b) => b.addEventListener("click", () => { LIB.lora = b.dataset.l; LIB.show = 240; renderLib(); }));
    if (st.building) LIB.timer = setTimeout(renderLib, 900);
  }
  body.querySelectorAll(".card").forEach((c) => c.addEventListener("click", () => jumpTo(c.dataset.p)));
  const more = $("libmore");
  if (more) more.addEventListener("click", () => { LIB.show += 240; renderLib(); });
}

function openLibrary() {
  LIB.marked = null;
  LIB.show = 240;
  const tabs = [["bookmarks", "Bookmarked"], ["rated", "Rated"], ["loras", "By LoRA"]];
  openPanel("library", `<h1><span class="mark">BlinkLoupe</span> library</h1>
    <div class="libbar">
      <div class="tabs">${tabs.map(([k, t]) => `<button class="tab" data-tab="${k}">${t}</button>`).join("")}</div>
      <label>Sort <select id="libsort"><option value="newest">Newest first</option><option value="oldest">Oldest first</option><option value="rating">Highest rated</option></select></label>
      <input id="libq" type="text" placeholder="Filter by file or folder name" value="${esc(LIB.q)}">
      <button class="primary" id="libclose">Close</button>
    </div>
    <div id="libbody"></div>`, "lib");
  $("libsort").value = LIB.sort;
  $("libsort").addEventListener("change", () => { LIB.sort = $("libsort").value; renderLib(); });
  $("libq").addEventListener("input", () => { LIB.q = $("libq").value; LIB.show = 240; renderLib(); });
  $("libclose").addEventListener("click", closePanel);
  $("panel").querySelectorAll("button.tab").forEach((b) => b.addEventListener("click", () => { LIB.tab = b.dataset.tab; LIB.show = 240; renderLib(); }));
  renderLib();
}

// ------------------------------------------------------------------------------------------------ toast

function toast(text, err) {
  toastEl.textContent = text;
  toastEl.className = err ? "err" : "";
  toastEl.hidden = false;
  clearTimeout(S.toastTimer);
  S.toastTimer = setTimeout(() => (toastEl.hidden = true), err ? 6000 : 2400);
}

// ------------------------------------------------------------------------------------------------ keys

const COMMON = {
  ArrowLeft: () => step(-1),
  ArrowRight: () => step(1),
  ArrowUp: () => step(-1),
  ArrowDown: () => step(1),
  PageUp: () => step(-10),
  PageDown: () => step(10),
  Home: oldest,
  End: goLive,
  " ": togglePause,
  p: () => mark("pick"),
  b: () => mark("bookmark"),
  l: openLibrary,
  v: () => invoke("blink", { path: S.cur ? S.cur.path : null }).catch((e) => toast(String(e), true)),
  P: hiresThis,
  "Ctrl+p": hiresPicks,
  u: () => mark("unflag"),
  0: () => mark("stars", 0), 1: () => mark("stars", 1), 2: () => mark("stars", 2), 3: () => mark("stars", 3), 4: () => mark("stars", 4), 5: () => mark("stars", 5),
  i: toggleInfo,
  h: () => { S.hudPinned = !S.hudPinned; showHud(); },
  f: () => { S.fit = { fit: "fill", fill: "1:1", "1:1": "fit" }[S.fit]; showHud(); },
  e: () => S.cur && invoke("reveal", { path: S.cur.path }),
  c: () => { if (S.cur) navigator.clipboard.writeText(S.cur.path).then(() => toast("Path copied")); },
  t: toggleTopmost,
  k: toggleRejected,
  F11: () => invoke("toggle_fullscreen"),
  F5: rescan,
  s: openSettings,
  F1: toggleHelp,
  "?": toggleHelp,
  "Ctrl+r": queueMarked,
};

const KEYMAPS = {
  loupe: Object.assign({}, COMMON, {
    x: () => mark("reject"),
    r: reroll,
    R: unreroll,
    o: () => { S.rot = (S.rot + 90) % 360; showHud(); },
    Escape: () => (S.info ? toggleInfo() : confirmQuit()),
    "Ctrl+q": confirmQuit,
  }),
  classic: Object.assign({}, COMMON, {
    x: reroll,
    X: unreroll,
    r: () => { S.rot = (S.rot + 90) % 360; showHud(); },
    d: () => mark("reject"),
    q: confirmQuit,
    Escape: () => (S.info ? toggleInfo() : confirmQuit()),
    "Ctrl+q": confirmQuit,
  }),
};

// A key as the guide and Settings > Actions write it: "Shift+H", "Ctrl+Alt+E", "F6", "Alt+1". A letter carries Shift as a
// modifier; a digit or symbol is the character typed, which already says whether Shift was down.
function keyName(ev) {
  const k = ev.key;
  if (!k || ["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock", "NumLock", "Dead", "Unidentified"].includes(k)) return "";
  const mods = [];
  if (ev.ctrlKey) mods.push("Ctrl");
  if (ev.altKey) mods.push("Alt");
  let base;
  if (k.length === 1 && /[a-z]/i.test(k)) {
    base = k.toUpperCase();
    if (ev.shiftKey) mods.push("Shift");
  } else if (k.length === 1) {
    base = k === " " ? "Space" : k;
  } else {
    base = k;
    if (ev.shiftKey) mods.push("Shift");
  }
  return [...mods, base].join("+");
}

// The keymap's own keys in keyName's form, so an action can't take one: "p" -> "P", "P" -> "Shift+P", "Ctrl+p" -> "Ctrl+P".
function builtinKeys(km) {
  const keys = Object.keys(KEYMAPS[km] || KEYMAPS.loupe).map((k) => {
    const c = /^Ctrl\+(.+)$/.exec(k);
    if (c) return "Ctrl+" + (c[1].length === 1 ? c[1].toUpperCase() : c[1]);
    if (k.length === 1 && /[a-z]/.test(k)) return k.toUpperCase();
    if (k.length === 1 && /[A-Z]/.test(k)) return "Shift+" + k;
    return k === " " ? "Space" : k;
  });
  return new Set([...keys, "Tab", "Enter"]);
}

function actionAt(ev) {
  const acts = (S.settings && S.settings.actions) || [];
  const name = keyName(ev).toLowerCase();
  return name ? acts.findIndex((a) => a.key.toLowerCase() === name) : -1;
}

function workflowAt(ev) {
  const wfs = (S.settings && S.settings.workflows) || [];
  const name = keyName(ev).toLowerCase();
  return name ? wfs.findIndex((w) => w.key.toLowerCase() === name) : -1;
}

// The F1 guide, grouped; each row is [keys separated by " / ", what they do]. Mouse entries are written in words.
function helpGroups(km) {
  const classic = km === "classic";
  return [
    ["Moving around", [
      ["← / → / wheel", "previous / next frame - stepping back leaves LIVE"],
      ["PgUp / PgDn", "10 frames back / forward"],
      ["Home", "the oldest frame"],
      ["End", "back to LIVE - the newest frame"],
      ["Space", "pause / resume LIVE"],
    ]],
    ["Marking", [
      ["P", "pick - written to PICKS.txt beside the image"],
      ["B", "bookmark - B again removes it (U leaves bookmarks alone)"],
      [classic ? "D" : "X", "skip - stepping passes over it"],
      ["0-5", "stars (the same number again, or 0, clears them)"],
      ["U", "clear pick, skip and stars"],
      ["K", "show / hide skipped frames"],
    ]],
    ["ComfyUI", [
      [classic ? "X" : "R", "re-roll - the same graph with a new seed"],
      [classic ? "Shift+X" : "Shift+R", "undo the last re-roll (while it waits in the queue)"],
      ["Ctrl+R", "queue this folder's REROLLS.txt marks"],
      ["Shift+P", "hires this frame - upscale + one low-denoise pass"],
      ["Ctrl+P", "hires every pick in this folder not hired yet"],
    ]],
    ...(S.settings && S.settings.actions.length
      ? [["Your actions (Settings > Actions)", S.settings.actions.map((a) => [a.key, a.label + (a.wait ? "" : " (starts it and carries on)")])]]
      : []),
    ...(S.settings && (S.settings.workflows || []).length
      ? [["Your workflows (Settings > Send to a workflow)", S.settings.workflows.map((w) => [w.key, "send this frame to " + w.label])]]
      : []),
    ["Viewing", [
      ["I", "info - model, LoRAs, sampler, seed, prompts"],
      ["H", "keep the HUD on screen"],
      ["F", "fit / fill / 1:1"],
      [classic ? "R" : "O", "rotate 90°"],
      ["T", "always on top"],
      ["F11", "full screen (double-click works too)"],
    ]],
    ["BlinkLoupe", [
      ["F1 / ?", "this guide - F1 again closes it"],
      ["S", "settings"],
      ["L", "library - bookmarked and rated frames, and frames by LoRA"],
      ["V", "Blink - compare this frame with its re-rolls and versions"],
      ["F5", "rescan the folders"],
      ["E", "show the file in Explorer"],
      ["C", "copy the file's path"],
      [classic ? "Esc / Q / Ctrl+Q" : "Esc / Ctrl+Q", "quit - asks first unless you turned that off (Esc closes the info panel first)"],
    ]],
  ];
}

document.addEventListener("keydown", (ev) => {
  if (S.panel) {
    // a panel is open: viewer keys are off, so typing in a field never triggers them
    if (S.panel === "keys" && (ev.key === "F1" || ev.key === "?")) {
      ev.preventDefault();
      closePanel();
    } else if (ev.key === "Escape") {
      closePanel();
    } else if (S.panel === "quit" && ev.key === "Enter") {
      ev.preventDefault();
      $("go").click();
    } else if (ev.key === "F1") {
      ev.preventDefault();
    }
    return;
  }
  const act = actionAt(ev);
  if (act >= 0) {
    // the user's own keys first: Settings refuses one the keymap uses, and this lets Ctrl+Shift / Alt combinations through
    ev.preventDefault();
    runAction(act);
    return;
  }
  const wf = workflowAt(ev);
  if (wf >= 0) {
    ev.preventDefault();
    sendToWorkflow(wf);
    return;
  }
  let k = ev.key;
  if (k.length === 1 && !ev.shiftKey) k = k.toLowerCase();
  if (ev.ctrlKey) k = "Ctrl+" + k.toLowerCase();
  const map = KEYMAPS[S.settings ? S.settings.keymap : "loupe"] || KEYMAPS.loupe;
  const fn = map[k];
  if (fn) {
    ev.preventDefault();
    fn();
  } else if (ev.ctrlKey || ev.key === "F5" || ev.key === "F3" || ev.key === "F7") {
    ev.preventDefault(); // keep the webview's own shortcuts (reload, print, find) out of the viewer
  }
});

document.addEventListener("wheel", (ev) => {
  if (S.panel || infoEl.contains(ev.target)) return;
  step(ev.deltaY < 0 ? -1 : 1);
}, { passive: true });

stage.addEventListener("click", () => showHud());
stage.addEventListener("dblclick", () => invoke("toggle_fullscreen"));

let idleTimer = null;
document.addEventListener("mousemove", () => {
  document.body.classList.remove("idle");
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => { if (!S.panel) document.body.classList.add("idle"); }, 2500);
});

async function toggleTopmost() {
  S.settings.topmost = !S.settings.topmost;
  await invoke("set_topmost", { on: S.settings.topmost });
  saveQuiet();
  showHud();
}

async function toggleRejected() {
  S.settings.show_rejected = !S.settings.show_rejected;
  await saveQuiet();
  toast(S.settings.show_rejected ? "Showing skipped frames" : "Skipped frames are passed over");
  showHud();
}

async function saveQuiet() {
  try {
    S.settings = await invoke("save_settings", { s: S.settings });
  } catch (e) {
    toast(String(e), true);
  }
}

// ------------------------------------------------------------------------------------------------ panels

function openPanel(kind, html, cls) {
  S.panel = kind;
  panel.className = cls || "";
  panel.innerHTML = html;
  veil.hidden = false;
  document.body.classList.remove("idle");
}

function closePanel() {
  S.panel = null;
  veil.hidden = true;
  panel.innerHTML = "";
  panel.className = "";
}

veil.addEventListener("mousedown", (ev) => {
  if (ev.target === veil && S.panel !== "first") closePanel();
});

function keyChips(keys) {
  return keys.split(" / ").map((k) => `<kbd>${esc(k)}</kbd>`).join('<span class="or">or</span>');
}

function toggleHelp() {
  if (S.panel === "keys") return closePanel();
  if (S.panel) return;
  const km = S.settings.keymap;
  const groups = helpGroups(km)
    .map(([title, rows]) => `<section class="group"><h2>${esc(title)}</h2><dl>${rows.map(([k, d]) => `<dt>${keyChips(k)}</dt><dd>${esc(d)}</dd>`).join("")}</dl></section>`)
    .join("");
  openPanel(
    "keys",
    `<h1><span class="mark">BlinkLoupe</span> keys</h1>
     <p class="lede">${km === "classic" ? "Classic keymap - the original Output Viewer's keys" : "Standard keymap"} · change it in Settings (S) · <kbd>F1</kbd> or <kbd>Esc</kbd> closes this guide</p>
     <div class="legend">${groups}</div>
     <p class="hint">Mouse: the wheel steps through frames, a click brings up the HUD, a double-click toggles full screen.</p>
     <div class="actions"><button class="link" id="toSettings">Settings…</button><span style="flex:1"></span><button class="primary" id="ok">Close</button></div>`,
    "wide"
  );
  $("ok").onclick = closePanel;
  $("toSettings").onclick = () => { closePanel(); openSettings(); };
  $("ok").focus();
}

// Esc / Q / Ctrl+Q: ask first unless the user said not to (Settings > Ask before quitting turns it back on)
function confirmQuit() {
  if (!S.settings.confirm_quit) return invoke("quit");
  if (S.panel) return;
  openPanel(
    "quit",
    `<h1>Quit <span class="mark">BlinkLoupe</span>?</h1>
     <p class="lede">BlinkLoupe stops watching for new frames. Your picks and marks are already saved beside the images.</p>
     <label class="check"><input type="checkbox" id="never"> Don't ask again</label>
     <p class="hint">You can turn this question back on in Settings (S) under Ask before quitting.</p>
     <div class="actions"><button id="stay">Stay</button><button class="primary" id="go">Quit</button></div>`,
    "small"
  );
  $("stay").onclick = closePanel;
  $("go").onclick = async () => {
    if ($("never").checked) {
      S.settings.confirm_quit = false;
      await saveQuiet();
    }
    invoke("quit");
  };
  $("go").focus();
}

function folderItems(list, removable) {
  return list.map((f, i) => `<li><span class="p">${esc(f.path || f)}</span>${f.source ? `<span class="src">${esc(f.source)}</span>` : ""}${removable ? `<button class="link" data-rm="${i}">Remove</button>` : `<button data-add="${esc(f.path)}">Add</button>`}</li>`).join("");
}

// Why a connection check found no ComfyUI: the server's own reason when one was given (refused token, certificate).
function noServer(st, typed) {
  if (st.error) return `No ComfyUI: ${st.error}`;
  return "No ComfyUI answered" + (typed ? " at that address" : " on 8188 or 8000 - type its address if it runs elsewhere");
}

// The follower's state in one line, for Settings.
function feedText(f) {
  if (!f || !f.on) return "";
  if (!f.url) return f.error || "Looking for ComfyUI…";
  let t = `Following ${f.url}${f.socket ? "" : " (polling - no status socket)"} · ${f.saved} saved`;
  if (f.local) t += ` · ${f.local} already in watched folders`;
  if (f.last) t += ` · last: ${f.last}`;
  if (f.error) t += ` · ${f.error}`;
  return t;
}

// The setup wizard: on first run, and again from Settings. Five short steps; nothing is saved until Start watching, except
// the ComfyUI address the Connect check uses.
async function openFirstRun() {
  const st = S.settings;
  const w = {
    step: 0,
    roots: (S.sessionRoots ? [] : st.roots || []).slice(),
    follow: st.follow_server,
    monitor: st.monitor,
    fullscreen: st.fullscreen,
    keymap: st.keymap,
    updates: st.check_updates,
    found: null,
    comfyChecked: false,
  };
  const steps = ["Welcome", "Your images", "ComfyUI", "Screen and keys", "Ready"];
  const mons = await invoke("monitors").catch(() => []);
  const opt = (v, label, cur) => `<option value="${esc(v)}"${v === cur ? " selected" : ""}>${esc(label)}</option>`;
  const dots = () => `<ol class="steps">${steps.map((t, i) => `<li class="${i === w.step ? "on" : i < w.step ? "done" : ""}">${esc(t)}</li>`).join("")}</ol>`;
  const nav = (next, nextLabel) => `<div class="actions">${w.step > 0 ? '<button id="back">Back</button>' : ""}<span style="flex:1"></span>${next ? `<button class="primary" id="next">${nextLabel || "Next"}</button>` : ""}</div>`;
  const pages = [
    () => `<h1>Welcome to <span class="mark">BlinkLoupe</span></h1>
      <p class="lede">BlinkLoupe shows every new image from your AI image tool the moment it is saved - full screen on a second monitor if you have one - and lets you sort them from the keyboard while you keep generating.</p>
      <ul class="points">
        <li><b>Live.</b> New frames appear on their own. Step back to study one and BlinkLoupe holds it, counting what arrives; End returns to live.</li>
        <li><b>One key each.</b> Pick, skip, rate 0-5, bookmark, and with ComfyUI re-roll or finish in high resolution.</li>
        <li><b>Your files stay put.</b> BlinkLoupe never moves, renames or changes an image. Marks are small text files beside them.</li>
      </ul>
      <p class="hint">This takes a minute. Everything here can be changed later in Settings (S).</p>`,
    () => `<h1>Your images</h1>
      <p class="lede">Which folders should BlinkLoupe watch? Their subfolders are watched too, four levels down.</p>
      <h2>Watch</h2><ul class="folders" id="chosen"></ul>
      <div><button id="browse">Choose a folder…</button></div>
      <h2>Found on this computer</h2><ul class="folders" id="found"><li><span class="p">Looking…</span></li></ul>`,
    () => `<h1>ComfyUI <span class="opt">optional</span></h1>
      <p class="lede">With ComfyUI running, R re-rolls a frame with the exact graph that made it, Shift+P finishes it in high resolution, and your own keys can send it into any workflow. BlinkLoupe works without it too.</p>
      <div class="row"><label for="faddr">Address</label><div><input id="faddr" type="text" value="${esc(st.comfy_url)}" placeholder="found by itself on this computer; elsewhere: http://192.168.1.20:8188 or https://your-server"><div class="status" id="fcon">Looking…</div></div></div>
      <div class="row"><label for="ftoken">Access token</label><input id="ftoken" type="password" autocomplete="off" value="${esc(st.comfy_token)}" placeholder="only if the server asks for one"></div>
      <div class="row"><label></label><div><button id="fconnect">Connect</button></div></div>
      <div class="row"><label></label><div><label class="check"><input type="checkbox" id="ffollow"${w.follow ? " checked" : ""}> Follow its queue too</label><div class="hint">Shows every image ComfyUI finishes, Preview Image outputs included - even from a ComfyUI on another computer or in the cloud. Their copies are saved to Pictures\\BlinkLoupe.</div></div></div>`,
    () => `<h1>Screen and keys</h1>
      <div class="row"><label for="wmon">Monitor</label><select id="wmon">${opt("", "Automatic - the largest second monitor", w.monitor)}${mons.map((m) => opt(m.name, `${m.name.replace(/^\\\\\.\\/, "")}  ${m.width}×${m.height}${m.primary ? "  (main)" : ""}`, w.monitor)).join("")}</select></div>
      <div class="row"><label for="wfull">Full screen</label><select id="wfull">${opt("auto", "Automatic - when there is a second monitor", w.fullscreen)}${opt("on", "Always", w.fullscreen)}${opt("off", "Never (a window)", w.fullscreen)}</select></div>
      <div class="row"><label>Keys</label><div>
        <label class="check"><input type="radio" name="wkeys" value="loupe"${w.keymap !== "classic" ? " checked" : ""}> Standard - P pick, X skip, 0-5 stars, R re-roll (like photo-culling tools)</label>
        <label class="check"><input type="radio" name="wkeys" value="classic"${w.keymap === "classic" ? " checked" : ""}> Classic - X re-roll, D skip, R rotate</label></div></div>
      <div class="row"><label>Updates</label><div><label class="check"><input type="checkbox" id="wupd"${w.updates ? " checked" : ""}> Check for a new version once a day</label><div class="hint">BlinkLoupe asks github.com whether a newer release is out. Nothing about you or your images is sent.</div></div></div>`,
    () => {
      const classic = w.keymap === "classic";
      const rows = [
        ["← → / wheel", "previous / next frame"],
        ["End", "back to the newest frame, live"],
        ["Space", "pause"],
        ["P", "pick"],
        [classic ? "D" : "X", "skip"],
        ["0 - 5", "stars"],
        ["B", "bookmark"],
        [classic ? "X" : "R", "re-roll with ComfyUI"],
        ["I", "what made this image: model, LoRAs, prompts"],
        ["L", "library: bookmarks, ratings, by LoRA"],
        ["V", "Blink: compare this frame with its versions"],
        ["S", "settings"],
        ["F1", "every key"],
      ];
      return `<h1>Ready</h1>
        <p class="lede">The keys you'll use most. <kbd>F1</kbd> shows them all, any time.</p>
        <table class="keys">${rows.map(([k, d]) => `<tr><td>${keyChips(k)}</td><td>${esc(d)}</td></tr>`).join("")}</table>
        <p class="hint">Run this setup again from Settings (S).</p>`;
    },
  ];
  const keep = () => {
    // read the page's fields into w before it is redrawn
    if ($("ffollow")) w.follow = $("ffollow").checked;
    if ($("wmon")) {
      w.monitor = $("wmon").value;
      w.fullscreen = $("wfull").value;
      w.keymap = (panel.querySelector('input[name="wkeys"]:checked') || {}).value || "loupe";
      w.updates = $("wupd").checked;
    }
  };
  const drawFolders = () => {
    $("chosen").innerHTML = w.roots.length ? folderItems(w.roots, true) : '<li><span class="p" style="color:var(--faint)">No folder yet - add one below, or follow ComfyUI on the next page</span></li>';
    $("chosen").querySelectorAll("[data-rm]").forEach((b) => (b.onclick = () => { w.roots.splice(+b.dataset.rm, 1); drawFolders(); }));
    if (!w.found) return;
    const rest = w.found.filter((f) => !w.roots.some((r) => r.toLowerCase() === f.path.toLowerCase()));
    $("found").innerHTML = rest.length ? folderItems(rest, false) : `<li><span class="p" style="color:var(--faint)">${w.found.length ? "All added" : "No ComfyUI, A1111 or Forge output folder found - choose one above"}</span></li>`;
    $("found").querySelectorAll("[data-add]").forEach((b) => (b.onclick = () => { w.roots.push(b.dataset.add); drawFolders(); }));
  };
  const connect = async (first) => {
    $("fcon").className = "status";
    $("fcon").textContent = "Looking…";
    S.settings.comfy_url = $("faddr").value.trim();
    S.settings.comfy_token = $("ftoken").value.trim();
    await saveQuiet();
    const c = await invoke("comfy_status", { force: true });
    if (!$("fcon")) return; // the page moved on meanwhile
    $("fcon").className = "status " + (c.url ? "ok" : "bad");
    $("fcon").textContent = c.url ? `Connected: ${c.url}` : noServer(c, $("faddr").value.trim());
    // following catches Preview Image outputs and never copies what a watched folder already holds, so a server that
    // answers is followed unless the person unticks it; one on another computer can only be followed
    if (c.url && first && !w.comfyChecked && (!w.roots.length || $("faddr").value.trim())) $("ffollow").checked = true;
    if (!c.url) $("ffollow").checked = false;
    w.comfyChecked = true;
    keep();
  };
  const draw = () => {
    openPanel("first", dots() + pages[w.step]() + nav(true, w.step === 0 ? "Set up" : w.step === steps.length - 1 ? "Start watching" : "Next"), "wizard");
    if ($("back")) $("back").onclick = () => { keep(); w.step--; draw(); };
    $("next").onclick = async () => {
      keep();
      if (w.step === 2 && !w.roots.length && !w.follow) {
        return toast("Choose a folder to watch (Back), or follow ComfyUI's queue", true);
      }
      if (w.step < steps.length - 1) { w.step++; draw(); return; }
      S.settings.roots = w.roots.slice();
      S.settings.follow_server = w.follow;
      S.settings.monitor = w.monitor;
      S.settings.fullscreen = w.fullscreen;
      S.settings.keymap = w.keymap;
      S.settings.check_updates = w.updates;
      await saveQuiet();
      closePanel();
      const info = await invoke("start");
      S.live = true;
      S.unseen = 0;
      if (info.newest) show(info.newest);
      else message("Waiting for images\n" + info.roots.join("\n") + "\n\nF1 shows the keys");
    };
    if (w.step === 1) {
      drawFolders();
      $("browse").onclick = async () => {
        const p = await invoke("pick_folder");
        if (p && !w.roots.includes(p)) { w.roots.push(p); drawFolders(); }
      };
      if (!w.found) {
        invoke("detect_folders").then((found) => {
          w.found = found;
          if (found.length && !w.roots.length) w.roots.push(found[0].path);
          if ($("chosen")) drawFolders();
        });
      }
    }
    if (w.step === 2) {
      $("fconnect").onclick = () => connect(false);
      $("ffollow").onchange = keep;
      connect(true);
    }
    $("next").focus();
  };
  draw();
}

async function openSettings() {
  const s = JSON.parse(JSON.stringify(S.settings));
  const mons = await invoke("monitors");
  const opt = (v, label, cur) => `<option value="${esc(v)}"${v === cur ? " selected" : ""}>${esc(label)}</option>`;
  openPanel("settings", `<h1><span class="mark">BlinkLoupe</span> settings</h1>
    ${S.sessionRoots ? `<div class="note">This session watches <b>${esc(S.sessionRoots.join(", "))}</b> (started with --root). Saved folders apply from the next normal start.</div>` : ""}
    <div class="row"><label></label><div><button class="link" id="rerun">Run the setup again…</button></div></div>
    <h2>Folders</h2><ul class="folders" id="roots"></ul>
    <div class="row"><label></label><div><button id="add">Add a folder…</button> <button class="link" id="detect">Find ComfyUI folders</button></div></div>
    <ul class="folders" id="found"></ul>
    <div class="row"><label for="depth">Subfolder depth</label><div><input id="depth" type="number" min="0" max="12" value="${s.depth}"><div class="hint">How many folder levels under each watched folder to include.</div></div></div>
    <h2>Display</h2>
    <div class="row"><label for="monitor">Monitor</label><select id="monitor">${opt("", "Automatic - the largest secondary monitor", s.monitor)}${mons.map((m) => opt(m.name, `${m.name.replace(/^\\\\\.\\/, "")}  ${m.width}×${m.height}${m.primary ? "  (primary)" : ""}`, s.monitor)).join("")}</select></div>
    <div class="row"><label for="fullscreen">Full screen</label><select id="fullscreen">${opt("auto", "Automatic - when a second monitor exists", s.fullscreen)}${opt("on", "Always", s.fullscreen)}${opt("off", "Never (a window)", s.fullscreen)}</select></div>
    <div class="row"><label for="hud_secs">HUD stays up</label><div><input id="hud_secs" type="number" min="1" max="120" value="${s.hud_secs}"><div class="hint">Seconds after each new frame (H pins it).</div></div></div>
    <div class="row"><label for="clock24">Clock</label><select id="clock24">${opt("12", "12-hour (2:14:07 pm)", s.clock24 ? "24" : "12")}${opt("24", "24-hour (14:14:07)", s.clock24 ? "24" : "12")}</select></div>
    <div class="row"><label for="blank">Black-frame alarm</label><div><input id="blank" type="number" min="0" max="255" value="${s.blank_threshold}"><div class="hint">A frame whose brightest pixel is below this (0-255) is flagged BLANK - a NaN render.</div></div></div>
    <h2>Keys</h2>
    <div class="row"><label for="keymap">Keymap</label><select id="keymap">${opt("loupe", "Standard - P pick, X skip, 0-5 stars, R re-roll", s.keymap)}${opt("classic", "Classic Output Viewer - P pick, X re-roll, R rotate", s.keymap)}</select></div>
    <div class="row"><label for="cq">Ask before quitting</label><div><label class="check"><input type="checkbox" id="cq"${s.confirm_quit ? " checked" : ""}> Esc ${s.keymap === "classic" ? "/ Q " : ""}/ Ctrl+Q ask "Quit BlinkLoupe?" first</label><div class="hint">Turned off by "Don't ask again" in that question; tick it to bring the question back.</div></div></div>
    <h2>Picks</h2>
    <div class="row"><label for="pcopy">Also copy picks to</label><div><input id="pcopy" type="text" placeholder="empty = picks go only to PICKS.txt beside the image" value="${esc(s.picks_copy_dir)}"><div class="hint">A pick copies the frame here, into a folder named after its set; unpicking removes that copy (unless it has been changed since). The frame itself never moves, and BlinkLoupe doesn't watch this folder.</div></div></div>
    <div class="row"><label></label><div><button id="pcopyb">Choose a folder…</button> <button class="link" id="pcopyx">Clear</button></div></div>
    <h2>Re-rolls</h2>
    <div class="row"><label for="reroll_mode">A re-roll</label><select id="reroll_mode">${opt("auto", "Queues in ComfyUI when it answers, else marks", s.reroll_mode)}${opt("queue", "Always queues in ComfyUI", s.reroll_mode)}${opt("mark", "Only marks REROLLS.txt (your own pipeline renders them)", s.reroll_mode)}</select></div>
    <div class="row"><label for="suffix">Re-roll file suffix</label><div><input id="suffix" type="text" value="${esc(s.reroll_suffix)}"><div class="hint">Added to each save node's filename prefix, so re-rolls land beside the original and are easy to tell apart. Empty keeps the prefix.</div></div></div>
    <div class="row"><label for="comfyurl">ComfyUI address</label><div><input id="comfyurl" type="text" placeholder="automatic: http://127.0.0.1:8188, then :8000 - or https://your-server" value="${esc(s.comfy_url)}"><div class="hint">http:// or https://. A server behind a password proxy: https://user:password@host. Under a path: https://host/comfy.</div><div class="status" id="cstat"></div></div></div>
    <div class="row"><label for="ctoken">Access token</label><div><input id="ctoken" type="password" autocomplete="off" placeholder="only if the server asks for one" value="${esc(s.comfy_token)}"><div class="hint">Sent as <code>Authorization: Bearer …</code> with every request (ComfyUI-Login, token proxies). Kept in BlinkLoupe's settings file on this computer.</div></div></div>
    <div class="row"><label></label><div><button id="test">Test connection</button></div></div>
    <h2>Following ComfyUI</h2>
    <div class="row"><label for="follow">Follow the server</label><div><label class="check"><input type="checkbox" id="follow"${s.follow_server ? " checked" : ""}> Show every image ComfyUI finishes, read from its queue</label><div class="hint">For a ComfyUI on another computer, in Docker or in the cloud, and for Preview Image outputs, which never reach the output folder. Its images are saved to the folder below and appear here live. Outputs already in a watched folder are not copied.</div></div></div>
    <div class="row"><label for="fprev">Preview Image outputs</label><div><label class="check"><input type="checkbox" id="fprev"${s.feed_previews ? " checked" : ""}> Include them</label></div></div>
    <div class="row"><label for="fdir">Save server images to</label><div><input id="fdir" type="text" placeholder="${esc(S.feedDefault || "")}" value="${esc(s.feed_dir)}"><div class="status" id="fstat"></div></div></div>
    <div class="row"><label></label><div><button id="fdirb">Choose a folder…</button></div></div>
    <h2>Hires on picks</h2>
    <p class="hint" style="margin:0 0 6px">Shift+P finishes the frame on screen, Ctrl+P every pick in its folder. The frame is uploaded to ComfyUI and finished with its own model, LoRAs, prompts and VAE: upscale, then one low-denoise pass with tiled VAE. The base pass never re-runs.</p>
    <div class="row"><label for="hscale">Scale</label><div><input id="hscale" type="number" min="1" max="8" step="0.25" value="${s.hires_scale}"><div class="hint">Times the frame's size…</div></div></div>
    <div class="row"><label for="hmp">At most (megapixels)</label><div><input id="hmp" type="number" min="0" max="64" step="0.1" value="${s.hires_max_mp}"><div class="hint">…capped here (0 = no cap). 2 x a 1 MP frame is 4 MP; 4.2 keeps a 2 MP frame near 4 MP.</div></div></div>
    <div class="row"><label for="hden">Denoise</label><input id="hden" type="number" min="0.05" max="1" step="0.05" value="${s.hires_denoise}"></div>
    <div class="row"><label for="hsteps">Steps</label><div><input id="hsteps" type="number" min="0" max="100" value="${s.hires_steps}"><div class="hint">0 = the frame's own step count.</div></div></div>
    <div class="row"><label for="hmodel">Upscale model</label><select id="hmodel">${opt("", "None - lanczos only", s.upscale_model)}${s.upscale_model ? opt(s.upscale_model, s.upscale_model, s.upscale_model) : ""}</select></div>
    <div class="row"><label for="hsuffix">File suffix</label><input id="hsuffix" type="text" value="${esc(s.hires_suffix)}"></div>
    <div class="row"><label for="hfolder">Folder suffix</label><div><input id="hfolder" type="text" placeholder="empty = beside the original" value="${esc(s.hires_folder_suffix)}"><div class="hint">"-hires" saves into a sibling folder: set/frame becomes set-hires/frame_hires.</div></div></div>
    <h2>Actions</h2>
    <p class="hint" style="margin:0 0 6px">Your own keys. Each runs a command on the frame on screen, in its folder: <code>{file}</code> is its path, <code>{folder}</code> its folder, <code>{name}</code> its file name, <code>{stem}</code> the name without the extension. Click the key box and press the key. With <b>Wait</b> on, BlinkLoupe shows the last line the command prints (<code>FINISHED &lt;file&gt;</code>, <code>ALREADY &lt;file&gt;</code> and <code>ERROR &lt;why&gt;</code> are read specially); with it off, BlinkLoupe starts it and carries on, for an editor or another viewer.</p>
    <div class="acthead"><span>Key</span><span>Name</span><span>Command</span><span></span><span></span></div>
    <div class="acts" id="acts"></div>
    <div class="row"><label></label><div><button id="addact">Add an action</button></div></div>
    <h2>Send to a workflow</h2>
    <p class="hint" style="margin:0 0 6px">A key that sends the frame on screen into one of your own ComfyUI workflows - an upscaler, a face fix, img2img, a video. Choose the file ComfyUI's <b>Workflow &gt; Export (API)</b> writes, or any image that workflow made. Its Load Image node gets the frame; with more than one, title the one BlinkLoupe fills <b>Loupe</b>. Text inputs can say <code>{positive}</code> <code>{negative}</code> <code>{stem}</code> <code>{name}</code> <code>{folder}</code> to take the frame's. <b>New seed</b> gives its samplers fresh seeds each time; <b>Beside</b> saves the result next to the frame as <code>&lt;name&gt;_&lt;key name&gt;</code>.</p>
    <div class="wfhead"><span>Key</span><span>Name</span><span>Workflow</span><span></span></div>
    <div class="acts" id="wfs"></div>
    <div class="row"><label></label><div><button id="addwf">Add a workflow</button></div></div>
    <h2>Updates</h2>
    <div class="row"><label>Version</label><div><span id="uver"></span><div class="status" id="ustat"></div></div></div>
    <div class="row"><label></label><div><button id="ucheck">Check now</button> <button class="primary" id="uinstall" hidden></button> <button id="upage" hidden>Release page</button> <button class="link" id="ulogs">Open the log folder</button></div></div>
    <div class="row"><label for="cupd">Check by itself</label><div><label class="check"><input type="checkbox" id="cupd"${s.check_updates ? " checked" : ""}> Once a day, ask github.com whether a newer BlinkLoupe is out</label><div class="hint">Nothing about you or your images is sent. A copy installed with BlinkLoupe's installer updates itself in one click; any other copy points you to the download.</div></div></div>
    <div class="row"><label for="uurl">Update feed</label><div><input id="uurl" type="text" placeholder="the public releases (leave empty)" value="${esc(s.update_url)}"><div class="hint">Only for testing your own builds: the address of a latest.json.</div></div></div>
    <div class="actions"><button class="link" id="keys">Keys…</button><span style="flex:1"></span><button id="cancel">Cancel</button><button class="primary" id="save">Save</button></div>`);
  const drawRoots = () => {
    $("roots").innerHTML = s.roots.length ? folderItems(s.roots, true) : '<li><span class="p" style="color:var(--faint)">No folders - add one</span></li>';
    $("roots").querySelectorAll("[data-rm]").forEach((b) => (b.onclick = () => { s.roots.splice(+b.dataset.rm, 1); drawRoots(); }));
  };
  drawRoots();
  const actRow = (a) => `<div class="act">
      <input class="akey" type="text" readonly placeholder="press a key" value="${esc(a.key)}" title="Click, then press the key (Backspace clears)">
      <input class="alabel" type="text" placeholder="Name" value="${esc(a.label)}">
      <input class="acmd" type="text" placeholder='"C:\\Tools\\tool.exe" "{file}"' value="${esc(a.cmd)}">
      <label class="check" title="Wait for it and show its last line"><input type="checkbox" class="await"${a.wait ? " checked" : ""}> Wait</label>
      <button class="link arm">Remove</button></div>`;
  const wireActs = () => {
    $("acts").querySelectorAll(".act").forEach((row) => {
      row.querySelector(".arm").onclick = () => row.remove();
      row.querySelector(".akey").onkeydown = (ev) => {
        if (ev.key === "Escape" || ev.key === "Tab") return;
        ev.preventDefault();
        if (ev.key === "Backspace" || ev.key === "Delete") ev.target.value = "";
        else if (keyName(ev)) ev.target.value = keyName(ev);
      };
    });
  };
  $("acts").innerHTML = (s.actions || []).map(actRow).join("");
  wireActs();
  const wfRow = (w) => `<div class="wf">
      <input class="akey" type="text" readonly placeholder="press a key" value="${esc(w.key)}" title="Click, then press the key (Backspace clears)">
      <input class="alabel" type="text" placeholder="Name" value="${esc(w.label)}">
      <input class="wfile" type="text" placeholder="the Export (API) .json, or an image it made" value="${esc(w.file)}">
      <button class="wbrowse">Choose…</button>
      <div class="wopts">
        <label class="check" title="New seeds on its samplers each time"><input type="checkbox" class="wseed"${w.new_seed ? " checked" : ""}> New seed</label>
        <label class="check" title="Save the result next to the frame"><input type="checkbox" class="wbeside"${w.beside ? " checked" : ""}> Beside the frame</label>
        <span style="flex:1"></span><button class="link arm">Remove</button></div>
      <div class="status wsum"></div></div>`;
  const checkWf = async (row) => {
    const f = row.querySelector(".wfile").value.trim();
    const st = row.querySelector(".wsum");
    st.className = "status wsum";
    if (!f) { st.textContent = ""; return; }
    st.textContent = "Reading…";
    try {
      st.textContent = await invoke("workflow_summary", { file: f });
      st.className = "status wsum ok";
    } catch (e) {
      st.textContent = String(e);
      st.className = "status wsum bad";
    }
  };
  const wireWfs = () => {
    $("wfs").querySelectorAll(".wf").forEach((row) => {
      if (row.dataset.wired) return;
      row.dataset.wired = "1";
      row.querySelector(".arm").onclick = () => row.remove();
      row.querySelector(".akey").onkeydown = (ev) => {
        if (ev.key === "Escape" || ev.key === "Tab") return;
        ev.preventDefault();
        if (ev.key === "Backspace" || ev.key === "Delete") ev.target.value = "";
        else if (keyName(ev)) ev.target.value = keyName(ev);
      };
      row.querySelector(".wfile").onchange = () => checkWf(row);
      row.querySelector(".wbrowse").onclick = async () => {
        const p = await invoke("pick_workflow");
        if (!p) return;
        row.querySelector(".wfile").value = p;
        const name = row.querySelector(".alabel");
        if (!name.value.trim()) name.value = p.split(/[\\/]/).pop().replace(/\.[^.]+$/, "");
        checkWf(row);
      };
      checkWf(row);
    });
  };
  $("wfs").innerHTML = (s.workflows || []).map(wfRow).join("");
  wireWfs();
  $("addwf").onclick = () => {
    $("wfs").insertAdjacentHTML("beforeend", wfRow({ key: "", label: "", file: "", new_seed: true, beside: true }));
    wireWfs();
    $("wfs").lastElementChild.querySelector(".akey").focus();
  };
  $("addact").onclick = () => {
    $("acts").insertAdjacentHTML("beforeend", actRow({ key: "", label: "", cmd: "", wait: true }));
    wireActs();
    $("acts").lastElementChild.querySelector(".akey").focus();
  };
  $("pcopyb").onclick = async () => {
    const p = await invoke("pick_folder");
    if (p) $("pcopy").value = p;
  };
  $("pcopyx").onclick = () => ($("pcopy").value = "");
  $("fdirb").onclick = async () => {
    const p = await invoke("pick_folder");
    if (p) $("fdir").value = p;
  };
  $("add").onclick = async () => {
    const p = await invoke("pick_folder");
    if (p && !s.roots.includes(p)) { s.roots.push(p); drawRoots(); }
  };
  $("detect").onclick = async () => {
    $("found").innerHTML = '<li><span class="p">Looking…</span></li>';
    const found = (await invoke("detect_folders")).filter((f) => !s.roots.some((r) => r.toLowerCase() === f.path.toLowerCase()));
    $("found").innerHTML = found.length ? folderItems(found, false) : '<li><span class="p" style="color:var(--faint)">Nothing new found</span></li>';
    $("found").querySelectorAll("[data-add]").forEach((b) => (b.onclick = () => { s.roots.push(b.dataset.add); b.closest("li").remove(); drawRoots(); }));
  };
  const test = async () => {
    $("cstat").className = "status";
    $("cstat").textContent = "Checking…";
    const keep = [S.settings.comfy_url, S.settings.comfy_token];
    S.settings.comfy_url = $("comfyurl").value.trim();
    S.settings.comfy_token = $("ctoken").value.trim();
    await saveQuiet();
    const st = await invoke("comfy_status", { force: true });
    [S.settings.comfy_url, S.settings.comfy_token] = keep;
    await saveQuiet();
    $("cstat").className = "status " + (st.url ? "ok" : "bad");
    $("cstat").textContent = st.url ? `Connected: ${st.url}` : noServer(st, $("comfyurl").value.trim());
  };
  $("test").onclick = test;
  $("keys").onclick = () => { closePanel(); toggleHelp(); };
  $("rerun").onclick = () => { closePanel(); openFirstRun(); };
  const drawUpdate = (u) => {
    if (!$("uver")) return;
    $("uver").textContent = `BlinkLoupe ${u.version}`;
    const when = u.state.checked ? clock(u.state.checked) : "";
    const n = u.newer;
    $("ustat").className = "status " + (n ? "ok" : u.state.error ? "bad" : "");
    $("ustat").textContent = n
      ? `BlinkLoupe ${n.version} is out${n.notes ? ": " + n.notes : ""}`
      : u.state.error ? `Couldn't check${when ? " at " + when : ""}: ${u.state.error}` : when ? `Up to date · checked ${when}` : "Not checked yet";
    $("uinstall").hidden = !(n && u.installed);
    $("uinstall").textContent = n ? `Install ${n.version}` : "";
    $("upage").hidden = !(n && !u.installed);
    $("upage").onclick = () => invoke("open_url", { url: u.page }).catch((e) => toast(String(e), true));
  };
  invoke("update_info").then(drawUpdate);
  $("ucheck").onclick = async () => {
    $("ustat").className = "status";
    $("ustat").textContent = "Checking…";
    const keep = S.settings.update_url;
    S.settings.update_url = $("uurl").value.trim();
    await saveQuiet();
    drawUpdate(await invoke("check_updates_now"));
    S.settings.update_url = keep;
    await saveQuiet();
  };
  $("uinstall").onclick = installUpdate;
  $("ulogs").onclick = () => invoke("open_log_folder").catch((e) => toast(String(e), true));
  $("cancel").onclick = closePanel;
  $("save").onclick = async () => {
    const taken = builtinKeys($("keymap").value);
    const acts = [], seen = new Set();
    for (const row of $("acts").querySelectorAll(".act")) {
      const a = {
        key: row.querySelector(".akey").value.trim(),
        label: row.querySelector(".alabel").value.trim(),
        cmd: row.querySelector(".acmd").value.trim(),
        wait: row.querySelector(".await").checked,
      };
      if (!a.key && !a.cmd) continue;
      const what = a.label || a.cmd;
      if (!a.key) return toast(`"${what}" needs a key - click its key box and press one`, true);
      if (!a.cmd) return toast(`${a.key} needs a command`, true);
      if ([...taken].some((t) => t.toLowerCase() === a.key.toLowerCase())) return toast(`${a.key} is one of BlinkLoupe's own keys (F1 lists them) - choose another`, true);
      if (seen.has(a.key.toLowerCase())) return toast(`${a.key} is used by two actions`, true);
      seen.add(a.key.toLowerCase());
      acts.push(a);
    }
    s.actions = acts;
    const wfs = [];
    for (const row of $("wfs").querySelectorAll(".wf")) {
      const w = {
        key: row.querySelector(".akey").value.trim(),
        label: row.querySelector(".alabel").value.trim(),
        file: row.querySelector(".wfile").value.trim().replace(/^"|"$/g, ""),
        new_seed: row.querySelector(".wseed").checked,
        beside: row.querySelector(".wbeside").checked,
      };
      if (!w.key && !w.file) continue;
      if (!w.key) return toast(`"${w.label || w.file}" needs a key - click its key box and press one`, true);
      if (!w.file) return toast(`${w.key} needs a workflow file`, true);
      if ([...taken].some((t) => t.toLowerCase() === w.key.toLowerCase())) return toast(`${w.key} is one of BlinkLoupe's own keys (F1 lists them) - choose another`, true);
      if (seen.has(w.key.toLowerCase())) return toast(`${w.key} is used twice (actions and workflows share keys)`, true);
      seen.add(w.key.toLowerCase());
      wfs.push(w);
    }
    s.workflows = wfs;
    s.picks_copy_dir = $("pcopy").value.trim();
    s.follow_server = $("follow").checked;
    s.feed_previews = $("fprev").checked;
    s.feed_dir = $("fdir").value.trim();
    const watched = (x) => JSON.stringify([x.roots, x.depth, x.picks_copy_dir, x.follow_server, x.feed_dir]);
    const before = watched(S.settings);
    s.depth = +$("depth").value || 0;
    s.monitor = $("monitor").value;
    s.fullscreen = $("fullscreen").value;
    s.hud_secs = +$("hud_secs").value || 6;
    s.clock24 = $("clock24").value === "24";
    s.blank_threshold = +$("blank").value || 0;
    s.keymap = $("keymap").value;
    s.confirm_quit = $("cq").checked;
    s.reroll_mode = $("reroll_mode").value;
    s.reroll_suffix = $("suffix").value;
    s.comfy_url = $("comfyurl").value.trim();
    s.comfy_token = $("ctoken").value.trim();
    s.hires_scale = parseFloat($("hscale").value) || 2;
    s.hires_max_mp = parseFloat($("hmp").value);
    if (!(s.hires_max_mp >= 0)) s.hires_max_mp = 4.2;
    s.hires_denoise = parseFloat($("hden").value) || 0.3;
    s.hires_steps = parseInt($("hsteps").value, 10) || 0;
    s.upscale_model = $("hmodel").value;
    s.hires_suffix = $("hsuffix").value;
    s.hires_folder_suffix = $("hfolder").value.trim();
    s.check_updates = $("cupd").checked;
    s.update_url = $("uurl").value.trim();
    try {
      S.settings = await invoke("save_settings", { s });
    } catch (e) {
      toast(String(e), true);
      return;
    }
    closePanel();
    if (watched(S.settings) !== before) {
      const info = await invoke("start");
      S.live = true;
      S.unseen = 0;
      if (info.newest) show(info.newest);
      else message("Waiting for images\n" + info.roots.join("\n") + "\n\nF1 shows the keys");
    }
    refreshComfy(true);
    toast("Settings saved");
    showHud();
  };
  invoke("comfy_status", { force: false }).then((st) => {
    if (!$("cstat")) return;
    $("cstat").className = "status " + (st.url ? "ok" : "");
    $("cstat").textContent = st.url ? `Connected: ${st.url}` : "Not connected";
    $("fstat").className = "status " + (st.feed.on && st.feed.url ? "ok" : st.feed.error ? "bad" : "");
    $("fstat").textContent = feedText(st.feed);
  });
  invoke("upscale_models").then((models) => {
    const sel = $("hmodel");
    if (!sel) return;
    const have = new Set([...sel.options].map((o) => o.value));
    for (const m of models) {
      if (have.has(m)) continue;
      const o = document.createElement("option");
      o.value = m;
      o.textContent = m;
      if (m === s.upscale_model) o.selected = true;
      sel.appendChild(o);
    }
  });
}

boot().catch((e) => {
  message("BlinkLoupe could not start\n" + e);
  log("boot failed: " + e);
});
