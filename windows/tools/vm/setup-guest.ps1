# Runs once at the first logon of the test VM (see autounattend.template.xml).
# Installs drivers and OpenSSH so the Mac can drive the VM over SSH, and keeps
# the session awake. Developer tools are installed later over SSH
# (provision-dev.ps1), when the network is known to work.
param([string]$ConfigRoot)

$ErrorActionPreference = 'Continue'
Start-Transcript -Path C:\vors-setup.log -Append

# Every driver on the virtio-win ISO that fits this machine (network, GPU, balloon...).
foreach ($d in Get-PSDrive -PSProvider FileSystem) {
    if (Test-Path (Join-Path $d.Root 'NetKVM')) {
        Get-ChildItem -Path $d.Root -Recurse -Filter *.inf -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\w11\\ARM64\\' } |
            ForEach-Object { pnputil /add-driver $_.FullName /install | Out-Null }
    }
}

# OpenSSH server from the bundled ARM64 zip (no Windows Update needed).
$zip = Join-Path $ConfigRoot 'OpenSSH-ARM64.zip'
Expand-Archive -Path $zip -DestinationPath 'C:\Program Files' -Force
& 'C:\Program Files\OpenSSH-ARM64\install-sshd.ps1'
Set-Service sshd -StartupType Automatic
New-NetFirewallRule -Name sshd -DisplayName 'OpenSSH Server' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22 -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path C:\ProgramData\ssh | Out-Null
Copy-Item (Join-Path $ConfigRoot 'authorized_keys') C:\ProgramData\ssh\administrators_authorized_keys -Force
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F' | Out-Null
New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell -Value 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -PropertyType String -Force | Out-Null
[Environment]::SetEnvironmentVariable('Path', $env:Path + ';C:\Program Files\OpenSSH-ARM64', 'Machine')
Start-Service sshd

# A test VM that never sleeps, locks or restarts on its own.
powercfg /change monitor-timeout-ac 0
powercfg /change standby-timeout-ac 0
powercfg /change hibernate-timeout-ac 0
reg add 'HKCU\Control Panel\Desktop' /v ScreenSaveActive /t REG_SZ /d 0 /f | Out-Null
reg add 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Personalization' /v NoLockScreen /t REG_DWORD /d 1 /f | Out-Null
reg add 'HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' /v NoAutoRebootWithLoggedOnUsers /t REG_DWORD /d 1 /f | Out-Null

# File extensions visible, no first-run tips.
reg add 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' /v HideFileExt /t REG_DWORD /d 0 /f | Out-Null
reg add 'HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager' /v SubscribedContent-310093Enabled /t REG_DWORD /d 0 /f | Out-Null

New-Item -ItemType File -Force -Path C:\vors-setup.done | Out-Null
Stop-Transcript
