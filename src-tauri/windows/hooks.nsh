!define MUI_BGCOLOR "070604"
!define MUI_TEXTCOLOR "F3E7D0"
!define MUI_INSTFILESPAGE_COLORS "F3E7D0 11100D"

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ClearErrors
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup' $0
    ${If} ${Errors}
    ${OrIf} $0 <> 0
      MessageBox MB_OK|MB_ICONSTOP "$(BlockfieldCleanupFailed)" /SD IDOK
      Abort
    ${EndIf}
  ${EndIf}
!macroend
