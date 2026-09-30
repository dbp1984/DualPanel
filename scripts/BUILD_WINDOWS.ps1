$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --release
Write-Host "Built: target\\release\\dbp.exe"
