# BlinkLoupe User Guide

For BlinkLoupe 0.10.0 on Windows 10 and 11.

BlinkLoupe is a companion for AI image generation. It has two windows:

- **The live view** (called Loupe before version 0.10.0) shows every new image full screen the moment it is saved, while
  you keep generating. You can sort images from the keyboard as they arrive, and with ComfyUI running you can make new
  versions of them.
- **Blink**, the comparison window, puts two images on top of each other so you can see exactly what changed.

You don't need to read this whole guide. Each part starts with a **quick start** that gets you going in a few minutes.
The guide then adds one idea at a time, and the last part, **Advanced**, covers everything in detail.

## Contents

- [Part 1: The live view](#part-1-the-live-view)
  - [Quick start: watch your images arrive](#quick-start-watch-your-images-arrive)
  - [1.1 Reading the screen](#11-reading-the-screen)
  - [1.2 LIVE, HISTORY and PAUSED](#12-live-history-and-paused)
  - [1.3 Moving around](#13-moving-around)
  - [1.4 How the picture is shown](#14-how-the-picture-is-shown)
  - [1.5 Marking images: pick, skip, stars, bookmark](#15-marking-images-pick-skip-stars-bookmark)
  - [1.6 Where marks are kept](#16-where-marks-are-kept)
  - [1.7 The info panel: how an image was made](#17-the-info-panel-how-an-image-was-made)
  - [1.8 The library: your bookmarked and rated images](#18-the-library-your-bookmarked-and-rated-images)
  - [1.9 Re-rolls: the same image with a new seed](#19-re-rolls-the-same-image-with-a-new-seed)
  - [1.10 Hires: finishing a pick at high resolution](#110-hires-finishing-a-pick-at-high-resolution)
  - [1.11 A tour of Settings](#111-a-tour-of-settings)
- [Part 2: Blink, the comparison window](#part-2-blink-the-comparison-window)
  - [Quick start: compare an image with its re-roll](#quick-start-compare-an-image-with-its-re-roll)
  - [2.1 The Blink window](#21-the-blink-window)
  - [2.2 A and B](#22-a-and-b)
  - [2.3 Zooming and panning](#23-zooming-and-panning)
  - [2.4 The seven views](#24-the-seven-views)
  - [2.5 Groups: what Blink finds for you](#25-groups-what-blink-finds-for-you)
  - [2.6 Comparing any images you choose](#26-comparing-any-images-you-choose)
  - [2.7 Reading the numbers](#27-reading-the-numbers)
  - [2.8 Comparing generation settings](#28-comparing-generation-settings)
- [Part 3: Advanced](#part-3-advanced)
  - [3.1 Your own keys (Actions)](#31-your-own-keys-actions)
  - [3.2 Send to a workflow](#32-send-to-a-workflow)
  - [3.3 Following a ComfyUI server](#33-following-a-comfyui-server)
  - [3.4 Servers elsewhere: HTTPS, tokens and passwords](#34-servers-elsewhere-https-tokens-and-passwords)
  - [3.5 How re-rolls work](#35-how-re-rolls-work)
  - [3.6 How hires works](#36-how-hires-works)
  - [3.7 How Blink finds groups](#37-how-blink-finds-groups)
  - [3.8 How new images are noticed](#38-how-new-images-are-noticed)
  - [3.9 What metadata BlinkLoupe reads](#39-what-metadata-blinkloupe-reads)
  - [3.10 Working with your own pipeline: the mark files](#310-working-with-your-own-pipeline-the-mark-files)
  - [3.11 The settings file](#311-the-settings-file)
  - [3.12 Starting BlinkLoupe from the command line](#312-starting-blinkloupe-from-the-command-line)
  - [3.13 Updates, uninstalling and privacy](#313-updates-uninstalling-and-privacy)
  - [3.14 Troubleshooting](#314-troubleshooting)
- [Appendix A: Every key](#appendix-a-every-key)
- [Appendix B: Words used in this guide](#appendix-b-words-used-in-this-guide)

---

# Part 1: The live view

## Quick start: watch your images arrive

This gets BlinkLoupe's main job working: it shows each new image the moment your image generator saves it.

**You need:** Windows 10 or 11 (64-bit), and an image generator that saves files to a folder. That can be ComfyUI,
A1111, Forge, SwarmUI or anything else that writes PNG, JPEG, WebP, GIF, BMP, MP4, WebM or MOV files. ComfyUI is
optional, and you don't need it for this quick start.

**1. Install.** Download `BlinkLoupe-0.10.0-setup.exe` from
[the latest release](https://github.com/mifunetron/blinkloupe-releases/releases/latest) and run it.

> Windows may say **"Windows protected your PC"**. That happens because BlinkLoupe isn't signed with a paid
> certificate yet. Click **More info**, then **Run anyway**.

Setup installs BlinkLoupe for your Windows account only, with no administrator rights needed. It adds two Start menu
entries, **BlinkLoupe** and **Blink (BlinkLoupe)**, and a Desktop shortcut if you leave that box ticked.

**2. Start BlinkLoupe.** The first time, a short setup appears:

| Step | What to do |
|---|---|
| Welcome | Click **Set up**. |
| Your images | Choose the folder your generator saves into. BlinkLoupe lists any output folders it found (ComfyUI, ComfyUI Desktop, portable ComfyUI, A1111, Forge, SwarmUI). If yours isn't listed, add it yourself. Folders inside it are watched too. |
| ComfyUI (optional) | If ComfyUI is running, BlinkLoupe usually finds it by itself. If you don't use ComfyUI, just click **Next**. |
| Screen and keys | Choose the monitor. **Automatic** uses your largest second monitor, or a window if you only have one screen. Leave the keymap on **Standard**. |
| Ready | A list of the keys you'll use most. Click **Start watching**. |

**3. Generate an image as you normally would.** As soon as the file is completely written, it appears in BlinkLoupe.
Each new image replaces the last one. This is **LIVE** mode, shown in the top-left corner.

**4. Learn three keys:**

| Key | Does |
|---|---|
| **←** | Go back to an earlier image. New images won't jump in while you look at it. |
| **End** | Return to LIVE, so the newest image shows again. |
| **F1** | Show every key. Press F1 again to close the list. |

That's all you need to use it as a live monitor. Press **Esc** to quit; BlinkLoupe asks first, so you can't close it by
accident.

> **Only one screen?** BlinkLoupe opens in a window. Press **F11** for full screen, and again to go back. You can also
> double-click the picture.

The rest of Part 1 adds one idea at a time. Read on when you want more.

---

## 1.1 Reading the screen

When a new image arrives, a strip of text, the **HUD** (heads-up display), shows over the picture for a few seconds and
then fades. Click the picture to bring it back. Press **H** to keep it on screen, and H again to let it fade.

**Top left:** the mode and the image's place.

```
LIVE                                   my-project\fox_00012_.png
```

- The mode: **LIVE**, **HISTORY** or **PAUSED** (see [1.2](#12-live-history-and-paused)).
- Where the image is, inside the folders you're watching.

**Bottom:** facts about the image, then tags.

```
1024×1024 · 1.0 MP   2:14:07 pm   frame 12/40   41 s after the previous   PICK  ★★★★
```

| Part | Means |
|---|---|
| `1024×1024 · 1.0 MP` | Its size in pixels and megapixels. |
| `2:14:07 pm` | When the file was saved. You can switch to a 24-hour clock in Settings. |
| `frame 12/40` | Its place in the folder. See [the set file](#the-set-file-loupe-setjson) for how a pipeline can make this read "frame 12 of the 40 planned". |
| `41 s after the previous` | Time since the image before it, which is roughly how long it took to render. |

**Tags** appear when they apply:

| Tag | Means |
|---|---|
| `PICK` | You picked it (P). |
| `BOOKMARK` | You bookmarked it (B). |
| `★★★` | Your star rating (1-5). |
| `SKIPPED` | You skipped it (X). |
| `REROLL ×2` | Re-rolls you've asked for and that are waiting in `REROLLS.txt`. |
| `SENT ×1` | Re-rolls of this image sent to ComfyUI since BlinkLoupe started. |
| `HIRES` | A hires finish has been queued for it. |
| `BLANK` (red) | The image is completely black, usually a failed render. See [Black-frame alarm](#display). |
| `fill`, `1:1`, `rot 90`, `top` | A view setting that's on: fill or actual size (F), rotated (O), always on top (T). |
| `showing 3 skipped` | Skipped images are being shown (K). |

**Bottom right:** ComfyUI's status. It says `ComfyUI not found`, or `ComfyUI · 2 in queue (1 yours)`, where "yours" is
the work BlinkLoupe queued. It adds `· following` when you've turned on [following a server](#33-following-a-comfyui-server).

## 1.2 LIVE, HISTORY and PAUSED

BlinkLoupe is always in one of three modes:

- **LIVE:** every new image is shown as soon as it lands. This is where you start.
- **HISTORY:** you stepped back to an earlier image. BlinkLoupe stays on it no matter what arrives, and counts the new
  ones instead: `HISTORY · 3 NEW - press End`. The HUD stays up while there's something new.
- **PAUSED:** **Space** freezes the view on the current image, as if you'd stepped back. Press Space again to resume
  LIVE.

**End** always takes you back to LIVE and shows the newest image. Stepping forward (→) past the newest image does the
same.

## 1.3 Moving around

| Key | Does |
|---|---|
| **←** / **→** (or **↑** / **↓**, or the mouse wheel) | Previous / next image. Going back switches to HISTORY. |
| **PgUp** / **PgDn** | 10 images back / forward. |
| **Home** | The oldest image. |
| **End** | Back to LIVE, the newest image. |
| **Space** | Pause / resume LIVE. |

Images are in the order they were saved, across every folder you watch.

## 1.4 How the picture is shown

| Key | Does |
|---|---|
| **F** | Cycles **fit** (the whole image), **fill** (fills the screen and crops the edges) and **1:1** (actual pixels). |
| **O** | Rotate 90°. |
| **F11** (or double-click) | Full screen on / off. |
| **T** | Always on top of other windows, on / off. |
| **H** | Keep the HUD on screen, on / off. |

Videos play silently on a loop.

## 1.5 Marking images: pick, skip, stars, bookmark

Marking lets you sort images while you generate, without stopping to open a file manager. Each mark is one key. Press
the same key again to undo it. A short message at the bottom confirms each mark.

| Key | Mark | What it's for |
|---|---|---|
| **P** | Pick | "This one is worth doing more with." Picks are what [hires](#110-hires-finishing-a-pick-at-high-resolution) works on. |
| **X** | Skip | "Not this one." Stepping with ← → passes over skipped images. Press **K** to show them again. |
| **1**-**5** | Stars | A rating. Press the same number again, or **0**, to clear it. |
| **B** | Bookmark | "I want to find this again." Bookmarks are listed in the [library](#18-the-library-your-bookmarked-and-rated-images). |
| **U** | Clear | Removes the pick, skip and stars in one go. Bookmarks stay. |

These are separate marks, so one image can be picked, rated four stars and bookmarked all at once.

> **Skipping never deletes anything.** BlinkLoupe never moves, renames or deletes your images. A skipped image is
> still in its folder; BlinkLoupe just passes over it.

## 1.6 Where marks are kept

Marks are small files saved next to your images, in the same folder:

| File | Holds |
|---|---|
| `PICKS.txt` | The file name of each picked image, one per line. |
| `REROLLS.txt` | Re-roll requests, one line each (see [1.9](#19-re-rolls-the-same-image-with-a-new-seed)). |
| `.loupe.json` | Stars, bookmarks, skips and hires counts. This file is hidden. |

Because they're plain files beside your images, marks stay with the folder if you move or back it up. Other programs
and scripts can read them too: a script could upscale everything listed in `PICKS.txt`. [3.10](#310-working-with-your-own-pipeline-the-mark-files)
describes the formats.

## 1.7 The info panel: how an image was made

Press **I** to open the info panel over the picture. Press I again, or Esc, to close it.

Most AI tools hide a record of how an image was made inside the file. BlinkLoupe reads it and shows:

- **Rating** and **Bookmark**
- **Model** (the checkpoint)
- **LoRAs**, each with its strength
- **Sampler:** steps, sampler and scheduler, CFG, seed, denoise, latent size, and how many sampler passes there were
- **Prompt** and **Negative**
- **Source:** where the information came from (a ComfyUI graph, A1111 / Forge parameters or SwarmUI), and for ComfyUI
  images whether the image can be re-rolled

The panel follows you as you step through images. If a file has no generation record, the panel says so.

## 1.8 The library: your bookmarked and rated images

Press **L** to open the library. It collects marked images from every folder you watch:

- **Bookmarked:** every bookmarked image.
- **Rated:** every image with stars.
- **By LoRA:** every LoRA named in your images' metadata, with a count. Click one to see the images that used it. The
  first time, BlinkLoupe reads the metadata in the background and shows its progress. After that it only reads new
  images.

You can sort by **Newest first**, **Oldest first** or **Highest rated**, and type in the box to filter by file or folder
name. Click any image to jump to it in the live view; that switches to HISTORY, and **End** brings you back to LIVE.

## 1.9 Re-rolls: the same image with a new seed

*This needs ComfyUI.*

**The idea.** Every image starts from random noise, and a number called the **seed** picks which noise. With the same
settings and a new seed you get the same subject, composition style and prompt, but a different picture. That's a
**re-roll**.

ComfyUI saves its whole **graph** (the workflow that made the image) inside every PNG it writes. So BlinkLoupe can
re-roll an image with one key, even weeks later and even if you've changed the workflow in ComfyUI since.

| Key | Does |
|---|---|
| **R** | Re-roll the image on screen. |
| **Shift+R** | Cancel the last re-roll, if ComfyUI hasn't started it yet. |
| **Ctrl+R** | Queue every re-roll marked in this folder's `REROLLS.txt`. |

When you press R:

1. BlinkLoupe reads the graph from the image.
2. It gives every sampler a new seed and leaves everything else alone: the prompt, the LoRAs, the wildcards.
3. It adds `_reroll` to the save name, so the result lands beside the original as, say, `fox_reroll_00001_.png`.
4. It sends the graph to ComfyUI's queue.

The new image arrives like any other, so you'll see it LIVE. If you're in HISTORY, the `NEW` counter goes up instead.

**Re-roll modes.** Settings (S) > Re-rolls > **A re-roll** decides what R does:

| Mode | R does |
|---|---|
| **Auto** (default) | Sends the re-roll to ComfyUI if it answers; otherwise writes a line in `REROLLS.txt` for later. |
| **Queue** | Always sends it to ComfyUI. |
| **Mark** | Only writes `REROLLS.txt`. This is for pipelines that render the marks themselves, or for batching: mark a few with R, then send them all with **Ctrl+R**, which removes the lines it queued. |

Press R twice to ask for two re-rolls. Shift+R takes back the most recent one: it cancels it in ComfyUI's queue, or
removes its line from `REROLLS.txt`. A re-roll that ComfyUI has already started can't be cancelled.

> If R says "no ComfyUI graph in this file": the image wasn't saved by ComfyUI, or it was saved in a way that strips
> metadata (a screenshot, an image editor, some websites). In Auto mode the request goes into `REROLLS.txt` instead.

## 1.10 Hires: finishing a pick at high resolution

*This needs ComfyUI.*

**The idea.** Generating straight at high resolution is slow and often goes wrong. A common trick is to generate small,
pick the good ones, then upscale those and run a light extra pass that adds real detail. That's what **hires** does.

| Key | Does |
|---|---|
| **Shift+P** | Finish the image on screen. |
| **Ctrl+P** | Finish every pick in this folder that hasn't been finished yet. |

BlinkLoupe uploads the image to ComfyUI and finishes it with the image's **own** model, LoRAs, prompts and VAE:

1. Upscale it: with an upscale model if you've chosen one, then a resize to the target size.
2. Run **one** gentle sampler pass over it (denoise 0.30 by default), which sharpens and adds detail without changing
   the picture.
3. Save it through the image's own save node, with `_hires` added to the name.

The original render is never run again, so the finish starts from exactly the pixels you picked. A `HIRES` tag appears
on the image, and Ctrl+P skips images that already have one.

Settings (S) > Hires on picks has the controls:

| Setting | Default | Means |
|---|---|---|
| Scale | 2.0 | How many times bigger. |
| At most (megapixels) | 4.2 | A cap, so big images don't run out of memory. 0 means no cap. |
| Denoise | 0.30 | How much the extra pass may change. Lower stays closer to the original; higher adds more detail and more change. |
| Steps | 0 | Steps for the extra pass. 0 uses the image's own step count. |
| Upscale model | None | An upscaler from your ComfyUI (for example a 4x ESRGAN model), run before the resize. None means a plain lanczos resize. |
| File suffix | `_hires` | Added to the file name. |
| Folder suffix | (empty) | Empty saves next to the original. `-hires` saves into a sibling folder: `my-set\fox.png` becomes `my-set-hires\fox_hires…png`. |

Hires works on still images, not videos. [3.6](#36-how-hires-works) has the details.

## 1.11 A tour of Settings

Press **S** to open Settings. Changes apply when you click **Save**; **Cancel** throws them away. While Settings is open,
the viewer keys are off, so typing in a box never marks an image.

**Run the setup again…** at the top reopens the first-run setup.

### Folders

- The folders BlinkLoupe watches. **Add a folder…** to add one; **Find ComfyUI folders** lists the output folders it can
  find. Remove one with its button.
- **Subfolder depth** (default 4): how many levels of folders inside each watched folder to include. 0 means only the
  folder itself.

### Display

- **Monitor:** Automatic (the largest secondary monitor) or a specific one.
- **Full screen:** Automatic (full screen when you have a second monitor), Always, or Never (a window).
- **HUD stays up:** seconds the HUD stays after each new image (default 6).
- **Clock:** 12-hour (2:14:07 pm) or 24-hour (14:14:07).
- **Black-frame alarm** (default 8): an image whose brightest pixel is darker than this (on a 0-255 scale) is tagged
  `BLANK` in red. Black images usually mean a failed render (a "NaN" error in the model).

### Keys

- **Keymap:** **Standard** (P pick, X skip, R re-roll, like photo-sorting tools) or **Classic** (X re-roll, D skip,
  R rotate, matching an older viewer this one grew from). [Appendix A](#appendix-a-every-key) lists both.
- **Ask before quitting:** Esc and Ctrl+Q ask "Quit BlinkLoupe?" first. Ticking "Don't ask again" in that question turns
  this off; tick it here to bring the question back. The window's own close button never asks.

### Picks

- **Also copy picks to:** a folder. Each pick is also copied there, into a subfolder named after the image's own folder.
  Unpicking removes the copy, unless the copy has been changed since. Useful for tools that don't read `PICKS.txt`.

### Re-rolls

- **A re-roll:** Auto, Queue or Mark (see [1.9](#19-re-rolls-the-same-image-with-a-new-seed)).
- **Re-roll file suffix** (default `_reroll`): added to the save name of each re-roll. Empty keeps the original name.
- **ComfyUI address:** empty means automatic (`http://127.0.0.1:8188`, then `:8000`). For other addresses see
  [3.4](#34-servers-elsewhere-https-tokens-and-passwords).
- **Access token:** only if your server asks for one.
- **Test connection:** checks the address and says what it found, or why it couldn't connect.

### Following ComfyUI

Off by default. See [3.3](#33-following-a-comfyui-server).

### Hires on picks

See [1.10](#110-hires-finishing-a-pick-at-high-resolution).

### Actions and Send to a workflow

Your own keys. See [3.1](#31-your-own-keys-actions) and [3.2](#32-send-to-a-workflow).

### Updates

- **Version:** the one you have, and whether a newer one is out.
- **Check now**, **Install** (when there's an update), **Release page**, **Open the log folder**.
- **Check by itself:** once a day, ask github.com whether there's a newer BlinkLoupe. Nothing about you or your images
  is sent.
- **Update feed:** leave this empty. It's only for testing your own builds.

---

# Part 2: Blink, the comparison window

## Quick start: compare an image with its re-roll

Blink answers "what actually changed?" between two images: two re-rolls, before and after an upscale, two settings, two
models.

**From the live view (the usual way):**

1. You need an image and a re-roll of it. With ComfyUI, press **R** on an image and wait for the re-roll to arrive
   LIVE.
2. With the re-roll on screen, press **V**.
3. Blink opens in its own window. The original is **A**, the re-roll is **B**. (Pressed on the original instead, V makes
   B its first version.)

   If your workflow saves every image under the same name with a number (ComfyUI's default `ComfyUI_00001_`,
   `ComfyUI_00002_`…), Blink can't tell which of them a re-roll came from. It opens the folder with the re-roll ticked
   instead: tick the original too, then click **Compare checked**. [2.5](#25-groups-what-blink-finds-for-you) explains
   how to name images so Blink pairs them by itself.
4. You start in the **Slider** view: move the mouse left and right over the picture. A is on the left of the line, B on
   the right.
5. Press **3** for **Flicker**, then tap **Space**. The two images swap in place, and anything that changed jumps out.
6. Press **1** for **Side by side**. Scroll the mouse wheel to zoom: both images zoom together. Drag to move around.
   Press **F** to fit them again.
7. Look at the panel on the right: the top line is a plain-words verdict, such as "small changes".

Close the Blink window when you're done; the live view keeps running.

**Without ComfyUI, or for any two images:**

- **Drag two or more images** (PNG, JPEG or WebP) from File Explorer onto the Blink window.
- Or open **Blink (BlinkLoupe)** from the Start menu, click **Folders** at the top left, open a folder, tick two or more
  images and click **Compare checked**.

If you press V on an image that has nothing to compare with yet, Blink opens its folder with that image ticked: tick
another one and click **Compare checked**.

**Press `?` in Blink for its keys.** Some letters do different things in Blink than in the live view. For example, P in
Blink switches sharp pixels on and off; it doesn't pick.

---

## 2.1 The Blink window

```
┌───────────────┬──────────────────────────────────────────────┬───────────────┐
│ search        │ 1 side by side  2 slider  3 flicker ...  (?) │ Images        │
│ All Versions  │ [controls for the current view]              │  A  ...       │
│ A/B  Folders  ├──────────────────────────────────────────────┤  B  ...       │
│               │                                              │ Difference    │
│ group list    │              the images                      │  verdict      │
│               │                                              │  SSIM, PSNR…  │
│               │                                              │ Histogram     │
│               ├──────────────────────────────────────────────┤ Settings      │
│               │ [A] [B] [ ] [ ]   thumbnails of the group    │ Prompt        │
└───────────────┴──────────────────────────────────────────────┴───────────────┘
```

- **Left:** the groups Blink found, a search box (**/** jumps to it) and four filters: **All**, **Versions**,
  **A/B folders** and **Folders**. **Tab** hides this column.
- **Top:** the seven views (keys 1-7) and the controls for the current one. `?` shows the keys.
- **Middle:** the images.
- **Bottom strip:** every image in the group, with **A** and **B** badges.
- **Right:** the numbers and the generation settings. **I** hides this column.

## 2.2 A and B

Blink always compares two images, **A** and **B**. A group can hold more (an original and five re-rolls, say), and you
choose which two:

| Do | To |
|---|---|
| **←** / **→** | Make B the previous / next image in the group. |
| **Shift+←** / **Shift+→** | Make A the previous / next image. |
| Click a thumbnail | Make it A. |
| Right-click a thumbnail | Make it B. |
| **Space** | Swap A and B. |
| **↑** / **↓** | The previous / next group in the list. |

## 2.3 Zooming and panning

Every view keeps A and B aligned: zoom or move one, and the other follows.

| Do | To |
|---|---|
| Mouse wheel | Zoom at the cursor. |
| Drag | Pan. |
| **F** | Fit to the window. |
| **Z** | Actual pixels (100%). |
| **+** / **-** | Zoom in / out. |
| Double-click | Switch between fit and actual pixels. |
| **P** | Sharp pixels when zoomed in, on / off. On shows each pixel as a crisp square; off smooths them. |
| **M** | Magnifier: a loupe by the cursor showing A, B and their difference, up close. |

## 2.4 The seven views

| Key | View | Best for |
|---|---|---|
| **1** | Side by side | A first look. A cursor mark shows the same spot in both. |
| **2** | Slider | Sweeping a line across: A on the left, B on the right. The line follows the mouse. |
| **3** | Flicker | Small changes. Space swaps A and B in place. |
| **4** | Difference | Seeing exactly where pixels changed. |
| **5** | Onion skin | B laid over A, see-through. |
| **6** | SSIM map | Where the *structure* changed (shapes, edges, texture), not just the colour. |
| **7** | Grid | Every image in the group at once. |

Blink remembers the last view you used.

**Slider controls.** **L**, or clicking on the line, locks it in place so you can move the mouse without moving it.

**Flicker controls.** **B** turns on **auto-blink**, which swaps A and B by itself. The slider next to it sets the speed,
from 150 to 1500 milliseconds (default 500). Your eye is very good at spotting things that jump; that's why astronomers
used a "blink comparator" to find moving objects (it found Pluto in 1930), and why this window is called Blink.

**Difference controls.** Choose how differences are drawn:

| Mode | Shows |
|---|---|
| heat map | How much each pixel changed, from dark (none) to hot colours (a lot). |
| grey | The same as a grey image. |
| brighter / darker | Red where B is brighter than A, blue where B is darker. |
| changed pixels | Magenta wherever a pixel changed by more than the **threshold** (1-64, default 8), over a dimmed A. |

**Gain** (1-32x, default 4x) makes small differences visible; **[** and **]** turn it down and up.

**Onion skin controls.** **B opacity** from 0% (only A) to 100% (only B).

**SSIM map controls.** **Gain** (1-20x); **[** and **]** change it. Bright areas are where the structure differs.

## 2.5 Groups: what Blink finds for you

You don't have to tell Blink which images belong together. It looks through every folder the live view watches and
groups them by their names:

| Group | Holds | Example |
|---|---|---|
| **Versions** | An image and everything made from it: re-rolls, hires finishes, upscales, results of [Send to a workflow](#32-send-to-a-workflow). They can be in the same folder or in a folder named after it, such as `my-set-hires`. | `fox_00001_.png`, its re-roll `fox_reroll_00001_.png`, its upscale `fox_00001__upscale-4x_00001_.png` |
| **A/B folders** | The same file names in sibling folders, one image per folder. This is for comparing whole runs: a set redone in another style, or the same prompts on two settings. | `test-baseline\cat.png` and `test-fp16\cat.png` |
| **Folders** | Any folder. Tick the images you want. | |

Groups are listed by folder. Each line shows the kind (versions or A/B), a name and how many images it holds. Type in
the search box to filter, and press **R** to look again for new images.

A version of a version joins the first image's group, labelled as a chain, for example `upscale-4x › flip`. Files in a
subfolder of a hires folder are labelled with that subfolder, for example `detail · base`.

> **Give each image its own name** if you want re-rolls and hires grouped for you. A re-roll is saved under the
> original's name plus `_reroll` (`fox` → `fox_reroll_00001_.png`), so Blink can match it only when one image in the
> folder is named `fox`. When a workflow saves a whole batch as `fox_00001_`, `fox_00002_`…, or under ComfyUI's
> default `ComfyUI_…`, a re-roll could belong to any of them, and Blink leaves it out rather than guess. A save prefix
> per image, such as `01_fox`, `02_fox`, avoids this. Results of Send to a workflow with **Beside** on always group,
> because they carry the image's full name.

[3.7](#37-how-blink-finds-groups) has the exact rules, which help if you want your own file names to group well.

## 2.6 Comparing any images you choose

- **Folders:** click **Folders**, then a folder (newest first) to see its images and subfolders. **Up** goes to the
  folder above. Tick two or more images and click **Compare checked**.
- **Drag and drop:** drop PNG, JPEG or WebP files from File Explorer onto the window.

Blink only reads images. It never changes, moves or saves anything.

## 2.7 Reading the numbers

The right panel measures A against B. When the sizes differ (an original against its hires, say), the smaller image is
resized up to the larger first, so its softness counts as a difference.

| Number | Means | Rule of thumb |
|---|---|---|
| **verdict** | A summary in words. | identical pixels · near-identical · small changes · clearly different · very different |
| **SSIM** | Structural similarity, from 0 to 1. Compares shapes, edges and texture the way the eye does. | 1 = same structure. Above 0.98 is near-identical, above 0.9 small changes, above 0.7 clearly different. |
| **PSNR** | Peak signal-to-noise ratio, in decibels. | Higher is closer. Above 40 dB is hard to see. ∞ means identical. |
| **mean \|A-B\|** | The average change per pixel, out of 255. | |
| **RMS diff** | Like the mean, but large changes count for more. | |
| **largest diff** | The biggest single change, out of 255. | |
| **changed > 8** | The share of pixels that changed by more than the threshold (set in the Difference view). | |
| **fine detail** | The "variance of the Laplacian" for A and for B, and B divided by A. More fine detail, sharp edges or noise reads higher. | A hires finish should read higher than its original. |
| **brightness** | Average brightness of A and of B. | |
| **contrast** | How spread out the brightness is (standard deviation) in A and in B. | |

Below them, a **histogram** draws how many pixels have each level of red, green and blue: solid lines for A, dashed for
B. A shift to the right means brighter; a narrower hump means less contrast.

## 2.8 Comparing generation settings

Below the numbers, Blink reads the generation record from each image (see [1.7](#17-the-info-panel-how-an-image-was-made))
and lines them up:

- **Settings:** model, LoRAs, sampler, scheduler, steps, CFG, denoise, seed and size, with the rows that differ
  highlighted.
- **Prompt:** "same in both", or both prompts shown when they differ.
- **Every graph input that differs:** for ComfyUI images, every node setting that isn't the same in both graphs, up to
  150 of them. This is how you find the one setting someone changed between two runs.

---

# Part 3: Advanced

Everything from here on is optional. It covers BlinkLoupe's features for people who build their own workflows and
pipelines, and how each feature works underneath.

## 3.1 Your own keys (Actions)

An **action** is a key that runs a program or script on the image on screen: an upscaler, an image editor, another
viewer, your own pipeline's next step.

**Add one:** Settings (S) > Actions > **Add an action**. Each row has:

| Field | What to enter |
|---|---|
| **Key** | Click the box, then press the key. Letters with Shift, Ctrl or Alt, F2-F12 and similar keys work. Backspace clears it. Settings refuses a key BlinkLoupe already uses. |
| **Name** | What the key guide and messages call it. |
| **Command** | The program and its arguments, with placeholders (below). |
| **Wait** | On: wait for the command to finish and show its result. Off: start it and carry on. |

**Placeholders** in the command are filled in with the image's details:

| Placeholder | Becomes | Example |
|---|---|---|
| `{file}` | The image's full path | `D:\renders\set\fox_00012_.png` |
| `{folder}` | Its folder | `D:\renders\set` |
| `{name}` | Its file name | `fox_00012_.png` |
| `{stem}` | Its file name without the extension | `fox_00012_` |

The command is split into arguments *before* the placeholders are filled in, so a path with spaces stays one argument
whether or not you put quotes around the placeholder. The command runs in the image's folder, with no console window.

**Examples:**

```
"C:\Program Files\GIMP 2\bin\gimp-2.10.exe" {file}                (Wait off: opens the image in GIMP)
"C:\Python312\python.exe" "D:\tools\upscale.py" {file}            (Wait on: runs a script and reports back)
```

**Writing a script that reports back.** With Wait on, BlinkLoupe shows the **last line** the command prints as a
message. Three line openings are understood:

| Last line | Message shown |
|---|---|
| `FINISHED <output file> (a note)` | "Upscale finished · <file name> · a note" |
| `ALREADY <output file>` | "Upscale · already done · <file name>" |
| `ERROR <why>` | The reason, as an error |

Any other last line is shown as it is. A non-zero exit code counts as a failure. For example, the end of a Python
script:

```python
print(f"FINISHED {out_path} (2.1 MP, 14 s)")
```

Your actions appear in the key guide (F1) under **Your actions**. Actions are checked before the built-in keys, which
lets combinations like Ctrl+Shift+H or Alt+1 through.

## 3.2 Send to a workflow

**Send to a workflow** binds a key to any ComfyUI workflow that has a **Load Image** node: an upscaler, a face fix, an
img2img pass, an image-to-video workflow. Press the key and the image on screen goes into the workflow, which is queued.
The result arrives LIVE like any new image.

### Preparing the workflow in ComfyUI

1. Build the workflow with a **Load Image** node where the image should go in.
2. If it has more than one Load Image node, give the one BlinkLoupe should fill a title containing **Loupe**:
   right-click the node > **Title**. Upper or lower case doesn't matter, and it can be anywhere in the title.
3. Save it with **Workflow > Export (API)**. BlinkLoupe needs this API format. A normal saved workflow (the kind with
   `nodes` and `links` inside) is refused with a hint to export the API version.

Instead of an exported file, you can also choose **any image that workflow saved**, since ComfyUI keeps the graph inside
the PNG.

### Adding the key

Settings (S) > Send to a workflow > **Add a workflow**:

| Field | Means |
|---|---|
| **Key** | As for actions. Actions and workflows share one set of keys. |
| **Name** | Shown in messages and the key guide. With Beside on, it's also added to the result's name. |
| **Workflow** | The exported file, or an image the workflow made. |
| **New seed** (on by default) | Give the workflow's samplers new seeds each time. Without it, sending the same image twice makes nothing new, because ComfyUI reuses its cached result. |
| **Beside** (on by default) | Save the result next to the image, named after it. |

Under each row, Settings describes what it found in the file: which node gets the image, how many seeds and save nodes
there are, and which placeholders it will fill.

### What happens on the key

1. The image is uploaded to ComfyUI's input folder, into `input\loupe`.
2. It goes into the chosen loader.
3. With **New seed**, samplers get new seeds.
4. With **Beside**, every save node is pointed at the image's folder, with the file name plus the workflow's name.
   `set/01_fox_00001_.png` sent with a workflow named "Upscale 4x" comes back as
   `set/01_fox_00001__upscale-4x_00001_.png`. Without Beside, the workflow's own save names are used.
5. **Placeholders** in any text box of the workflow are filled in:

| Placeholder | Becomes |
|---|---|
| `{positive}` / `{negative}` | The image's prompts, as the info panel reads them |
| `{name}` | Its file name |
| `{stem}` | Its file name without the extension |
| `{folder}` | Its folder's name |

So a CLIP Text Encode saying `{positive}, sharp focus` carries the image's own prompt into an img2img pass, and a save
prefix of `tests/{stem}_flip` names the result after the image.

### Loaders BlinkLoupe can fill

| Loader | How |
|---|---|
| Core **Load Image**, and other nodes with "LoadImage" in their name and an `image` input | Uploaded |
| Core **Load Video**, Video Helper Suite **Load Video** (upload and FFmpeg) | Uploaded, when the item on screen is a video |
| **WAS Image Load**, Video Helper Suite's path loaders | Given the image's path instead of an upload. This only works when ComfyUI runs on the same computer. |

Mask loaders are only filled when they're titled "Loupe". Your workflows appear in the key guide (F1) under **Your
workflows**.

## 3.3 Following a ComfyUI server

Watching a folder only works when ComfyUI saves to this computer's disk. It also misses **Preview Image** outputs,
which ComfyUI keeps in a temporary folder. **Following** fixes both: BlinkLoupe asks ComfyUI itself for every image it
finishes, from any workflow, queued from anywhere.

**Turn it on:** Settings (S) > Following ComfyUI > **Follow the server**. The first-run setup offers it too.

| Setting | Default | Means |
|---|---|---|
| Follow the server | off | Show every image the server at the ComfyUI address finishes. |
| Preview Image outputs | on | Include previews. |
| Save server images to | `Pictures\BlinkLoupe` | Where copies of the server's images are saved. |

**Where the images go.** Each image is saved as `<server>\<subfolder>\<file>` inside that folder, with previews under
`<server>\previews\`. That folder is watched, so images arrive LIVE, and every mark, re-roll and action works on them.
Re-rolls and hires go back to the same server.

**How it works.** ComfyUI tells every connected client when its queue changes (the `status` message on its `/ws`
socket). When a job finishes, BlinkLoupe reads that job's outputs from `/history` and downloads each image through
`/view`. If the socket isn't available, it checks the newest history entry every 2 seconds instead, and keeps trying the
socket every 15 seconds.

**Details:**

- With a ComfyUI on this computer, an output that's already in a watched folder isn't copied again.
- When following starts, the server's newest finished job is shown; older history is left alone.
- File names from the server are cleaned before they become paths, so a server can never write outside the folder.
- Images are downloaded under a temporary name and renamed when complete, so you never see half an image.

## 3.4 Servers elsewhere: HTTPS, tokens and passwords

Everything BlinkLoupe asks of ComfyUI (following, re-rolls, hires, sending to workflows, Test connection) works over
`https://` as well as `http://`. HTTPS uses Windows' own certificate store, so a cloud server's ordinary certificate
works with nothing to set up. A self-signed certificate is refused, so on your own network use `http://`.

| The server wants | Enter |
|---|---|
| Nothing | Its address: `http://192.168.1.20:8188`, or `https://abc123-8188.proxy.example.net` |
| A token (ComfyUI-Login, a token proxy) | The address, and the token in Settings > **Access token**. It's sent as `Authorization: Bearer <token>` with every request, the status socket included. |
| A user name and password (an nginx or Caddy password proxy) | `https://user:password@host` in the address. It's sent as Basic auth. Write `%40` for an `@` in the password. |
| ComfyUI served under a path | `https://host/comfy` |

If a server refuses, **Test connection** and the following status say why: a refused token or password (401 / 403), a
certificate problem, or nothing answering. Passwords are removed from any address shown on screen or written to the
log. The token is never logged; it's stored in BlinkLoupe's settings file on your computer.

The update check never sends your token or password to github.com.

## 3.5 How re-rolls work

When you re-roll an image, BlinkLoupe edits a copy of its graph:

- **Seeds that change:** `seed` on KSampler, `noise_seed` on RandomNoise, KSamplerAdvanced and SamplerCustom, and a
  seed node linked into any of them.
- **Seeds that stay:** wildcard processors and detailers. The prompt a wildcard chose stays the same, so a re-roll
  changes the picture, not the subject.
- **Save names:** every save node's `filename_prefix` gets the re-roll suffix (`_reroll` by default; Settings >
  Re-rolls). For Image Saver, its `filename`.
- **Workflow:** the image's saved UI workflow travels along, so the re-roll's PNG opens in ComfyUI like the original.

Samplers are found through common node packs too: Efficiency, Impact, rgthree, Comfyroll, WAS, Searge and Image Saver.
**Videos** re-roll the same way, when the video carries its graph (ComfyUI's own video savers, Video Helper Suite's
Video Combine with metadata on).

**Where ComfyUI is found:** the address in Settings, else `127.0.0.1:8188`, then `127.0.0.1:8000`.

## 3.6 How hires works

Shift+P builds a new, small graph from the image's own:

```
Load Image (the uploaded frame)
  → upscale model (optional)
  → lanczos resize to the target size
  → VAE Encode (Tiled)
  → one KSampler pass
  → VAE Decode (Tiled)
  → the image's own save node, with _hires on the name
```

- **What's kept:** only what the image's last sampler needed: the model chain with every LoRA, the prompts and the VAE.
  The sampler, scheduler and CFG are the image's own. The seed is the image's seed plus 500000. Steps are the image's
  own unless Settings says otherwise.
- **Sampler types:** KSampler, KSamplerAdvanced, SamplerCustom and SamplerCustomAdvanced (with CFGGuider or
  BasicGuider). Efficiency's SDXL tuples and Impact's basic pipes are unpacked.
- **VAE:** from the decode node, else the sampler's optional VAE, else the loader's VAE output.
- **Checking:** every input is checked against what your ComfyUI reports it has (`/object_info`) before queueing.
- **Target size:** the image's size times **Scale**, made smaller if it would pass **At most** megapixels, rounded to
  multiples of 8, and never smaller than the original:

| Original | Scale 2.0, cap 4.2 MP | Result |
|---|---|---|
| 1024 × 1024 (1.0 MP) | 2048 × 2048 (4.19 MP) | under the cap: 2048 × 2048 |
| 832 × 1216 (1.0 MP) | 1664 × 2432 (4.05 MP) | under the cap: 1664 × 2432 |
| 1536 × 1536 (2.4 MP) | 3072 × 3072 (9.4 MP) | capped: 2048 × 2048 |

- **Refused:** videos, and graphs that save through a video saver ("hires finishes stills").
- **Uploads:** uploaded frames collect in ComfyUI's `input\loupe` folder. BlinkLoupe never deletes them; clear that
  folder yourself now and then.
- **The `HIRES` count** in `.loupe.json` records each finish queued, which is how Ctrl+P knows what's done.

Tiled VAE keeps memory use down on big images. If your own finishing pipeline is better than this simple pass, bind it
to a key with [Actions](#31-your-own-keys-actions) or [Send to a workflow](#32-send-to-a-workflow).

## 3.7 How Blink finds groups

Blink groups images by name, using these rules. It reads the list of files the live view already keeps, so there's no
extra scan of your disk.

**Save counters.** A trailing counter of 4 or more digits (`_00001_`, `_00000`) is ignored when comparing names:
`fox_00012_` is treated as `fox`.

**Versions.** For each folder, Blink considers its images plus those in **sibling folders named after it** (the folder
name followed by `-`, `_`, a space or `.`, then more: `set-hires`, `set_upscaled`) and their subfolders. An image is a
version of an original when its name extends the original's name with `_` and a **tag**. Blink tries two ways, in
order:

1. **The original's full name.** `fox_00001__upscale-4x_00001_` extends `fox_00001_` with the tag `upscale-4x`. Results
   of Send to a workflow with Beside on are named this way. When several originals match, the longest name wins.
2. **The original's name without its counter.** `fox_reroll_00001_` extends `fox` (from `fox_00001_`) with the tag
   `reroll`. Re-rolls and BlinkLoupe's hires finishes are named this way, because they add their suffix to the save
   prefix. This is used only when **exactly one** original in the folder has that name. If several do (a batch of
   `fox_00001_`, `fox_00002_`…), the re-roll could belong to any of them, so it's left out rather than guessed.

A tag must start with a letter: `fox_00002_` is the next image of a batch, not a version of `fox_00001_`. Versions of
versions follow their parents back to the first original, and their labels show the chain (`upscale-4x › flip`).

**A/B folders.** Sibling folders are paired when they share **at least two file names and at least half of the smaller
folder's names**. A folder with a single image joins on that one name. Names whose core is `ComfyUI` (ComfyUI's default
prefix) never pair folders, because every folder of plain runs shares them. Each shared name becomes a group, one image
per folder, oldest folder first. Labels are the folder names without the part they all start with:
`vram-bench-baseline` and `vram-bench-fp16` show as `baseline` and `fp16`.

**Tips for names that group well:**

- Give each image its own save prefix (`01_fox`, `02_fox`), so its re-rolls and hires can be matched to it.
- Save versions with the original's name plus `_<tag>`. **Beside** in Send to a workflow does this for you.
- Keep finished versions in a folder named after the original folder, such as `my-set-hires`.
- To compare two runs, give their folders a shared start (`test-a`, `test-b`) and use the same file names in both.

## 3.8 How new images are noticed

- **Checking:** once a second (the `poll_ms` setting), BlinkLoupe checks each watched folder's modified time and lists a
  folder's files only when that has changed. This is light enough to run beside a render, and it works on network
  drives.
- **Complete files only:** a new file is shown once its size has stayed the same for one check **and** its format's end
  marker is present (PNG `IEND`, JPEG `EOI`, the WebP length, the GIF trailer). A video is shown once its size has held
  for 3 seconds. You never see half a frame.
- **What's watched:** PNG, JPEG, WebP, GIF, BMP, MP4, M4V, WebM and MOV files, down to **Subfolder depth** levels.
  Folders whose names start with a dot are skipped. So is the **Also copy picks to** folder.
- **F5** rescans everything from scratch.
- **Black-frame alarm:** BlinkLoupe measures each image as it's shown. If the brightest pixel is below the threshold,
  the image is tagged `BLANK` and the mode turns red.

## 3.9 What metadata BlinkLoupe reads

| Source | Read from |
|---|---|
| ComfyUI | The graph in a PNG's `prompt` text chunk. BlinkLoupe follows it from the save node back to the sampler that made the image. |
| ComfyUI video | The graph saved inside MP4, M4V, MOV and WebM / MKV files, and in animated PNGs (ComfyUI's SaveAnimatedPNG). |
| A1111 / Forge | The `parameters` text. |
| SwarmUI | `sui_image_params`. |
| JPEG / WebP | EXIF data. |

**ComfyUI node packs understood:** Efficiency (loaders, samplers, LoRA stacks), Impact (Wildcard Encode, basic pipes),
rgthree (Power Lora Loader, Lora Loader Stack, Power Prompt, contexts), Comfyroll (LoRA stacks, text nodes), WAS (text
and save nodes), Image Saver, Searge, and Video Helper Suite. Primitive, reroute and display nodes are followed.

**LoRAs** are read from loader nodes and stacks, with their strengths. A LoRA that's switched off or set to strength 0
is left out. `<lora:name:strength>` tags in Impact's Wildcard Encode and rgthree's Power Prompt are read too.

## 3.10 Working with your own pipeline: the mark files

BlinkLoupe only reads and writes plain files, so any script or pipeline can work with it. Close BlinkLoupe before
editing these files by hand.

### `PICKS.txt`

One picked file name per line, in the image's folder:

```
fox_00012_.png
fox_00017_.png
```

### `REROLLS.txt`

One line per requested re-roll. A name twice means two re-rolls. **Ctrl+R** queues them and removes the lines it queued.

```
fox_00003_.png
fox_00003_.png
```

### `.loupe.json`

Hidden, one per folder. Only marks that are set are written:

```json
{
  "version": 1,
  "files": {
    "fox_00012_.png": { "stars": 4, "bookmark": true },
    "fox_00013_.png": { "rejected": true },
    "fox_00017_.png": { "hires": 1 }
  }
}
```

`rejected` is a skip; the name dates from an earlier version.

### Picks copied to a folder

With Settings > Picks > **Also copy picks to** set to `D:\picks`, picking `D:\renders\my-set\fox.png` copies it to
`D:\picks\my-set\fox.png`. Unpicking removes the copy only if it's still the same size, so a copy you've edited stays.

### The set file: `.loupe-set.json`

Counting files makes the newest image of a batch that's still rendering always read "12/12". A pipeline that writes
this file into the output folder **before** it starts queueing gets "frame 12/40" instead:

```json
{"total": 40}
```

An image's number is a leading `NN_` in its name (`03_fox_00000.png` is frame 3). Without one (`ComfyUI_00012_.png`),
it's the image's place among the folder's images that aren't re-rolls. Re-rolls show as `frame 3/40 · reroll`.

A folder of finished versions, whose names carry their source's numbers, can count each kind of file on its own:

```json
{"total": 20, "by": "kind", "kinds": ["upscaled", "facefix"]}
```

A file is of a kind when `_<kind>_` or `_<kind>.` appears in its name, and the HUD reads `upscaled 2/20`. `"kinds"`
defaults to `["pass", "hires"]`. Other keys in the file are ignored. A folder without the file falls back to counting.

## 3.11 The settings file

Settings are stored in `%APPDATA%\BlinkLoupe\settings.json`. Use the Settings panel when you can. If you edit the file
by hand, close BlinkLoupe first. Every setting has a default, so you can delete a line to reset it.

| Key | Default | In Settings as |
|---|---|---|
| `roots` | `[]` | Folders |
| `depth` | `4` | Subfolder depth |
| `poll_ms` | `1000` | *(file only)* milliseconds between folder checks |
| `monitor` | `""` | Monitor (empty = automatic) |
| `fullscreen` | `"auto"` | Full screen: `auto`, `on`, `off` |
| `keymap` | `"loupe"` | Keymap: `loupe` (Standard) or `classic` |
| `reroll_mode` | `"auto"` | A re-roll: `auto`, `queue`, `mark` |
| `reroll_suffix` | `"_reroll"` | Re-roll file suffix |
| `comfy_url` | `""` | ComfyUI address (empty = automatic) |
| `comfy_token` | `""` | Access token |
| `clock24` | `false` | Clock |
| `hud_secs` | `6` | HUD stays up |
| `topmost` | `false` | Always on top (T) |
| `show_rejected` | `false` | Show skipped images (K) |
| `blank_threshold` | `8` | Black-frame alarm |
| `hires_scale` | `2.0` | Hires: Scale |
| `hires_max_mp` | `4.2` | Hires: At most (megapixels) |
| `hires_denoise` | `0.30` | Hires: Denoise |
| `hires_steps` | `0` | Hires: Steps |
| `upscale_model` | `""` | Hires: Upscale model |
| `hires_suffix` | `"_hires"` | Hires: File suffix |
| `hires_folder_suffix` | `""` | Hires: Folder suffix |
| `confirm_quit` | `true` | Ask before quitting |
| `actions` | `[]` | Actions: `{"key", "label", "cmd", "wait"}` |
| `workflows` | `[]` | Send to a workflow: `{"key", "label", "file", "new_seed", "beside"}` |
| `picks_copy_dir` | `""` | Also copy picks to |
| `follow_server` | `false` | Follow the server |
| `feed_dir` | `""` | Save server images to (empty = `Pictures\BlinkLoupe`) |
| `feed_previews` | `true` | Preview Image outputs |
| `check_updates` | `true` | Check by itself |
| `update_url` | `""` | Update feed |

An action as stored:

```json
{"key": "Shift+H", "label": "Hires", "cmd": "\"C:\\Python312\\python.exe\" \"D:\\tools\\finish.py\" {file}", "wait": true}
```

**Other files BlinkLoupe keeps:**

| File | Holds |
|---|---|
| `%APPDATA%\BlinkLoupe\update.json` | When it last checked for updates |
| `%APPDATA%\BlinkLoupe\lora-cache.json` | The library's LoRA index, so only new images are read |
| `%LOCALAPPDATA%\BlinkLoupe\blinkloupe.log` | The log (Settings > Updates > Open the log folder) |

Coming from Loupe 0.9.0, BlinkLoupe copies `%APPDATA%\Loupe` the first time it starts. The old folder is left as it
was.

## 3.12 Starting BlinkLoupe from the command line

```
BlinkLoupe.exe                                   the live view, with your saved folders
BlinkLoupe.exe --blink                           Blink only (the Start menu's "Blink (BlinkLoupe)")
BlinkLoupe.exe --root "D:\renders\test"          watch only this folder, for this session
BlinkLoupe.exe --root "D:\renders\test" --windowed   the same, in a window rather than full screen
```

- `--root` sessions don't save their folder, and don't check for updates. Settings shows a note that the session is
  using `--root`.
- **One copy at a time.** Starting BlinkLoupe again while it runs doesn't open a second copy; it brings the running
  one forward. With `--blink`, it opens or fronts the running copy's Blink window.
- The installed program is `%LOCALAPPDATA%\Programs\BlinkLoupe\BlinkLoupe.exe`.

## 3.13 Updates, uninstalling and privacy

**Updates.** Once a day (if **Check by itself** is on), BlinkLoupe asks github.com whether there's a newer release.
When there is, it says so once. Settings > Updates > **Install** downloads it, checks its size and SHA-256 checksum,
installs it and restarts BlinkLoupe. A copy that wasn't installed with the installer shows a link to the release page
instead.

**Uninstalling.** Windows Settings > Apps > **BlinkLoupe** > Uninstall. It asks whether to remove BlinkLoupe's settings
and log too (the default is to keep them). Your marks are files beside your images and are never touched.

**Privacy.** BlinkLoupe talks to:

- your ComfyUI, at the address in Settings or `127.0.0.1`
- github.com, once a day, if you leave the update check on

Nothing else. Nothing about your images leaves your computer, except what you send to your own ComfyUI.

## 3.14 Troubleshooting

| What you see | Try |
|---|---|
| "Windows protected your PC" when installing | **More info** > **Run anyway**. |
| A blank or white window | Install Microsoft's **WebView2 Evergreen Runtime**. Setup offers the download page if it's missing. |
| No images appear | Settings (S) > Folders: is your output folder listed? Are your images deeper than **Subfolder depth**? Is the folder's name starting with a dot? Press **F5** to rescan. |
| Images appear on the wrong monitor, or not full screen | Settings > Display > **Monitor** and **Full screen**. **F11** toggles full screen. |
| A key does nothing | Is a panel open (Settings, the library, the key guide)? Viewer keys are off while one is. Close it with Esc. |
| An image I skipped has vanished | It's still in the folder. Press **K** to show skipped images, then **X** on it to un-skip. |
| R, Shift+P or a workflow key says "ComfyUI is not answering" | Is ComfyUI running? Settings > ComfyUI address > **Test connection**. It says why a server refused. |
| R or Shift+P says the file "carries no ComfyUI graph" | Re-roll and hires need an image saved by ComfyUI with its metadata. Check the info panel (I): its Source line says what was found. |
| R says "no sampler seed found in this graph" | BlinkLoupe couldn't find a sampler seed to change. The graph may use a sampler node it doesn't know; use [Send to a workflow](#32-send-to-a-workflow) with New seed instead. |
| R writes to `REROLLS.txt` instead of rendering | The mode is Auto and ComfyUI didn't answer (or the file has no graph), or the mode is Mark. Start ComfyUI and press **Ctrl+R** to send the marks. |
| Shift+R says "Too late" | ComfyUI has already started that re-roll. |
| A re-roll, hires or workflow key shows a node's name and an error | That's ComfyUI refusing the graph, with its own reason: often a model or LoRA file it can't find, or a custom node that isn't installed. |
| A hires image looks too different | Lower **Denoise** (try 0.2). |
| Send to a workflow says the file is a UI workflow | In ComfyUI use **Workflow > Export (API)**. |
| Send to a workflow says there are several Load Image nodes | Title the one to fill **Loupe** and export again. |
| Sending the same image twice makes nothing | Turn on **New seed** for that workflow. |
| Following shows nothing | Settings > Following ComfyUI shows a status line. Check the address with Test connection. |
| Blink doesn't group my images | Check the names against [3.7](#37-how-blink-finds-groups), or tick them under **Folders**. Press **R** in Blink to look again. |
| Anything else | Settings > Updates > **Open the log folder**, and send `blinkloupe.log` to whoever gave you BlinkLoupe. |

---

# Appendix A: Every key

### The live view (Standard keymap)

| Key | Does |
|---|---|
| ← → ↑ ↓ / wheel | Previous / next image (going back leaves LIVE) |
| PgUp / PgDn | 10 back / forward |
| Home | Oldest image |
| End | Back to LIVE |
| Space | Pause / resume LIVE |
| P | Pick |
| X | Skip |
| 1-5 / 0 | Stars / clear stars |
| B | Bookmark |
| U | Clear pick, skip and stars |
| K | Show / hide skipped images |
| R | Re-roll |
| Shift+R | Cancel the last re-roll |
| Ctrl+R | Queue this folder's `REROLLS.txt` |
| Shift+P | Hires this image |
| Ctrl+P | Hires every pick in this folder not done yet |
| I | Info panel |
| L | Library |
| V | Open in Blink |
| H | Keep the HUD on screen |
| F | Fit / fill / 1:1 |
| O | Rotate 90° |
| T | Always on top |
| F11 / double-click | Full screen |
| E | Show the file in File Explorer |
| C | Copy the file's path |
| F5 | Rescan the folders |
| S | Settings |
| F1 / ? | Key guide |
| Esc | Close the info panel; otherwise quit (asks first) |
| Ctrl+Q | Quit (asks first) |
| Click | Show the HUD |

Plus your own keys from Actions and Send to a workflow.

### Classic keymap differences

| Key | Classic does | Standard key for it |
|---|---|---|
| X | Re-roll | R |
| Shift+X | Cancel the last re-roll | Shift+R |
| D | Skip | X |
| R | Rotate | O |
| Q | Quit | Esc / Ctrl+Q |

Everything else is the same.

### Blink

| Key | Does |
|---|---|
| 1-7 | Side by side, slider, flicker, difference, onion skin, SSIM map, grid |
| Space | Swap A and B (the flick, in flicker) |
| ← → | B = previous / next image in the group |
| Shift+← → | A = previous / next image |
| ↑ ↓ | Previous / next group |
| Click / right-click a thumbnail | Make it A / B |
| Wheel, drag | Zoom at the cursor, pan |
| F / Z | Fit / actual pixels |
| + / - | Zoom in / out |
| Double-click | Fit ↔ actual pixels |
| M | Magnifier |
| P | Sharp pixels on / off |
| L | Lock the slider's line |
| B | Auto-blink on / off (flicker) |
| [ / ] | Difference or SSIM gain down / up |
| / | Search |
| Tab | Hide / show the group list |
| I | Hide / show the numbers panel |
| R | Look again for new images |
| ? / Esc | Show / close the keys |

# Appendix B: Words used in this guide

| Word | Means |
|---|---|
| **A / B** | The two images Blink compares. |
| **Action** | A key you've bound to your own command (Settings > Actions). |
| **CFG** | How strictly the model follows the prompt. |
| **Checkpoint / model** | The main AI model file that makes the image. |
| **Denoise** | How much a sampler pass may change its input, from 0 (nothing) to 1 (everything). |
| **Following** | Getting images straight from a ComfyUI server instead of from a folder. |
| **Graph / workflow** | ComfyUI's network of nodes that makes an image. ComfyUI saves it inside each PNG. |
| **Group** | Images Blink found belong together: versions of one image, or the same name in A/B folders. |
| **Hires** | Upscaling an image and running one light pass that adds detail. |
| **HUD** | The text over the picture that says what you're looking at. |
| **LIVE / HISTORY / PAUSED** | Showing each new image / staying on one you stepped back to / frozen with Space. |
| **LoRA** | A small add-on model that adds a style, character or concept. |
| **Mark** | Pick, skip, stars, bookmark or a re-roll request, saved as plain files beside your images. |
| **Pick** | An image marked as worth doing more with. |
| **Re-roll** | The same graph with a new seed: same settings, different picture. |
| **Sampler / scheduler / steps** | How the model turns noise into an image, and in how many steps. |
| **Seed** | The number that chooses the starting noise. Same seed and settings, same image. |
| **Set** | A folder of images from one run. |
| **Skip** | An image marked "not this one". It's passed over, never deleted. |
| **SSIM / PSNR** | Two ways of measuring how alike two images are (see [2.7](#27-reading-the-numbers)). |
| **VAE** | The part of the model that turns its internal image into pixels and back. |
| **Version** | An image made from another: a re-roll, an upscale, a hires finish. |
| **Watched folder** | A folder BlinkLoupe checks for new images. |

---

*BlinkLoupe User Guide © 2026 mifunetron. Licensed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/): you may share and adapt it, as long as you credit
mifunetron and say what you changed.*
