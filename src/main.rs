// ─── main.rs ─────────────────────────────────────────────────────────────────
// Impulse Instruct — LLM-first audio synthesizer
//
// Usage:
//   impulse-instruct [OPTIONS]
//
// Options:
//   --no-api           Disable HTTP/MCP API (default: on, port 8765)
//   --port <N>         HTTP port (default 8765)
//   --model <path>     Path to GGUF model file
//   --log <level>      Log level: error/warn/info/debug (default info)
//   --mock             Run with mock LLM responses (no real model needed)
//
// Thread model:
//   Main/UI:  eframe runs here (required by macOS/Windows)
//   Audio:    cpal callback (real-time, elevated by OS)
//   LLM:      std::thread (blocking inference, spawns llama-server subprocess)
//   HTTP:     tokio runtime in separate OS thread (disabled by --no-api)

use impulse_instruct::midi::MidiEvent;
use impulse_instruct::{api, audio, banner, llm, midi, state, ui};

use parking_lot::RwLock;
use std::sync::Arc;

use audio::AudioEngine;
use llm::{LlmInput, run_llm_loop};
use state::AppState;

/// Default HTTP/MCP API port.  Override with `--port <N>`.
const DEFAULT_API_PORT: u16 = 8765;
/// Default OSC listener port (SuperCollider's default).  Override with
/// `--osc-port <N>`.  Only used when `--osc` / `--osc-port` is passed.
const DEFAULT_OSC_PORT: u16 = 57120;

// ─── CLI args (no extra deps — just std::env::args) ──────────────────────────

struct Args {
    no_api: bool,
    port: u16,
    model: Option<String>,
    log_level: Option<String>, // None = use persisted setting; Some = CLI override
    mock: bool,
    osc_port: Option<u16>, // None = disabled; Some(port) = OSC listener enabled
    skip_wizard: bool,
    fresh_session: bool,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut result = Self {
            no_api: false,
            port: DEFAULT_API_PORT,
            model: None,
            log_level: None,
            mock: false,
            osc_port: None,
            skip_wizard: false,
            fresh_session: false,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--no-api" => result.no_api = true,
                "--mock" => result.mock = true,
                "--skip-wizard" => result.skip_wizard = true,
                "--fresh-session" => result.fresh_session = true,
                "--port" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        result.port = v.parse().unwrap_or(DEFAULT_API_PORT);
                    }
                }
                "--model" => {
                    i += 1;
                    result.model = args.get(i).cloned();
                }
                "--log" => {
                    i += 1;
                    result.log_level = args.get(i).cloned();
                }
                "--osc" => result.osc_port = Some(DEFAULT_OSC_PORT),
                "--osc-port" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        result.osc_port = Some(v.parse().unwrap_or(DEFAULT_OSC_PORT));
                    }
                }
                "--help" | "-h" => {
                    println!("Impulse Instruct — LLM-first audio synthesizer\n");
                    println!("USAGE: impulse-instruct [OPTIONS]\n");
                    println!("OPTIONS:");
                    println!(
                        "  --no-api           Disable HTTP/MCP API (default: on, port {DEFAULT_API_PORT})"
                    );
                    println!("  --port <N>         HTTP port (default: {DEFAULT_API_PORT})");
                    println!("  --model <path>     GGUF model path");
                    println!("  --log <level>      Log level (default: info)");
                    println!("  --mock             Run without LLM (mock responses only)");
                    println!("  --skip-wizard      Skip the setup wizard on launch");
                    println!("  --fresh-session    Start with empty rack, ignore saved session");
                    println!(
                        "  --osc              Enable OSC input on port {DEFAULT_OSC_PORT} (UDP)"
                    );
                    println!("  --osc-port <N>     Enable OSC input on port N (UDP)");
                    std::process::exit(0);
                }
                other => log::warn!("Unknown argument: {}", other),
            }
            i += 1;
        }
        result
    }
}

fn main() {
    if let Err(e) = run() {
        let msg = format!("Impulse Instruct failed to start:\n\n{e}");
        eprintln!("{msg}");
        // Write a log file next to the binary so users who launched without a
        // terminal can find the error (the dialog below may not be available
        // in all environments).
        if let Ok(exe) = std::env::current_exe() {
            let log = exe.with_file_name("impulse-instruct-error.log");
            let _ = std::fs::write(&log, &msg);
            eprintln!("Error log written to {}", log.display());
        }
        show_startup_error(&msg);
        std::process::exit(1);
    }
}

