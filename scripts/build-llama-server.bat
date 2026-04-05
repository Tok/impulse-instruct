@echo off
rem ─── scripts\build-llama-server.bat ──────────────────────────────────────────
rem Build the official upstream llama.cpp server for standard GGUF models.
rem
rem Use this for: Qwen3, Qwen3-14B, Gemma 4, and any standard GGUF model.
rem
rem Output: .llama-official-build\bin\llama-server.exe
rem
rem Requires: git cmake Visual Studio Build Tools (or MSVC) cuda-toolkit
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

set SRC_DIR=.llama-src-official
set BUILD_DIR=.llama-official-build
set TARGET=%BUILD_DIR%\bin\llama-server.exe

if exist "%TARGET%" (
    echo [OK] official llama-server already built: %TARGET%
    exit /b 0
)

echo ==========================================
echo   Building official llama.cpp server...
echo ==========================================
echo (one-time build -- takes ~3 min)
echo.

if not exist "%SRC_DIR%" (
    git clone --depth 1 https://github.com/ggml-org/llama.cpp "%SRC_DIR%"
    if errorlevel 1 exit /b 1
) else (
    echo Source already cloned.
)

cmake -B "%BUILD_DIR%" -S "%SRC_DIR%" ^
  -DCMAKE_BUILD_TYPE=Release ^
  -DGGML_CUDA=ON ^
  -DLLAMA_CURL=OFF ^
  -DLLAMA_BUILD_TESTS=OFF ^
  -DLLAMA_BUILD_EXAMPLES=OFF ^
  -DLLAMA_BUILD_SERVER=ON
if errorlevel 1 exit /b 1

cmake --build "%BUILD_DIR%" --target llama-server --config Release -j
if errorlevel 1 exit /b 1

echo.
echo [OK] Built: %TARGET%
echo.
echo This server handles: Qwen3, Qwen3-14B, Gemma 4, and all standard GGUF models.
echo The PrismML server (.llama-build\) is still required for Bonsai 8B.
echo.
echo Impulse Instruct picks the right binary automatically based on model name.
