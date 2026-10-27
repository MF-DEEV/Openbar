# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Installed with Vorssaint (resources\identity) and run by the NSIS installer
# hooks: trusts the package certificate, then registers the signed sparse
# package that lends Vorssaint.exe its identity (notifications, calendar).
#   -Action Trusted     exit 0 when this PC already trusts the certificate
#   -Action Trust       (elevated) LocalMachine\TrustedPeople gets the certificate
#   -Action Register    (as the user) the package, with -Directory as its external location
#   -Action Unregister  (as the user) removes the package
# Every failure is reported in %TEMP%\Vorssaint-identity.log and as exit 1;
# the island works without an identity, only without those two sections.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateSet('Trusted', 'Trust', 'Register', 'Unregister')][string]$Action,
    [string]$Directory
)

$ErrorActionPreference = 'Stop'
$packageName = 'Vorssaint.Island'
$publisher = 'CN=Vorssaint'
$package = Join-Path $PSScriptRoot 'Vorssaint.msix'
$certificatePath = Join-Path $PSScriptRoot 'Vorssaint.cer'
$log = Join-Path ([IO.Path]::GetTempPath()) 'Vorssaint-identity.log'

function Get-PackageCertificate {
    $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 -ArgumentList $certificatePath
    # Only the publisher's own code-signing certificate is ever trusted.
    $codeSigning = $certificate.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' }
    if ($certificate.Subject -ne $publisher -or -not $codeSigning) {
        throw "Unexpected certificate: $($certificate.Subject)"
    }
    return $certificate
}

try {
    switch ($Action) {
        'Trusted' {
            $certificate = Get-PackageCertificate
            if (Test-Path -LiteralPath ('Cert:\LocalMachine\TrustedPeople\' + $certificate.Thumbprint)) { exit 0 }
            exit 1
        }
        'Trust' {
            $certificate = Get-PackageCertificate
            $store = New-Object System.Security.Cryptography.X509Certificates.X509Store -ArgumentList 'TrustedPeople', 'LocalMachine'
            $store.Open('ReadWrite')
            try { $store.Add($certificate) } finally { $store.Close() }
        }
        'Register' {
            $folder = (Resolve-Path -LiteralPath $Directory).ProviderPath
            if (-not (Test-Path -LiteralPath (Join-Path $folder 'Vorssaint.exe'))) {
                throw "Vorssaint.exe is not in $folder."
            }
            # Without trust the package can't install; a working registration stays.
            $thumbprint = (Get-PackageCertificate).Thumbprint
            if (-not (Test-Path -LiteralPath ('Cert:\LocalMachine\TrustedPeople\' + $thumbprint))) {
                throw 'The package certificate is not trusted on this PC.'
            }
            try {
                Add-AppxPackage -Path $package -ExternalLocation $folder
            } catch {
                # A package registered before (an older build of the same version,
                # or a development registration) makes way for this one.
                if (-not (Get-AppxPackage -Name $packageName)) { throw }
                Get-AppxPackage -Name $packageName | Remove-AppxPackage
                Add-AppxPackage -Path $package -ExternalLocation $folder
            }
        }
        'Unregister' {
            Get-AppxPackage -Name $packageName | Remove-AppxPackage
        }
    }
    exit 0
} catch {
    Add-Content -LiteralPath $log -Value ('{0:u} {1}: {2}' -f (Get-Date), $Action, $_)
    exit 1
}
