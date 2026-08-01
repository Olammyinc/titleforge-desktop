; NSIS Hook: Copy the bundled VC++ runtime DLLs from the resources folder to
; sit next to TitleForge.exe ($INSTDIR). llama-cpp-2 links against MSVCP140.dll
; and VCOMP140.DLL; the Windows loader searches the EXE's own directory first,
; so app-local DLLs resolve with no admin, no UAC, no redist installer.
;
; The DLLs are shipped as bundle resources (tauri.conf bundle.resources: "vcrt/")
; which land at $INSTDIR\resources\vcrt\. This hook moves them beside the exe.
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
  ${If} ${FileExists} "$INSTDIR\resources\vcrt\msvcp140.dll"
    CopyFiles /SILENT "$INSTDIR\resources\vcrt\msvcp140.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\resources\vcrt\vcruntime140.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\resources\vcrt\vcruntime140_1.dll" "$INSTDIR\"
    CopyFiles /SILENT "$INSTDIR\resources\vcrt\vcomp140.dll" "$INSTDIR\"
    DetailPrint "VC++ runtime DLLs placed next to TitleForge.exe"
  ${Else}
    DetailPrint "NOTE: vcrt resources not found — clean Windows may lack MSVCP140.dll / VCOMP140.DLL"
  ${EndIf}
  Pop $0
!macroend
