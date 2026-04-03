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
//
// Thread model:
//   Main/UI:  eframe runs here (required by macOS/Windows)
//   Audio:    cpal callback (real-time, elevated by OS)
//   LLM:      std::thread (blocking inference, spawns llama-server subprocess)
//   HTTP:     tokio runtime in separate OS thread (disabled by --no-api)

mod api;
mod audio;
mod banner;
mod export;
mod llm;
mod midi;
mod sequencer;
mod state;
mod sysinfo;
mod ui;
#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "llm-tests"))]
mod llm_suite;

use midi::MidiEvent;

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
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut result = Self {
            no_api: false,
            port: 8765,
            model: None,
            log_level: "info".into(),
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--no-api" => result.no_api = true,
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
                "--help" | "-h" => {
                    println!("Impulse Instruct — LLM-first audio synthesizer\n");
                    println!("USAGE: impulse-instruct [OPTIONS]\n");
                    println!("OPTIONS:");
                    println!("  --no-api           Disable HTTP/MCP API (default: on, port 8765)");
                    println!("  --port <N>         HTTP port (default: 8765)");
                    println!("  --model <path>     GGUF model path");
                    println!("  --log <level>      Log level (default: info)");
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

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(&args.log_level)
    ).init();

    crate::banner::print_banner();

    log::info!("Impulse Instruct starting…");
    if !args.no_api {
        log::info!("HTTP API enabled on port {} (pass --no-api to disable)", args.port);
    }

    // ── Shared state ──────────────────────────────────────────────────────────
    let app_state = Arc::new(RwLock::new(AppState::default()));

    // Apply --model override
    if let Some(ref model_path) = args.model {
        app_state.write().llm.model_path = model_path.clone();
    }

    // ── Channels ─────────────────────────────────────────────────────────────
    let (llm_tx, llm_rx) = crossbeam_channel::bounded::<LlmInput>(16);
    let (llm_out_tx, llm_out_rx) = crossbeam_channel::bounded::<llm::LlmOutput>(32);

    // ── LLM thread ────────────────────────────────────────────────────────────
    {
        let state = Arc::clone(&app_state);
        let out_tx = llm_out_tx.clone();
        std::thread::Builder::new()
            .name("llm".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_llm_loop(state, llm_rx, out_tx))
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

    // ── Audio engine ──────────────────────────────────────────────────────────
    let audio_engine = AudioEngine::new(Arc::clone(&app_state))?;

    // ── MIDI input ────────────────────────────────────────────────────────────
    let (midi_tx, midi_rx) = crossbeam_channel::bounded::<MidiEvent>(256);
    let (_midi_listener, midi_port) = midi::MidiListener::auto_connect(midi_tx);
    if let Some(ref name) = midi_port {
        log::info!("MIDI: connected to '{}'", name);
    }

    // ── UI (blocks this thread) ───────────────────────────────────────────────
    let state_for_ui = Arc::clone(&app_state);
    let audio_tx = audio_engine.params_tx;
    let scope_rx = audio_engine.scope_rx;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Impulse Instruct")
            .with_maximized(true)
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
