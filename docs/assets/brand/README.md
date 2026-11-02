# Brand assets

| File | Use |
| --- | --- |
| `openbar-hero.gif` | The README banner, a six-second loop at 1600 × 450. |
| `openbar-hero.png` | A still frame of the banner. |
| `social-preview.png` | The repository's social preview, 1280 × 640. |

## How the banner is made

`source/glass.html` draws the helmet and the word into a mask, turns the mask into a rounded bevel with a distance field, and lights it in a WebGL shader: smoky violet glass, a lavender rim and a beam that crosses the word once per loop. The lettering is [Saira](https://fonts.google.com/specimen/Saira) Black, stretched wide, under the SIL Open Font License.

To record it again, serve this folder and run the recorder with Playwright:

```sh
cd docs/assets/brand/source
python3 -m http.server 5288 &
node record.mjs "http://127.0.0.1:5288/glass.html" frames 1280 360 150 6 2
gifski --fps 25 --width 1600 --quality 85 -o ../openbar-hero.gif frames/f*.png
```

The page takes query parameters: `text`, `w`, `h`, `fill`, `stretch`, `bevel`, `period`, `t` for a still at a given second, and `tag` for a line under the word, which the social preview uses (`?w=1280&h=640&fill=0.72&tag=…`).
