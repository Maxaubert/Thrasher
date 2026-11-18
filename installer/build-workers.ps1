# Builds the two worker variants installer/build.ps1 packs:
#   CPU workers  -> <target>\release\        (go in the installer)
#   CUDA workers -> <target>\release\cuda\   (go in the GPU pack)
# Build thrasher.exe first (cargo build --release -p thrasher-app), and run one cargo
# command at a time (docs/building.md). CUDA goes first: switching features rebuilds
# whisper.cpp / llama.cpp, and the CPU pass must be the one left in release\.
# Needs the CMake, LIBCLANG_PATH and CUDA setup from docs/building.md.

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
$release = "$target\release"

# Portable builds for any PC (docs/building.md, "Installer and releases").
if (-not $env:CMAKE_CUDA_ARCHITECTURES) { $env:CMAKE_CUDA_ARCHITECTURES = '75;80;86;89;120' }
$env:GGML_NATIVE = 'OFF'

function Invoke-Cargo([string[]]$CargoArgs) {
    Write-Host "cargo $($CargoArgs -join ' ')"
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo $($CargoArgs -join ' ') failed ($LASTEXITCODE)" }
}

Push-Location $root
try {
    Invoke-Cargo @('build', '--release', '-p', 'thrasher-app', '--features', 'whisper-cuda', '--bin', 'thrasher-transcribe')
    Invoke-Cargo @('build', '--release', '-p', 'thrasher-ask', '--features', 'cuda')
    New-Item -ItemType Directory -Force "$release\cuda" | Out-Null
    Copy-Item "$release\thrasher-transcribe.exe", "$release\thrasher-ask.exe" "$release\cuda\" -Force

    Invoke-Cargo @('build', '--release', '-p', 'thrasher-app', '--features', 'whisper', '--bin', 'thrasher-transcribe')
    Invoke-Cargo @('build', '--release', '-p', 'thrasher-ask', '--features', 'llm')
} finally {
    Pop-Location
}

Get-Item "$release\thrasher-transcribe.exe", "$release\thrasher-ask.exe",
    "$release\cuda\thrasher-transcribe.exe", "$release\cuda\thrasher-ask.exe" |
    Select-Object FullName, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
