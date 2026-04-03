@echo off
:: ─── download-models.bat ──────────────────────────────────────────────────────
:: Download the Bonsai 8B GGUF model for Impulse Instruct.
::
:: Model: prism-ml/Bonsai-8B-gguf (Apache 2.0 license)
:: Quantizations:
::   Q4_K_M  ~5.0 GB  recommended (default)
::   Q3_K_M  ~4.0 GB  smaller, slightly lower quality
::   Q5_K_M  ~6.0 GB  higher quality
::   Q2_K    ~2.8 GB  smallest
::   Q8_0    ~8.6 GB  near-lossless
::
:: NOTE: A free HuggingFace account is required.
::   Sign up at https://huggingface.co/join
::   Then log in: huggingface-cli login   (paste your access token)
::
:: Usage:
::   download-models.bat              download Q4_K_M (recommended)
::   download-models.bat Q3_K_M       specific quantization
::   download-models.bat --list       show all options
:: ──────────────────────────────────────────────────────────────────────────────
setlocal EnableDelayedExpansion
cd /d "%~dp0"

set HF_REPO=prism-ml/Bonsai-8B-gguf
set MODEL_DIR=models
set QUANT=%~1
if "%QUANT%"=="" set QUANT=Q4_K_M

:: ── --list ────────────────────────────────────────────────────────────────────
if "%QUANT%"=="--list" (
    echo.
    echo  Available quantizations:
    echo.
    echo    Q2_K    ~2.8 GB  ^(smallest, fastest^)
    echo    Q3_K_M  ~4.0 GB
    echo    Q4_K_M  ~5.0 GB  ^(recommended^)
    echo    Q5_K_M  ~6.0 GB
    echo    Q8_0    ~8.6 GB  ^(near-lossless, slowest^)
    echo.
    echo  Usage: download-models.bat ^<QUANT^>
    echo  Example: download-models.bat Q3_K_M
    echo.
    exit /b 0
)

set MODEL_FILE=Bonsai-8B-%QUANT%.gguf
set OUTPUT_PATH=%MODEL_DIR%\%MODEL_FILE%
set DEFAULT_PATH=%MODEL_DIR%\bonsai-8b-q4.gguf
set HF_URL=https://huggingface.co/%HF_REPO%/resolve/main/%MODEL_FILE%

if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

:: ── Already downloaded? ────────────────────────────────────────────────────────
if exist "%OUTPUT_PATH%" (
    echo.
    echo  Model already present: %OUTPUT_PATH%
    echo  Delete it to re-download.
    echo.
    goto :make_default
)

echo.
echo  ============================================================
echo   Impulse Instruct -- Model Downloader
echo  ============================================================
echo   Repo:  %HF_REPO%
echo   File:  %MODEL_FILE%
echo   Size:  ~5 GB  ^(download may take a while^)
echo  ============================================================
echo.

:: ── Step 1: Try huggingface-cli ───────────────────────────────────────────────
where huggingface-cli >/dev/null 2>&1
if %errorlevel%==0 goto :use_hf_cli

:: ── Step 2: Try pip to get huggingface-cli ────────────────────────────────────
where pip >/dev/null 2>&1
if %errorlevel%==0 (
    echo  huggingface-cli not found -- installing via pip...
    pip install -q huggingface_hub
    where huggingface-cli >/dev/null 2>&1
    if %errorlevel%==0 goto :use_hf_cli
)

:: pip not found either -- try winget Python install
where winget >/dev/null 2>&1
if %errorlevel%==0 (
    echo  Python/pip not found -- attempting to install Python via winget...
    echo  ^(You may see a UAC prompt^)
    winget install -e --id Python.Python.3.12 --silent --accept-package-agreements --accept-source-agreements
    :: Refresh PATH for this session
    for /f "tokens=*" %%i in ('where python 2^>nul') do set PYTHON_BIN=%%i
    if defined PYTHON_BIN (
        echo  Python installed. Installing huggingface_hub...
        python -m pip install -q huggingface_hub
        where huggingface-cli >/dev/null 2>&1
        if %errorlevel%==0 goto :use_hf_cli
    )
)

