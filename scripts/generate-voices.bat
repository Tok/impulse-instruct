@echo off
REM ─── generate-voices.bat ───────────────────────────────────────────────────
REM Generate NeuTTS reference voice clips from espeak-ng (Windows version).
REM Run once after setup — creates voices\*.wav + *.txt pairs.
REM ────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0\.."

if not exist "voices" mkdir voices

where espeak-ng >nul 2>&1 || (
    echo ERROR: espeak-ng not found. Install: choco install espeak-ng
    exit /b 1
)

set "FORCE=0"
if "%~1"=="--force" set "FORCE=1"

REM ── Default voice ──────────────────────────────────────────────────────────
if exist "voices\default.wav" if "%FORCE%"=="0" (
    echo   default: exists (use --force to regenerate)
    goto :mc
)
espeak-ng -v en -p 50 -s 140 -w voices\default.wav "Welcome to Impulse Instruct. This is an AI powered music production studio with modular synthesis, drum machines, and intelligent agents that create music together in real time."
echo Welcome to Impulse Instruct. This is an AI powered music production studio with modular synthesis, drum machines, and intelligent agents that create music together in real time.> voices\default.txt
echo   default: generated

:mc
if exist "voices\mc.wav" if "%FORCE%"=="0" (
    echo   mc: exists
    goto :dj
)
espeak-ng -v en+m3 -p 65 -s 170 -w voices\mc.wav "Selector! Big up the junglist massive! We have the bass, we have the breaks, we have the riddim. Pull up and rewind! This one goes out to all the ravers in the building tonight."
echo Selector! Big up the junglist massive! We have the bass, we have the breaks, we have the riddim. Pull up and rewind! This one goes out to all the ravers in the building tonight.> voices\mc.txt
echo   mc: generated

:dj
if exist "voices\dj.wav" if "%FORCE%"=="0" (
    echo   dj: exists
    goto :robot
)
espeak-ng -v en+m4 -p 40 -s 130 -w voices\dj.wav "Good evening ladies and gentlemen. Welcome to the show. Tonight we have a very special set lined up for you. Sit back, relax, and let the music take you on a journey through sound."
echo Good evening ladies and gentlemen. Welcome to the show. Tonight we have a very special set lined up for you. Sit back, relax, and let the music take you on a journey through sound.> voices\dj.txt
echo   dj: generated

:robot
if exist "voices\robot.wav" if "%FORCE%"=="0" (
    echo   robot: exists
    goto :done
)
espeak-ng -v en+m7 -p 30 -s 120 -w voices\robot.wav "System initialised. Audio synthesis module online. All parameters nominal. Beginning music generation sequence. Standby for output."
echo System initialised. Audio synthesis module online. All parameters nominal. Beginning music generation sequence. Standby for output.> voices\robot.txt
echo   robot: generated

:done
echo.
echo Voice references in voices\
echo Replace any .wav with your own recording and update the .txt transcript.
