// ─── main.rs ─────────────────────────────────────────────────────────────────
// Impulse Instruct — LLM-first audio synthesizer
// Thread model:
//   Main/UI:  eframe runs here (required by macOS/Windows)
//   Audio:    cpal callback (real-time, elevated by OS)
//   LLM:      std::thread (blocking inference)
//   HTTP:     tokio runtime in separate OS thread

mod api;
mod audio;
mod llm;
mod midi;
mod sequencer;
mod state;
mod ui;

use parking_lot::RwLock;
use std::sync::Arc;

use audio::AudioEngine;
use llm::{LlmInput, run_llm_loop};
use state::AppState;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Impulse Instruct starting…");

    // ── Shared state ──────────────────────────────────────────────────────────
    let app_state = Arc::new(RwLock::new(AppState::default()));

    // ── Channels ─────────────────────────────────────────────────────────────
    let (llm_tx, llm_rx) = crossbeam_channel::bounded::<LlmInput>(16);
    let (llm_out_tx, llm_out_rx) = crossbeam_channel::bounded::<llm::LlmOutput>(32);

    // ── LLM thread ────────────────────────────────────────────────────────────
    {
        let state = Arc::clone(&app_state);
        let out_tx = llm_out_tx.clone();
        std::thread::Builder::new()
            .name("llm".into())
            .stack_size(8 * 1024 * 1024) // 8MB stack for inference
            .spawn(move || run_llm_loop(state, llm_rx, out_tx))
            .expect("failed to spawn LLM thread");
    }

    // ── HTTP API thread ───────────────────────────────────────────────────────
    {
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
                    if let Err(e) = api::run_server(api_state, 8765).await {
                        log::error!("HTTP server error: {}", e);
                    }
                });
            })
            .expect("failed to spawn HTTP thread");
    }

    // ── Audio engine ──────────────────────────────────────────────────────────
    let audio_engine = AudioEngine::new(Arc::clone(&app_state))?;

    // ── UI (blocks this thread) ───────────────────────────────────────────────
    let state_for_ui = Arc::clone(&app_state);
    let audio_tx = audio_engine.params_tx;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Impulse Instruct")
            .with_inner_size([1100.0, 720.0])
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
                llm_tx,
                llm_out_rx,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("UI error: {}", e))?;

    Ok(())
}
