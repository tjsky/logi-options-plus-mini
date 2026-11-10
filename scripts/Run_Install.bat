@echo off

cd /d "%~dp0"


echo Starting Logi Options+ Installer...


PowerShell.exe -NoProfile -ExecutionPolicy Bypass -File "logi-options-plus-mini.ps1"


pause