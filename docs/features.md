# Impulse Instruct - Implemented Features

A detailed log of what's built.

---

## Core synth

- **Bass synth** - saw/square/supersaw oscillator, 4-pole Moog ladder filter (LP/HP/BP), sub-osc, noise, FM pair, portamento, waveshaper, overdrive, per-step accent + slide
- **Hoover lead** - supersaw into aggressive highpass sweep, pitch LFO, dedicated voice in UI
- **AN1X-style VA voice** - dual OSC (saw/square/tri/sin/noise), OSC2 coarse+fine detune, hard sync, ring mod, sub-osc, 3 filter modes, ADSR x 2, pitch envelope, per-voice LFO x 2 with delay/fade, pitch drift, free EG (8-step drawable envelope)
- **Drum machines** - Kit A (808-style: kick with pitch envelope, snare, hihat x 2, toms) + Kit B (909-style: kick, snare, hihat x 2, clap, rim)
- **Standalone noise voice** - white/pink/brown, volume + color + cutoff, AR envelope (5s attack, 10s release), filter LFO (0.05-10 Hz), sample-and-hold modulation (0.5-20 Hz), LLM-addressable
- **Amen break sampler voice** - DrumVoice::Amen in sequencer, linear-interp playback, AudioCommand::LoadSampler, AMEN tab with path/pitch/volume/loop UI
- **Gabber kick** - CLIP knob on both kicks: hard flat-top distortion, LLM-addressable via `kit_a.kick.clip` / `kit_b.kick.clip`
- **LFO matrix** - 4 independent slots, any waveform, wireable to any parameter, BPM sync, phase reset on transport start

## Sequencer

- 16-step base, variable step count per pattern (8/16/32/64), swing
- Per-voice step counts for polyrhythm (kick 16, hihat 12, bass 7...)
- Per-step: velocity, probability (0-100%), ratchet (1-4x), accent, slide
- Euclidean rhythm generator
- Pattern bank (8 slots), chain playback (up to 8 patterns in sequence)
- Live record - MIDI keyboard writes directly into steps
- Time signature selector (4/4, 3/4, 5/4, 6/8, 7/8...)
- Mute/solo per row, pattern copy/paste

## FX chain and routing

- Reverb, delay, chorus/ensemble, phaser (4-stage all-pass), ring modulator
- Waveshaper (pre-FX tanh), bitcrush (bit depth + sample rate), EQ (3-band biquad)
- Master compressor/limiter, tape saturation, drive
- **Modular rack** - zone-based module cards (Global/Voice/FxMod zones), RackState + Cable + PortRef, Bezier cable overlay with 3D tube rendering
- **Cable drag-to-patch** - click+drag from any port to create a cable; right-click a port to disconnect all cables on it; port hover glow (white halo idle, pulsing ring on valid targets, faster pulse when hovered); PointingHand/Crosshair cursor feedback; scroll area disabled near ports so drag never gets stolen
- **FX plan compilation** - `compile_fx_plan()` topologically sorts the cable graph into a `FxPlan`; `process_block()` iterates the plan instead of a fixed chain; default rack cables mirror the original serial order
- **Cable cycle detection** - `connect()` rejects audio cables that would create cycles (BFS reachability check); `strip_audio_cycles()` sanitizes session data on load; grayscale cable colors (R=G=B)
- **Per-voice FX buses** - voice mix split into 8 buses (AcidBass, DrumKit808, DrumKit909, HooverLead, An1xVoice, AmenSampler, NoiseVoice, GranularTexture) + TTS bus; each routed through its compiled chain before the global chain
- **Gated reverb** - `fx.reverb_gate_time` (0-2 s), GATE knob in FX panel
- **Master pitch offset** - `fx.master_pitch_st` (+-12 st), PITCH knob in MASTER group
- **Autotune FX module** - `ModuleKind::FxAutotune`; two-head grain overlap-add pitch shifter (`fx.autotune_amount` 0–1 → 0..+12 st, `fx.autotune_mix`); pre-allocated 4096-sample ring buffer (no audio-thread allocations); LLM-addressable via `fx.autotune_amount` / `fx.autotune_mix`

