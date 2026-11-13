#!/bin/zsh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

# Points the package managers at a published release (#213):
#   Tools/bump-packages.sh mac <version>       Homebrew cask in MF-DEEV/homebrew-openbar
#   Tools/bump-packages.sh windows <version>   winget manifest, as a PR to microsoft/winget-pkgs
# Both read the files GitHub serves for the release, so run it after the
# release is published. --dry-run prints what would change and pushes nothing.
# Tools/release.sh calls it; it can also be run by hand to retry.
set -euo pipefail
cd "$(dirname "$0")/.."

DRY=0
args=()
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY=1 ;;
        *) args+=("$arg") ;;
    esac
done
(( ${#args[@]} == 2 )) || { echo "usage: $0 mac|windows <version> [--dry-run]" >&2; exit 2; }
PLATFORM="${args[1]}"
VERSION="${args[2]}"
[[ "$VERSION" =~ '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' ]] || { echo "✗ not a version: $VERSION" >&2; exit 2; }
REPO="MF-DEEV/Openbar"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

sha_of() {
    local url="$1" file="$WORK/${1:t}"
    curl -fsSL --retry 3 -o "$file" "$url" || { echo "✗ cannot download $url" >&2; exit 1; }
    shasum -a 256 "$file" | cut -d' ' -f1
}

case "$PLATFORM" in
mac)
    SHA="$(sha_of "https://github.com/$REPO/releases/download/v$VERSION/Openbar-$VERSION.dmg")"
    gh repo clone MF-DEEV/homebrew-openbar "$WORK/tap" -- -q
    mkdir -p "$WORK/tap/Casks"
    sed -e "s/@VERSION@/$VERSION/" -e "s/@SHA256@/$SHA/" packaging/homebrew/openbar.rb > "$WORK/tap/Casks/openbar.rb"
    cd "$WORK/tap"
    if git diff --quiet && [[ -z "$(git status --porcelain)" ]]; then
        echo "✓ the cask already names $VERSION"
        exit 0
    fi
    git --no-pager diff
    if (( DRY )); then echo "(dry run: not pushed)"; exit 0; fi
    git add Casks/openbar.rb
    git commit -q -m "openbar $VERSION"
    git push -q origin HEAD
    echo "✓ cask openbar → $VERSION"
    ;;
windows)
    base="https://github.com/$REPO/releases/download/windows-v$VERSION"
    X64="$(sha_of "$base/Openbar_${VERSION}_x64-setup.exe")"
    ARM="$(sha_of "$base/Openbar_${VERSION}_arm64-setup.exe")"
    OUT="$WORK/manifests/m/MFDEV/Openbar/$VERSION"
    mkdir -p "$OUT"
    for template in packaging/winget/*.yaml; do
        sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA256_X64@/$X64/" -e "s/@SHA256_ARM64@/$ARM/" \
            -e "s/@DATE@/$(date -u +%Y-%m-%d)/" "$template" > "$OUT/${template:t}"
    done
    if (( DRY )); then
        cat "$OUT"/*.yaml
        echo "(dry run: not submitted)"
        exit 0
    fi
    command -v komac >/dev/null || { echo "✗ komac is missing: brew install komac" >&2; exit 1; }
    GITHUB_TOKEN="$(gh auth token)" komac submit --yes "$OUT"
    echo "✓ winget PR for MFDEV.Openbar $VERSION opened"
    ;;
*)
    echo "usage: $0 mac|windows <version> [--dry-run]" >&2
    exit 2
    ;;
esac