/// Show a native error dialog when startup fails before the UI is available.
/// Tries platform-specific GUI tools in order; falls back gracefully.
fn show_startup_error(msg: &str) {
    #[cfg(unix)]
    {
        let args_list: &[(&str, &[&str])] = &[
            (
                "zenity",
                &["--error", "--no-wrap", "--title=Impulse Instruct", "--text"],
            ),
            ("kdialog", &["--error", "--title=Impulse Instruct"]),
            ("xmessage", &["-center", "-title", "Impulse Instruct"]),
        ];
        for (tool, prefix) in args_list {
            let ok = std::process::Command::new(tool)
                .args(*prefix)
                .arg(msg)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return;
            }
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxA(
                hwnd: *mut core::ffi::c_void,
                text: *const u8,
                caption: *const u8,
                utype: u32,
            ) -> i32;
        }
        let text = format!("{msg}\0");
        let caption = b"Impulse Instruct\0";
        // SAFETY: MessageBoxA is always available on Windows; pointers are valid
        // for the duration of the call; strings are null-terminated.
        unsafe {
            MessageBoxA(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                0x10, // MB_ICONERROR
            );
        }
    }
}

fn run() -> anyhow::Result<()> {
    // Catch panics from ALL threads and log them before exit.
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("\n!!! PANIC !!!\n{}\nBacktrace:\n{}", info, bt);
        log::error!("PANIC: {}\n{}", info, bt);
    }));

    // Install SIGINT/SIGTERM handler BEFORE spawning any threads so the
    // signal mask is inherited everywhere. Without this, Ctrl-C leaves
    // llama-server children orphaned (Rust Drop doesn't run on signals).
    install_signal_handler();

    let args = Args::parse();

    // Resolve log level: CLI --log overrides persisted setting, which overrides "info"
    let persisted_level = impulse_instruct::state::load_session()
        .as_ref()
        .and_then(|s| s.log_level_idx)
        .and_then(|idx| {
            ["error", "warn", "info", "debug", "trace"]
                .get(idx)
                .copied()
        });
    let effective_level = args
        .log_level
        .as_deref()
        .or(persisted_level)
        .unwrap_or("info");
    // Init env_logger with max filter (trace) — runtime level controlled by
    // log::set_max_level (from UI prefs or --log flag). This lets the user
    // change log level at runtime without restarting.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .format(|buf, record| {
            use std::io::Write;
            let ts = buf.timestamp_millis();
            let ts_str = format!("{}", ts)
                .replace('T', " ")
                .trim_end_matches('Z')
                .to_string();
            let msg = format!("{}", record.args());
            let colored = impulse_instruct::log_fmt::colorize(&msg);
            writeln!(buf, "{} {:5} {}", ts_str, record.level(), colored)
        })
        .init();
    // Apply the effective log level as the global gate
    let level_filter = match effective_level {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };
    log::set_max_level(level_filter);

    banner::print_banner();

    log::info!("Starting…");
    if !args.no_api {
        log::info!(
            "HTTP API enabled on port {} (pass --no-api to disable)",
            args.port
        );
    }

    // ── Shared state ──────────────────────────────────────────────────────────
    let app_state = Arc::new(RwLock::new(AppState::default()));

    // Load session (rack layout, style, ui prefs) from last run.
    // When --fresh-session is passed, start with the Empty rack preset
    // (sequencer + master + console only). Demo scripts use this to ensure
    // a clean, reproducible starting state.
    if args.fresh_session {
        log::info!("Fresh session requested — starting with Empty rack preset");
        let empty = &impulse_instruct::state::RACK_PRESETS[0]; // "Empty"
        let mut s = app_state.write();
        s.rack = impulse_instruct::state::RackState::from_preset(empty);
        // Clear agents/TTS pre-populated by AppState::default() for the default
        // rack. Without this, a stale "default" agent survives and fires the
        // startup auto-prompt before any scenario has set up its own agents.
        s.llm_agents.clear();
        s.tts_modules.clear();
    } else if let Some(session) = impulse_instruct::state::load_session() {
        impulse_instruct::state::apply_session(&mut app_state.write(), session);
        log::info!("Session restored from session.json");
    }

    // Apply --model override, falling back to the last-used model from session / settings.json
    if let Some(ref model_path) = args.model {
        app_state.write().llm.model_path = model_path.clone();
        log::info!("Model (CLI): {}", model_path);
    } else if app_state.read().llm.model_path.is_empty()
        && let Some(saved) = impulse_instruct::state::load_model_setting()
    {
        app_state.write().llm.model_path = saved.clone();
        log::info!("Model (settings): {}", saved);
    }
    {
        let s = app_state.read();
        log::info!(
            "Model: {}",
            if s.llm.model_path.is_empty() {
                "(none — mock mode)".to_string()
            } else {
                s.llm.model_path.clone()
            }
        );
        log::info!(
            "Rack: {} modules, {} cables, {} agents, {} TTS modules",
            s.rack.modules.len(),
            s.rack.cables.len(),
            s.llm_agents.len(),
            s.tts_modules.len()
        );
        if !s.llm_agents.is_empty() {
            for a in &s.llm_agents {
                log::info!(
                    "  Agent {}: {} scope={:?} model={:?}",
                    a.id,
                    a.persona_name,
                    a.scope,
                    a.model_path
                );
            }
        }
        if let Some(ref style) = s.llm.active_style {
            log::info!("Style: {}", style);
        }
    }

    // ── Channels ─────────────────────────────────────────────────────────────
    // Unbounded so model-load stalls (30–90 s for `wait_for_ready`) and
    // long pipeline turns can't cause `try_send` to silently drop a user
    // prompt or a SwitchAgentModel control message.  Throughput is human-
    // paced; growing the queue isn't a real concern.
    let (llm_tx, llm_rx) = crossbeam_channel::unbounded::<LlmInput>();
    let (llm_out_tx, llm_out_rx) = crossbeam_channel::bounded::<llm::LlmOutput>(32);

    // ── Audio engine (before LLM thread so we can share tts_tx) ─────────────
    log::info!("Starting audio engine…");
    let audio_engine = AudioEngine::new(Arc::clone(&app_state))?;
    log::info!(
        "Audio engine started (sample_rate={}Hz)",
        audio_engine.sample_rate
    );

    // ── LLM thread ────────────────────────────────────────────────────────────
    log::info!("Spawning LLM thread…");
    {
        let state = Arc::clone(&app_state);
        let out_tx = llm_out_tx.clone();
        let mock = args.mock;
        let tts_tx = audio_engine.tts_tx.clone();
        std::thread::Builder::new()
            .name("llm".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_llm_loop(state, llm_rx, out_tx, mock, tts_tx))
            .expect("failed to spawn LLM thread");
    }
    log::info!("LLM thread spawned");

    // ── API log channel (lock-free, API→UI) ────────────────────────────────
    let (api_log_tx, api_log_rx) = crossbeam_channel::bounded::<String>(128);
    let api_params_dirty = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // ── HTTP API thread (on by default; disabled by --no-api) ────────────────
    let api_port = if !args.no_api { Some(args.port) } else { None };
    if let Some(port) = api_port {
        let state = Arc::clone(&app_state);
        let llm_tx_http = llm_tx.clone();
        let log_tx = api_log_tx.clone();
        let dirty = Arc::clone(&api_params_dirty);
        std::thread::Builder::new()
            .name("http".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async move {
                    let api_state = api::ApiState {
                        app_state: state,
                        llm_tx: llm_tx_http,
                        api_log_tx: log_tx,
                        params_dirty: dirty,
                    };
                    if let Err(e) = api::run_server(api_state, port).await {
                        log::error!("HTTP server error: {}", e);
                    }
                });
            })
            .expect("failed to spawn HTTP thread");
    }

    // ── MIDI input ────────────────────────────────────────────────────────────
    let (midi_tx, midi_rx) = crossbeam_channel::bounded::<MidiEvent>(256);
    let (_midi_listener, midi_port) = midi::MidiListener::auto_connect(midi_tx);
    if let Some(ref name) = midi_port {
        log::info!("MIDI: connected to '{}'", name);
    }

    // ── MIDI clock output ─────────────────────────────────────────────────────
    {
        let mut midi_clock_rx = audio_engine.midi_clock_rx;
        let (clock_out, _clock_port) = midi::MidiClockOutput::auto_connect();
        if let Some(mut sender) = clock_out {
            std::thread::Builder::new()
                .name("midi-clock-out".into())
                .spawn(move || {
                    loop {
                        while let Ok(byte) = midi_clock_rx.pop() {
                            sender.send_byte(byte);
                        }
                        std::thread::sleep(std::time::Duration::from_micros(500));
                    }
                })
                .expect("failed to spawn MIDI clock out thread");
        } else {
            // Still need to drain the rx so it doesn't fill up; drop it here if no output.
            drop(midi_clock_rx);
        }
    }

    // ── OSC listener (optional, disabled by default) ─────────────────────────
    let _osc_listener = args.osc_port.map(|port| {
        impulse_instruct::osc::OscListener::start(port, Arc::clone(&app_state), llm_tx.clone())
    });

    // ── UI (blocks this thread) ───────────────────────────────────────────────
    let state_for_ui = Arc::clone(&app_state);
    let audio_channels = ui::AudioChannels {
        params_tx: audio_engine.params_tx,
        scope_rx: audio_engine.scope_rx,
        capture_rx: audio_engine.capture_rx,
        dsp_load_rx: audio_engine.dsp_load_rx,
        stereo_rx: audio_engine.stereo_rx,
        granular_capture_rx: audio_engine.granular_capture_rx,
        tts_tx: audio_engine.tts_tx.clone(),
        sample_instrument_poly: Arc::clone(&audio_engine.sample_instrument_poly),
    };

    log::info!("Launching UI window…");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Impulse Instruct")
            .with_icon(std::sync::Arc::new(make_window_icon()))
            .with_maximized(true)
            .with_inner_size([1920.0, 1080.0]) // fallback if maximized hint is ignored by WM
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Impulse Instruct",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(ui::ImpulseApp::new(
                cc,
                state_for_ui,
                audio_channels,
                llm_tx,
                llm_out_rx,
                midi_rx,
                midi_port,
                api_log_rx,
                api_port,
                // Honour the CLI flag, OR auto-skip when the user already
                // completed the wizard in a previous session.
                args.skip_wizard
                    || impulse_instruct::state::load_session()
                        .and_then(|s| s.wizard_done)
                        .unwrap_or(false),
                api_params_dirty,
            )))
        }),
    )
    .map_err(|e| {
        log::error!("eframe exited with error: {}", e);
        anyhow::anyhow!("UI error: {}", e)
    })?;
    log::info!("eframe::run_native returned — window closed normally");

    Ok(())
}

