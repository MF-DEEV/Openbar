# Companion art

The island's companion is a bean made of plush, knit, gummy or clay, with a head item it can put on and take off. Its body and head items are sprites rendered in Blender, one per turn of its head. Its eyes are drawn by the apps as outlines filled with the fabric they are stitched from, so every face can still turn into any other.

## Rebuilding the sprites

1. Render with Blender 5.2:

   ```sh
   Tools/companion/render_all.sh ~/companion-renders
   ```

   This renders 16-bit PNGs for every material, color and turn of the head, every head item, and the eye fabrics. Finished files are skipped, so a run can be resumed. Blender 5.2.3 sometimes aborts while it compiles its Metal kernels, and the script retries when that happens. `TEXTURES`, `RES` and `SAMPLES` override what it renders.

2. Build the apps' sprites and tables:

   ```sh
   python3 Tools/companion/build_assets.py ~/companion-renders
   ```

   This writes `Resources/Companion/*.webp`, which the Mac and Windows/Linux apps share. It also generates `Sources/Openbar/Services/Notch/NotchMascotArt.swift` and `windows/src/mascot/art.ts`, which hold the sprite frame, the eyes' place for each turn of the head and each item's headroom. Both apps' tests check that every sprite a look needs exists and that the two tables agree.

## Files

- `blender/companion_lib.py` holds the shapes (the bean, its feet, the helmet and its crest), the base materials and the studio.
- `blender/companion_v2.py` holds the materials as the apps show them, the eye fabrics and the head items.
- `blender/render_sprites.py` renders one material's sprites. The colors, eye tones and head items are listed at its top.
