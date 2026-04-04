// ─── state/persistence.rs ────────────────────────────────────────────────────
// Project save / load — pure I/O, no state mutation.

use super::AppState;

const SETTINGS_PATH: &str = "settings.json";

/// Write the model path to `settings.json` so it persists across restarts.
pub fn save_model_setting(model_path: &str) {
    let json = format!(
        "{{\"model_path\":{}}}\n",
        serde_json::to_string(model_path).unwrap_or_default()
    );
    if let Err(e) = std::fs::write(SETTINGS_PATH, json) {
        log::warn!("Could not write {SETTINGS_PATH}: {e}");
    }
}

/// Read the saved model path from `settings.json`, if it exists and is valid.
pub fn load_model_setting() -> Option<String> {
    let json = std::fs::read_to_string(SETTINGS_PATH).ok()?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let path = v.get("model_path")?.as_str()?.to_string();
    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

/// Serialise `state` to `project-<unix_seconds>.json` in the current directory.
/// Returns the path written on success.
pub fn save_project(state: &AppState) -> Result<std::path::PathBuf, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = std::path::PathBuf::from(format!("project-{}.json", ts));
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serialise error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))?;
    Ok(path)
}

/// Load an `AppState` from a JSON project file.
pub fn load_project(path: &std::path::Path) -> Result<AppState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("parse error: {e}"))
}
