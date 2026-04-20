@echo off
rem ─── scripts\run-llm-theory.bat ──────────────────────────────────────────────
rem Run the music theory + producer lingo test suite (llm_suite_theory).
rem
rem Tests cover: four-on-the-floor, backbeat, half-time, one-drop, offbeat hihats,
rem D minor triad, A minor pentatonic, natural minor scale, Reese bass, sub bass,
rem chord stabs, slow attack pad.
rem
rem Usage:
rem   scripts\run-llm-theory.bat                         all models in models\
rem   scripts\run-llm-theory.bat models\gemma-4-E4B-it-Q4_K_M.gguf   single model
rem ─────────────────────────────────────────────────────────────────────────────
set LLM_SUITE=llm_suite_theory
call "%~dp0run-llm-tests.bat" %*
