"""loopgif.py IN OUT N FADE : crossfade frames N+1..N+FADE into 1..FADE -> OUT/l_####.png (N frames)."""
import os, sys
from PIL import Image
src, dst, N, F = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
os.makedirs(dst, exist_ok=True)
fr = lambda i: Image.open(os.path.join(src, f"f_{i:04d}.png")).convert("RGB")
for i in range(1, N + 1):
    im = fr(i)
    if i <= F:
        t = (i - 1) / F
        im = Image.blend(fr(N + i), im, t)
    im.save(os.path.join(dst, f"l_{i:04d}.png"))
print("LOOP", N, "->", dst)
