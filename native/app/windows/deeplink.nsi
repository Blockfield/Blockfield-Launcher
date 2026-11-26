; Native launcher deep-link registration (Tauri installer.nsi parity).
; Tauri registers its `deep_link_protocols` (blockfield:// + the Discord
; invite scheme) under HKCU/HKLM at install time; the native NSIS installer
; includes this fragment so the same OS entries exist for native builds.
; Runtime `register()` in src/deep_link.rs repeats the HKCU entries for
; portable runs; the installer owns the machine-wide state.

!macro RegisterBlockfieldDeepLinks MainBinary
  WriteRegStr SHCTX "Software\Classes\blockfield" "URL Protocol" ""
  WriteRegStr SHCTX "Software\Classes\blockfield" "" "URL:blockfield protocol"
  WriteRegStr SHCTX "Software\Classes\blockfield\DefaultIcon" "" "$\"$INSTDIR\${MainBinary}$\",0"
  WriteRegStr SHCTX "Software\Classes\blockfield\shell\open\command" "" "$\"$INSTDIR\${MainBinary}$\" $\"%1$\""
  WriteRegStr SHCTX "Software\Classes\discord-1548300877125779456" "URL Protocol" ""
  WriteRegStr SHCTX "Software\Classes\discord-1548300877125779456" "" "URL:discord-1548300877125779456 protocol"
  WriteRegStr SHCTX "Software\Classes\discord-1548300877125779456\DefaultIcon" "" "$\"$INSTDIR\${MainBinary}$\",0"
  WriteRegStr SHCTX "Software\Classes\discord-1548300877125779456\shell\open\command" "" "$\"$INSTDIR\${MainBinary}$\" $\"%1$\""
!macroend

!macro UnregisterBlockfieldDeepLinks
  DeleteRegKey SHCTX "Software\Classes\blockfield"
  DeleteRegKey SHCTX "Software\Classes\discord-1548300877125779456"
!macroend
