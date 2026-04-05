@echo off
rem ─── scripts\download-models.bat ─────────────────────────────────────────────
rem Download GGUF models for Impulse Instruct.
rem
rem Usage:
rem   scripts\download-models.bat              Gemma 4 E4B (default, ~4.6 GB, best overall)
rem   scripts\download-models.bat bonsai       Bonsai-8B (1-bit Q1, ~1.1 GB, fallback)
rem   scripts\download-models.bat qwen3        Qwen3-8B Q4_K_M (~5 GB, optional)
rem   scripts\download-models.bat qwen3-14b    Qwen3-14B Q4_K_M (~9 GB, optional)
rem
rem NOTE: A free HuggingFace account is required.
rem   Sign up at https://huggingface.co/join
rem   Then log in: huggingface-cli login
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

set MODEL_DIR=models
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

set MODEL=%~1
if "%MODEL%"=="" set MODEL=gemma4

rem ── Model selection ───────────────────────────────────────────────────────────
if "%MODEL%"=="gemma4" (
    set HF_REPO=unsloth/gemma-4-E4B-it-GGUF
    set MODEL_FILE=gemma-4-E4B-it-Q4_K_M.gguf
    set MODEL_DESC=Gemma 4 E4B Q4_K_M (unsloth, ~4.6 GB) -- default, best accuracy + speed
    goto download
)
if "%MODEL%"=="bonsai" (
    set HF_REPO=prism-ml/Bonsai-8B-gguf
    set MODEL_FILE=Bonsai-8B.gguf
    set MODEL_DESC=Bonsai-8B Q1_0_g128 (PrismML, ~1.1 GB) -- tiny fallback, no chain-of-thought
    goto download
)
if "%MODEL%"=="qwen3" (
    set HF_REPO=bartowski/Qwen_Qwen3-8B-GGUF
    set MODEL_FILE=Qwen_Qwen3-8B-Q4_K_M.gguf
    set MODEL_DESC=Qwen3-8B Q4_K_M (bartowski, ~5 GB) -- optional, supports /think chain-of-thought
    goto download
)
if "%MODEL%"=="qwen3-14b" (
    set HF_REPO=bartowski/Qwen_Qwen3-14B-GGUF
    set MODEL_FILE=Qwen_Qwen3-14B-Q4_K_M.gguf
    set MODEL_DESC=Qwen3-14B Q4_K_M (bartowski, ~9 GB) -- optional large, needs 12 GB VRAM
    goto download
)

echo Unknown model: '%MODEL%'
echo Available: gemma4 (default), bonsai, qwen3, qwen3-14b
exit /b 1

:download
set OUTPUT_PATH=%MODEL_DIR%\%MODEL_FILE%

echo Model: %MODEL_DESC%
echo.

if exist "%OUTPUT_PATH%" (
    echo [OK] Model already present: %OUTPUT_PATH%
    echo   Delete it to re-download.
    exit /b 0
)

echo Downloading %HF_REPO% -^> %OUTPUT_PATH%
echo.

rem ── Check for huggingface-cli ─────────────────────────────────────────────────
where hf >nul 2>&1
if not errorlevel 1 (
    set HF_CMD=hf
    goto hf_download
)
where huggingface-cli >nul 2>&1
if not errorlevel 1 (
    set HF_CMD=huggingface-cli
    goto hf_download
)

rem Try pip install
where pip >nul 2>&1
if not errorlevel 1 (
    echo hf not found -- installing huggingface_hub...
    pip install -q huggingface_hub
    where hf >nul 2>&1
    if not errorlevel 1 (
        set HF_CMD=hf
        goto hf_download
    )
    where huggingface-cli >nul 2>&1
    if not errorlevel 1 (
        set HF_CMD=huggingface-cli
        goto hf_download
    )
)

rem Last resort: curl
where curl >nul 2>&1
if not errorlevel 1 (
    echo Falling back to direct download via curl...
    echo   https://huggingface.co/%HF_REPO%/resolve/main/%MODEL_FILE%
    echo.
    curl -L -o "%OUTPUT_PATH%" "https://huggingface.co/%HF_REPO%/resolve/main/%MODEL_FILE%"
    goto check_file
)

echo ERROR: No download tool found. Install huggingface_hub (pip install huggingface_hub) or curl.
exit /b 1

:hf_download
%HF_CMD% download "%HF_REPO%" "%MODEL_FILE%" --local-dir "%MODEL_DIR%"
if errorlevel 1 (
    echo.
    echo Login may be required. Run:  %HF_CMD% login
    exit /b 1
)

:check_file
if exist "%OUTPUT_PATH%" (
    echo.
    echo [OK] Model ready: %OUTPUT_PATH%
    echo.
    echo Run with: start.bat
) else (
    echo ERROR: Download failed. File not found at %OUTPUT_PATH%
    exit /b 1
)
