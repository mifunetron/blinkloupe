"""Draws the installer's welcome / finish sidebar (164x314, 24-bit BMP, NSIS Modern UI) from BlinkLoupe's icon.

    python tools/make_installer_art.py   ->   tools/installer-side.bmp
"""
import os
from PIL import Image, ImageDraw, ImageFont

here = os.path.dirname(os.path.abspath(__file__))
icon = Image.open(os.path.join(here, "..", "src-tauri", "icons", "icon.png")).convert("RGBA")
W, H = 164, 314
bg, accent, dim = (22, 24, 28), (232, 196, 106), (150, 155, 162)
img = Image.new("RGB", (W, H), bg)
d = ImageDraw.Draw(img)
# a faint accent band down the right edge, like the HUD's frame
d.rectangle([W - 3, 0, W, H], fill=(60, 52, 32))
mark = icon.resize((96, 96), Image.LANCZOS)
img.paste(mark, ((W - 96) // 2, 64), mark)


def font(name, size):
    try:
        return ImageFont.truetype(os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts", name), size)
    except OSError:
        return ImageFont.load_default()


title = font("seguisb.ttf", 23)
small = font("segoeui.ttf", 12)
for text, f, fill, y in [("BlinkLoupe", title, accent, 176), ("live view and comparison", small, dim, 214), ("for AI image generation", small, dim, 230)]:
    w = d.textlength(text, font=f)
    d.text(((W - w) / 2, y), text, font=f, fill=fill)
img.save(os.path.join(here, "installer-side.bmp"))
print("tools/installer-side.bmp", img.size)
