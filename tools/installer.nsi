; BlinkLoupe's Windows installer (NSIS 3, Modern UI 2). Built by tools\release.ps1:
;   makensis /DVERSION=0.10.0 tools\installer.nsi   ->   dist\BlinkLoupe-0.10.0-setup.exe
; Per-user: no administrator rights, installs to %LOCALAPPDATA%\Programs\BlinkLoupe, listed in Apps & features.
; /S installs silently; /S /R (the in-app updater) also starts BlinkLoupe again afterwards.
; Loupe was renamed BlinkLoupe on 2026-10-14: an old Loupe install (0.9.0) is removed after this one is in place, keeping
; its settings, which BlinkLoupe copies over on its first start.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !error "Pass the version: makensis /DVERSION=x.y.z installer.nsi"
!endif

!define APP "BlinkLoupe"
!define EXE "BlinkLoupe.exe"
!define PUBLISHER "mifunetron"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\BlinkLoupe"
!define OLD_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Loupe"
!define WEBVIEW2 "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

Name "${APP}"
OutFile "..\dist\BlinkLoupe-${VERSION}-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\BlinkLoupe"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "BlinkLoupe ${VERSION}"
ShowInstDetails nevershow
ShowUninstDetails nevershow

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "FileDescription" "BlinkLoupe setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "${PUBLISHER}"

!define MUI_ICON "..\src-tauri\icons\icon.ico"
!define MUI_UNICON "..\src-tauri\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEFINISHPAGE_BITMAP "installer-side.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "installer-side.bmp"
!define MUI_WELCOMEPAGE_TITLE "BlinkLoupe ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "BlinkLoupe shows every new image from your AI image tool full screen the moment it is saved, lets you pick, skip, rate and re-roll from the keyboard, and compares a frame with its versions side by side, by slider, by flicker and by difference.$\r$\n$\r$\nIt installs for you only, with no administrator rights, and never changes or moves your images.$\r$\n$\r$\nIf BlinkLoupe (or Loupe) is running, setup closes it first."
!define MUI_COMPONENTSPAGE_NODESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start BlinkLoupe"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Relaunch

; Closes a running BlinkLoupe or Loupe (their windows' own close buttons never ask), then waits for the program file.
!macro CloseApp
  nsExec::Exec 'taskkill /IM blinkloupe.exe'
  Pop $0
  nsExec::Exec 'taskkill /IM loupe.exe'
  Pop $0
  StrCpy $1 0
  ${Do}
    ClearErrors
    Delete "$INSTDIR\${EXE}"
    ${IfNot} ${FileExists} "$INSTDIR\${EXE}"
      ${Break}
    ${EndIf}
    IntOp $1 $1 + 1
    ${If} $1 = 20
      nsExec::Exec 'taskkill /F /IM blinkloupe.exe'
      Pop $0
    ${EndIf}
    ${If} $1 > 40
      ${Break}
    ${EndIf}
    Sleep 250
  ${Loop}
!macroend

Function .onInit
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/R" $1
  ${IfNot} ${Errors}
    StrCpy $Relaunch "1"
  ${EndIf}
  ${If} ${Silent}
    ; an update never adds a Desktop shortcut the person removed
    SectionSetFlags 1 0
    Return
  ${EndIf}
  ; BlinkLoupe draws its windows with Microsoft's WebView2, built into Windows 11 and most up-to-date Windows 10
  ReadRegStr $0 HKLM "${WEBVIEW2}" "pv"
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    ReadRegStr $0 HKCU "${WEBVIEW2}" "pv"
  ${EndIf}
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    MessageBox MB_YESNO|MB_ICONINFORMATION "BlinkLoupe needs Microsoft's WebView2 Runtime, which this computer doesn't seem to have.$\r$\n$\r$\nOpen Microsoft's download page now? Install the Evergreen Runtime from there; BlinkLoupe will start once it is in place." /SD IDNO IDNO +2
      ExecShell "open" "https://developer.microsoft.com/microsoft-edge/webview2/#download"
  ${EndIf}
FunctionEnd

Section "BlinkLoupe" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  !insertmacro CloseApp
  File "/oname=${EXE}" "..\src-tauri\target\release\blinkloupe.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\BlinkLoupe.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\${EXE}" 0
  CreateShortcut "$SMPROGRAMS\Blink (BlinkLoupe).lnk" "$INSTDIR\${EXE}" "--blink" "$INSTDIR\${EXE}" 0 SW_SHOWNORMAL "" "Compare images with BlinkLoupe's Blink"

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "https://github.com/mifunetron/blinkloupe-releases"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  ; the old Loupe install (0.9.0), if any: its silent uninstaller keeps its settings
  ReadRegStr $0 HKCU "${OLD_KEY}" "InstallLocation"
  ${If} $0 != ""
  ${AndIf} ${FileExists} "$0\uninstall.exe"
    ExecWait '"$0\uninstall.exe" /S _?=$0'
    Delete "$0\uninstall.exe"
    RMDir "$0"
    DeleteRegKey HKCU "${OLD_KEY}"
  ${EndIf}
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\BlinkLoupe.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\${EXE}" 0
SectionEnd

Function .onInstSuccess
  ${If} $Relaunch == "1"
    Exec '"$INSTDIR\${EXE}"'
  ${EndIf}
FunctionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM blinkloupe.exe'
  Pop $0
  Sleep 800
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\BlinkLoupe.lnk"
  Delete "$SMPROGRAMS\Blink (BlinkLoupe).lnk"
  Delete "$DESKTOP\BlinkLoupe.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  ; settings and the log stay unless the person says otherwise; marks live beside the images and are never touched
  MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "Also remove BlinkLoupe's settings and log?$\r$\n$\r$\nYour picks, ratings and other marks stay beside your images either way." /SD IDNO IDNO keep
    RMDir /r "$APPDATA\BlinkLoupe"
    RMDir /r "$LOCALAPPDATA\BlinkLoupe"
    RMDir /r "$LOCALAPPDATA\dev.mifunetron.blinkloupe"
  keep:
SectionEnd
