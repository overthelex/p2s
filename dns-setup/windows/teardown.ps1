# Remove P2S DNS config from Windows
# Run as Administrator in PowerShell.

param(
    [string[]]$TLDs = @("p2s", "vovkes", "100500")
)

$ErrorActionPreference = "Stop"

if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Error "Run as Administrator"
    exit 1
}

$nrptPath = "HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig"

foreach ($tld in $TLDs) {
    $rulePath = "$nrptPath\P2S-$tld"
    if (Test-Path $rulePath) {
        Remove-Item $rulePath -Recurse -Force
        Write-Host "Removed NRPT rule for .$tld"
    }
}

Clear-DnsClientCache
Write-Host "P2S DNS config removed."