## Intelligence

- LLM runs locally via llama-server subprocess (official llama.cpp for Gemma/Qwen; PrismML fork for Bonsai 1-bit)
- Jam mode - PULSE evolves the pattern autonomously; heat slider 0-100% gates/throttles jam rate
- Behaviour templates: "build", "drop", "breakdown", "tension", "euphoric"
- Lock system - touch a knob to claim it; LLM won't override it
- Compact step arrays: index list `[0,4,8,12]` or inline `[1,0,0,0,...]` or clear `[]`
- Music theory grounding - root note + scale in system prompt, scale-snap on bass notes
- Instruction set - pre-written JSON templates for common phrases ("make an amen break", "remove claps", etc.)
- LFO dot-notation sanitization - handles malformed LLM output gracefully
- Sampling params exposed in settings: top_k, top_p, min_p, repeat_penalty, frequency_penalty, seed
- Reasoning (thinking) blocks shown in log (toggle)
- AI persona name - editable, used in system prompt
- **LLM jam tools** - ramp scheduling (`"ramp"` key), behaviour templates, heat-aware guidance in prompt
- **Internal music API** - `src/music_api/mod.rs`; all 10 ChordQuality variants, amen_pattern, scale_run, random_diatonic_chord; LLM dispatches via `"music_api"` JSON block
- **Audio feedback (Phase 1)** - LISTEN button captures audio, runs per-band RMS + transient analysis, prepends structured snapshot to prompt; response logged as `LISTEN ->`

## Multi-agent production team

- **Multiple LLM agents** - each agent has its own persona, model, scope, heat, temperature, conversation mode, style, and user instructions
- **Multi-model server pool** - `LlamaServerPool` manages N llama-server processes (ports 8766+), ref-counted per model; agents sharing a model share a single server
- **Per-agent model selector** - dropdown on each agent card; `None` inherits global default
- **Round-robin scheduling** - agents take turns during jam cycles; only enabled rack modules participate
- **Cable-driven scope** - `PortKind::Control` cables from agent to module define what each agent may control; `scope_from_control_cables()` resolves scope at inference time; empty scope = agent controls everything
- **Dynamic spawning** - agents can request new agents (`LlmAction::SpawnAgent`) or dismiss themselves (`LlmAction::DismissAgent`) via JSON; gated by `agent_autonomy` flag; auto-wire control cables on spawn
- **VRAM budget module** - `src/llm/vram.rs` with model profiles (Gemma, Bonsai, DeepSeek, Qwen3), VRAM estimates, and preset configurations
- **VRAM budget guard** - `would_exceed_vram()` rejects agent spawns that would exceed GPU memory; checked at SpawnAgent action + server pool acquire; prevents silent OOM crashes
- **Startup wizard** - always shows on startup; resume last session or start fresh with a preset (Solo/Duo/Swarm/Band/Voices/Lite); GPU VRAM detection + budget bar
- **VRAM estimate on agent cards** - shows `~X.XG VRAM` below model selector
- **Agent persona in log** - output and thinking lines show the correct agent persona name, not the global singleton
- **Console routes to agents** - typed prompts go to the first enabled agent instead of bypassing the agent system

## TTS / MC mode

- **NeuTTS Air voice cloning** — local GGUF model (~527MB), persistent Python server on port 8770; voice identity from 3-15s reference audio clips; per-module settings (voice reference, temperature, top-k, top-p)
- **TTS as rack module** — agents speak through TTS modules connected via control cables; no cable = no speech; single `ModuleKind::NeuTts` replaces old espeak/coqui dual-engine system
- Pitch-snap — synthesised voice quantised to nearest in-key note (autocorrelation pitch detection + resampling)
- API `"tts": true` on agent creation auto-adds a TTS module and wires it

