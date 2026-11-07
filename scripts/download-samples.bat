@echo off
setlocal enabledelayedexpansion
rem ─── download-samples.bat ────────────────────────────────────────────────────
rem Download CC-licensed sample packs for Impulse Instruct.
rem
rem Usage:
rem   download-samples.bat                  show help
rem   download-samples.bat salamander       Salamander Grand Piano [~730 MB, SFZ]
rem   download-samples.bat sso              Sonatina Symphonic Orch [~1.3 GB, SFZ]
rem   download-samples.bat vsco2            VSCO 2 CE [~2.3 GB, SFZ, CC0]
rem   download-samples.bat instruments-all  all three SFZ packs [~4.4 GB total]
rem   download-samples.bat amen             curated source URLs for amen breaks
rem   download-samples.bat textures         curated source URLs for granular textures
rem   download-samples.bat wavetables       curated source URLs for Serum-style wavetables
rem   download-samples.bat impulses         curated source URLs for IRs
rem
rem AUTOMATED PACKS fetch the official GitHub mirrors into
rem samples\instruments\^<pack^>\.  Uses `git clone --depth 1` when git is
rem available; otherwise falls back to the GitHub zipball via curl + tar
rem [both built into Windows 10/11], so end-user binaries without git
rem installed still work.
rem REFERENCE-ONLY PACKS print the curated source URLs from samples\README.md.
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0"
if exist "..\Cargo.toml" cd ..

set INSTR_DIR=samples\instruments
if not exist "%INSTR_DIR%" mkdir "%INSTR_DIR%"
if not exist "samples\amen" mkdir "samples\amen"
if not exist "samples\textures" mkdir "samples\textures"
if not exist "samples\wavetables" mkdir "samples\wavetables"
if not exist "samples\impulses" mkdir "samples\impulses"
if not exist "samples\birds" mkdir "samples\birds"

rem Absolute paths so users know exactly where files belong on disk —
rem the script always cd's to the repo root, so %CD% is canonical.
set REPO_ROOT=%CD%
set ABS_INSTR=%REPO_ROOT%\samples\instruments
set ABS_AMEN=%REPO_ROOT%\samples\amen
set ABS_TEXTURES=%REPO_ROOT%\samples\textures
set ABS_WAVETABLES=%REPO_ROOT%\samples\wavetables
set ABS_IMPULSES=%REPO_ROOT%\samples\impulses

set PACK=%~1

if "%PACK%"==""        goto usage
if /i "%PACK%"=="help" goto usage
if /i "%PACK%"=="-h"   goto usage
if /i "%PACK%"=="--help" goto usage

if /i "%PACK%"=="salamander"      goto pack_salamander
if /i "%PACK%"=="sso"             goto pack_sso
if /i "%PACK%"=="vsco2"           goto pack_vsco2
if /i "%PACK%"=="instruments-all" goto pack_all
if /i "%PACK%"=="amen"            goto print_amen
if /i "%PACK%"=="textures"        goto print_textures
if /i "%PACK%"=="wavetables"      goto print_wavetables
if /i "%PACK%"=="impulses"        goto print_impulses
if /i "%PACK%"=="birds"           goto print_birds

echo Unknown pack: '%PACK%'
echo.
goto usage

:usage
echo Usage: download-samples.bat ^<pack^>
echo.
echo Automated [git clone, drops into samples\instruments\^<pack^>\]:
echo   salamander         Salamander Grand Piano V3       [~730 MB]
echo   sso                Sonatina Symphonic Orchestra    [~1.3 GB]
echo   vsco2              VSCO 2 Community Edition [CC0]  [~2.3 GB]
echo   instruments-all    all three of the above          [~4.4 GB total]
echo.
echo Reference-only [prints curated source URLs]:
echo   amen               Amen breaks ^-^> samples\amen\
echo   textures           Granular textures ^-^> samples\textures\
echo   wavetables         Serum-style wavetables ^-^> samples\wavetables\
echo   impulses           IRs for convolution reverb ^-^> samples\impulses\
echo   birds              CC0 bird-call corpus ^-^> samples\birds\ [granular voice]
echo.
echo See samples\README.md for the long-form pack notes.
exit /b 0

rem ── Pack handlers ────────────────────────────────────────────────────────────
:pack_salamander
call :clone_pack "sfzinstruments/SalamanderGrandPiano" "SalamanderGrandPiano" "~730 MB"
goto license

:pack_sso
call :clone_pack "peastman/sso" "sso" "~1.3 GB"
goto license

