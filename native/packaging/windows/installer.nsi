; Standalone NSIS installer for the Blockfield Native (Slint) launcher.
; Reuses the Tauri installer's branding assets, localization and ownership-aware
; uninstall cleanup contract without depending on Tauri's bundler plugins.
Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !error "VERSION must be defined (e.g. -DVERSION=1.0.31)"
!endif
!ifndef VERSION_QUAD
  !error "VERSION_QUAD must be defined (e.g. -DVERSION_QUAD=1.0.31.0)"
!endif
!ifndef BINARY_PATH
  !error "BINARY_PATH must be defined"
!endif
!ifndef ICON_PATH
  !error "ICON_PATH must be defined"
!endif
!ifndef HEADER_BMP
  !error "HEADER_BMP must be defined"
!endif
!ifndef SIDEBAR_BMP
  !error "SIDEBAR_BMP must be defined"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE must be defined"
!endif
!ifndef DISCORD_APP_ID
  !define DISCORD_APP_ID "1548300877125779456"
!endif

!define MANUFACTURER "Blockfield"
!define PRODUCTNAME "blockfield-native"
!define DISPLAYNAME "Blockfield Launcher"
!define MAINBINARYNAME "blockfield-native"
!define BUNDLEID "com.blockfield.launcher"
!define HOMEPAGE "https://blockfield.pro"
!define COPYRIGHT "Blockfield"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUPRODUCTKEY "Software\${MANUFACTURER}\${PRODUCTNAME}"

; Dark theme matching src-tauri/windows/hooks.nsh.
!define MUI_BGCOLOR "070604"
!define MUI_TEXTCOLOR "F3E7D0"
!define MUI_INSTFILESPAGE_COLORS "F3E7D0 11100D"
!define MUI_FORCECLASSICCONTROLS

!define MUI_ICON "${ICON_PATH}"
!define MUI_UNICON "${ICON_PATH}"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "${HEADER_BMP}"
!define MUI_HEADERIMAGE_UNBITMAP "${HEADER_BMP}"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBAR_BMP}"

Var PassiveMode
Var UpdateMode

Name "${DISPLAYNAME}"
BrandingText "${COPYRIGHT}"
OutFile "${OUT_FILE}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\${PRODUCTNAME}"
InstallDirRegKey HKCU "${MANUPRODUCTKEY}" ""

VIProductVersion "${VERSION_QUAD}"
VIAddVersionKey "ProductName" "${DISPLAYNAME}"
VIAddVersionKey "FileDescription" "${DISPLAYNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

; Installer pages
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME

!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_DIRECTORY

!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_NOAUTOCLOSE
!define MUI_FINISHPAGE_RUN "$INSTDIR\${MAINBINARYNAME}.exe"
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(CreateDesktopShortcutText)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateDesktopShortcut
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!define MUI_UNCONFIRMPAGE_TEXT_TOP "$(BlockfieldCleanupHint)"
!define MUI_PAGE_CUSTOMFUNCTION_PRE un.SkipIfPassive
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "Ukrainian"

LangString CreateDesktopShortcutText ${LANG_ENGLISH} "Create Desktop shortcut"
LangString CreateDesktopShortcutText ${LANG_RUSSIAN} "Создать ярлык на рабочем столе"
LangString CreateDesktopShortcutText ${LANG_UKRAINIAN} "Створити ярлик на робочому столі"

LangString BlockfieldCleanupHint ${LANG_ENGLISH} "Blockfield Launcher and unchanged automatically downloaded game files will be removed. Manually added or modified files, worlds and screenshots are kept."
LangString BlockfieldCleanupHint ${LANG_RUSSIAN} "Будут удалены Blockfield Launcher и неизменённые автоматически загруженные файлы игры. Добавленные или изменённые вручную файлы, миры и скриншоты сохранятся."
LangString BlockfieldCleanupHint ${LANG_UKRAINIAN} "Буде видалено Blockfield Launcher та незмінені автоматично завантажені файли гри. Додані або змінені вручну файли, світи й знімки екрана збережуться."

