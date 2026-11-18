# Packs the release build into installer\dist\:
#   Thrasher-Setup-x64-<version>.exe     app + CPU workers + Visual C++ runtime
#   Thrasher-GPU-CUDA-x64-<version>.zip  CUDA workers + NVIDIA cuBLAS (~550 MB of the old installer)
# The installer's "NVIDIA GPU acceleration" part downloads the zip from the public repo
# Maxaubert/thrasher-gpu-pack (Thrasher itself is private, so its own releases need a
# login) and checks it against the SHA-256 this script bakes in.
# Build first: thrasher.exe (cargo build --release -p thrasher-app), then
# installer\build-workers.ps1 (docs/building.md, "Installer and releases").

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
$cuda = $env:CUDA_PATH
if (-not $cuda) { throw 'CUDA_PATH is not set; the CUDA toolkit is needed for cuBLAS.' }
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT" -Directory | Sort-Object FullName | Select-Object -Last 1
$nsis = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
$packRepo = 'Maxaubert/thrasher-gpu-pack'
$packName = "Thrasher-GPU-CUDA-x64-$version.zip"
$packUrl = "https://github.com/$packRepo/releases/download/v$version/$packName"

$files = @(
    "$target\release\thrasher.exe",
    "$target\release\thrasher-transcribe.exe",
    "$target\release\thrasher-ask.exe",
    "$($crt.FullName)\vcruntime140.dll",
    "$($crt.FullName)\vcruntime140_1.dll",
    "$($crt.FullName)\msvcp140.dll"
)
$gpuFiles = @(
    "$target\release\cuda\thrasher-transcribe.exe",
    "$target\release\cuda\thrasher-ask.exe",
    (Get-ChildItem "$cuda\bin" -Recurse -Filter cublas64_*.dll | Where-Object FullName -like '*\x64\*' | Select-Object -First 1).FullName,
    (Get-ChildItem "$cuda\bin" -Recurse -Filter cublasLt64_*.dll | Where-Object FullName -like '*\x64\*' | Select-Object -First 1).FullName
)

# A CPU worker is a few MB; a CUDA one is hundreds. Catch a swapped pair before it ships.
foreach ($w in 'thrasher-transcribe.exe', 'thrasher-ask.exe') {
    $cpu = (Get-Item "$target\release\$w").Length
    $gpu = (Get-Item "$target\release\cuda\$w").Length
    if ($cpu -ge $gpu) { throw "$w in release\ is not smaller than the CUDA one; run installer\build-workers.ps1" }
}

$stage = "$PSScriptRoot\stage"
$gpuStage = "$PSScriptRoot\stage-gpu"
$dist = "$PSScriptRoot\dist"
Remove-Item $stage, $gpuStage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage, $gpuStage, $dist | Out-Null
foreach ($f in $files) {
    if (-not $f -or -not (Test-Path $f)) { throw "Missing: $f" }
    Copy-Item $f $stage
}
foreach ($f in $gpuFiles) {
    if (-not $f -or -not (Test-Path $f)) { throw "Missing: $f" }
    Copy-Item $f $gpuStage
}

$pack = "$dist\$packName"
Remove-Item $pack -ErrorAction SilentlyContinue
& "$env:WINDIR\System32\tar.exe" -a -cf $pack -C $gpuStage @((Get-ChildItem $gpuStage).Name)
if ($LASTEXITCODE -ne 0) { throw "tar failed ($LASTEXITCODE)" }
$packSha = (Get-FileHash $pack -Algorithm SHA256).Hash
$packMb = [math]::Ceiling((Get-Item $pack).Length / 1MB)

$out = "$dist\Thrasher-Setup-x64-$version.exe"
& $nsis /V2 "/DVERSION=$version" "/DSTAGE=$stage" "/DOUTFILE=$out" `
    "/DGPU_URL=$packUrl" "/DGPU_SHA256=$packSha" "/DGPU_MB=$packMb" "$PSScriptRoot\thrasher.nsi"
if ($LASTEXITCODE -ne 0) { throw "makensis failed ($LASTEXITCODE)" }
Remove-Item $stage, $gpuStage -Recurse -Force
Get-Item $out, $pack | Select-Object FullName, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }

Write-Host ''
Write-Host 'Publish the pack BEFORE the installer, or its GPU part fails to download:'
Write-Host "  gh release create v$version -R $packRepo `"$pack`" --title `"Thrasher $version GPU pack`" --notes `"CUDA workers and cuBLAS for Thrasher $version.`""
Write-Host "  gh release create v$version `"$out`""
