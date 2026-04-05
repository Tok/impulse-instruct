@echo off
rem ─── scripts\build-bonsai-server.bat ─────────────────────────────────────────
rem Clone and build PrismML's llama.cpp fork for Bonsai-8B (Q1_0_g128).
rem
rem Output: .llama-build\bin\llama-server.exe
rem
rem Requires: git cmake Visual Studio Build Tools (or MSVC)
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

set SRC_DIR=.llama-src-prismml
set BUILD_DIR=.llama-build
set TARGET=%BUILD_DIR%\bin\llama-server.exe

if exist "%TARGET%" (
    echo [OK] llama-server already built: %TARGET%
    exit /b 0
)

echo ==========================================
echo   Building PrismML llama-server...
echo ==========================================
echo (one-time build -- takes ~3 min)
echo.

if not exist "%SRC_DIR%" (
    git clone --depth 1 https://github.com/PrismML-Eng/llama.cpp "%SRC_DIR%"
    if errorlevel 1 exit /b 1
) else (
    echo Source already cloned, skipping clone.
)

cmake -B "%BUILD_DIR%" -S "%SRC_DIR%" ^
  -DCMAKE_BUILD_TYPE=Release ^
  -DGGML_CUDA=ON ^
  -DLLAMA_CURL=OFF ^
  -DBUILD_SHARED_LIBS=OFF ^
  -DLLAMA_BUILD_TESTS=OFF ^
  -DLLAMA_BUILD_EXAMPLES=OFF
if errorlevel 1 exit /b 1

cmake --build "%BUILD_DIR%" --target llama-server --config Release -j
if errorlevel 1 exit /b 1

echo.
echo [OK] Done: %TARGET%
echo.
echo Next: scripts\download-models.bat
echo Then: start.bat
