@echo off
rem Habilita SSH numa VM ja instalada: de dois cliques em Z:\tools\win11-vm\oem\enable-ssh.bat
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0enable-ssh.ps1"
pause
