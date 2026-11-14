"""blender -b --factory-startup -P kpost.py -- IN_DIR OUT_DIR [GRADE]

Every IN_DIR/f_####.exr (multilayer: Combined + 'stars' AOV) -> OUT_DIR/f_####.png:
bloom, the reel's crossed star flares (white-pink core, green-tinted arms),
saturation, crushed blacks, vignette, sRGB. Skips frames already done unless
FORCE=1. GRADE (env or arg) picks a per-shot grade: road | throne | pyramids.
"""
import glob
import os
import sys

import numpy as np
import OpenImageIO as oiio

a = sys.argv[sys.argv.index("--") + 1:]
IN, OUT = a[0], a[1]
GRADE = a[2] if len(a) > 2 else os.environ.get("GRADE", "road")
os.makedirs(OUT, exist_ok=True)

GRADES = {
    # sat, black point, gamma-ish contrast, streak gain, bloom gain, vignette
    "road": dict(sat=1.22, black=0.012, contrast=1.08, streak=70.0, bloom=0.7, vig=0.42),
    "throne": dict(sat=1.18, black=0.010, contrast=1.06, streak=10.0, bloom=0.85, vig=0.5),
    "pyramids": dict(sat=1.2, black=0.012, contrast=1.08, streak=12.0, bloom=0.7, vig=0.45),
    "banner": dict(sat=1.18, black=0.008, contrast=1.06, streak=14.0, bloom=0.8, vig=0.38),
    "word": dict(sat=1.12, black=0.004, contrast=1.04, streak=8.0, bloom=0.6, vig=0.0),
    "button": dict(sat=1.1, black=0.0, contrast=1.03, streak=6.0, bloom=0.45, vig=0.0),
}
G = GRADES[GRADE]


def read(path):
    """Multipart EXR: Combined and the stars AOV live in separate parts."""
    inp = oiio.ImageInput.open(path)
    layers = {}
    i = 0
    while inp.seek_subimage(i, 0):
        names = list(inp.spec().channelnames)
        px = inp.read_image(oiio.FLOAT)
        for layer in ("Combined", "stars"):
            idx = [names.index(n) for c in "RGB" for n in names if n.endswith(f"{layer}.{c}")]
            if len(idx) == 3:
                layers[layer] = px[..., idx].astype(np.float32)
            if layer == "Combined":
                ia = [names.index(n) for n in names if n.endswith("Combined.A")]
                if ia:
                    layers["alpha"] = px[..., ia[0]].astype(np.float32)
        i += 1
    inp.close()
    B = layers["Combined"]
    return B, layers.get("stars", np.zeros_like(B)), layers.get("alpha")


_K = {}


def fft_conv(img, ker, key):
    ph, pw = img.shape[0] + ker.shape[0], img.shape[1] + ker.shape[1]
    kk = (key, ph, pw)
    if kk not in _K:
        _K[kk] = np.fft.rfft2(ker, s=(ph, pw))
    K = _K[kk]
    out = np.empty_like(img)
    oy, ox = ker.shape[0] // 2, ker.shape[1] // 2
    for c in range(img.shape[2]):
        r = np.fft.irfft2(np.fft.rfft2(img[..., c], s=(ph, pw)) * K, s=(ph, pw))
        out[..., c] = r[oy:oy + img.shape[0], ox:ox + img.shape[1]]
    return out


def gauss_ker(sigma):
    r = max(1, int(sigma * 3))
    x = np.arange(-r, r + 1, dtype=np.float32)
    g = np.exp(-x * x / (2 * sigma * sigma))
    g /= g.sum()
    return g[:, None] * g[None, :]


