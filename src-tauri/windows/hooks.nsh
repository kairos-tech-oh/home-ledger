; Home Ledger's additions to Tauri's NSIS installer:
;
;  - the app's folder, where hl.exe is installed beside it, goes on the
;    user's PATH, and comes off it again on uninstall;
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
; NSIS strings stop at NSIS_MAX_STRLEN characters. A PATH that long may have
; been cut short when read, and writing it back would lose the rest of it, so
; a PATH at the limit is left alone and the installer says so instead.

; In: $R0 the PATH. Out: $R9 = 1 when $INSTDIR is one of its entries.
!macro HlPathHas
  StrCpy $R1 ";$R0;"
  StrCpy $R2 ";$INSTDIR;"
  StrLen $R3 $R2
  StrLen $R4 $R1
  StrCpy $R9 0
  StrCpy $R5 0
  ${DoWhile} $R5 <= $R4
    StrCpy $R6 $R1 $R3 $R5
    ${If} $R6 == $R2
      StrCpy $R9 1
      ${Break}
    ${EndIf}
    IntOp $R5 $R5 + 1
  ${Loop}
!macroend

; In: $R0 the PATH. Out: $R0 without $INSTDIR, other entries in order.
!macro HlPathWithout
  StrCpy $R1 "$R0;"
  StrCpy $R7 ""
  StrCpy $R8 ""
  StrLen $R4 $R1
  StrCpy $R5 0
  ${DoWhile} $R5 < $R4
    StrCpy $R6 $R1 1 $R5
    ${If} $R6 == ";"
      ${If} $R8 != ""
      ${AndIf} $R8 != $INSTDIR
        ${If} $R7 == ""
          StrCpy $R7 $R8
        ${Else}
          StrCpy $R7 "$R7;$R8"
        ${EndIf}
      ${EndIf}
      StrCpy $R8 ""
    ${Else}
      StrCpy $R8 "$R8$R6"
    ${EndIf}
    IntOp $R5 $R5 + 1
  ${Loop}
  StrCpy $R0 $R7
!macroend

!macro HlAddToPath
  ReadRegStr $R0 HKCU "Environment" "Path"
  StrLen $R4 $R0
  IntOp $R3 ${NSIS_MAX_STRLEN} - 2
  ${If} $R4 >= $R3
    DetailPrint "Your PATH is too long to change safely; add $INSTDIR to it yourself to use hl."
  ${Else}
    !insertmacro HlPathHas
    ${If} $R9 == 0
      ${If} $R0 == ""
        StrCpy $R0 "$INSTDIR"
      ${Else}
        StrCpy $R0 "$R0;$INSTDIR"
      ${EndIf}
      WriteRegExpandStr HKCU "Environment" "Path" $R0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}
  ${EndIf}
!macroend

!macro HlRemoveFromPath
  ReadRegStr $R0 HKCU "Environment" "Path"
  StrLen $R4 $R0
  IntOp $R3 ${NSIS_MAX_STRLEN} - 2
  ${If} $R4 < $R3
    !insertmacro HlPathHas
    ${If} $R9 == 1
      !insertmacro HlPathWithout
      WriteRegExpandStr HKCU "Environment" "Path" $R0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}
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
  !insertmacro HlAddToPath
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An update runs the old uninstaller first; only a real uninstall cleans up.
  ${If} $UpdateMode <> 1
    Delete "$INSTDIR\ledger.cmd"
    !insertmacro HlRemoveFromPath
    DeleteRegKey HKCU "${HL_KEY}"
    DeleteRegKey /ifempty HKCU "Software\home-ledger"
  ${EndIf}
!macroend
