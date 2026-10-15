"""Draws BlinkLoupe's icon into src-tauri/icons/icon.ico (16-256 px) and icon.png: Blink's tile, split amber (A) | teal (B)
with the comparison's white divider, under Loupe's magnifier, whose lens shows the split brighter. Small sizes thicken
the lines. (Loupe's own icon, until 2026-10-14, was the magnifier alone on a dark tile.)
    python tools/make_icon.py"""
import os
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "src-tauri", "icons", "icon.ico")
A, B, INK, WHITE = (255, 180, 77), (95, 211, 198), (21, 23, 27), (255, 255, 255)


def lighter(c, t):
    return tuple(int(v + (255 - v) * t) for v in c)


def draw(n):
    S = 1024
    k = S / 64.0
    small = n <= 32
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([2 * k, 2 * k, 62 * k, 62 * k], radius=13 * k, fill=255)
    tile = Image.new("RGBA", (S, S), A + (255,))
    ImageDraw.Draw(tile).rectangle([32 * k, 0, S, S], fill=B + (255,))
    im.paste(tile, (0, 0), mask)
    d = ImageDraw.Draw(im)
    # the divider, under the lens
    bar = 1.6 if not small else 2.6
    d.rectangle([(32 - bar) * k, 2 * k, (32 + bar) * k, 62 * k], fill=WHITE + (255,))
    # the magnifier: handle first, then the lens over it
    cx, cy, r = 29.5, 29.5, 16.5 if not small else 18
    hw = 7.5 if not small else 9
    d.line([((cx + r * 0.62) * k, (cy + r * 0.62) * k), (55 * k, 55 * k)], fill=INK + (255,), width=int(hw * k))
    d.ellipse([(55 - hw / 2) * k, (55 - hw / 2) * k, (55 + hw / 2) * k, (55 + hw / 2) * k], fill=INK + (255,))
    lens = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ld = ImageDraw.Draw(lens)
    ld.rectangle([0, 0, 32 * k, S], fill=lighter(A, 0.45) + (255,))
    ld.rectangle([32 * k, 0, S, S], fill=lighter(B, 0.45) + (255,))
    ld.rectangle([(32 - bar * 1.6) * k, 0, (32 + bar * 1.6) * k, S], fill=WHITE + (255,))
    lm = Image.new("L", (S, S), 0)
    ImageDraw.Draw(lm).ellipse([(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k], fill=255)
    im.paste(lens, (0, 0), lm)
    rim = (3.4 if not small else 4.6) * k
    d.ellipse([(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k], outline=INK + (255,), width=int(rim))
    if not small:  # a glint on the glass
        d.arc([(cx - r * 0.66) * k, (cy - r * 0.66) * k, (cx + r * 0.66) * k, (cy + r * 0.66) * k], 200, 250, fill=WHITE + (230,), width=int(2.2 * k))
    return im.resize((n, n), Image.LANCZOS)


os.makedirs(os.path.dirname(OUT), exist_ok=True)
sizes = [256, 128, 64, 48, 32, 24, 16]
frames = [draw(s) for s in sizes]
frames[0].save(OUT, format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[1:])
frames[0].save(os.path.join(os.path.dirname(OUT), "icon.png"))
prev = Image.new("RGBA", (sum(sizes) + 10 * len(sizes) + 10, 276), (40, 42, 48, 255))
x = 10
for s, f in zip(sizes, frames):
    prev.paste(f, (x, 266 - s), f)
    x += s + 10
prev.save(os.path.join(HERE, "icon-preview.png"))
print("wrote", os.path.normpath(OUT))