:pack_vsco2
call :clone_pack "sgossner/VSCO-2-CE" "VSCO-2-CE" "~2.3 GB"
goto license

:pack_all
call :clone_pack "sfzinstruments/SalamanderGrandPiano" "SalamanderGrandPiano" "~730 MB"
call :clone_pack "peastman/sso" "sso" "~1.3 GB"
call :clone_pack "sgossner/VSCO-2-CE" "VSCO-2-CE" "~2.3 GB"
goto license

:clone_pack
set REPO=%~1
set DEST_NAME=%~2
set SIZE=%~3
set DEST=%INSTR_DIR%\%DEST_NAME%

if exist "%DEST%" (
    echo [OK] %DEST_NAME% already present at %DEST%
    echo      Delete the directory to re-download.
    exit /b 0
)

echo.
echo About to download %DEST_NAME% [%SIZE%] into %INSTR_DIR%\.
set /p REPLY="Continue? [Y/n] "
if "%REPLY%"=="" set REPLY=y
if /i not "%REPLY%"=="y" (
    echo   Skipped.
    exit /b 0
)

echo.
where git >nul 2>&1
if not errorlevel 1 (
    echo Cloning https://github.com/%REPO%.git -^> %DEST%
    git clone --depth 1 "https://github.com/%REPO%.git" "%DEST%"
    if errorlevel 1 (
        echo   git clone failed.
        exit /b 1
    )
    goto clone_done
)

echo git not found -- falling back to GitHub zipball via curl + tar.
where curl >nul 2>&1
if errorlevel 1 (
    echo ERROR: neither 'git' nor 'curl' was found.
    echo Install Git for Windows  https://git-scm.com/download/win
    echo or use Windows 10/11 [curl + tar are built in].
    exit /b 1
)
where tar >nul 2>&1
if errorlevel 1 (
    echo ERROR: 'tar' not found [needed to extract the zip].
    echo Windows 10 1803+ ships tar built-in; on older Windows install Git
    echo for Windows or 7-Zip and extract manually.
    exit /b 1
)

rem GitHub zipball: extracts to %INSTR_DIR%\^<repo_basename^>-master\
rem Strip everything up through the slash to get just the repo name.
set REPO_NAME=%REPO:*/=%
set TMP_ZIP=%DEST%.zip
set ZIP_URL=https://github.com/%REPO%/archive/refs/heads/master.zip

echo Downloading %ZIP_URL%
curl -fL --progress-bar -o "%TMP_ZIP%" "%ZIP_URL%"
if errorlevel 1 (
    echo   download failed.
    if exist "%TMP_ZIP%" del "%TMP_ZIP%"
    exit /b 1
)
echo Extracting -^> %DEST%
tar -xf "%TMP_ZIP%" -C "%INSTR_DIR%"
if errorlevel 1 (
    echo   extract failed.
    if exist "%TMP_ZIP%" del "%TMP_ZIP%"
    exit /b 1
)
del "%TMP_ZIP%"
if exist "%INSTR_DIR%\%REPO_NAME%-master" (
    move /y "%INSTR_DIR%\%REPO_NAME%-master" "%DEST%" >nul
)

:clone_done
echo.
echo [OK] %DEST_NAME% ready
echo      Location: %REPO_ROOT%\%DEST%\
echo      Load via the SAMPLER+ card's LOAD button -- the file dialog can
echo      navigate into the pack folder to pick a .sfz.
exit /b 0

rem ── Reference-only printers ──────────────────────────────────────────────────
:print_amen
echo ---- Amen breaks -----------------------------------------------------------
echo.
echo   PLACE FILES HERE:  %ABS_AMEN%\
echo.
echo The AMEN sampler module reads .wav files from that folder.  Curated sources:
echo.
echo   https://archive.org/details/amen-breaks
echo   https://archive.org/details/amen-breaks-compilation
echo.
echo Workflow:
echo   1. Download a .zip from one of the archive.org pages above.
echo   2. Extract the .wav files into  %ABS_AMEN%\
echo   3. The module's file picker lists them automatically on next launch.
goto license

:print_textures
echo ---- Granular textures -----------------------------------------------------
echo.
echo   PLACE FILES HERE:  %ABS_TEXTURES%\
echo.
echo The GRAN granular texture module reads .wav files from that folder.
echo Longer, slowly-evolving material grains best [pads, drones, field recs].
echo.
echo Curated sources:
echo.
echo   https://archive.org/details/opensource_audio   mixed-bag, check per-item license
echo   https://archive.org/details/audio_ambient      ambient and drone uploads
echo   https://freesound.org                          search: drone / pad / texture / field
echo.
echo Workflow:
echo   1. Pick a CC0 / CC-BY upload from one of the pages above.
echo   2. Drop the .wav into  %ABS_TEXTURES%\
echo   3. The granular voice's picker lists them automatically.
goto license

