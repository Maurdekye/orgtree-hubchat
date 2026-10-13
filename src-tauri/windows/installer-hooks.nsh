; Hubchat NSIS installer hooks (tauri.conf.json: bundle.windows.nsis.installerHooks).
; Included by src-tauri/windows/installer.nsi before anything else, so this
; directory is known here: ${__FILEDIR__} is the directory of THIS file even
; though makensis runs the template from Tauri's own output folder.
!define HUBCHAT_WINDOWS_DIR "${__FILEDIR__}"

; Runs inside the Install section, before the stock "is the app running" check.
; An explicit Upgrade closed Hubchat on the Upgrade page; ask again right before
; files are replaced, so a Hubchat started in between (autostart, a link) is
; closed through the same graceful path rather than hitting the stock
; terminate-the-process prompt.
!macro NSIS_HOOK_PREINSTALL
  ${If} $HubUpgradeSelected == "1"
    Call HubUpgradeClose
    Pop $0
    ${If} $0 != "1"
      SetErrorLevel 1
      Quit
    ${EndIf}
  ${EndIf}
!macroend

; Marks this installation as made by a Hubchat that understands
; `--quit-for-upgrade` (src-tauri/src/lib.rs), so the next installer's Upgrade
; asks it to quit instead of treating it as an older Hubchat with no such request.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "${UNINSTKEY}" "HubchatQuitControl" "1"
!macroend
