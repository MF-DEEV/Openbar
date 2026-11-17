# Installs what the island is tested against, run over SSH once the VM is up:
#   ssh -p 2222 -i ~/VMs/win11/id_ed25519 vors@127.0.0.1 powershell -File C:\vors\provision-dev.ps1
# Idempotent: anything already installed is skipped.
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
Start-Transcript -Path C:\vors-provision.log -Append

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

# winget registers itself for a new user a little after the first logon.
for ($i = 0; $i -lt 30 -and -not (Have winget); $i++) {
    Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 10
}

$packages = @(
    'Git.Git',                       # Codex and Claude Code expect Git (and Git Bash) on Windows
    'OpenJS.NodeJS.LTS',             # the npm-installed Codex CLI
    'Microsoft.WindowsTerminal',     # inbox on 24H2+, kept current
    'Microsoft.VisualStudioCode'     # the IDE bridge's test host
)
foreach ($id in $packages) {
    winget install --id $id --exact --silent --accept-package-agreements --accept-source-agreements --disable-interactivity --scope machine 2>&1 | Out-Host
}

# New PATH entries for the rest of this script.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')

if (-not (Have codex)) { npm install -g @openai/codex 2>&1 | Out-Host }

# WebView2 ships with Windows 11; report its version for the record.
Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue | Select-Object pv | Out-Host

New-Item -ItemType Directory -Force -Path C:\vors | Out-Null
"provisioned $(Get-Date -Format o)" | Set-Content C:\vors\provisioned.txt
Stop-Transcript
