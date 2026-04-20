@echo off
rem ─── scripts\run-llm-bass.bat ────────────────────────────────────────────────
rem Run the bass-voice directive suite (llm_suite_bass) against models.
rem
rem Verifies multi-voice coverage, accent/slide emission, subset rule,
rem and drum population on beat-driven styles.
rem
rem Usage:
rem   scripts\run-llm-bass.bat                         all models in models\
rem   scripts\run-llm-bass.bat models\gemma-4-E4B-it-Q4_K_M.gguf   single model
rem ─────────────────────────────────────────────────────────────────────────────
set LLM_SUITE=llm_suite_bass
call "%~dp0run-llm-tests.bat" %*
