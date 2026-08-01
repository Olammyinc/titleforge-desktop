; NSIS Hook: Copy the bundled VC++ runtime DLLs from the resources folder to
; sit next to TitleForge.exe ($INSTDIR). llama-cpp-2 links against MSVCP140.dll
; and VCOMP140.DLL; the Windows loader searches the EXE's own directory first,
; so app-local DLLs resolve with no admin, no UAC, no redist installer.
;
; The DLLs are shipped as bundle resources (tauri.conf bundle.resources: "vcrt/")
; which land at $INSTDIR\vcrt\. This hook copies them beside the exe.
;
; PATH NOTE (verified against the generated installer.nsi, 2026-08-01):
; Tauri v2 does NOT use a "resources" subfolder the way v1 did. It maps each
; declared resource by its own path:
;   "vcrt/"              -> $INSTDIR\vcrt\
;   "../seed-data.json"  -> $INSTDIR\_up_\seed-data.json
;   "../models/x.gguf"   -> $INSTDIR\_up_\models\x.gguf
; Anything reached via ../ goes under _up_\. There is no $INSTDIR\resources\.
; An earlier version of this hook checked $INSTDIR\resources\vcrt\ — that path
; never exists, so the guard always failed, the Else branch fired, and no DLLs
; were copied. Clean Windows then failed with MSVCP140.dll / VCOMP140.DLL.
;
; Runs POSTINSTALL (after files are copied), which is the correct time — the
; resources exist by then. Uses only NSIS core (no registry, no elevation).
;
; File: src-tauri/windows/hooks.nsh
; Referenced: tauri.conf.json → bundle.windows.nsis.installerHooks

!macro NSIS_HOOK_POSTINSTALL
  ; The 4 runtime DLLs llama-cpp-2 (and msvcp140's own deps) need beside the exe.
  Push $0
  ; Use a per-line copy so a missing file doesn't abort the whole install.
  ; $INSTDIR is where TitleForge.exe was installed (currentUser mode).
  ${If} ${FileExists} "$INSTDIR\vcrt\msvcp140.dll"
    CopyFiles /SILENT "$INSTDIR\vcrt\msvcp140.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\vcrt\vcruntime140.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\vcrt\vcruntime140_1.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\vcrt\vcomp140.dll" "$INSTDIR\"
    DetailPrint "VC++ runtime DLLs placed next to TitleForge.exe"
  ${Else}
    ; Loud, not silent. A missing runtime means the app cannot start at all on a
    ; clean Windows box, so this must be visible without opening "Show details".
    MessageBox MB_ICONEXCLAMATION|MB_OK "TitleForge: VC++ runtime files were not \
found at $INSTDIR\vcrt.$\r$\n$\r$\nThe app may fail to start with an \
MSVCP140.dll or VCOMP140.DLL error. Please report this — the installer is \
built incorrectly."
    DetailPrint "ERROR: vcrt not found at $INSTDIR\vcrt — MSVCP140.dll / VCOMP140.DLL will be missing"
  ${EndIf}
  Pop $0
!macroend
