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
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; A running Thrasher (or one of its workers) from this folder locks its files.
!macro StopThrasher
  nsExec::Exec `powershell -NoProfile -NonInteractive -Command "Get-Process thrasher,thrasher-transcribe,thrasher-ask -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '$INSTDIR\*' } | Stop-Process -Force; Start-Sleep -Milliseconds 500"`
  Pop $0
!macroend

Section "Thrasher" SecMain
  SectionIn RO
  !insertmacro StopThrasher
  SetOutPath "$INSTDIR"
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
