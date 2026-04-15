!include "LogicLib.nsh"

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Stopping TieZ before uninstall..."

  # 应用关闭主窗口时会缩到托盘，卸载器不能依赖普通的关闭请求。
  # 这里在开始删除文件前强制结束已安装的进程，避免 exe 被占用。
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM TieZ.exe'
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM tiez-app.exe'
  Sleep 1200

  # 先清理常见的自启动注册表项，避免系统在卸载后继续拉起已删除的程序。
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "TieZ"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tie-z"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tiez-app"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "Restoring Windows Clipboard settings..."
  
  # 1. Restore EnableClipboardHistory and EnableCloudClipboard to default (1)
  # This ensures Win+V works again even if the app was used to disable it.
  WriteRegDWORD HKCU "Software\Microsoft\Clipboard" "EnableClipboardHistory" 1
  WriteRegDWORD HKCU "Software\Microsoft\Clipboard" "EnableCloudClipboard" 1
  
  # 2. Remove 'V' from DisabledHotkeys
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" "DisabledHotkeys"
  ${If} $0 != ""
    # Simple primitive string removal for 'V' and 'v'
    Push "V" # String to replace
    Push ""  # Replace with
    Push $0  # Original string
    Call un.StrReplace
    Pop $0
    
    Push "v" # String to replace
    Push ""  # Replace with
    Push $0  # Original string
    Call un.StrReplace
    Pop $0
    
    ${If} $0 == ""
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" "DisabledHotkeys"
    ${Else}
      WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" "DisabledHotkeys" $0
    ${EndIf}
  ${EndIf}

  # 3. Clean up Policy if it exists
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Policies\Explorer" "DisallowClipboardHistory"
  DeleteRegValue HKCU "Software\Policies\Microsoft\Windows\System" "AllowClipboardHistory"
  DeleteRegValue HKCU "Software\Policies\Microsoft\Windows\System" "AllowCrossDeviceClipboard"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "TieZ"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tie-z"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "tiez-app"
  # 清理 NSIS 记录的上次安装路径，避免后续静默安装继续复用旧目录。
  DeleteRegKey HKCU "Software\tiez\TieZ"
  DeleteRegKey /ifempty HKCU "Software\tiez"
  
  DetailPrint "Windows Clipboard settings restored."
  
  # 4. Restart Explorer to make DisabledHotkeys changes take effect
  # We use a silent powershell command to be as non-intrusive as possible
  DetailPrint "Restarting Explorer to apply changes..."
  nsExec::Exec '"powershell.exe" -NoProfile -WindowStyle Hidden -Command "Stop-Process -Name explorer -Force; Start-Process explorer"'
  DetailPrint "Explorer restarted."

  # 5. 如果卸载时程序刚被关闭，或者 uninstall.exe 还没完全退出，
  # NSIS 可能暂时删不掉安装目录。这里额外拉起一个后台清理助手，
  # 等卸载器退出后再尝试删除残留目录和文件。
  DetailPrint "Scheduling leftover install directory cleanup..."
  nsExec::Exec '"$SYSDIR\cmd.exe" /C start "" /MIN powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command "Start-Sleep -Seconds 3; Stop-Process -Name TieZ,tiez-app -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath ''$INSTDIR'' -Force -Recurse -ErrorAction SilentlyContinue"'
!macroend

# Function for string replacement (Uninstall version)
Function un.StrReplace
  Exch $0 # Original string (input/output)
  Exch
  Exch $1 # Replace with
  Exch
  Exch 2
  Exch $2 # String to replace
  Exch 2
  Push $3 # Length of string to replace
  Push $4 # Current original string length
  Push $5 # Length of replacement string
  Push $6 # Current index
  Push $7 # Current substring
  
  StrLen $3 $2
  ${If} $3 == 0
    Goto StrReplace_End
  ${EndIf}
  
  StrLen $4 $0
  StrLen $5 $1
  StrCpy $6 0
  
  StrReplace_Loop:
    StrCpy $7 $0 $3 $6
    ${If} $7 == $2
      # Found a match
      StrCpy $7 $0 $6 # Text before match
      IntOp $6 $6 + $3
      StrCpy $0 $0 "" $6 # Text after match
      StrCpy $0 $7$1$0 # New string
      StrLen $4 $0 # New length
      IntOp $6 $7 + $5 # Move index past replacement
    ${Else}
      IntOp $6 $6 + 1
    ${EndIf}
    
    ${If} $6 < $4
      Goto StrReplace_Loop
    ${EndIf}
    
  StrReplace_End:
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Exch $0
FunctionEnd
