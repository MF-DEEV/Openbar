#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint
#
# Builds the signed identity package inside the Windows build VM (which has
# the Windows SDK) and brings Vorssaint.msix and Vorssaint.cer back to
# packaging/out, ready for an installer built with tauri.identity.conf.json:
#
#   VORSSAINT_SIGNING_PFX='C:\Users\vors\keys\vorssaint-signing.pfx' \
#   VORSSAINT_SIGNING_PASSWORD='…' tools/package-identity/build-sparse-vm.sh [-NewCertificate] [-Revision N]
#
# The PFX path is inside the VM and outside any checkout. The password goes
# to the VM on stdin, never on a command line. Extra arguments are passed to
# build-sparse.ps1 (-NewCertificate makes the key the first time).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
vm="${VM_DIR:-$HOME/VMs/win11}"
remote="${VORSSAINT_VM_IDENTITY_DIR:-C:/vors/identity}"
: "${VORSSAINT_SIGNING_PFX:?the signing PFX path inside the VM}"
: "${VORSSAINT_SIGNING_PASSWORD:?the signing PFX password}"
for arg in "$@"; do
  [[ "$arg" =~ ^(-NewCertificate|-Revision|[0-9]+)$ ]] || { echo "unexpected argument: $arg" >&2; exit 2; }
done
[[ "$VORSSAINT_SIGNING_PFX" != *"'"* ]] || { echo "the PFX path cannot contain a quote" >&2; exit 2; }

# The script finds the manifest, the icon and the app version by the repository's layout.
"$vm/ssh.sh" "New-Item -ItemType Directory -Force -Path '$remote/packaging','$remote/src-tauri','$remote/tools/package-identity' | Out-Null"
"$vm/scp.sh" "$root/packaging/AppxManifest.xml" "$root/packaging/icon.png" "vors@127.0.0.1:$remote/packaging/"
"$vm/scp.sh" "$root/src-tauri/tauri.conf.json" "vors@127.0.0.1:$remote/src-tauri/"
"$vm/scp.sh" "$here/build-sparse.ps1" "vors@127.0.0.1:$remote/tools/package-identity/"

printf '%s' "$VORSSAINT_SIGNING_PASSWORD" | "$vm/ssh.sh" \
  "\$env:VORSSAINT_SIGNING_PASSWORD = [Console]::In.ReadToEnd(); \
   \$env:VORSSAINT_SIGNING_PFX = '$VORSSAINT_SIGNING_PFX'; \
   & '$remote/tools/package-identity/build-sparse.ps1' $*; \
   if (-not \$?) { exit 1 }"

mkdir -p "$root/packaging/out"
"$vm/scp.sh" "vors@127.0.0.1:$remote/packaging/out/Vorssaint.msix" "vors@127.0.0.1:$remote/packaging/out/Vorssaint.cer" \
  "$root/packaging/out/"
echo "Built $root/packaging/out/Vorssaint.msix and Vorssaint.cer."
echo "Installer: bunx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --config src-tauri/tauri.identity.conf.json"
