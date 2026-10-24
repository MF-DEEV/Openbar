; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (C) 2026 Vorssaint
;
; Hooks into Tauri's NSIS installer. Uninstalling closes the island and
; takes out what the app added for this user: its Start with Windows entry
; and its hooks in Claude Code's settings (with a backup, as Settings does),
; so Claude never calls a program that's gone. An update keeps both.

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /IM "Vorssaint.exe" /F'
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\Vorssaint.exe" --remove-hooks'
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Vorssaint"
  ${EndIf}
!macroend
