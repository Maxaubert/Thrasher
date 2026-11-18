; Thrasher installer (NSIS 3). Built by installer/build.ps1, which passes VERSION and
; STAGE (a folder holding every file to install) and OUTFILE.
; Installs for the current user only, so it needs no admin rights.

Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !error "Pass /DVERSION=x.y.z"
!endif
!ifndef STAGE
  !error "Pass /DSTAGE=<folder with the files to install>"
!endif
!ifndef OUTFILE
  !define OUTFILE "Thrasher-Setup-x64-${VERSION}.exe"
!endif
; The CUDA pack the "NVIDIA GPU acceleration" part downloads (build.ps1 fills these in).
!ifndef GPU_URL
  !error "Pass /DGPU_URL=<pack zip URL>"
!endif
!ifndef GPU_SHA256
  !error "Pass /DGPU_SHA256=<pack zip SHA-256>"
!endif
!ifndef GPU_MB
  !error "Pass /DGPU_MB=<pack size in MB>"
!endif

!define APP "Thrasher"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Thrasher"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${APP}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Thrasher"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 64
ManifestDPIAware true
; A file that cannot be replaced fails the install instead of silently keeping the old one.
AllowSkipFiles off

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "LegalCopyright" "Max Aubert"

!define MUI_ICON "..\crates\thrasher-app\ui\icon\thrasher.ico"
!define MUI_UNICON "..\crates\thrasher-app\ui\icon\thrasher.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\thrasher.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start Thrasher"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; A running Thrasher (or one of its workers) from this folder locks its files. The installer
; is 32-bit, and a 32-bit PowerShell cannot read a 64-bit process's Path, so use the 64-bit
; one through Sysnative (v1.0.0's first build left the old exe in place because of this).
!macro StopThrasher
  nsExec::Exec `"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -Command "Get-Process thrasher,thrasher-transcribe,thrasher-ask -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force; Start-Sleep -Milliseconds 500"`
  Pop $0
!macroend

Section "Thrasher" SecMain
  SectionIn RO
  !insertmacro StopThrasher
  SetOutPath "$INSTDIR"
  ; cuBLAS from an earlier GPU install would sit unused next to the CPU workers below.
  Delete "$INSTDIR\cublas*.dll"
  File "${STAGE}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortCut "$SMPROGRAMS\Thrasher.lnk" "$INSTDIR\thrasher.exe"

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Max Aubert"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\thrasher.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  SectionGetSize ${SecMain} $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" $0
SectionEnd

; Swaps the CPU workers for CUDA ones from the public pack repo (the Thrasher repo is
; private). Only selected when an NVIDIA driver is present (.onInit). A failed download
; keeps the CPU workers, so transcription and the AI still work, only slower.
Section "NVIDIA GPU acceleration (downloads ${GPU_MB} MB)" SecGpu
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "gpu-pack.ps1"
  SetOutPath "$INSTDIR"
  DetailPrint "Downloading the GPU pack (${GPU_MB} MB)..."
  nsExec::ExecToLog `"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\gpu-pack.ps1" -Url "${GPU_URL}" -Sha256 "${GPU_SHA256}" -Dest "$INSTDIR" -Work "$PLUGINSDIR"`
  Pop $0
  StrCmp $0 "0" gpu_done
    MessageBox MB_OK|MB_ICONEXCLAMATION "GPU acceleration could not be installed (see the details). Thrasher is installed and uses the CPU for transcription and the AI. Run the installer again to retry." /SD IDOK
  gpu_done:
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "The recorder, with transcription and the local AI on the CPU."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecGpu} "Runs transcription and the local AI on an NVIDIA graphics card, many times faster. Downloads NVIDIA cuBLAS and the CUDA workers (${GPU_MB} MB)."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; nvcuda.dll is the NVIDIA driver's CUDA library. Without it the CUDA workers cannot run,
; so the GPU part starts unticked (and stays off in a silent install).
Function .onInit
  IfFileExists "$WINDIR\Sysnative\nvcuda.dll" +2 0
    SectionSetFlags ${SecGpu} 0
FunctionEnd

; Removes the program only. Recordings, settings and downloaded models stay.
Section "Uninstall"
  !insertmacro StopThrasher
  Delete "$INSTDIR\thrasher.exe"
  Delete "$INSTDIR\thrasher-transcribe.exe"
  Delete "$INSTDIR\thrasher-ask.exe"
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Thrasher.lnk"
  ; "Start with Windows" (crates/thrasher-app/src/startup.rs) would point at a missing exe.
  DeleteRegValue HKCU "${RUN_KEY}" "Thrasher"
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
