@echo off
rem ─── start.bat ───────────────────────────────────────────────────────────────
rem Launch Impulse Instruct on Windows.
rem Usage:
rem   start.bat               run with mock LLM, no API
rem   start.bat --api         enable HTTP/MCP API on port 8765
rem   start.bat --api --port 9000
rem   start.bat --model models\my-model.gguf
rem   start.bat --dev         debug build + verbose logging
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0"

set BUILD_MODE=release
set CARGO_FEATURES=
set LOG_LEVEL=info
set EXTRA_ARGS=

:parse_args
if "%~1"=="" goto build
if "%~1"=="--dev" (
    set BUILD_MODE=debug
    set LOG_LEVEL=debug
    shift
    goto parse_args
)
if "%~1"=="--llm" (
    set CARGO_FEATURES=--features llm
    shift
    goto parse_args
)
set EXTRA_ARGS=%EXTRA_ARGS% %1
shift
goto parse_args

:build
if "%BUILD_MODE%"=="release" (
    set CARGO_BUILD_FLAG=--release
) else (
    set CARGO_BUILD_FLAG=
)

echo ^-^> Building (%BUILD_MODE%)...
cargo build %CARGO_BUILD_FLAG% %CARGO_FEATURES%
if errorlevel 1 exit /b 1

echo ^-^> Starting Impulse Instruct...
set RUST_LOG=%LOG_LEVEL%
target\%BUILD_MODE%\impulse-instruct.exe --log %LOG_LEVEL% %EXTRA_ARGS%
