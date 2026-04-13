@echo off
setlocal enabledelayedexpansion
rem ─── download-models.bat ─────────────────────────────────────────────────────
rem Download GGUF models for Impulse Instruct.
rem
rem Usage:
rem   download-models.bat                  Gemma 4 E4B [default, ~4.6 GB, best overall]
rem   download-models.bat bonsai           Bonsai-8B [1-bit Q1, ~1.1 GB, lightweight]
rem   download-models.bat neutts           NeuTTS Air Q4 [~527 MB, TTS voice cloning]
rem   download-models.bat deepseek-r1-7b   DeepSeek-R1-Distill-Qwen-7B [~5 GB, CoT]
rem   download-models.bat deepseek-r1-14b  DeepSeek-R1-Distill-Qwen-14B [~9 GB, CoT]
rem   download-models.bat qwen3            Qwen3-8B Q4_K_M [~5 GB, optional]
rem   download-models.bat qwen3-14b        Qwen3-14B Q4_K_M [~9 GB, optional]
rem
rem NOTE: Most models require a free HuggingFace account.
rem   Sign up at https://huggingface.co/join
rem   Then log in: hf auth login  (or: huggingface-cli login)
rem
rem PREFER MANUAL DOWNLOAD? Non-technical Windows users can just browse
rem the HuggingFace URL printed below and drop the .gguf into the models\
rem folder next to impulse-instruct-windows-x86_64.exe.
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0"
rem If we're inside a scripts\ subdir (dev checkout), step up to repo root.
if exist "..\Cargo.toml" cd ..

set MODEL_DIR=models
if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

set MODEL=%~1
if "%MODEL%"=="" set MODEL=gemma4

rem ── Model selection ───────────────────────────────────────────────────────────
rem NOTE: cmd parses parentheses in set-values even inside if (...) blocks,
rem so avoid "(" and ")" in descriptions. Use brackets instead.
if /i "%MODEL%"=="gemma4" (
    set HF_REPO=unsloth/gemma-4-E4B-it-GGUF
    set MODEL_FILE=gemma-4-E4B-it-Q4_K_M.gguf
    set "MODEL_DESC=Gemma 4 E4B Q4_K_M [unsloth, ~4.6 GB] -- default, required for first run"
    goto download
)
if /i "%MODEL%"=="bonsai" (
    set HF_REPO=prism-ml/Bonsai-8B-gguf
    set MODEL_FILE=Bonsai-8B.gguf
    set "MODEL_DESC=Bonsai-8B Q1_0_g128 [PrismML, ~1.1 GB] -- optional, multi-agent specialists"
    goto download
)
if /i "%MODEL%"=="neutts" (
    set HF_REPO=neuphonic/neutts-air-q4-gguf
    set MODEL_FILE=neutts-air-q4.gguf
    set HF_FILE=neutts-air-Q4_0.gguf
    set "MODEL_DESC=NeuTTS Air Q4 [~527 MB] -- optional, TTS/MC voice cloning"
    goto download
)
if /i "%MODEL%"=="qwen3" (
    set HF_REPO=bartowski/Qwen_Qwen3-8B-GGUF
    set MODEL_FILE=Qwen_Qwen3-8B-Q4_K_M.gguf
    set "MODEL_DESC=Qwen3-8B Q4_K_M [bartowski, ~5 GB] -- optional, chain-of-thought"
    goto download
)
if /i "%MODEL%"=="qwen3-14b" (
    set HF_REPO=bartowski/Qwen_Qwen3-14B-GGUF
    set MODEL_FILE=Qwen_Qwen3-14B-Q4_K_M.gguf
    set "MODEL_DESC=Qwen3-14B Q4_K_M [bartowski, ~9 GB] -- optional large, 12 GB VRAM"
    goto download
)
if /i "%MODEL%"=="deepseek-r1-7b" (
    set HF_REPO=bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF
    set MODEL_FILE=DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf
    set "MODEL_DESC=DeepSeek-R1-Distill-Qwen-7B Q4_K_M [~5 GB] -- CoT, Qwen2.5 base"
    goto download
)
if /i "%MODEL%"=="deepseek-r1-14b" (
    set HF_REPO=bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF
    set MODEL_FILE=DeepSeek-R1-Distill-Qwen-14B-Q4_K_M.gguf
    set "MODEL_DESC=DeepSeek-R1-Distill-Qwen-14B Q4_K_M [~9 GB] -- CoT, 12 GB VRAM"
    goto download
)

