; Home Ledger's additions to Tauri's NSIS installer:
;
;  - the app's folder, where hl.exe is installed beside it, goes on the
;    user's PATH, and comes off it again on uninstall, both done by hl itself;
;  - a checkbox on the Welcome page offers `ledger` as a second name for hl,
;    remembered so an in-app update (which shows no pages) keeps the choice.
;
; Included by Tauri's template before any page is declared, so everything
; that reads the template's own variables ($UpdateMode) is a macro, expanded
; later inside its sections, rather than a function defined here.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

; The same key `hl alias` reads and writes, so the two always agree.
!define HL_KEY "Software\home-ledger\hl"

Var HlAliasBox
Var HlAliasChoice

; ------------------------------------------------------------ Welcome page

!define MUI_PAGE_CUSTOMFUNCTION_SHOW HlWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE HlWelcomeLeave

Function HlWelcomeShow
  ReadRegStr $HlAliasChoice HKCU "${HL_KEY}" "ledgerAlias"
  ${If} $HlAliasChoice == ""
    StrCpy $HlAliasChoice 1
  ${EndIf}
  ${NSD_CreateCheckbox} 120u 175u 195u 12u "Also add the $\"ledger$\" command (another name for hl)"
  Pop $HlAliasBox
  SetCtlColors $HlAliasBox "" "${MUI_BGCOLOR}"
  ${If} $HlAliasChoice == 1
    ${NSD_Check} $HlAliasBox
  ${EndIf}
FunctionEnd

Function HlWelcomeLeave
  ${NSD_GetState} $HlAliasBox $HlAliasChoice
FunctionEnd

; ------------------------------------------------------------------- PATH
;
; Never read or written here. NSIS strings stop at NSIS_MAX_STRLEN, and past
; it ReadRegStr gives back an empty string rather than a cut-short one; an
; earlier version of this script took that for an empty PATH and wrote the
; app's folder over everything on it. hl reads and writes the value whole
; through the registry API, keeps its type, refuses any change that is not
; exactly one entry added or removed, and copies the old value to
; HKCU\Software\home-ledger\path-backup first, which an uninstall leaves in
; place. Its output goes to the log, so nothing flashes on screen.

!macro HlPath ACTION
  nsExec::ExecToLog '"$INSTDIR\hl.exe" install-path ${ACTION} "$INSTDIR"'
  Pop $R0
  ${If} $R0 == 0
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${Else}
    DetailPrint "PATH was left as it was; add or remove $INSTDIR yourself to change where hl is found."
  ${EndIf}
!macroend

; ------------------------------------------------------------------ hooks

!macro NSIS_HOOK_POSTINSTALL
  ; No pages in an update or a silent install: keep what was chosen before,
  ; and add nothing new without being asked.
  ${If} $HlAliasChoice == ""
    ReadRegStr $HlAliasChoice HKCU "${HL_KEY}" "ledgerAlias"
  ${EndIf}
  ${If} $HlAliasChoice == 1
    FileOpen $R0 "$INSTDIR\ledger.cmd" w
    FileWrite $R0 '@"%~dp0hl.exe" %*$\r$\n'
    FileClose $R0
    WriteRegStr HKCU "${HL_KEY}" "ledgerAlias" "1"
  ${Else}
    Delete "$INSTDIR\ledger.cmd"
    ${If} $HlAliasChoice == 0
      WriteRegStr HKCU "${HL_KEY}" "ledgerAlias" "0"
    ${EndIf}
  ${EndIf}
  !insertmacro HlPath add
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An update runs the old uninstaller first; only a real uninstall cleans up.
  ${If} $UpdateMode <> 1
    Delete "$INSTDIR\ledger.cmd"
    !insertmacro HlPath remove
    DeleteRegKey HKCU "${HL_KEY}"
    DeleteRegKey /ifempty HKCU "Software\home-ledger"
  ${EndIf}
!macroend
