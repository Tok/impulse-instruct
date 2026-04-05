@echo off
REM Impulse Instruct launcher
REM Keeps the console window open after the program exits so you can read
REM any error messages (missing model, no audio device, etc.).
cd /d "%~dp0"
cmd /k "impulse-instruct-windows-x86_64.exe"
