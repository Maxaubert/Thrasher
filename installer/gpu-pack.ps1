# Run by the installer's "NVIDIA GPU acceleration" part (thrasher.nsi). Downloads the
# CUDA pack, checks its SHA-256 against the value baked into the installer, and only
# then replaces the CPU workers with the CUDA ones and adds cuBLAS. Any failure leaves
# the CPU workers in place, so Thrasher keeps working, just on the CPU.
# Exit code 0 = installed, anything else = not installed (the reason is on stdout).

param(
    [Parameter(Mandatory)][string]$Url,
    [Parameter(Mandatory)][string]$Sha256,
    [Parameter(Mandatory)][string]$Dest,
    [Parameter(Mandatory)][string]$Work
)

$ErrorActionPreference = 'Stop'
$zip = Join-Path $Work 'gpu-pack.zip'
$unpacked = Join-Path $Work 'gpu-pack'

try {
    Write-Output "Downloading $Url"
    # curl.exe ships with Windows 10 1803 and later and is far faster than Invoke-WebRequest.
    & "$env:WINDIR\System32\curl.exe" -sS -L --fail --retry 3 -o $zip $Url
    if ($LASTEXITCODE -ne 0) { throw "download failed (curl exit $LASTEXITCODE)" }

    # .NET rather than Get-FileHash: that cmdlet lives in a module that fails to load
    # when the installer inherits a PowerShell 7 PSModulePath (seen 2026-11-18).
    $stream = [IO.File]::OpenRead($zip)
    try {
        $actual = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($stream)) -replace '-', ''
    } finally {
        $stream.Dispose()
    }
    if ($actual -ne $Sha256.ToUpperInvariant()) { throw "checksum mismatch: got $actual" }

    New-Item -ItemType Directory -Force $unpacked | Out-Null
    & "$env:WINDIR\System32\tar.exe" -xf $zip -C $unpacked
    if ($LASTEXITCODE -ne 0) { throw "unpacking failed (tar exit $LASTEXITCODE)" }
    foreach ($name in 'thrasher-transcribe.exe', 'thrasher-ask.exe') {
        if (-not (Test-Path (Join-Path $unpacked $name))) { throw "the pack has no $name" }
    }

    # cuBLAS first: a CUDA worker without it cannot start, a CPU worker ignores it.
    Get-ChildItem $unpacked -Filter *.dll | Move-Item -Destination $Dest -Force
    Get-ChildItem $unpacked -Filter *.exe | Move-Item -Destination $Dest -Force
    Write-Output 'GPU acceleration installed.'
    exit 0
} catch {
    Write-Output "GPU acceleration not installed: $($_.Exception.Message)"
    exit 1
} finally {
    Remove-Item $zip, $unpacked -Recurse -Force -ErrorAction SilentlyContinue
}
