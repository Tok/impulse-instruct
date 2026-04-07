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

// ─── CLI args (no extra deps — just std::env::args) ──────────────────────────

struct Args {
    no_api: bool,
    port: u16,
    model: Option<String>,
    log_level: String,
    mock: bool,
    osc_port: Option<u16>, // None = disabled; Some(port) = OSC listener enabled
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut result = Self {
            no_api: false,
            port: 8765,
            model: None,
            log_level: "info".into(),
            mock: false,
            osc_port: None,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--no-api" => result.no_api = true,
                "--mock" => result.mock = true,
                "--port" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        result.port = v.parse().unwrap_or(8765);
                    }
                }
                "--model" => {
                    i += 1;
                    result.model = args.get(i).cloned();
                }
                "--log" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        result.log_level = v.clone();
                    }
                }
                "--osc" => result.osc_port = Some(57120),
                "--osc-port" => {
                    i += 1;
                    if let Some(v) = args.get(i) {
                        result.osc_port = Some(v.parse().unwrap_or(57120));
                    }
                }
                "--help" | "-h" => {
                    println!("Impulse Instruct — LLM-first audio synthesizer\n");
                    println!("USAGE: impulse-instruct [OPTIONS]\n");
                    println!("OPTIONS:");
                    println!("  --no-api           Disable HTTP/MCP API (default: on, port 8765)");
                    println!("  --port <N>         HTTP port (default: 8765)");
                    println!("  --model <path>     GGUF model path");
                    println!("  --log <level>      Log level (default: info)");
                    println!("  --mock             Run without LLM (mock responses only)");
                    println!("  --osc              Enable OSC input on port 57120 (UDP)");
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
    let args = Args::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&args.log_level))
        .format(|buf, record| {
            use std::io::Write;
            let ts = buf.timestamp_millis();
            // Replace the default ISO8601/UTC 'T'+'Z' format with a plain
            // human-readable timestamp: "2026-04-03 22:00:04.123 INFO ..."
            let ts_str = format!("{}", ts)
                .replace('T', " ")
                .trim_end_matches('Z')
                .to_string();
            writeln!(buf, "{} {:5} {}", ts_str, record.level(), record.args())
        })
        .init();

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
    if let Some(session) = impulse_instruct::state::load_session() {
        impulse_instruct::state::apply_session(&mut app_state.write(), session);
        log::info!("Session restored from session.json");
    }

    // Apply --model override, falling back to the last-used model from session / settings.json
    if let Some(ref model_path) = args.model {
        app_state.write().llm.model_path = model_path.clone();
    } else if app_state.read().llm.model_path.is_empty()
        && let Some(saved) = impulse_instruct::state::load_model_setting()
    {
        app_state.write().llm.model_path = saved;
    }

    // ── Channels ─────────────────────────────────────────────────────────────
    let (llm_tx, llm_rx) = crossbeam_channel::bounded::<LlmInput>(16);
    let (llm_out_tx, llm_out_rx) = crossbeam_channel::bounded::<llm::LlmOutput>(32);

    // ── Audio engine (before LLM thread so we can share tts_tx) ─────────────
    let audio_engine = AudioEngine::new(Arc::clone(&app_state))?;

    // ── LLM thread ────────────────────────────────────────────────────────────
    {
        let state = Arc::clone(&app_state);
        let out_tx = llm_out_tx.clone();
        let mock = args.mock;
        let tts_tx = Arc::clone(&audio_engine.tts_tx);
        std::thread::Builder::new()
            .name("llm".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_llm_loop(state, llm_rx, out_tx, mock, tts_tx))
            .expect("failed to spawn LLM thread");
    }

    // ── HTTP API thread (on by default; disabled by --no-api) ────────────────
    let api_port = if !args.no_api { Some(args.port) } else { None };
    if let Some(port) = api_port {
        let state = Arc::clone(&app_state);
        let llm_tx_http = llm_tx.clone();
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
    };

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
                api_port,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("UI error: {}", e))?;

    Ok(())
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
