# Packs the release build into installer\dist\Thrasher-Setup-x64-<version>.exe.
# Build the three exes first (docs/building.md, "Installer"). The script copies them,
# NVIDIA's cuBLAS (the workers need it) and the Visual C++ runtime into a staging folder,
# then runs NSIS on installer\thrasher.nsi.

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
$cuda = $env:CUDA_PATH
if (-not $cuda) { throw 'CUDA_PATH is not set; the CUDA toolkit is needed for cuBLAS.' }
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT" -Directory | Sort-Object FullName | Select-Object -Last 1
$nsis = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"

$files = @(
    "$target\release\thrasher.exe",
    "$target\release\thrasher-transcribe.exe",
    "$target\release\thrasher-ask.exe",
    (Get-ChildItem "$cuda\bin" -Recurse -Filter cublas64_*.dll | Where-Object FullName -like '*\x64\*' | Select-Object -First 1).FullName,
    (Get-ChildItem "$cuda\bin" -Recurse -Filter cublasLt64_*.dll | Where-Object FullName -like '*\x64\*' | Select-Object -First 1).FullName,
    "$($crt.FullName)\vcruntime140.dll",
    "$($crt.FullName)\vcruntime140_1.dll",
    "$($crt.FullName)\msvcp140.dll"
)

$stage = "$PSScriptRoot\stage"
$dist = "$PSScriptRoot\dist"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage, $dist | Out-Null
foreach ($f in $files) {
    if (-not $f -or -not (Test-Path $f)) { throw "Missing: $f" }
    Copy-Item $f $stage
}

$out = "$dist\Thrasher-Setup-x64-$version.exe"
& $nsis /V2 "/DVERSION=$version" "/DSTAGE=$stage" "/DOUTFILE=$out" "$PSScriptRoot\thrasher.nsi"
if ($LASTEXITCODE -ne 0) { throw "makensis failed ($LASTEXITCODE)" }
Remove-Item $stage -Recurse -Force
Get-Item $out | Select-Object FullName, Length
