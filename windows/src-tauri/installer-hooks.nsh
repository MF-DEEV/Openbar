; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (C) 2026 Vorssaint
;
; Hooks into Tauri's NSIS installer. Uninstalling closes the island and
; takes out what the app added for this user: its Start with Windows entry,
; its hooks in Claude Code's settings (with a backup, as Settings does),
; so Claude never calls a program that's gone, the IDE bridge extension in
; its editors, and the network helper an administrator may have allowed
; (docs/process-metrics.md). An update keeps them all.
;
; An installer built with tauri.identity.conf.json also carries the signed
; sparse package that gives Openbar.exe its identity (notifications,
; calendar; see docs/notifications.md). Installing trusts its certificate
; once per PC, which needs an administrator (one UAC prompt), then registers
; the package for this user; uninstalling removes the package, an update
; keeps it. None of it can fail the install: the island runs without an
; identity, only without those two sections.

; The native PowerShell: the installer is a 32-bit process, and the Appx
; cmdlets refuse to run in 32-bit PowerShell.
!macro OPENBAR_POWERSHELL
  StrCpy $R9 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${IfNot} ${FileExists} "$R9"
    StrCpy $R9 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
!macroend

!define OPENBAR_IDENTITY '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\identity\identity.ps1"'

!macro NSIS_HOOK_POSTINSTALL
  ; The island used to install as Vorssaint. Bring its settings, hooks and
  ; Start with Windows entry over to Openbar, then remove the old app with its
  ; own uninstaller, so the two never run side by side.
  ${If} ${FileExists} "$LOCALAPPDATA\Vorssaint\uninstall.exe"
    nsExec::Exec 'taskkill /IM "Vorssaint.exe" /F'
    Pop $0
    nsExec::Exec '"$INSTDIR\Openbar.exe" --migrate-legacy'
    Pop $0
    ExecWait '"$LOCALAPPDATA\Vorssaint\uninstall.exe" /S _?=$LOCALAPPDATA\Vorssaint'
    ; Run in place, the old uninstaller leaves itself behind, and Windows
    ; may hold the file for a moment after it exits.
    StrCpy $R8 0
    openbar_remove_old_uninstaller:
    Delete "$LOCALAPPDATA\Vorssaint\uninstall.exe"
    ${If} ${FileExists} "$LOCALAPPDATA\Vorssaint\uninstall.exe"
    ${AndIf} $R8 < 20
      Sleep 250
      IntOp $R8 $R8 + 1
      Goto openbar_remove_old_uninstaller
    ${EndIf}
    RMDir "$LOCALAPPDATA\Vorssaint"
  ${EndIf}
  ${If} ${FileExists} "$INSTDIR\identity\Openbar.msix"
    !insertmacro OPENBAR_POWERSHELL
    nsExec::Exec '"$R9" ${OPENBAR_IDENTITY} -Action Trusted'
    Pop $0
    ${If} $0 != 0
      ; The installer runs per user, so only this step asks for elevation.
      ; Elevation starts the program through the AppInfo service, which can't
      ; see Sysnative, so this one uses System32: a 32-bit PowerShell, which is
      ; enough to add a certificate to the machine store.
      ExecShellWait "runas" "$WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" '${OPENBAR_IDENTITY} -Action Trust' SW_HIDE
      ClearErrors
    ${EndIf}
    ; Registered as the user who installs, whom the package is for.
    nsExec::Exec '"$R9" ${OPENBAR_IDENTITY} -Action Register -Directory "$INSTDIR"'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /IM "Openbar.exe" /F'
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\Openbar.exe" --remove-hooks'
    nsExec::Exec '"$INSTDIR\Openbar.exe" --remove-ide-bridge'
    ; The per-app network helper, if it was approved: removing it takes an
    ; administrator again, so Windows asks only on PCs that have it.
    nsExec::Exec '"$INSTDIR\Openbar.exe" --remove-network-helper'
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Openbar"
    ${If} ${FileExists} "$INSTDIR\identity\identity.ps1"
      !insertmacro OPENBAR_POWERSHELL
      nsExec::Exec '"$R9" ${OPENBAR_IDENTITY} -Action Unregister'
      Pop $0
    ${EndIf}
  ${EndIf}
!macroend
