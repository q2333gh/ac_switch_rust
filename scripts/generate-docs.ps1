$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$docsDir = Join-Path $projectRoot "doc"
$cliDocPath = Join-Path $docsDir "cli.md"
$cargoHome = Join-Path $projectRoot ".cargo-local"
$toolsRoot = Join-Path $projectRoot ".tools"
$cargoRdme = Join-Path $toolsRoot "bin\cargo-rdme.exe"
New-Item -ItemType Directory -Force -Path $docsDir | Out-Null

cargo run --manifest-path (Join-Path $projectRoot "Cargo.toml") --example generate_cli_docs --quiet -- $cliDocPath

if (-not (Test-Path $cargoRdme)) {
    Write-Host "Installing cargo-rdme into workspace-local tools..."
    $env:CARGO_HOME = $cargoHome
    cargo install cargo-rdme --version 1.5.0 --locked --root $toolsRoot
}

& $cargoRdme --manifest-path (Join-Path $projectRoot "Cargo.toml") --force