/// Install SIGINT/SIGTERM handler on unix. Rust's Drop doesn't run when
/// the process is killed by a signal, which orphaned our llama-server
/// children. Here we block the signals on the main thread so spawned
/// threads inherit the block, then spin a dedicated thread that
/// sigwait()s, kills llama-server children, and exits the process.
#[cfg(unix)]
fn install_signal_handler() {
    use std::mem::MaybeUninit;
    unsafe {
        let mut mask: MaybeUninit<libc::sigset_t> = MaybeUninit::uninit();
        libc::sigemptyset(mask.as_mut_ptr());
        libc::sigaddset(mask.as_mut_ptr(), libc::SIGINT);
        libc::sigaddset(mask.as_mut_ptr(), libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, mask.as_ptr(), std::ptr::null_mut());
    }
    std::thread::Builder::new()
        .name("signal-handler".into())
        .spawn(|| {
            let mut sig: i32 = 0;
            unsafe {
                let mut mask: MaybeUninit<libc::sigset_t> = MaybeUninit::uninit();
                libc::sigemptyset(mask.as_mut_ptr());
                libc::sigaddset(mask.as_mut_ptr(), libc::SIGINT);
                libc::sigaddset(mask.as_mut_ptr(), libc::SIGTERM);
                libc::sigwait(mask.as_ptr(), &mut sig);
            }
            let name = match sig {
                libc::SIGINT => "SIGINT",
                libc::SIGTERM => "SIGTERM",
                _ => "signal",
            };
            eprintln!("\n{name} received — shutting down, killing llama-server children");
            log::warn!("{name} received — initiating shutdown");
            let _ = std::process::Command::new("pkill")
                .args(["-TERM", "-f", "llama-server .* --model"])
                .status();
            // Short grace period so pkill lands before we exit.
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = std::process::Command::new("pkill")
                .args(["-KILL", "-f", "llama-server .* --model"])
                .status();
            std::process::exit(128 + sig);
        })
        .expect("failed to spawn signal-handler thread");
}

