# Demo Recording System

Automated demo video recording for Impulse Instruct. Builds the app, launches it,
drives it via the HTTP API, records screen + audio, adds TTS narration, and encodes
the final video with NVENC.

## Quick start

```bash
./demo/record-demo.sh                  # full pipeline
./demo/record-demo.sh --skip-build     # skip cargo build
./demo/record-demo.sh --no-tts         # no narration
./demo/record-demo.sh --dry-run        # just generate TTS clips
```

Output: `demo/output/impulse_demo_<timestamp>.mp4`

## Dependencies

| Tool | Package | Purpose |
|------|---------|---------|
| `ffmpeg` (with NVENC) | `ffmpeg` | Screen recording + encoding |
| NeuTTS Air | `scripts/download-models.sh` (offers setup) | TTS narration (voice cloning GGUF) |
| `espeak-ng` | `espeak-ng` | Reference-voice rendering for `scripts/generate-voices.sh` |
| `jq` | `jq` | Safe JSON payload building for NeuTTS requests |
| `paplay` | `pulseaudio-utils` | TTS audio playback |
| `pw-record` | `pipewire` | Per-app audio capture |
| `wmctrl` | `wmctrl` | Window detection |
| `xwininfo` | `x11-utils` | Window geometry |
| `bc` | `bc` | Subtitle timing math |
| `python3` + `libxdo3` | `libxdo-dev` | X11 automation (xdo.py) |

## Files

| File | Purpose |
|------|---------|
| `demo/record-demo.sh` | Main orchestrator — build, launch, record, encode |
| `demo/scenario.sh` | The demo sequence — 16 scenes with API calls + TTS |
| `demo/lib.sh` | Helpers: API wrappers, TTS, PipeWire, scroll/flip |
| `demo/xdo.py` | X11 automation via libxdo3 (replaces xdotool) |
| `demo/test-api.sh` | Smoke test: runs all API calls without recording |
| `demo/test-api-minimal.sh` | Isolated tests to diagnose which API call crashes |

## How it works

1. **Build** — `cargo build --release`
2. **Pre-generate TTS** — start `scripts/neutts-server.py`, walk the
   scenario matching `narrate` / `say` / `scene` lines, write all clips
   to `$BATCH_DIR/tts/`. Retries up to 3× with a 120 s curl timeout; the
   end of this step prints `(N ok, M failed)` with the missing clip IDs
3. **Pre-generate SRT** — `pregenerate_srt` parses the scenario again
   (`say` / `narrate` / `scene` / `pause` / `wait_seconds`) and writes a
   complete subtitle file before recording starts; durations are
   `max(clip_duration, reading_time)` so subtitles stay on screen long
   enough even if NeuTTS truncated the audio
4. **Stop NeuTTS server** — frees GPU memory for the main LLM during
   recording; runtime playback uses cached WAVs
5. **Launch app** — `--skip-wizard --fresh-session --model <gemma4>` so
   recordings never inherit the user's saved rack
6. **Window detection** — `wmctrl` finds the app window, gets geometry
7. **Screen recording** — `ffmpeg` with x11grab + NVENC (`h264_nvenc`),
   `-pix_fmt yuv420p -vf "crop=trunc(iw/2)*2:trunc(ih/2)*2"` for even
   dimensions
8. **Audio capture** — `pw-record` targets the app's PipeWire node
   (isolated) or a virtual recording sink
9. **Scenario** — sources the scenario file; high-level helpers
   (`say`, `wait_seconds`, `scene`) drive the app via HTTP API
10. **Runtime narration** — `narrate --wait` blocks on `paplay` of the
    cached WAV per clip; `[narrate] <id> (<dur>s)` lines log each play,
    `PLAY FAILED` surfaces paplay/aplay failures that used to be silent
11. **Post-processing** — re-encode with
    `-sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp"`,
    mux audio, optionally burn the pre-generated SRT in

## Demo scenario structure

**Part 1: Solo mode** (Gemma 4, single agent)
- Reset rack to minimal → add 303, 808, 909 → add PULSE agent
- Show control cable wiring (rack flip)
- LLM builds pattern from natural language
- Filter tweaking, FX modules, parameter locking

**Part 2: Multi-agent band** (Gemma 4 + Bonsai specialists)
- Remove PULSE → add BASS, DRUMS, FX agents
- Show per-agent control cables
- Each agent receives individual prompts
- Creative direction scene

## HTTP API endpoints used

```
POST /api/rack/reset                    strip to seq + master + console
POST /api/rack/add    {"kind":"bass"}   add module (auto-wires audio cable)
POST /api/rack/agent  {"persona":"BASS","scope":["bass"],"model":"bonsai"}
POST /api/scroll      {"target":"bass"} scroll to module
POST /api/flip        {"show_back":true} show cables
POST /api/prompt      {"prompt":"...","agent":"BASS"} target specific agent
POST /api/sequencer/play
POST /api/sequencer/stop
POST /api/lock        {"paths":["sequencer.kick_a_steps"]}
POST /api/unlock      {"paths":["sequencer.kick_a_steps"]}
```

## Tips

- **Best of 5** — LLM responses vary. Run multiple takes and pick the best.
- **Kill stale servers** — `pkill -f llama-server` before recording.
- **Kill stale ffmpeg** — `pkill -f ffmpeg` if a previous run left zombies.
- **Bonsai needs explicit prompts** — include parameter values ("set cutoff to 0.3").
- **Check wiring** — flip the rack (`/api/flip`) to verify control cables.
- **TTS cache** — delete `demo/tts_cache/` to regenerate after editing narration text.
