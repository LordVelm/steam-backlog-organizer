!macro NSIS_HOOK_POSTINSTALL
  ; No custom post-install actions needed
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  MessageBox MB_YESNO "Delete all Gamekeeper data including the AI model (~3.5 GB)?$\n$\nThis removes cached library data, settings, and the downloaded AI model from:$\n$APPDATA\Gamekeeper" IDYES delete_data IDNO skip_delete
  delete_data:
    RMDir /r "$APPDATA\Gamekeeper"
  skip_delete:
!macroend