:print_wavetables
echo ---- Wavetables ------------------------------------------------------------
echo.
echo   PLACE FILES HERE:  %ABS_WAVETABLES%\
echo.
echo The WAVETABLE voice reads Serum-style frame-stack .wav files
echo [2048-sample frames concatenated into one buffer] from that folder.
echo.
echo Curated sources:
echo.
echo   https://wavetables.com                                    large CC0 collection
echo   https://waveedit.online                                   browseable single-cycles
echo   https://www.adventurekid.se/akrt/                         AKWF -- Adventure Kid free
echo.
echo Workflow:
echo   1. Download a Serum-format .wav [any frame count].
echo   2. Drop it into  %ABS_WAVETABLES%\
echo   3. Load via the WAVETABLE card's LOAD button or POST /api/wavetable.
goto license

:print_impulses
echo ---- Impulse responses -----------------------------------------------------
echo.
echo   PLACE FILES HERE:  %ABS_IMPULSES%\
echo.
echo The CONV REV convolution-reverb module reads .wav IRs from that folder.
echo Short IRs [0.5 - 2 s] work best for musical reverb.
echo.
echo Curated sources:
echo.
echo   https://archive.org/details/ir-library    halls, plates, outdoor spaces
echo   https://openairlib.net                    academic IR archive [Univ. of York]
echo   https://www.voxengo.com/impulses/         small Voxengo free pack
echo.
echo Workflow:
echo   1. Download an IR .wav [any sample rate; the loader resamples].
echo   2. Drop it into  %ABS_IMPULSES%\
echo   3. Load via the ConvReverb card's LOAD IR button or POST /api/conv_reverb.
goto license

:print_birds
echo ---- Bird-song corpus ------------------------------------------------------
echo.
echo   PLACE FILES HERE:  %REPO_ROOT%\samples\birds\
echo.
echo The GRAN granular texture module reads .wav files; for bird-song
echo material drop curated calls into  samples\birds\  so they're easy
echo to find later [LOAD button can browse there directly].
echo.
echo Curated sources [all CC-licensed; check per-clip terms]:
echo.
echo   https://xeno-canto.org                 huge community-curated bird-call archive
echo   https://archive.org/details/birdsong   public-domain field recordings
echo   https://freesound.org                  search for: birds, calls, tweet, dawn
echo.
echo Workflow:
echo   1. Pick a clean clip [3-30 s] from one of the sources above.
echo   2. Drop it into  %REPO_ROOT%\samples\birds\
echo   3. Load via the GRAN card's LOAD button.  Set DENSITY high +
echo      PITCH-SCATTER moderate for chirpy / chorus textures.
goto license

:license
echo.
echo ---- License notice --------------------------------------------------------
if /i "%PACK%"=="salamander" (
    echo Salamander Grand Piano V3 by Alexander Holm -- CC-BY 3.0.
    echo See: https://github.com/sfzinstruments/SalamanderGrandPiano
)
if /i "%PACK%"=="sso" (
    echo Sonatina Symphonic Orchestra by Mattias Westlund [mirror by Peter Eastman].
    echo Released under SSO's free-use terms; see LICENSE in the cloned repo.
    echo Repo: https://github.com/peastman/sso
)
if /i "%PACK%"=="vsco2" (
    echo VSCO 2 Community Edition by Versilian Studios -- CC0 1.0 [public domain].
    echo See: https://github.com/sgossner/VSCO-2-CE
)
if /i "%PACK%"=="instruments-all" (
    echo Salamander Grand Piano: CC-BY 3.0  [Alexander Holm]
    echo Sonatina Symphonic Orchestra: free-use terms [Westlund / Eastman]
    echo VSCO 2 CE: CC0 1.0 [Versilian Studios]
)
if /i "%PACK%"=="amen"       echo Per-pack licenses vary -- check each download's terms before redistribution.
if /i "%PACK%"=="textures"   echo Per-pack licenses vary -- check each download's terms before redistribution.
if /i "%PACK%"=="wavetables" echo Per-pack licenses vary -- check each download's terms before redistribution.
if /i "%PACK%"=="impulses"   echo Per-pack licenses vary -- check each download's terms before redistribution.
echo ----------------------------------------------------------------------------
endlocal
exit /b 0