LangString BlockfieldCleanupFailed ${LANG_ENGLISH} "Safe cleanup could not finish. No unverified files were removed. Close the game and try again; if needed, reinstall the launcher before uninstalling."
LangString BlockfieldCleanupFailed ${LANG_RUSSIAN} "Безопасную очистку не удалось завершить. Файлы без подтверждённого происхождения не удалены. Закройте игру и повторите; при необходимости переустановите лаунчер перед удалением."
LangString BlockfieldCleanupFailed ${LANG_UKRAINIAN} "Не вдалося завершити безпечне очищення. Файли без підтвердженого походження не видалено. Закрийте гру й повторіть; за потреби перевстановіть лаунчер перед видаленням."

Function SkipIfPassive
  ${If} $PassiveMode = 1
    Abort
  ${EndIf}
FunctionEnd

Function un.SkipIfPassive
  ${If} $PassiveMode = 1
    Abort
  ${EndIf}
FunctionEnd

Function CreateDesktopShortcut
  CreateShortcut "$DESKTOP\${DISPLAYNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
FunctionEnd

Function .onInit
  ClearErrors
  ${GetOptions} $CMDLINE "/P" $R0
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $R0
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
FunctionEnd

Function un.onInit
  ClearErrors
  ${GetOptions} $CMDLINE "/P" $R0
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $R0
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
FunctionEnd

!macro RegisterScheme SCHEME
  WriteRegStr HKCU "Software\Classes\${SCHEME}" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\${SCHEME}" "" "URL:${BUNDLEID} protocol"
  WriteRegStr HKCU "Software\Classes\${SCHEME}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr HKCU "Software\Classes\${SCHEME}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro UnregisterScheme SCHEME
  ReadRegStr $R7 HKCU "Software\Classes\${SCHEME}\shell\open\command" ""
  ${If} $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
    DeleteRegKey HKCU "Software\Classes\${SCHEME}"
  ${EndIf}
!macroend

Section Install
  SetOutPath "$INSTDIR"
  File "/oname=${MAINBINARYNAME}.exe" "${BINARY_PATH}"

  !insertmacro RegisterScheme "blockfield"
  !insertmacro RegisterScheme "discord-${DISCORD_APP_ID}"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "" "$INSTDIR"

  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${DISPLAYNAME}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  WriteRegStr HKCU "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
  WriteRegStr HKCU "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
  WriteRegStr HKCU "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" "$0"

  CreateDirectory "$SMPROGRAMS\${DISPLAYNAME}"
  CreateShortcut "$SMPROGRAMS\${DISPLAYNAME}\${DISPLAYNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"

  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    Call CreateDesktopShortcut
    SetAutoClose true
  ${EndIf}
SectionEnd

Section Uninstall
  ; Skip destructive cleanup during upgrades (exact parity with src-tauri/windows/hooks.nsh).
  ${If} $UpdateMode <> 1
    ClearErrors
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup' $0
    ${If} ${Errors}
    ${OrIf} $0 <> 0
      MessageBox MB_OK|MB_ICONSTOP "$(BlockfieldCleanupFailed)" /SD IDOK
      Abort
    ${EndIf}
  ${EndIf}

  !insertmacro UnregisterScheme "blockfield"
  !insertmacro UnregisterScheme "discord-${DISCORD_APP_ID}"

  Delete "$INSTDIR\${MAINBINARYNAME}.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  ${If} $UpdateMode <> 1
    Delete "$DESKTOP\${DISPLAYNAME}.lnk"
    Delete "$SMPROGRAMS\${DISPLAYNAME}\${DISPLAYNAME}.lnk"
    RMDir "$SMPROGRAMS\${DISPLAYNAME}"
  ${EndIf}

  DeleteRegKey HKCU "${UNINSTKEY}"
  DeleteRegKey HKCU "${MANUPRODUCTKEY}"
SectionEnd