#[cfg(not(unix))]
fn install_signal_handler() {
    // Windows: no equivalent without adding a crate; ctrl-C just kills the
    // process. The demo script already sweeps llama-server itself there.
}

/// Generate the window icon pixel buffer.
/// Matches assets/icon.svg: 256×256, "IMPULSE INSTRUCT" title, Huth-colored octave,
/// black 1px borders on every key (white and black).
fn make_window_icon() -> egui::IconData {
    const W: u32 = 256;
    const H: u32 = 256;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    // ── pixel helpers ─────────────────────────────────────────────────────────

    let put = |rgba: &mut Vec<u8>, px: u32, py: u32, r: u8, g: u8, b: u8| {
        if px < W && py < H {
            let i = ((py * W + px) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    };

    let fill_rect = |rgba: &mut Vec<u8>, x0: u32, y0: u32, w: u32, h: u32, r: u8, g: u8, b: u8| {
        for py in y0..y0 + h {
            for px in x0..x0 + w {
                if px < W && py < H {
                    let i = ((py * W + px) * 4) as usize;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = 255;
                }
            }
        }
    };

    // Background
    for chunk in rgba.chunks_mut(4) {
        chunk[0] = 12;
        chunk[1] = 12;
        chunk[2] = 12;
        chunk[3] = 255;
    }

    // ── keyboard (matches SVG: translate(27, 52), stride=29, white 28×160, black 17×100) ──
    let tx: u32 = 27;
    let ty: u32 = 52;
    let wkey_w: u32 = 28;
    let wkey_h: u32 = 160;
    let bkey_w: u32 = 17;
    let bkey_h: u32 = 100;

    // White keys with 1px black border
    let white_keys = [
        (0u32, 0x33u8, 0x66u8, 0xDDu8), // C
        (29, 0x33, 0xAA, 0x66),         // D
        (58, 0xDD, 0xCC, 0x22),         // E
        (87, 0xEE, 0x88, 0x22),         // F
        (116, 0xEE, 0x33, 0x66),        // G
        (145, 0x99, 0x66, 0xCC),        // A
        (174, 0x44, 0x33, 0xAA),        // B
    ];
    for (rx, r, g, b) in white_keys {
        // Fill interior (inset 1px from border)
        fill_rect(
            &mut rgba,
            tx + rx + 1,
            ty + 1,
            wkey_w - 2,
            wkey_h - 2,
            r,
            g,
            b,
        );
        // 1px black border
        for px in tx + rx..tx + rx + wkey_w {
            put(&mut rgba, px, ty, 0, 0, 0);
        } // top
        for px in tx + rx..tx + rx + wkey_w {
            put(&mut rgba, px, ty + wkey_h - 1, 0, 0, 0);
        } // bottom
        for py in ty..ty + wkey_h {
            put(&mut rgba, tx + rx, py, 0, 0, 0);
        } // left
        for py in ty..ty + wkey_h {
            put(&mut rgba, tx + rx + wkey_w - 1, py, 0, 0, 0);
        } // right
    }

    // Black keys drawn on top, with 1px black border
    let black_keys = [
        (20u32, 0x22u8, 0x99u8, 0xBBu8), // C#
        (49, 0x88, 0xCC, 0x22),          // D#
        (107, 0xDD, 0x44, 0x22),         // F#
        (136, 0xCC, 0x11, 0x44),         // G#
        (165, 0x77, 0x44, 0xBB),         // A#
    ];
    for (rx, r, g, b) in black_keys {
        fill_rect(
            &mut rgba,
            tx + rx + 1,
            ty + 1,
            bkey_w - 2,
            bkey_h - 2,
            r,
            g,
            b,
        );
        for px in tx + rx..tx + rx + bkey_w {
            put(&mut rgba, px, ty, 0, 0, 0);
        }
        for px in tx + rx..tx + rx + bkey_w {
            put(&mut rgba, px, ty + bkey_h - 1, 0, 0, 0);
        }
        for py in ty..ty + bkey_h {
            put(&mut rgba, tx + rx, py, 0, 0, 0);
        }
        for py in ty..ty + bkey_h {
            put(&mut rgba, tx + rx + bkey_w - 1, py, 0, 0, 0);
        }
    }

    // ── sine wave decoration above the keyboard ───────────────────────────────
    // Two cycles across the full width, centred in the 52px gap above the keys.
    // Drawn with soft alpha-falloff so it looks smooth at icon size.
    let wave_cx: f32 = (ty as f32) / 2.0; // vertical centre of the gap
    let amplitude: f32 = 14.0;
    let cycles: f32 = 2.5;
    for px in 0..W {
        let phase = (px as f32 / W as f32) * cycles * 2.0 * std::f32::consts::PI;
        let wave_y = wave_cx + amplitude * phase.sin();
        // Draw 3px soft column around the wave
        for dy in -2i32..=2i32 {
            let py = wave_y + dy as f32;
            if py < 0.0 || py >= H as f32 {
                continue;
            }
            let dist = (dy as f32).abs();
            let brightness = ((1.0 - dist / 2.5) * 255.0).max(0.0) as u8;
            let i = ((py as u32 * W + px) * 4) as usize;
            if i + 3 < rgba.len() {
                rgba[i] = brightness;
                rgba[i + 1] = brightness;
                rgba[i + 2] = brightness;
                rgba[i + 3] = 255;
            }
        }
    }

    egui::IconData {
        rgba,
        width: W,
        height: H,
    }
}
