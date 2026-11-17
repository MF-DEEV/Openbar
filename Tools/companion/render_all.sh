#!/bin/zsh
# Renders every texture's sprites into $1 (default: ~/Desktop/Projekty/openbar-promo/companion/sprites).
# Blender 5.2.3 sometimes aborts while it compiles Metal kernels (ccl::path_cache_get),
# so each texture is retried until its run reports ALLDONE; finished files are skipped.
OUT=${1:-$HOME/Desktop/Projekty/openbar-promo/companion/sprites}
HERE=${0:A:h}
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
for t in ${=TEXTURES:-clay plush knit gummy}; do
  for try in 1 2 3 4 5 6; do
    perl -e 'alarm 7200; exec @ARGV' "$BLENDER" -b -P "$HERE/blender/render_sprites.py" -- $t "$OUT" ${RES:-384} ${SAMPLES:-96} > "$OUT/log-$t.txt" 2>&1
    grep -q "ALLDONE $t" "$OUT/log-$t.txt" && { echo "$t done"; break; }
    echo "$t try $try failed: $(grep -c SPRITE "$OUT/log-$t.txt") sprites this run"
  done
done
echo ALLDONE
