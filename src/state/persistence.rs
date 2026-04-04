// ─── state/persistence.rs ────────────────────────────────────────────────────
// Project save / load — pure I/O, no state mutation.

use super::AppState;

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
