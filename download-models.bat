@echo off
:: ─── download-models.bat ──────────────────────────────────────────────────────
:: Download the Bonsai 8B GGUF model for Impulse Instruct.
::
:: Model: prism-ml/Bonsai-8B-gguf (Apache 2.0 license)
:: File:  Bonsai-8B.gguf  (~1.1 GB, 1-bit Q1_0_g128 — requires PrismML llama-server)
::
:: NOTE: A free HuggingFace account is required.
::   Sign up at https://huggingface.co/join
::   Then log in: huggingface-cli login   (paste your access token)
::
:: Usage:
::   download-models.bat
:: ──────────────────────────────────────────────────────────────────────────────
setlocal EnableDelayedExpansion
cd /d "%~dp0"

set HF_REPO=prism-ml/Bonsai-8B-gguf
set MODEL_FILE=Bonsai-8B.gguf
set MODEL_DIR=models
set OUTPUT_PATH=%MODEL_DIR%\%MODEL_FILE%
set HF_URL=https://huggingface.co/%HF_REPO%/resolve/main/%MODEL_FILE%

if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

:: ── Already downloaded? ────────────────────────────────────────────────────────
if exist "%OUTPUT_PATH%" (
    echo.
    echo  Model already present: %OUTPUT_PATH%
    echo  Delete it to re-download.
    echo.
    goto :done
)

echo.
echo  ============================================================
echo   Impulse Instruct -- Model Downloader
echo  ============================================================
echo   Repo:  %HF_REPO%
echo   File:  %MODEL_FILE%
echo   Size:  ~1.1 GB  (1-bit quantization, PrismML format)
echo  ============================================================
echo.

:: ── Step 1: Try huggingface-cli ───────────────────────────────────────────────
where huggingface-cli >nul 2>&1
if %errorlevel%==0 goto :use_hf_cli

:: ── Step 2: Try pip to get huggingface-cli ────────────────────────────────────
where pip >nul 2>&1
if %errorlevel%==0 (
    echo  huggingface-cli not found -- installing via pip...
    pip install -q huggingface_hub
    where huggingface-cli >nul 2>&1
    if %errorlevel%==0 goto :use_hf_cli
)

:: ── Step 3: Try winget Python install ─────────────────────────────────────────
where winget >nul 2>&1
if %errorlevel%==0 (
    echo  Python not found -- installing via winget...
    winget install -e --id Python.Python.3.12 --silent --accept-package-agreements --accept-source-agreements
    for /f "tokens=*" %%i in ('where python 2^>nul') do set PYTHON_BIN=%%i
    if defined PYTHON_BIN (
        python -m pip install -q huggingface_hub
        where huggingface-cli >nul 2>&1
        if %errorlevel%==0 goto :use_hf_cli
    )
)

:: ── Step 4: Fall back to curl ─────────────────────────────────────────────────
where curl >nul 2>&1
if %errorlevel%==0 (
    echo  Using curl...
    curl -L --progress-bar -C - -o "%OUTPUT_PATH%" "%HF_URL%"
    goto :check_file
)

echo.
echo  ERROR: No download tool found.
echo  Install Python from https://www.python.org/downloads/ then re-run.
echo.
exit /b 1

:use_hf_cli
huggingface-cli whoami >nul 2>&1
if %errorlevel% neq 0 (
    echo.
    echo  ============================================================
    echo   HuggingFace login required
    echo  ============================================================
    echo   Sign up (free): https://huggingface.co/join
    echo   Get a token:    https://huggingface.co/settings/tokens
    echo  ============================================================
    echo.
    huggingface-cli login
    if %errorlevel% neq 0 (
        echo  Login failed. Re-run after logging in.
        exit /b 1
    )
)
echo  Downloading via huggingface-cli (resumable)...
huggingface-cli download %HF_REPO% %MODEL_FILE% --local-dir %MODEL_DIR% --local-dir-use-symlinks False

:check_file
if not exist "%OUTPUT_PATH%" (
    echo.
    echo  ERROR: Download failed -- file not found at %OUTPUT_PATH%
    exit /b 1
)

:done
echo.
echo  ============================================================
echo   Model ready: %OUTPUT_PATH%
echo  ============================================================
echo.
echo   NOTE: This model requires the PrismML llama-server binary.
echo   Build it on Linux and cross-compile, or run on Linux/WSL.
echo.
echo  License: Bonsai 8B by prism-ml -- Apache License 2.0
echo  https://huggingface.co/%HF_REPO%
echo.

endlocal
