// ─── sysinfo.rs ──────────────────────────────────────────────────────────────
// Background system info poller: GPU VRAM, RAM, driver version.
// Runs in its own thread, updates shared state every few seconds.

use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct SysInfo {
    pub gpu_name: String,
    pub driver_version: String,
    pub cuda_version: String,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

/// Spawn a background thread that polls system info every `interval_secs`
/// and writes the result into `shared`. Errors are silently ignored —
/// the struct stays at its default zero values on unsupported systems.
pub fn spawn_poller(shared: Arc<Mutex<SysInfo>>, interval_secs: u64) {
    std::thread::Builder::new()
        .name("sysinfo".into())
        .spawn(move || {
            loop {
                let info = poll_once();
                if let Ok(mut guard) = shared.lock() {
                    *guard = info;
                }
                std::thread::sleep(Duration::from_secs(interval_secs));
            }
        })
        .ok();
}

fn poll_once() -> SysInfo {
    let mut info = SysInfo::default();

    // ── GPU: name, driver, VRAM ──────────────────────────────────────────────
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let parts: Vec<&str> = text
            .lines()
            .next()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .collect();
        if parts.len() >= 4 {
            info.gpu_name = parts[0].to_string();
            info.driver_version = parts[1].to_string();
            info.vram_used_mb = parts[2].parse().unwrap_or(0);
            info.vram_total_mb = parts[3].parse().unwrap_or(0);
        }
    }

    // ── CUDA version ─────────────────────────────────────────────────────────
    // nvidia-smi reports it in the top-right of its default output; easier to
    // grab from nvcc if present, or fall back to the smi `cuda_version` field.
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
        .output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let cc = text.trim().to_string();
        if !cc.is_empty() {
            info.cuda_version = format!("sm_{}", cc.replace('.', ""));
        }
    }
    // Try nvcc --version for the actual CUDA toolkit version (overrides sm_xx)
    if let Ok(out) = std::process::Command::new("nvcc").arg("--version").output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        // Line like: "Cuda compilation tools, release 12.4, V12.4.131"
        for line in text.lines() {
            if line.contains("release")
                && let Some(start) = line.find("release ")
            {
                let rest = &line[start + 8..];
                let ver = rest.split(',').next().unwrap_or("").trim();
                if !ver.is_empty() {
                    info.cuda_version = format!("CUDA {}", ver);
                }
            }
        }
    }

    // ── RAM from /proc/meminfo ────────────────────────────────────────────────
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        let mut total_kb: u64 = 0;
        let mut available_kb: u64 = 0;
        for line in contents.lines() {
            let mut parts = line.split_whitespace();
            match parts.next() {
                Some("MemTotal:") => {
                    total_kb = parts.next().unwrap_or("0").parse().unwrap_or(0);
                }
                Some("MemAvailable:") => {
                    available_kb = parts.next().unwrap_or("0").parse().unwrap_or(0);
                }
                _ => {}
            }
        }
        info.ram_total_mb = total_kb / 1024;
        info.ram_used_mb = info.ram_total_mb.saturating_sub(available_kb / 1024);
    }

    info
}

/// Format MB as "X.XG" if ≥ 1024, else "XXXXXM".
pub fn fmt_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1}G", mb as f64 / 1024.0)
    } else {
        format!("{}M", mb)
    }
}
