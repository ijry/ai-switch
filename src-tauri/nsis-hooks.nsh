; Installer hooks for AI Switch.
;
; The Tailscale sidecar (ai-switch-tsnet.exe) is installed next to the main
; executable. Windows locks a running exe exclusively, so if a sidecar from an
; earlier session is still alive the file copy below fails and the whole update
; fails with it.
;
; Builds from before the stdin watchdog landed leak that process, and those are
; exactly the installs that need this update — the fix cannot install itself
; while the bug it fixes is holding the lock. Clearing it here is what breaks
; that deadlock.
;
; taskkill matches on image name, so a sidecar belonging to another copy of the
; app (a dev build, say) is ended too. That is accepted: this runs only during
; an install, and the app respawns its sidecar on demand.

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping AI Switch secure network component..."
  nsExec::Exec 'taskkill /F /IM ai-switch-tsnet.exe'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Stopping AI Switch secure network component..."
  nsExec::Exec 'taskkill /F /IM ai-switch-tsnet.exe'
  Pop $0
!macroend
