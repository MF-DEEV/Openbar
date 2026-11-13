# Brand assets

| File | Use |
| --- | --- |
| `openbar-hero.gif` | The README banner, a six-second loop at 1600 × 450. |
| `openbar-hero.png` | A still frame of the banner. |
| `social-preview.png` | The repository's social preview, 1280 × 640. |
| `../readme/download/*.png` | The README's download buttons for Mac, Windows and Linux, 1120 × 320. |

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

## The download buttons

The same page draws an operating system's mark in place of the helmet: `mark` takes an SVG (`marks/apple.svg`, `marks/windows.svg`, `marks/linux.svg`). The Apple mark is from [Simple Icons](https://simpleicons.org) (CC0); the Windows panes and the Tux are drawn here. Record each one, then put the cards together:

```sh
for b in apple:mac:Mac windows:windows:Windows linux:linux:Linux; do
  IFS=: read mark name text <<<"$b"
  node record.mjs "http://127.0.0.1:5288/glass.html?w=540&h=112&mark=marks/$mark.svg&text=$text&stretch=1.25&t=1.2" glass/$name.png 540 112 1 6 2
done
python3 buttons.py glass ../../readme/download
```
