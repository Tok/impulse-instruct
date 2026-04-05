@echo off
rem ─── scripts\run-tests.bat ───────────────────────────────────────────────────
rem Run unit tests.
rem
rem Usage:
rem   scripts\run-tests.bat              run all unit tests
rem   scripts\run-tests.bat sequencer    run tests matching "sequencer"
rem
rem For LLM response tests (requires running llama-server):
rem   scripts\run-llm-tests.bat
rem ──────────────────────────────────────────────────────────────────────────────
cd /d "%~dp0.."

set FILTER=%~1

echo ==========================================
echo   Running tests...
echo ==========================================

if "%FILTER%"=="" (
    cargo test
) else (
    cargo test -- %FILTER%
)
