# LinkFYR installer scripts (defensive).
# - HTTPS only, checksum verification, arch detection, no surprise sudo.
# - Refuses to run when piped without --yes (prevents `curl | sh` foot-guns).
# Usage:
#   irm https://linkfyr.example/install.ps1 | iex   # requires -ForceExecution env gate
#   Recommended: download then run:  .\install.ps1 [-Version v0.1.0] [-Prefix ~/.linkfyr]
[CmdletBinding()]
param(
  [string]$Version = "latest",
  [string]$Channel = "stable",
  [string]$Prefix = "$env:USERPROFILE\.linkfyr",
  [switch]$Yes,
  [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = "PotenFYR-Studios/linkfyr"
$Base = "https://github.com/$Repo/releases"

# Piping protection: refuse when stdin is redirected (iex pipeline) without -Yes.
if (-not $Yes -and -not [Console]::IsInputRedirected -eq $false) {
  if ($MyInvocation.InvocationName -eq "iex" -or $MyInvocation.Line -match "\|\s*iex") {
    Write-Error @"
Refusing to run via 'irm | iex' without consent (this script installs software).
Download it first, inspect it, then run: .\install.ps1 -Yes
"@
    exit 1
  }
}

function Get-Checksum($asset, $expected, $file) {
  if (-not $expected) { Write-Warning "no checksum published for $asset; skipping verification"; return }
  $actual = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne ($expected -split '\s+')[0].ToLower()) {
    throw "checksum mismatch for ${asset}: expected $expected, got $actual"
  }
}

if ($Uninstall) {
  if (Test-Path $Prefix) { Remove-Item $Prefix -Recurse -Force }
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if ($userPath -like "*$Prefix*") {
    $cleaned = ($userPath -split ';' | Where-Object { $_ -and $_ -notlike "$Prefix*" }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $cleaned, "User")
  }
  Write-Host "LinkFYR CLI removed from $Prefix (app uninstall: Windows Apps settings)."
  exit 0
}

# Arch detection
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
  "ARM64" { "arm64" }
  default { "x64" }
}

# Resolve version
if ($Version -eq "latest") {
  $rel = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
  $Version = $rel.tag_name
}
$ver = $Version.TrimStart("v")

# Fetch checksums first, so we never execute unverified bytes.
$sumsUrl = "$Base/download/$Version/SHA256SUMS"
try {
  $sums = (Invoke-WebRequest $sumsUrl -UseBasicParsing).Content -split "`n"
} catch {
  Write-Warning "SHA256SUMS not found for $Version; aborting (we never install unverified builds)."
  exit 1
}

$name = "linkfyr-cli_${ver}_windows_${arch}.zip"
$url = "$Base/download/$Version/$name"
$expected = $sums | Where-Object { $_ -like "*$name*" } | Select-Object -First 1
if (-not $expected) { throw "no checksum entry for $name" }

$tmp = Join-Path $env:TEMP "linkfyr-install-$([guid]::NewGuid())"
New-Item -ItemType Directory -Force -Path $tmp, $Prefix | Out-Null
$zip = Join-Path $tmp $name

Write-Host "Downloading $url"
Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
Get-Checksum $name $expected $zip

Expand-Archive $zip -DestinationPath $tmp -Force
Copy-Item (Join-Path $tmp "linkfyr.exe") $Prefix -Force
Remove-Item $tmp -Recurse -Force

# PATH (user scope; no admin)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$Prefix*") {
  [Environment]::SetEnvironmentVariable("Path", "$Prefix;$userPath", "User")
  Write-Host "Added $Prefix to your user PATH. Restart your shell."
}

Write-Host "Installed linkfyr $Version -> $Prefix\linkfyr.exe"
Write-Host "Try: linkfyr status"
