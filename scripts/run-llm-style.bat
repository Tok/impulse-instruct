@echo off
rem ─── scripts\run-llm-style.bat ───────────────────────────────────────────────
rem Run the artist/genre reference test suite (llm_suite_style) against models.
rem
rem Usage:
rem   scripts\run-llm-style.bat                         all models in models\
rem   scripts\run-llm-style.bat models\Bonsai-8B.gguf   single model
rem ─────────────────────────────────────────────────────────────────────────────
set LLM_SUITE=llm_suite_style
call "%~dp0run-llm-tests.bat" %*
