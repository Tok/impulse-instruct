@echo off
rem ─── scripts\build-all.bat ───────────────────────────────────────────────────
rem Build release binaries for Linux (WSL) and Windows.
rem
rem Windows build: target\x86_64-pc-windows-msvc\release\impulse-instruct.exe
rem
rem Prerequisites:
rem   rustup update stable
rem   cargo install cargo-xwin
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

set DIST=dist
if not exist "%DIST%" mkdir "%DIST%"

set FEATURE_FLAGS=
if not "%FEATURES%"=="" set FEATURE_FLAGS=--features %FEATURES%

rem ── Windows build ─────────────────────────────────────────────────────────────
echo ==========================================
echo   Building Windows (x86_64-pc-windows-msvc)...
echo ==========================================

set WIN_TARGET=x86_64-pc-windows-msvc
rustup target add %WIN_TARGET% 2>nul

cargo xwin build --release --target %WIN_TARGET% %FEATURE_FLAGS%
if errorlevel 1 (
    echo ERROR: Windows build failed
    exit /b 1
)

copy /y "target\%WIN_TARGET%\release\impulse-instruct.exe" "%DIST%\impulse-instruct-windows-x86_64.exe"
echo [OK] Windows: %DIST%\impulse-instruct-windows-x86_64.exe

echo.
echo ==========================================
echo   Build complete. Output in .\%DIST%\
dir /b "%DIST%\"
echo ==========================================
