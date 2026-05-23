# P2S DNS setup for Windows
#
# Uses NRPT (Name Resolution Policy Table) — built into Windows 8+.
# Only custom TLD queries go to p2s-resolve. System DNS unchanged.
# Run as Administrator in PowerShell.

param(
    [string]$ResolverIP = "127.0.0.53",
    [string[]]$TLDs = @("p2s", "vovkes", "100500")
)

$ErrorActionPreference = "Stop"

# Check admin
if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Error "Run as Administrator"
    exit 1
}

$nrptPath = "HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig"

foreach ($tld in $TLDs) {
    $ruleName = "P2S-$tld"
    $rulePath = "$nrptPath\$ruleName"

    if (Test-Path $rulePath) {
        Remove-Item $rulePath -Recurse -Force
    }

    New-Item -Path $rulePath -Force | Out-Null
    # Match all names under this TLD
    New-ItemProperty -Path $rulePath -Name "Name" -Value ".$tld" -PropertyType String | Out-Null
    # Generic DNS server rule
    New-ItemProperty -Path $rulePath -Name "GenericDNSServers" -Value $ResolverIP -PropertyType String | Out-Null
    New-ItemProperty -Path $rulePath -Name "ConfigOptions" -Value 0x8 -PropertyType DWord | Out-Null
    New-ItemProperty -Path $rulePath -Name "Version" -Value 0x2 -PropertyType DWord | Out-Null

    Write-Host "Created NRPT rule for .$tld -> $ResolverIP"
}

# Flush DNS cache to apply immediately
Clear-DnsClientCache

Write-Host ""
Write-Host "Done. Verify with: Get-DnsClientNrptRule"
Write-Host "Test with:   Resolve-DnsName myservice.p2s"
Write-Host "Uninstall:   .\teardown.ps1"
