"""clean_screen.py IN_DIR OUT_DIR WALLPAPER : keep only the island and the
stand-in Terminal window from each recorded frame (1512x982), over a clean
wallpaper with a plain menu bar, so no private desktop content is shown."""
import glob, os, sys
import numpy as np
from PIL import Image, ImageDraw, ImageFilter
from scipy import ndimage

IN, OUT, WP = sys.argv[1:4]
os.makedirs(OUT, exist_ok=True)
W, H = 1512, 982
wp = Image.open(WP).convert("RGB").resize((W, H), Image.LANCZOS)
wp = Image.blend(wp, Image.new("RGB", (W, H), (4, 2, 14)), 0.35)
MB = 37                                    # menu bar height (pt)
ISL = (440, 0, 1160, 380)                  # island area incl. the side buttons
WIN = (380, 400, 1130, 880)                # stand-in Terminal window

base = np.asarray(wp).copy()
base[:MB] = (10, 8, 20)
win_mask = Image.new("L", (W, H), 0)
ImageDraw.Draw(win_mask).rounded_rectangle(WIN, radius=11, fill=255)
win_mask = np.asarray(win_mask) > 0
shadow = Image.new("L", (W, H), 0)
ImageDraw.Draw(shadow).rounded_rectangle((WIN[0], WIN[1] + 10, WIN[2], WIN[3] + 14), radius=14, fill=170)
shadow = np.asarray(shadow.filter(ImageFilter.GaussianBlur(18)), dtype=np.float32)[..., None] / 255.0
base = (base * (1 - shadow * 0.85)).astype(np.uint8)

for f in sorted(glob.glob(os.path.join(IN, "s_*.jpg"))):
    fr = np.asarray(Image.open(f).convert("RGB"))
    out = base.copy()
    # island: dark pixels in its area, holes (text, icons) filled
    x0, y0, x1, y1 = ISL
    crop = fr[y0:y1, x0:x1].astype(np.int16)
    dark = crop.max(axis=2) < 14
    dark = ndimage.binary_opening(dark, iterations=2)
    lab, n = ndimage.label(dark)
    keep = np.zeros_like(dark)
    if n:
        sizes = ndimage.sum(dark, lab, range(1, n + 1))
        for i, s in enumerate(sizes, 1):
            if s > 400:
                keep |= lab == i
    keep = ndimage.binary_closing(keep, iterations=6)
    keep = ndimage.binary_fill_holes(keep)
    m = np.zeros((H, W), bool)
    m[y0:y1, x0:x1] = keep
    m |= win_mask
    soft = ndimage.gaussian_filter(m.astype(np.float32), 0.8)[..., None]
    out = (fr * soft + out * (1 - soft)).astype(np.uint8)
    Image.fromarray(out).save(os.path.join(OUT, os.path.basename(f)), quality=92)
print("CLEAN", len(glob.glob(os.path.join(OUT, "*.jpg"))))