def post(B, S):
    H, W = B.shape[:2]
    k = W / 1080.0
    B = np.maximum(B - S, 0)          # the stars come back below as styled flares
    S = S * G.get("star_k", 0.35)
    lum = B @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    thr = np.clip((lum - 0.85) / 1.5, 0, 1)[..., None]
    hi = B * thr
    bloom = (0.55 * fft_conv(hi, gauss_ker(4 * k), "b1") + 0.35 * fft_conv(hi, gauss_ker(16 * k), "b2")
             + 0.25 * fft_conv(hi, gauss_ker(55 * k), "b3"))
    # star flares, torn: four main rays of unequal length broken up along
    # their length, a few thin stray rays, a pink core, green-tinted arms
    L = int(140 * k) | 1
    c = L // 2
    yy, xx = np.mgrid[-c:c + 1, -c:c + 1].astype(np.float32)
    rr = np.sqrt(xx * xx + yy * yy) + 1e-3
    ang = np.arctan2(yy, xx)
    rs = np.random.default_rng(5)
    w = max(0.6, 0.8 * k)

    def ray(theta, length, width, broken):
        d = np.abs(np.angle(np.exp(1j * (ang - theta))))
        across = rr * np.sin(np.minimum(d, np.pi / 2))
        along = rr * np.cos(np.minimum(d, np.pi / 2))
        prof = np.exp(-along / length) * np.exp(-(across ** 2) / (2 * width * width)) * (d < np.pi / 2)
        if broken:
            xp = np.arange(0, c + 2, 3 * k + 1)
            nz = np.interp(along, xp, rs.uniform(0.15, 1.0, len(xp)))
            prof *= nz
        return prof
    ker = np.zeros_like(rr)
    for i, th in enumerate((0, np.pi / 2, np.pi, -np.pi / 2)):
        ker += ray(th + rs.uniform(-0.06, 0.06), (9 + rs.uniform(-3, 6)) * k, w * rs.uniform(0.8, 1.5), True)
    ker_x = np.zeros_like(rr)
    for _ in range(7):
        ker_x += 0.35 * ray(rs.uniform(-np.pi, np.pi), rs.uniform(2.5, 6) * k, w * 0.7, True)
    s1 = ker.sum() + ker_x.sum()
    Ls = (S @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32))[..., None]
    streak = fft_conv(Ls, ker / s1, "s1") * G["streak"]
    streak_x = fft_conv(Ls, ker_x / s1, "s2") * G["streak"]
    core = fft_conv(S, gauss_ker(1.5 * k), "c") * 4.0
    halo = fft_conv(S, gauss_ker(4 * k), "h") * 26.0 + fft_conv(S, gauss_ker(12 * k), "h2") * 10.0
    img = (B + bloom * G["bloom"]
           + streak * np.array([0.42, 1.0, 0.36], np.float32)
           + streak_x * np.array([1.0, 0.55, 1.0], np.float32)
           + core * np.array([1.0, 0.8, 1.0], np.float32)
           + halo * np.array([1.0, 0.4, 1.25], np.float32))
    l2 = (img @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32))[..., None]
    img = l2 + (img - l2) * G["sat"]
    img = np.maximum(img - G["black"], 0) / (1 - G["black"])
    ny, nx = np.mgrid[0:H, 0:W].astype(np.float32)
    r = np.sqrt(((nx - W / 2) / (W / 2)) ** 2 * 0.7 + ((ny - H / 2) / (H / 2)) ** 2)
    img *= (1 - G["vig"] * np.clip(r - 0.4, 0, 1) ** 1.6)[..., None]
    img = np.clip(img, 0, None)
    img = img / (1 + 0.04 * img)          # soft shoulder for the hot spots
    img = np.clip(img * 1.04, 0, 1)
    srgb = np.where(img <= 0.0031308, img * 12.92, 1.055 * np.power(img, 1 / 2.4) - 0.055)
    srgb = np.clip((srgb - 0.5) * G["contrast"] + 0.5, 0, 1)
    return srgb


def write_png(path, rgb):
    H, W = rgb.shape[:2]
    spec = oiio.ImageSpec(W, H, rgb.shape[2], oiio.UINT8)
    o = oiio.ImageOutput.create(path)
    o.open(path, spec)
    ok = o.write_image(np.ascontiguousarray((rgb * 255 + 0.5).astype(np.uint8)))
    if not ok:
        print("WRITE FAIL", o.geterror())
    o.close()


files = sorted(glob.glob(os.path.join(IN, "f_*.exr")))
done = 0
for f in files:
    if os.path.getsize(f) < 1000:
        continue                      # placeholder of a frame still rendering
    out = os.path.join(OUT, os.path.basename(f)[:-4] + ".png")
    if os.path.exists(out) and os.environ.get("FORCE", "0") == "0":
        continue
    B, S, A = read(f)
    print("IN", f, B.shape, float(B.mean()), float(S.max()))
    rgb = post(B, S)
    if os.environ.get("ALPHA") == "1" and A is not None:
        # premultiplied render -> straight alpha; the glow widens alpha a little
        glow = np.clip(rgb.max(axis=2) * 1.2, 0, 1)
        a = np.clip(np.maximum(A, glow * (A < 0.99)), 0, 1)
        rgb = np.where(a[..., None] > 1e-4, np.clip(rgb / np.maximum(a[..., None], 1e-4), 0, 1), 0)
        rgb = np.concatenate([rgb, a[..., None]], axis=2)
    write_png(out, rgb)
    done += 1
print("POST", GRADE, done, "frames ->", OUT)
