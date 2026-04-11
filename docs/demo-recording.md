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
| `espeak-ng` | `espeak-ng` | TTS narration |
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
2. **Launch** — starts the app with `--skip-wizard --model <gemma4>`
3. **Window detection** — `wmctrl` finds the app window, gets geometry
4. **Screen recording** — `ffmpeg` with x11grab + NVENC (`h264_nvenc`)
5. **Audio capture** — `pw-record` targets the app's PipeWire node (isolated)
6. **Scenario** — sources `scenario.sh` which drives the app via HTTP API
7. **TTS narration** — pre-generated clips played via `paplay`
8. **Post-processing** — mux video + audio, burn subtitles, NVENC encode

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