## Style catalog (`styles.json`)

29 genre styles with the following fields (all user-editable):

| Field | Description |
|-------|-------------|
| `id`, `name` | Identifier and display name |
| `keywords` | Trigger words for auto-detection from prompts |
| `bpm_range` | Informational BPM range |
| `brief` | Short creative brief (~50 tokens) for smaller models |
| `description` | Full creative brief (~150 tokens) |
| `seed_patterns` | 16-step starter patterns (kick, snare, hihat, bass) |
| `suggested_root`, `suggested_scale` | Tonic and scale suggestion |
| `baseline_params` | Parameter reset applied when style is selected |
| `mc_lines` | Example MC/DJ lines for this style (optional, fed to MC-mode agents as reference) |
| `themes` | Topic words for singer/rapper agents (optional, gives creative direction) |

`mc_lines` and `themes` are injected into the system prompt — mc_lines only for MC/DJ conversation modes, themes for all modes. Styles that don't suit vocal content (minimal techno, IDM) omit these fields.

## Real-time mix observer

Continuous audio + pattern analysis running every ~2s. Results shown in the header bar and injected into every LLM system prompt as `AUDIO: ...` context. Agents see the mix state and can self-correct.

**Audio-level checks:**
- CLIPPING (peak > -1dB), near clip (peak > -3dB)
- sub overload, harsh highs, mid overload (band RMS thresholds)
- muddy low end (low >> mid by 20dB)
- over-compressed (crest < 3dB)
- near silence (peak < -40dB)
- snare rush (high RMS + fast transients)

**Pattern/mix checks:**
- bass very dense (>80% steps active)
- bass sparse (≤2 steps in 16)
- bass monotone (all active notes identical)
- no bass notes / no kick (while sequencer running)
- reverb high / delay feedback high / heavy distortion (FX extremes)

Alerts cycle in the header (2 at a time, rotating each second). Multiple alerts joined in LLM context with `!!` prefix.

## I/O

- MIDI in - NoteOn/Off to bass synth + live record; CC to synth params; Start/Stop to transport; MIDI clock in with 8-pulse rolling average BPM sync
- MIDI clock out - 24 PPQN, sent on dedicated thread via rtrb ring buffer (alloc-free audio path)
- WAV export (32-bit float), MP3 export (ffmpeg)
- Stem export - renders bass/kit_a/kit_b/amen/noise/hoover/an1x separately
- HTTP/MCP REST API on port 8765 (`--api` flag)
- OSC input - UDP listener on `--osc` (port 57120) or `--osc-port N`; addresses `/impulse/<section>/<param>`, `/impulse/sequencer/play|stop`, `/impulse/prompt`
- Project save/load - JSON snapshots; StateHistory ring buffer (50 deep), Ctrl+Z/Y, Edit menu, LLM snapshots before apply

## UI

