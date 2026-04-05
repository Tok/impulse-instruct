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

fn main() -> anyhow::Result<()> {
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

    // Apply --model override, falling back to the last-used model from settings.json
    if let Some(ref model_path) = args.model {
        app_state.write().llm.model_path = model_path.clone();
    } else if let Some(saved) = impulse_instruct::state::load_model_setting() {
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
    let audio_tx = audio_engine.params_tx;
    let scope_rx = audio_engine.scope_rx;

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
                audio_tx,
                scope_rx,
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

/// Generate the window icon pixel buffer — one Huth-colored octave on dark background.
/// Mirrors docs/icon.svg: 256×256, white keys (28×144 px) + black keys (17×90 px).
fn make_window_icon() -> egui::IconData {
    const W: u32 = 256;
    const H: u32 = 256;
    let mut rgba = vec![0u8; (W * H * 4) as usize];
    // Background: #0C0C0C opaque
    for chunk in rgba.chunks_mut(4) {
        chunk[0] = 12;
        chunk[1] = 12;
        chunk[2] = 12;
        chunk[3] = 255;
    }

    let mut fill = |x0: u32, y0: u32, w: u32, h: u32, r: u8, g: u8, b: u8| {
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

    // Translate: x_start=27, y_start=56 (same as SVG group)
    let tx: u32 = 27;
    let ty: u32 = 56;

    // White keys: stride=29, width=28, height=144
    let white = [
        (0u32, 0x33u8, 0x66u8, 0xDDu8), // C
        (29, 0x33, 0xAA, 0x66),         // D
        (58, 0xDD, 0xCC, 0x22),         // E
        (87, 0xEE, 0x88, 0x22),         // F
        (116, 0xEE, 0x33, 0x66),        // G
        (145, 0x99, 0x66, 0xCC),        // A
        (174, 0x44, 0x33, 0xAA),        // B
    ];
    for (rx, r, g, b) in white {
        fill(tx + rx, ty, 28, 144, r, g, b);
    }

    // Black keys: width=17, height=90, centered in white-key gaps
    let black = [
        (20u32, 0x22u8, 0x99u8, 0xBBu8), // C#
        (49, 0x88, 0xCC, 0x22),          // D#
        (107, 0xDD, 0x44, 0x22),         // F#
        (136, 0xCC, 0x11, 0x44),         // G#
        (165, 0x77, 0x44, 0xBB),         // A#
    ];
    for (rx, r, g, b) in black {
        fill(tx + rx, ty, 17, 90, r, g, b);
    }

    egui::IconData {
        rgba,
        width: W,
        height: H,
    }
}
