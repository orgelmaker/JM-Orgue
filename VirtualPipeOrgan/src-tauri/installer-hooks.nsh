; JM-Orgue NSIS installer hooks.
;
; Na installatie: als er GEEN ASIO-audiostuurprogramma op de pc staat, bied dan
; (optioneel) aan om het meegebundelde ASIO4ALL te installeren — voor lage latency.
; JM-Orgue werkt ook zonder ASIO (via WASAPI), dus dit is puur een keuze.
;
; De ASIO4ALL-setup wordt via de NSIS File-directive in deze installer ingebed en
; bij installatie naar de tijdelijke map uitgepakt; er blijft niets extra's achter.

!macro NSIS_HOOK_POSTINSTALL
  ; Detecteer een bestaand ASIO-stuurprogramma (64-bit registerweergave).
  SetRegView 64
  ClearErrors
  EnumRegKey $0 HKLM "SOFTWARE\ASIO" 0
  SetRegView 32

  ; $0 leeg => geen enkel ASIO-stuurprogramma geregistreerd.
  StrCmp $0 "" 0 asio4all_done
    MessageBox MB_YESNO|MB_ICONQUESTION "Er is geen ASIO-audiostuurprogramma gevonden.$\r$\n$\r$\nWil je ASIO4ALL installeren voor lagere latency?$\r$\n(Aanbevolen als je geen audio-interface met eigen ASIO-driver hebt. JM-Orgue werkt ook zonder, via WASAPI.)" /SD IDNO IDNO asio4all_done
      InitPluginsDir
      File "/oname=$PLUGINSDIR\asio4all_setup.exe" "C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\src-tauri\installers\asio4all_setup.exe"
      ExecWait '"$PLUGINSDIR\asio4all_setup.exe"'
  asio4all_done:
!macroend
