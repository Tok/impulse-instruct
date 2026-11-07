//! Shared, on-demand startup for the optional local speech service.
use crate::state::tts_types::TTS_PORT;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static STARTUP: Mutex<()> = Mutex::new(());

pub fn is_online() -> bool {
    let client: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(250)))
        .build()
        .into();
    client
        .get(&format!("http://127.0.0.1:{TTS_PORT}/health"))
        .call()
        .is_ok_and(|response| response.status().is_success())
}

/// Blocking: call from a worker, never the UI or audio thread.
/// Uses the installed environment; errors remain visible in the application log.
pub fn ensure_server() -> bool {
    let Ok(_guard) = STARTUP.lock() else {
        return false;
    };
    if is_online() {
        return true;
    }
    #[cfg(windows)]
    let python = ".tts-venv/Scripts/python.exe";
    #[cfg(not(windows))]
    let python = ".tts-venv/bin/python";
    if !std::path::Path::new(python).exists() {
        log::warn!("TTS is not installed; run ./scripts/setup-tts.sh to enable the MC");
        return false;
    }
    let mut child = match std::process::Command::new(python)
        .args(["scripts/tts-server.py", "--port", &TTS_PORT.to_string()])
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            log::warn!("TTS startup failed: {error}");
            return false;
        }
    };
    log::info!("TTS: starting local speech server");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(90) {
        if is_online() {
            return true;
        }
        if let Ok(Some(status)) = child.try_wait() {
            log::warn!("TTS server exited during startup: {status}");
            return false;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = child.kill();
    let _ = child.wait();
    log::warn!("TTS server did not become ready within 90 seconds");
    false
}