echo Unknown model: '%MODEL%'
echo Available: gemma4 [default], bonsai, neutts, deepseek-r1-7b, deepseek-r1-14b, qwen3, qwen3-14b
exit /b 1

:download
if "%HF_FILE%"=="" set HF_FILE=%MODEL_FILE%
set OUTPUT_PATH=%MODEL_DIR%\%MODEL_FILE%
set HF_URL=https://huggingface.co/%HF_REPO%/resolve/main/%HF_FILE%

echo.
echo Model:  !MODEL_DESC!
echo Source: https://huggingface.co/%HF_REPO%
echo Target: %OUTPUT_PATH%
echo.

if exist "%OUTPUT_PATH%" (
    echo [OK] Model already present: %OUTPUT_PATH%
    echo      Delete it to re-download.
    exit /b 0
)

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

rem Try pip install if Python is available
where pip >nul 2>&1
if not errorlevel 1 (
    echo hf not found -- trying: pip install huggingface_hub
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

rem Fallback: direct download via curl (Windows 10/11 ships with curl)
where curl >nul 2>&1
if not errorlevel 1 (
    echo Falling back to direct download via curl...
    echo   %HF_URL%
    echo.
    curl -L -o "%OUTPUT_PATH%" "%HF_URL%"
    goto check_file
)

rem ── No tools available -- show manual download instructions ───────────────────
echo.
echo ══════════════════════════════════════════════════════════════════════
echo   MANUAL DOWNLOAD RECOMMENDED
echo ══════════════════════════════════════════════════════════════════════
echo.
echo   No download tool was found [hf, huggingface-cli, pip, curl].
echo   Rather than installing one, just download the file in your browser:
echo.
echo   1. Open this URL:
echo        %HF_URL%
echo.
echo   2. Log in [or sign up, free] at https://huggingface.co/join
echo      if the repo asks for authentication.
echo.
echo   3. Click the download arrow next to "%HF_FILE%".
echo.
echo   4. Move the downloaded file to:
echo        %CD%\%OUTPUT_PATH%
echo.
echo   5. Launch start.bat again.
echo.
echo ══════════════════════════════════════════════════════════════════════
exit /b 1

:hf_download
echo Using %HF_CMD%...
%HF_CMD% download "%HF_REPO%" "%HF_FILE%" --local-dir "%MODEL_DIR%"
if errorlevel 1 (
    echo.
    echo Login may be required. Run:  %HF_CMD% auth login
    echo Or download manually from:   %HF_URL%
    exit /b 1
)
if /i not "%HF_FILE%"=="%MODEL_FILE%" (
    if exist "%MODEL_DIR%\%HF_FILE%" move /y "%MODEL_DIR%\%HF_FILE%" "%OUTPUT_PATH%" >nul
)

:check_file
if exist "%OUTPUT_PATH%" (
    echo.
    echo [OK] Model ready: %OUTPUT_PATH%
    echo.
    echo Run with: start.bat
    echo.
    echo ---- Optional sample packs ------------------------------------------
    echo   Impulse Instruct doesn't bundle audio samples. Grab what you want:
    echo.
    echo   Amen breaks [for the AMEN sampler module]
    echo     https://archive.org/details/amen-breaks
    echo     https://archive.org/details/amen-breaks-compilation
    echo     -^> extract .wav files into  samples\amen\
    echo.
    echo   Voice references [for the NeuTTS MC / TTS module]
    echo     https://archive.org/details/librivoxaudio     [clean, recommended]
    echo     https://commonvoice.mozilla.org/               [CC0 short clips]
    echo     -^> drop a 3-15s mono WAV into  voices\
    echo        alongside a matching  voices\^<name^>.txt  transcript
    echo ---------------------------------------------------------------------
) else (
    echo.
    echo ERROR: Download failed. File not found at %OUTPUT_PATH%
    echo Try manual download from: %HF_URL%
    exit /b 1
)
endlocal
