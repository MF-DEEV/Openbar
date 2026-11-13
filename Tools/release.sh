#!/bin/zsh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

# Cuts a release from the checked-out commit (#213), in one go:
#   Tools/release.sh --mac --notes-mac notes-mac.md
#   Tools/release.sh --windows --notes-windows notes-win.md
#   Tools/release.sh --mac --windows --notes-mac a.md --notes-windows b.md
#
# For each platform it builds the installers, signs them with the update keys
# (the installed apps refuse anything else), tags the commit, publishes the
# GitHub release with the files the in-app updater reads, and then points
# Homebrew and winget at it (Tools/bump-packages.sh). Versions come from
# Resources/Info.plist and windows/src-tauri/tauri.conf.json; bump them in a
# PR first. Keys: see docs/RELEASING.md. --dry-run builds and signs into dist/
# and stops before tagging.
set -euo pipefail
cd "$(dirname "$0")/.."

MAC=0 WIN=0 DRY=0 NOTES_MAC="" NOTES_WIN=""
while (( $# )); do
    case "$1" in
        --mac) MAC=1 ;;
        --windows) WIN=1 ;;
        --dry-run) DRY=1 ;;
        --notes-mac) NOTES_MAC="$2"; shift ;;
        --notes-windows) NOTES_WIN="$2"; shift ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
    shift
done
(( MAC || WIN )) || { echo "usage: $0 [--mac] [--windows] [--notes-mac F] [--notes-windows F] [--dry-run]" >&2; exit 2; }
(( !MAC )) || [[ -f "$NOTES_MAC" ]] || { echo "✗ --notes-mac <file> is required" >&2; exit 2; }
(( !WIN )) || [[ -f "$NOTES_WIN" ]] || { echo "✗ --notes-windows <file> is required" >&2; exit 2; }

REPO="MF-DEEV/Openbar"
COMMIT="$(git rev-parse HEAD)"
if (( !DRY )); then
    [[ -z "$(git status --porcelain --untracked-files=no)" ]] || { echo "✗ uncommitted changes" >&2; exit 1; }
    git fetch -q origin
    git branch -r --contains "$COMMIT" | grep -q . || { echo "✗ push $COMMIT first" >&2; exit 1; }
fi
mkdir -p dist

if (( MAC )); then
    MAC_V="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' Resources/Info.plist)"
    MAC_TAG="v$MAC_V"
    echo "▸ Mac $MAC_V"
    expected="$(sed -n 's/.*publicKeyBase64 = "\(.*\)".*/\1/p' Sources/Openbar/Services/Update/UpdateSignature.swift)"
    actual="$(swift Tools/sign-update.swift --public-key)"
    [[ "$expected" == "$actual" ]] || { echo "✗ the keychain's update key doesn't match UpdateSignature.publicKeyBase64" >&2; exit 1; }
    ./build.sh
    Tools/make-dmg.sh
    DMG="dist/Openbar-$MAC_V.dmg"
    swift Tools/sign-update.swift "$DMG" >/dev/null
    echo "✓ $DMG + .sig"
fi

if (( WIN )); then
    WIN_V="$(python3 -c 'import json;print(json.load(open("windows/src-tauri/tauri.conf.json"))["version"])')"
    WIN_TAG="windows-v$WIN_V"
    echo "▸ Windows $WIN_V"
    KEY="$HOME/.openbar-release/tauri-updater.key"
    [[ -f "$KEY" ]] || { echo "✗ $KEY is missing" >&2; exit 1; }
    pub="$(python3 -c 'import json;print(json.load(open("windows/src-tauri/tauri.conf.json"))["plugins"]["updater"]["pubkey"])')"
    [[ "$pub" == "$(cat "$KEY.pub")" ]] || { echo "✗ $KEY.pub doesn't match tauri.conf.json's updater pubkey" >&2; exit 1; }
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s openbar-tauri-updater -a key-password -w)"
    export PATH="/opt/homebrew/opt/rustup/bin:/opt/homebrew/opt/llvm/bin:$HOME/.cargo/bin:$PATH" XWIN_ACCEPT_LICENSE=1
    typeset -A ARCH=(x86_64-pc-windows-msvc x64 aarch64-pc-windows-msvc arm64)
    typeset -A PLATFORM=(x64 windows-x86_64 arm64 windows-aarch64)
    platforms=()
    for target arch in ${(kv)ARCH}; do
        (cd windows && tools/xwin-cargo.sh bunx tauri build --runner cargo-xwin --target "$target")
        exe="windows/src-tauri/target/$target/release/bundle/nsis/Openbar_${WIN_V}_${arch}-setup.exe"
        cp "$exe" dist/
        (cd windows && bunx tauri signer sign -f "$KEY" "../dist/${exe:t}" >/dev/null)
        platforms+=("${PLATFORM[$arch]}" "https://github.com/$REPO/releases/download/$WIN_TAG/${exe:t}" "$(cat "dist/${exe:t}.sig")")
        echo "✓ dist/${exe:t} + .sig"
    done
    python3 - "$WIN_V" "$NOTES_WIN" "${platforms[@]}" > dist/latest.json <<'PY'
import json, sys, datetime
version, notes_file, *rest = sys.argv[1:]
platforms = {rest[i]: {"url": rest[i + 1], "signature": rest[i + 2]} for i in range(0, len(rest), 3)}
print(json.dumps({
    "version": version,
    "notes": open(notes_file).read().strip(),
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": platforms,
}, indent=2))
PY
    echo "✓ dist/latest.json"
fi

if (( DRY )); then
    echo "(dry run: nothing tagged or published)"
    exit 0
fi

prerelease() { [[ "$1" == *-* || "$2" == 1 ]] && echo --prerelease || true; }
if (( MAC )); then
    git tag "$MAC_TAG" "$COMMIT"
    git push -q origin "$MAC_TAG"
    title="Openbar for Mac ${MAC_V/-beta./ beta }"
    gh release create "$MAC_TAG" -R "$REPO" --verify-tag --title "$title" --notes-file "$NOTES_MAC" \
        $(prerelease "$MAC_V" 0) "dist/Openbar-$MAC_V.dmg" "dist/Openbar-$MAC_V.dmg.sig"
    Tools/bump-packages.sh mac "$MAC_V"
fi
if (( WIN )); then
    # Every Windows release so far is a beta: below 1.0 it stays one.
    beta=0; [[ "$WIN_V" == 0.* ]] && beta=1
    git tag "$WIN_TAG" "$COMMIT"
    git push -q origin "$WIN_TAG"
    title="Openbar for Windows $WIN_V"; (( beta )) && title+=" (beta)"
    gh release create "$WIN_TAG" -R "$REPO" --verify-tag --title "$title" --notes-file "$NOTES_WIN" \
        $(prerelease "$WIN_V" "$beta") dist/Openbar_${WIN_V}_*-setup.exe dist/Openbar_${WIN_V}_*-setup.exe.sig dist/latest.json
    Tools/bump-packages.sh windows "$WIN_V"
fi
echo "✓ released"
