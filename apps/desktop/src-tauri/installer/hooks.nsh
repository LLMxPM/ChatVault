; 拾文 NSIS 钩子：在文件替换前检查后台归档并清理本安装目录的计划任务，保留用户资料库。
!define CHATVAULT_HOOK_DIR "${__FILEDIR__}"

; 输入安装目录，通过脚本参数传递，避免将用户路径拼接为 PowerShell 代码。
!macro ChatVaultPrepare
  InitPluginsDir
  File /oname=$PLUGINSDIR\chatvault-prepare.ps1 "${CHATVAULT_HOOK_DIR}\prepare.ps1"
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\chatvault-prepare.ps1" -InstallDirectory "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    MessageBox MB_OK|MB_ICONEXCLAMATION "无法准备安装或卸载。请等待归档任务结束，并检查计划任务权限后重试。$\r$\n$1" /SD IDOK
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro ChatVaultPrepare
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 清理选项涉及待上传附件，明确确认后才允许 Tauri 清理本地应用数据。
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONEXCLAMATION "清理应用数据将删除本地索引和待上传附件副本，无法撤销。微信原文件和远端归档不受影响。是否仍要清理？" /SD IDNO IDYES +2
    StrCpy $DeleteAppDataCheckboxState 0
  ${EndIf}
  !insertmacro ChatVaultPrepare
!macroend
