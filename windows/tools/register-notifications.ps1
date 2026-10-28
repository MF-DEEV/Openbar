# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Development only: register the existing sparse manifest against a built exe.
# Does not enable Developer Mode, import certificates, or restart the app.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath
)

$ErrorActionPreference = 'Stop'
$developerSettings = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue
if ($null -eq $developerSettings -or $developerSettings.AllowDevelopmentWithoutDevLicense -ne 1) {
    throw 'This development registration requires Windows Developer Mode to already be enabled. Release installers must carry a signed identity package.'
}

$executable = Get-Item -LiteralPath $ExecutablePath
if ($executable.PSIsContainer -or $executable.Name -ne 'Vorssaint.exe') {
    throw 'ExecutablePath must point to the built Vorssaint.exe.'
}
$manifestPath = Join-Path $PSScriptRoot '..\packaging\AppxManifest.xml'
$manifest = (Resolve-Path -LiteralPath $manifestPath).ProviderPath
$externalLocation = $executable.DirectoryName
Add-AppxPackage -Path $manifest -Register -ExternalLocation $externalLocation
Write-Output 'Package identity registered for this build. Restart Vorssaint, open Notifications or Calendar, and allow access.'
