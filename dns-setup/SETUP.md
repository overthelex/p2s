# P2S DNS Setup — Custom TLD Resolution

Route queries for custom TLDs (`.p2s`, `.vovkes`, `.100500`) to the `p2s-resolve` daemon.
Only custom TLD traffic is intercepted — standard DNS is untouched.

## Prerequisites

`p2s-resolve` must be running on `127.0.0.53:5353` (default).

## Per-Platform Setup

### macOS

Native per-TLD resolver support via `/etc/resolver/`:

```bash
sudo ./macos/setup.sh
# Verify: scutil --dns | grep p2s
# Remove: sudo ./macos/teardown.sh
```

Each TLD gets its own file in `/etc/resolver/<tld>`. No system DNS changes.

### Linux (systemd-resolved)

Split DNS via resolved drop-in config. Ubuntu 18.04+, Fedora 33+, Arch, Debian 11+:

```bash
sudo ./linux/setup-systemd-resolved.sh
# Verify: resolvectl status
# Remove: sudo rm /etc/systemd/resolved.conf.d/p2s.conf && sudo systemctl restart systemd-resolved
```

Install the systemd service for `p2s-resolve`:

```bash
sudo cp linux/p2s-resolve.service /etc/systemd/system/
sudo systemctl enable --now p2s-resolve
```

### Linux (dnsmasq)

For systems without systemd-resolved (Alpine, minimal installs):

```bash
sudo ./linux/setup-dnsmasq.sh
# Verify: cat /etc/dnsmasq.d/p2s.conf
# Remove: sudo rm /etc/dnsmasq.d/p2s.conf && sudo systemctl restart dnsmasq
```

### Windows

NRPT (Name Resolution Policy Table) — built into Windows 8+:

```powershell
# Run as Administrator
.\windows\setup.ps1
# Verify: Get-DnsClientNrptRule
# Remove: .\windows\teardown.ps1
```

Install `p2s-resolve` as a Windows service:

```cmd
sc.exe create p2s-resolve binPath="C:\Program Files\p2s\p2s-resolve.exe --listen 127.0.0.53:5353" start=auto
sc.exe start p2s-resolve
```

### FreeBSD / Unbound

Stub zones in local-unbound:

```bash
sudo ./freebsd/setup.sh
# Verify: unbound-checkconf
# Remove: sudo rm /var/unbound/conf.d/p2s.conf && sudo service local_unbound restart
```

## Custom TLDs

Set `P2S_TLDS` environment variable to override the default list:

```bash
P2S_TLDS="p2s vovkes 100500 myorg" sudo ./linux/setup-systemd-resolved.sh
```

## How It Works

```
Browser → "api.vovkes" → System DNS
                              │
              Is .vovkes a custom TLD?
              ┌──────┴──────┐
              │ YES         │ NO
              ▼             ▼
         p2s-resolve    Normal DNS
         (127.0.0.53)   (8.8.8.8 etc)
              │
         DHT lookup:
         BLAKE3("p2s:name:" || "api.vovkes")
              │
         NameRecord → owner pubkey
              │
         BLAKE3(pubkey) → CardRecord
              │
         endpoint IP → DNS response
```
