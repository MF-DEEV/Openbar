#!/usr/bin/env bash
# Builds the Windows ARM64 app on the Mac (cargo-xwin), copies it into the
# test VM and restarts it in the desktop session. VORS_APP_DIR and VORS_TASK
# pick another folder and scheduled task, so a branch can be tested beside
# the build other work uses (e.g. VORS_APP_DIR=C:/vors/sections VORS_TASK=VorsSections).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
vm="${VM_DIR:-$HOME/VMs/win11}"
app_dir="${VORS_APP_DIR:-C:/vors/app}"
task="${VORS_TASK:-VorsIsland}"
export PATH="/opt/homebrew/opt/rustup/bin:/opt/homebrew/opt/llvm/bin:$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"

(cd "$root" && bun run build >/dev/null)
(cd "$root/src-tauri" && XWIN_ACCEPT_LICENSE=1 cargo xwin build --release --target aarch64-pc-windows-msvc \
  --features tauri/custom-protocol 2>&1 | grep -E "^(error|warning: unused)|Finished" || true)

"$vm/ssh.sh" 'Stop-Process -Name Vorssaint -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500' || true
"$vm/scp.sh" "$root/src-tauri/target/aarch64-pc-windows-msvc/release/vorssaint-windows.exe" "vors@127.0.0.1:$app_dir/Vorssaint.exe"
"$vm/ssh.sh" "schtasks /Run /TN $task | Out-Null; Start-Sleep 3; (Get-Process Vorssaint -ErrorAction SilentlyContinue).Id"
