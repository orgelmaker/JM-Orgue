$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut("$env:USERPROFILE\Desktop\JM-Orgue.lnk")
$Shortcut.TargetPath = "C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan\target\release\vpo-app.exe"
$Shortcut.WorkingDirectory = "C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan"
$Shortcut.Description = "JM-Orgue Virtual Pipe Organ"
$Shortcut.Save()
Write-Host "Snelkoppeling aangemaakt op bureaublad!"
