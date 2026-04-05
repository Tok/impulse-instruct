@echo off
setlocal enabledelayedexpansion
rem ─── scripts\run-llm-tests.bat ───────────────────────────────────────────────
rem Run the LLM integration suite against one or all models.
rem
rem Default model set: Gemma 4 E4B + Bonsai 8B (officially evaluated).
rem Qwen and Llama variants skipped by default. Use --all-models to test everything in models\.
rem They may work; the system prompt just isn't tuned for them.
rem
rem Server binary selection:
rem   Bonsai / *bonsai* -> .llama-build\bin\llama-server.exe          (PrismML fork)
rem   All other models  -> .llama-official-build\bin\llama-server.exe  (official)
rem   fallback          -> llama-server from PATH
rem
rem Usage:
rem   scripts\run-llm-tests.bat                                # Gemma + Bonsai, all suites
rem   scripts\run-llm-tests.bat --all-models                   # all *.gguf in models\
rem   scripts\run-llm-tests.bat models\Bonsai-8B.gguf          # single model
rem   scripts\run-llm-tests.bat models\Bonsai-8B.gguf acid     # single model + filter
rem   scripts\run-llm-style.bat                                # style suite only
rem   scripts\run-llm-theory.bat                               # theory suite only
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

if "%LLM_SUITE%"=="" set LLM_SUITE=llm_suite

set MODEL_ARG=
set FILTER=
set ALL_MODELS=0

rem Parse args
for %%a in (%*) do (
    if "%%a"=="--all-models" (
        set ALL_MODELS=1
    ) else if "%%a"=="--verbose" (
        set LLM_SUITE_VERBOSE=1
        echo -^> Verbose mode: full JSON output (no truncation)
    ) else if "!MODEL_ARG!"=="" (
        set MODEL_ARG=%%a
    ) else (
        set FILTER=%%a
    )
)

echo ==========================================
echo   LLM integration suite  [suite: %LLM_SUITE%]
echo ==========================================

rem ── Compile once up front ─────────────────────────────────────────────────────
echo -^> Compiling test binary...
cargo build --features llm-tests --tests -q
if errorlevel 1 exit /b 1
echo.

set OVERALL_EXIT=0

rem ── Per-model loop ─────────────────────────────────────────────────────────────
if not "%MODEL_ARG%"=="" (
    rem Single explicit model path
    if not exist "%MODEL_ARG%" (
        echo ERROR: model file not found: %MODEL_ARG%
        exit /b 1
    )
    call :run_model "%MODEL_ARG%"
) else (
    rem Scan models\ — filter by default unless --all-models
    set MODELS_RAN=0
    for %%f in (models\*.gguf) do (
        set FNAME=%%~nxf
        set SKIP=0

        rem Skip non-default models unless --all-models
        if "!ALL_MODELS!"=="0" (
            echo !FNAME! | findstr /i "qwen" >nul
            if not errorlevel 1 set SKIP=1
            echo !FNAME! | findstr /i "llama" >nul
            if not errorlevel 1 set SKIP=1
        )

        if "!SKIP!"=="0" (
            set MODELS_RAN=1
            call :run_model "%%f"
        )
    )
    if "!MODELS_RAN!"=="0" (
        echo ERROR: no default models found (gemma/bonsai). Use --all-models to include Qwen and others.
        exit /b 1
    )
)

exit /b %OVERALL_EXIT%

rem ──────────────────────────────────────────────────────────────────────────────
:run_model
set MODEL_PATH=%~1
set MODEL_NAME=%~n1%~x1

rem Pick server binary
set SERVER_BIN=
echo %MODEL_NAME% | findstr /i "bonsai" >nul
if not errorlevel 1 (
    set SERVER_BIN=.llama-build\bin\llama-server.exe
) else if exist ".llama-official-build\bin\llama-server.exe" (
    set SERVER_BIN=.llama-official-build\bin\llama-server.exe
) else (
    where llama-server >nul 2>&1
    if not errorlevel 1 set SERVER_BIN=llama-server
)

echo ==========================================
echo   Model: %MODEL_NAME%
echo ==========================================

if "%SERVER_BIN%"=="" (
    echo ERROR: no llama-server binary found
    echo   For Bonsai models:  scripts\build-bonsai-server.bat
    echo   For other models:   scripts\build-llama-server.bat
    set OVERALL_EXIT=1
    exit /b 0
)
if not exist "%SERVER_BIN%" (
    if not "%SERVER_BIN%"=="llama-server" (
        echo ERROR: %SERVER_BIN% not found
        set OVERALL_EXIT=1
        exit /b 0
    )
)

rem Kill any stale server
tasklist /fi "imagename eq llama-server.exe" 2>nul | find /i "llama-server.exe" >nul
if not errorlevel 1 (
    echo -^> Killing stale llama-server...
    taskkill /f /im llama-server.exe >nul 2>&1
    timeout /t 1 /nobreak >nul
)

rem Pick a free port via PowerShell
for /f %%p in ('powershell -noprofile -command "$l=New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0);$l.Start();$p=$l.LocalEndpoint.Port;$l.Stop();$p"') do set PORT=%%p
set LLAMA_SERVER_URL=http://127.0.0.1:%PORT%

echo -^> Starting server on port %PORT%...
start /b "" "%SERVER_BIN%" --model "%MODEL_PATH%" --host 127.0.0.1 --port %PORT% --ctx-size 16384 --n-gpu-layers 99

rem Wait for server ready (up to 120s)
echo -^> Waiting for server to be ready...
set READY=0
for /l %%i in (1,1,120) do (
    if "!READY!"=="0" (
        curl -sf "%LLAMA_SERVER_URL%/health" 2>nul | findstr /c:"\"ok\"" >nul 2>&1
        if not errorlevel 1 (
            echo    ready
            set READY=1
        ) else (
            timeout /t 1 /nobreak >nul
        )
    )
)

if "!READY!"=="0" (
    echo ERROR: server did not become ready within 120s
    taskkill /f /im llama-server.exe >nul 2>&1
    set OVERALL_EXIT=1
    exit /b 0
)

rem Run test suite
echo -^> Running %LLM_SUITE% tests...
echo.
if "%FILTER%"=="" (
    cargo test --features llm-tests -- %LLM_SUITE% --nocapture --test-threads 1
) else (
    cargo test --features llm-tests -- %LLM_SUITE% %FILTER% --nocapture --test-threads 1
)
if errorlevel 1 set OVERALL_EXIT=1

rem Stop server
echo.
echo -^> Stopping server...
taskkill /f /im llama-server.exe >nul 2>&1
echo.
exit /b 0