- **12-column grid rack** - RPG-inventory-style module placement with snap-to-grid drag and drop; bin-packing auto-arrange with center-biased positioning; per-zone dynamic height
- **Two knob styles** - chrome (concentric rings, scale marks, glint arc) and flat/brushed (radial spokes, knurled edge, hub disc); freely mixable via `ControlPrefs::flat()`; fixed sizes (KNOB_PX=55, PAD_PX=34)
- **Knob value arc** - 270-degree outer range ring on all knobs showing full range with filled portion up to current value
- **Module remove with confirmation** - centered dialog on all non-core modules; disconnects cables and cleans up agents
- **Drag overlap prevention** - AABB collision check rejects drops onto occupied grid cells; red ghost overlay for blocked positions
- **Right-justified PAN sliders** - all voice panels (bass, 808, 909, AN1X, hoover, noise)
- **Right-justified step grids** - sequencer step buttons pushed to right edge via computed spacer
- **Full sequencer labels** - BANK, CHAIN, STEPS, SWING, SNAP, ACCENT, SLIDE; drum voices: 808 KICK, 909 CLOSED HH, etc.
- **Wider sequencer sliders** - BPM/SWING 200px, drum volume 100px
- **Uniform glass pane heights** - per-row min_height in hoover, AN1X, bass, 808, 909
- **Rack presets in wizard** - Empty/Basic/Standard/Full; wizard renamed "Rack Setup"
- **3x scroll speed** - mouse wheel boost for faster rack navigation
- 5 panels: Sequencer / Bass (303) / 808 / 909 / FX; AN1X and Hoover in sequencer area
- Chrome knobs, glass sliders, embossed buttons (neumorphic grayscale)
- **Skeuomorphic step buttons** - active inset well (debossed 2px) with inverted edge highlights; velocity bloom over inset; chrome knob well shadow + catch-light
- Velocity lanes below each step row (drag bars)
- XY pads (CUT x RES, ENV x DEC, REVERB mix x size, DELAY mix x feedback, 808 PITCH x DECAY); pair indicator in corner
- Oscilloscope strip (rolling 512-sample waveform) + ring scope (polar plot, single-polyline, write-head dot)
- ADSR envelope visualizer (interactive - drag zones)
- Piano display - Huth *Farbige Noten* (1888) 12-color theory, C2-C5; Off/Piano/Full setting
- Huth sequencer cells (Full mode) - colored U-cup notation on bass/hoover/AN1X rows; gate-proportional height
- Model selector - scan `models/`, hot-swap without restart
- Reasoning toggle; thinking blocks shown in log
- LLM strip: LISTEN button + live audio analysis display (sub/low/mid/high RMS, peak, crest, transients); collapsible to prompt row only (▲/▼ toggle)
- **Rack canvas** - zone-based horizontal module cards with Bezier cable overlay; responsive voice card grid (1/2/3 columns adaptive); Tab/toolbar toggle for cables
- **Cable signal animation** - normalised to arc length (constant perceived speed regardless of cable length); 2-5 dots per cable based on length
- **LFO visual cables** - active LFO slots synthesise rack cables from state (lfo.target → ModuleKind mapping) so LFO connections show without needing a rack cable entry
- **Central touch-paint mode** - `· / U / F` toolbar row; clicking a knob paints its param mode when mode is active; replaces broken right-click cycling
- **UI preferences** - UI scale (0.5–3.0×, instant via pixels_per_point), Huth style, CRT effect, phosphor settings; persisted in session.json
- **Responsive header** - heat slider fills remaining width; COOL/WARM/HOT/FIRE/CHAOS tier labels with color ramp; monitor volume labelled MON (listen-only, not export)
- **Zone visual hierarchy** - zone rails (Global/Voices/FX+Mod) have distinct gray backgrounds (24/18/14); module cards have 6px side + 8px top/bottom inner margin; 3-dot drag handle in every title bar
- **Per-zone collapse** - each zone rail has ▶/▼ toggle; collapses all cards in that zone to recover screen space
- **Preferences AI sub-tabs** - AI tab split into Model / Sampling / Personality / TTS sub-tabs; Sampling labelled "experimental"
- **Huth note coloring in log** - in-UI log colorizes note names (C4, A#3), frequencies (440Hz), MIDI context (note 60) with Huth palette; `colorize_log()` in `llm_strip.rs`; text remains selectable/copy-paste-able; safe word-boundary guards prevent false positives (D&B, E-flat etc.); quality word extension colors "A minor", "G major" as a single span
- **Log level persistence** - `log_level_idx` persisted in `session.json`; survives restarts
- **Skeuomorphic XY pad** — thick beveled outer frame (raised panel, inset rubber well), corner tick marks, rubber nub cursor with layered dome, specular catch-light, and hover glow ring; Y axis label/value overlaid inside pad; no left label strip
- **Centered module layout** — knobs and controls center-align horizontally within glass groups and rack module cards (no more left-clustering dead space)
- **Fixed control sizes** — knobs (55px), step buttons (34px), XY pads (172px), ADSR displays (77px); constants in `ui_prefs.rs`
- **Rounded sequencer step buttons** — rounding increased to 22% of pad size; neumorphic bevel uses rect_stroke pairs so highlights follow the rounded shape
- **Scaled envelope display** — decay/ADSR height scales with XY pad size (30% of xy_size, configurable via ENV HEIGHT override); width spans both pads
- **Huth ANSI terminal output** — `log::info!` LLM response lines and thinking tokens emit ANSI 24-bit color escape codes for note names, frequencies, and MIDI numbers when stdout is a TTY; matches in-UI log colorization
- **Huth piano key labels** — white and black key labels on the piano display use their Huth chromatic color instead of a flat gray
- **Header heat slider width** — heat slider fills all available header width; tier name (COOL/WARM/HOT/FIRE/CHAOS) and percentage painted as overlays on the slider rather than consuming separate fixed allocations
- **VRAM/RAM bar visibility** — memory bars drawn with an explicit gray-38 track so the full bar extent is always visible on the dark background; fill brightens to gray-160 above 85% usage
- **show_cables default on** — rack cables shown by default for new sessions
- **Thinking token UX** — toggle button label shows `{persona} (think)`; thinking lines rendered in a darker gray in the in-UI log; thinking forwarded to console via `log::info!`
- **Huth note labels in step cells** — active bass/hoover/AN1X step buttons show the note name (e.g. "C4") in Huth color above the velocity dot; `huth_note_cell` shows label at top-center; only when pad size ≥ 26 px
- **Per-voice FX send matrix** — compact grid at top of FX panel: voice rows (BASS/808/909/HOV/AN1X/AMEN/NOISE) × FX columns (REV/DLY/CHR/PHS/WVS/BIT/EQ/CMP/TAPE/DRV/RING/AUTO); click cell to toggle rack cable and recompile FX plan immediately
- **Autosave interval setting** — Preferences → System tab; Immediate / 5s / 30s / Manual; throttled via `last_save_time`; persisted in session.json
- **Even control spacing** — `even_group_width()` + `glass_group_fill()` helpers distribute glass groups evenly across panel width; applied to drum panels (Kit A/B) and FX panel (max 4 cols)
- **Hoover LP+BP mix** — Chamberlin SVF now mixes lowpass (body) with bandpass (resonant peak); amount scales with resonance param; tanh soft-clip prevents harshness; tighter q curve
- **Separate LLM temperature slider** — `llm.temperature: f32` (0–2, default 0.9) is now a first-class field decoupled from `llm.heat` (mutation rate); temperature is sent directly to llama-server; TEMP DragValue appears in the LLM strip header alongside the HEAT slider

## Intelligence

- Heat controls mutation rate and top_p widening (top_p widens with heat); CHAOS tier (≥90%) adds explicit "maximum disorder" instruction to system prompt
- TEMP slider (0–2) controls inference sampling temperature independently of heat; default 0.9

## Testing and build

- Unit tests across submodules (seq_tests, state_tests, llm_tests, audio::analysis, jam_tools_tests, music_api_tests, ui::note, ui::llm_strip), split at 1000-line limit per file
- 479 unit tests total
- 39 LLM integration tests in 3 suites: `llm_suite` (core), `llm_suite_style` (artist refs), `llm_suite_theory` (music theory + producer lingo)
- Pre-commit hook: fmt + clippy + tests + 1000-line LOC limit
- `scripts/run-tests.sh --coverage` - HTML coverage report (lcov)
- Cross-compile to Windows EXE via `cargo-xwin` + `scripts/build-all.sh`
- `scripts/download-models.sh` - Gemma 4 E4B (default), Bonsai 8B, Qwen3-8B, Qwen3-14B
- Windows `.bat` equivalents for all scripts (`start.bat`, `scripts/*.bat`)
- **CI/CD security** - `ci.yml` runs tests + tarpaulin + Codecov on `main` and `develop`; `release` job on `v*` tags builds Linux+Windows in GH Actions (no local builds), attaches `.sha256` sidecars and SLSA level-2 build provenance attestation
- Release zips include start scripts (`start.sh`/`start.bat`) and download helpers

## v0.6.x additions

### Analysis modules (rackable, FxMod zone)

- **Spectrum analyser** (`ModuleKind::SpectrumAnalyzer`) - 1024-point FFT via rustfft, 64 logarithmic frequency bands (20 Hz - 20 kHz), exponential smoothing knob, peak-hold markers with slow decay, grayscale bar display, 320px wide
- **Stereo correlation meter** (`ModuleKind::StereoMeter`) - phase correlation bar (-1 to +1) and L/R balance indicator; stereo ring buffer from audio callback; `stereo_correlation()` pure function in analysis.rs
- **Activity timeline** (`ModuleKind::ActivityTimeline`) - structured scrollable log of agent actions with relative timestamps, action tags (RSP/THK/UPD/NEW/DEL/YOU/SYS), persona names, 500-entry rolling buffer

### Presets and controls

- **Gabber kick preset** - `apply_gabber_kick_preset()`: extreme pitch sweep (0.9 depth, 0.6 time), heavy clip (0.8), button in Kit A panel
- **Bipolar param_control** - `param_control_bipolar()` maps -1..+1 to 0..1 for knob display; bass osc_detune now uses knob instead of DragValue
- **Step probability indicator** - active step buttons show a corner dot when probability < 100%; brightness scales with probability

### Per-module scaling and layout

- **Context-sensitive Ctrl+MW zoom** - over a module card: scales all modules of that kind; over empty space: global UI scale; `detect_ctrl_zoom()` with `ZoomTarget` enum
- **Per-kind scale storage** - `HashMap<ModuleKind, f32>` on ImpulseApp; scale affects content (knobs, margins, spacing) but not title bar height
- **View menu** - Compact All (0.6x), Expand All (1.0x), Arrange (canonical order), Reset Layout (clear + arrange); `arrange_canonical()` on RackState

### Lock state visualization

- **Knob mode visuals** - body darker when UserOwned, brighter when LlmFocus; catch-light and chrome rim shimmer at 1 Hz on Focus knobs (grayscale animated)
- **Slider mode tinting** - track background darker (U) / brighter (F); fill color varies per mode
- **Alt+click cycling** - Alt+click on flat knobs, chrome knobs, and slider tracks cycles Free / User / Focus

### Footer and header

- **Footer mode indicators** - [Ctrl] [Alt] [Tab:BACK] with tooltips; highlight when active
- **Header agent status** - compact round-robin display after HEAT slider; pulsing dot + persona name per enabled agent; bright when inferring, dim when idle

### Wizard improvements

- Removed redundant Skip button; "Resume" shown only with prior session
- Fresh install requires preset selection ("Start" disabled until chosen)
- Rack hidden + sequencer stopped while wizard is visible
- Clean-slate preset application (removes all existing agents first)

### Ambient / textural synthesis

- **Long envelopes** - AN1X ADSR attack up to 10s, release up to 30s for glacial pads; bass 303 decay extended to 5s
- **Granular texture module** (`ModuleKind::GranularTexture`) - new voice: loads WAV via `AudioCommand::LoadGranular`, plays up to 32 overlapping Hann-windowed grains with density, size, position, jitter, pitch scatter, spray params; true stereo output with per-grain pan law; full rack/UI/LLM integration
- **Tape delay with modulation** - wow/flutter LFO modulates delay read position (fractional interpolation), soft-clip tape saturation on feedback, max time extended to 2s; `delay_wow_flutter`, `delay_saturation` params
- **Reverb freeze** - `reverb_freeze` bool sets comb feedback to 1.0 and input to 0.0; tail holds indefinitely for drone/ambient
- **Pad presets** - 4 AN1X presets: warm pad, evolving texture, glass pad, sub drone; meditation style in styles.json; dark/space ambient baselines now enable AN1X with pad settings
- **Noise voice improvements** - AR envelope (attack 5s, release 10s), filter LFO (0.05-10 Hz), sample-and-hold modulation (0.5-20 Hz) for rhythmic texture
- **Cross-modulation** - bass osc → AN1X pitch FM (±24 st), noise → bass filter cutoff; `xmod_bass_to_an1x_pitch`, `xmod_noise_to_filter` params

### DSP improvements

- **Per-voice bass params** - `BassVoiceParams` struct snapshotted independently for all 4 bass voices; each voice reads its own cutoff/resonance/waveform/filter mode; voice 0 synced with LFO/free-EG modulation
- **Sidechain compression** - kick (808+909) ducks bass/pad/hoover/granular; `sidechain_amount`, `sidechain_attack` (0.1-50ms), `sidechain_release` (10-500ms)
- **Multiband compressor** - 3-band crossover at 200 Hz / 3 kHz with independent per-band envelope followers; `compressor_multiband` param toggles mode
- **Stereo width control** - chorus-based decorrelation on master output; `stereo_width` (0=mono, 0.5=normal, 1=wide)

### UI/UX improvements

- **Clickable footer mode toggles** - double-click Ctrl/Alt/Tab indicators to lock mode on without holding key; locks stored in egui temp data, read by zoom/widgets/cables
- **Per-module collapse** - click title bar drag zone to collapse/expand module cards; state stored in egui temp data per module ID
- **Module drag reorder polish** - insertion line indicator during drag; undo support on reorder
- **Keyboard shortcuts help overlay** - ? or F1 toggles foreground overlay listing all shortcuts
- **Undo for agent changes** - `push_history()` before agent spawn/dismiss mutations

### Visualization

- **CRT scan-line overlay** - scan lines (6px spacing, alpha 18) + edge vignette; toggled via `crt_effect` in UiPrefs
- **Ring scope** - polar waveform plot of scope buffer with simulated write-head marker; displayed alongside linear oscilloscope

### Intelligence improvements

- **Agent memory** - `_comment` snippets persisted in per-agent `memory[]` (max 20); injected into system prompt section; survives session restart via session.json serialization
- **Style learning** - `observe_user_edit()` records "user prefers high/low X" into `style_observations[]` (max 10); injected as learned preferences in system prompt; wired into bass panel (fires on extreme knob positions >0.7 or <0.3)
- **Inter-agent messaging** - `SendHint` LlmAction via JSON `send_hint` field; hints queued in target agent's `pending_hints[]` (max 5); consumed on next inference cycle and injected into prompt

### Refactoring and test coverage

- **479 unit tests**; suites: `llm_apply_tests` (68), `persistence_tests` (25), `helpers_tests` (7), `music_tests` (13), `dsp_tests` (16), `fx_plan_tests` (7), `vram_tests` (9)
- **`rack.connect_control(from_id, to_id)`** - replaces 8-line PortRef boilerplate at 6 call sites
- **`spawn_agent()` pure function** - transitions.rs; wizard.rs and SpawnAgent handler refactored to use it
- **`format_llm_display()` pure function** - extracted from drain_llm_outputs into transitions.rs
- **`BassVoiceParams` struct** - per-voice AudioParams snapshot
- **Bass303 extracted** to `src/audio/dsp/bass303.rs` (line-limit split)
- **DSP utilities extracted** to `src/audio/dsp/dsp_util.rs` (midi_to_hz, tanh)
- **Samplers extracted** to `src/audio/dsp/samplers.rs` (AmenVoice, GranularVoice)
- **Dead code removed** - `sync_default_agent`
- **Windows code-signing** - signtool step in `build-all.bat` (set SIGN_CERT + SIGN_PASS)
