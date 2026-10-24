#!/usr/bin/env bash
# Starts the Windows 11 ARM64 test VM headless under QEMU with Apple's
# hypervisor. The Mac reaches it through:
#   ssh  -p 2222 -i $VM_DIR/id_ed25519 vors@127.0.0.1
#   vnc  vnc://127.0.0.1:5905 (Screen Sharing, to watch or help by hand)
#   qmp  $VM_DIR/qmp.sock (qmp.py: screenshots, keys, clicks)
#
#   run-vm.sh install   first boot from the Windows ISO (WIN_ISO=path)
#   run-vm.sh           every later boot
set -euo pipefail

vm_dir="${VM_DIR:-$HOME/VMs/win11}"
share="$(brew --prefix qemu)/share/qemu"
mode="${1:-run}"
cd "$vm_dir"

[[ -f disk.qcow2 ]] || qemu-img create -q -f qcow2 disk.qcow2 96G
# The firmware keeps its boot entries in a 64 MiB variable store of its own.
[[ -f efivars.fd ]] || { dd if=/dev/zero of=efivars.fd bs=1m count=64 2>/dev/null; }

# No TPM: tpm-tis-device fails under Apple's hypervisor (16K pages);
# autounattend.xml tells Setup to skip the TPM and Secure Boot checks.

install_drives=()
if [[ "$mode" == install ]]; then
  : "${WIN_ISO:?set WIN_ISO to the Windows 11 ARM64 ISO}"
  VM_GPU="${VM_GPU:-ramfb}"
  install_drives=(
    -drive "if=none,id=win,media=cdrom,readonly=on,file=$WIN_ISO"
    -device usb-storage,drive=win,bootindex=0
    -drive "if=none,id=cfg,media=cdrom,readonly=on,file=$vm_dir/config.iso"
    -device usb-storage,drive=cfg
  )
fi

# Windows Setup draws through ramfb (VM_GPU=ramfb, up to 1024x768): the
# firmware's virtio-gpu screen stops updating once Windows takes over. After
# the first logon installs viogpudo, virtio-gpu gives the full 1920x1080.
gpu=(-device ramfb)
[[ "${VM_GPU:-virtio}" == virtio ]] && gpu=(-device virtio-gpu-pci,xres="${VM_W:-1920}",yres="${VM_H:-1080}")

# A USB sound card, so players and media controls work. Its output goes
# nowhere by default; VM_AUDIO=mac plays it through the Mac's speakers.
audio=(-audiodev none,id=snd0 -device usb-audio,audiodev=snd0)
[[ "${VM_AUDIO:-none}" == mac ]] && audio=(-audiodev coreaudio,id=snd0 -device usb-audio,audiodev=snd0)

rm -f qmp.sock
exec qemu-system-aarch64 \
  -name vorssaint-win11 \
  -machine virt,highmem=on,gic-version=3 -accel hvf -cpu host \
  -smp "${VM_CPUS:-8}" -m "${VM_MEM:-12G}" \
  -drive "if=pflash,format=raw,readonly=on,file=$share/edk2-aarch64-code.fd" \
  -drive "if=pflash,format=raw,file=$vm_dir/efivars.fd" \
  "${gpu[@]}" \
  -device qemu-xhci,id=xhci -device usb-kbd -device usb-tablet \
  ${audio[@]+"${audio[@]}"} \
  -drive if=none,id=hd,file="$vm_dir/disk.qcow2",discard=unmap \
  -device nvme,drive=hd,serial=vorsdisk,bootindex=1 \
  -drive "if=none,id=virtio,media=cdrom,readonly=on,file=$vm_dir/virtio-win.iso" \
  -device usb-storage,drive=virtio \
  ${install_drives[@]+"${install_drives[@]}"} \
  -nic user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:2222-:22 \
  -qmp unix:"$vm_dir/qmp.sock",server,nowait \
  -vnc 127.0.0.1:5 \
  -display none
