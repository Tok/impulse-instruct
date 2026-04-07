// ─── ui/util.rs ──────────────────────────────────────────────────────────────
// Small UI utilities: model scanning, browser open.

/// Scan for .gguf model files. Checks:
///   1. models/ relative to CWD (dev: cargo run from repo root)
///   2. models/ next to the binary (dist: user unpacked the release)
///   3. The repo root itself (for convenience when a .gguf is dropped there)
pub(crate) fn scan_models() -> Vec<String> {
    let mut dirs: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from("models")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let sibling = parent.join("models");
        if sibling != std::path::Path::new("models") {
            dirs.push(sibling);
        }
    }
    let mut found: Vec<String> = dirs
        .into_iter()
        .flat_map(|dir| {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "gguf").unwrap_or(false))
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Open a URL in the system browser (cross-platform, no extra dep).
pub(crate) fn webbrowser_open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;
    Ok(())
}
