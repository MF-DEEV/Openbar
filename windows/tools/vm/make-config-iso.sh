#!/usr/bin/env bash
# Builds the small ISO that carries autounattend.xml, the first-logon script,
# OpenSSH and the SSH public key into the VM.
#   VM_DIR (default ~/VMs/win11) must hold id_ed25519.pub, password.txt and
#   OpenSSH-ARM64.zip.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
vm_dir="${VM_DIR:-$HOME/VMs/win11}"
user="${VM_USER:-vors}"
password="$(cat "$vm_dir/password.txt")"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

sed -e "s/@USER@/$user/g" -e "s/@PASSWORD@/$password/g" \
    "$here/autounattend.template.xml" > "$stage/autounattend.xml"
cp "$here/setup-guest.ps1" "$stage/"
cp "$vm_dir/id_ed25519.pub" "$stage/authorized_keys"
cp "$vm_dir/OpenSSH-ARM64.zip" "$stage/"

rm -f "$vm_dir/config.iso"
hdiutil makehybrid -quiet -iso -joliet -default-volume-name VORSCFG -o "$vm_dir/config.iso" "$stage"
echo "$vm_dir/config.iso"