:: ── Step 3: Fall back to curl (built into Windows 10 1803+) ───────────────────
where curl >/dev/null 2>&1
if %errorlevel%==0 (
    echo  Using curl ^(no resuming -- if it fails, re-run and it will retry^)...
    echo  URL: %HF_URL%
    echo.
    curl -L --progress-bar -C - -o "%OUTPUT_PATH%" "%HF_URL%"
    goto :check_file
)

:: ── Nothing worked -- show manual instructions ────────────────────────────────
echo.
echo  ============================================================
echo   Could not find a download tool.
echo   Please follow ONE of these steps, then re-run this script:
echo  ============================================================
echo.
echo   OPTION A -- Install Python (easiest, recommended):
echo     1. Open https://www.python.org/downloads/
echo     2. Download and run the installer
echo        ^(check "Add Python to PATH" during install^)
echo     3. Open a new Command Prompt and run this script again.
echo        It will auto-install huggingface-cli and download the model.
echo.
echo   OPTION B -- Direct browser download:
echo     1. Open this URL in your browser:
echo        %HF_URL%
echo     2. Save the file as:
echo        %OUTPUT_PATH%
echo        ^(in the same folder as this script^)
echo.
echo   OPTION C -- Windows Package Manager ^(winget^):
echo     1. Open PowerShell as Administrator
echo     2. Run: winget install Python.Python.3.12
echo     3. Re-open a normal Command Prompt and run this script again.
echo.
exit /b 1

:use_hf_cli
:: Check if already logged in; if not, prompt to log in
huggingface-cli whoami >nul 2>&1
if %errorlevel% neq 0 (
    echo.
    echo  ============================================================
    echo   HuggingFace login required
    echo  ============================================================
    echo   A free account is needed to download this model.
    echo.
    echo   1. Sign up ^(free^): https://huggingface.co/join
    echo   2. Get your access token: https://huggingface.co/settings/tokens
    echo      ^(Create a token with "Read" permissions^)
    echo   3. Paste your token below when prompted.
    echo  ============================================================
    echo.
    huggingface-cli login
    if %errorlevel% neq 0 (
        echo.
        echo  Login failed or cancelled. Re-run this script after logging in.
        exit /b 1
    )
)
echo  Using huggingface-cli ^(resumable download^)...
echo.
huggingface-cli download %HF_REPO% --include "*%QUANT%*.gguf" --local-dir %MODEL_DIR% --local-dir-use-symlinks False
:: Rename if CLI used a different filename
for %%F in ("%MODEL_DIR%\*%QUANT%*.gguf") do (
    if /i not "%%~nxF"=="%MODEL_FILE%" (
        move /y "%%F" "%OUTPUT_PATH%" >/dev/null 2>&1
    )
)

:check_file
if not exist "%OUTPUT_PATH%" (
    echo.
    echo  ERROR: Download failed -- file not found at %OUTPUT_PATH%
    echo.
    echo  Possible reasons:
    echo    - Network interrupted ^(re-run to resume with huggingface-cli^)
    echo    - Wrong quantization name ^(run: download-models.bat --list^)
    echo    - HuggingFace CDN issue ^(try again in a few minutes^)
    echo.
    exit /b 1
)

:make_default
:: Copy as the default filename the app looks for (no symlinks needed)
if not "%OUTPUT_PATH%"=="%DEFAULT_PATH%" (
    if not exist "%DEFAULT_PATH%" (
        echo  Copying as default model name...
        copy /y "%OUTPUT_PATH%" "%DEFAULT_PATH%" >/dev/null
    )
)

echo.
echo  ============================================================
echo   Model ready!
echo  ============================================================
echo   File: %OUTPUT_PATH%
echo.
echo   Launch Impulse Instruct with LLM inference:
echo     impulse-instruct.exe --llm
echo.
echo   Or from source:
echo     cargo run --features llm --release
echo  ============================================================
echo.
echo  License: Bonsai 8B by prism-ml -- Apache License 2.0
echo  https://huggingface.co/%HF_REPO%
echo.

endlocal
