# Run the complete LinkFYR verification suite in Docker (no host
# toolchain, no tests on your machine). Requires Docker Desktop or a
# compatible engine.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "== LinkFYR Docker verification ==" -ForegroundColor Cyan

$failed = $false
docker compose -f docker-compose.test.yml build rust
if ($LASTEXITCODE -ne 0) { $failed = $true }
else {
    docker compose -f docker-compose.test.yml run --rm rust
    if ($LASTEXITCODE -ne 0) { $failed = $true }
}

docker compose -f docker-compose.test.yml build frontend
if (-not $failed) {
    if ($LASTEXITCODE -ne 0) { $failed = $true }
    else {
        docker compose -f docker-compose.test.yml run --rm frontend
        if ($LASTEXITCODE -ne 0) { $failed = $true }
    }
}

if ($failed) {
    Write-Host "DOCKER VERIFICATION FAILED" -ForegroundColor Red
    exit 1
}
Write-Host "DOCKER VERIFICATION PASSED" -ForegroundColor Green
