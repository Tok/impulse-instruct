// ─── model_eval/main.rs ───────────────────────────────────────────────────────
// Model quality evaluation: tests each GGUF against style criteria and prints
// a comparison table so you can see which models pass which styles.
//
// Usage:
//   cargo run --bin model_eval                        # scan models/ directory
//   cargo run --bin model_eval -- --models models/qwen3.gguf,models/bonsai.gguf
//   cargo run --bin model_eval -- --styles acid,rave,ambient
//   cargo run --bin model_eval -- --json              # machine-readable output
//   cargo run --bin model_eval -- --timeout 120       # seconds per inference
//
// NOTE: Close the main app first — both share llama-server port 8766.

mod specs;

use impulse_instruct::llm::styles::StyleCatalog;
use impulse_instruct::llm::{LlamaServerBackend, LlmBackend, SamplingParams, build_system_prompt};
use impulse_instruct::state::{AppState, apply_llm_update};
use specs::{CheckResult, StyleSpec, build_style_specs};
use std::time::Instant;

// ─── Result types ─────────────────────────────────────────────────────────────

struct StyleResult {
    style_id: String,
    check_results: Vec<(&'static str, CheckResult)>,
    inference_ms: u64,
    json_valid: bool,
}

struct ModelResult {
    model_name: String,
    style_results: Vec<StyleResult>,
    total_ms: u64,
}

impl ModelResult {
    fn passes(&self) -> usize {
        self.style_results
            .iter()
            .flat_map(|r| r.check_results.iter().map(|(_, c)| c.is_pass()))
            .filter(|&p| p)
            .count()
    }
    fn total_checks(&self) -> usize {
        self.style_results
            .iter()
            .map(|r| r.check_results.len())
            .sum()
    }
    fn score_pct(&self) -> f32 {
        let t = self.total_checks();
        if t == 0 {
            0.0
        } else {
            self.passes() as f32 / t as f32 * 100.0
        }
    }
}

// ─── Core eval loop ──────────────────────────────────────────────────────────

fn run_model(model_path: &str, specs: &[&StyleSpec], timeout_secs: u64) -> ModelResult {
    let model_name = std::path::Path::new(model_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(model_path)
        .to_string();

    eprintln!("\n▶  Model: {model_name}");
    eprintln!("   Loading server…");

    let mut backend = LlamaServerBackend::new(model_path, 32768, 8766);
    let t_start = Instant::now();
    let mut style_results = Vec::new();

    for spec in specs {
        let catalog = StyleCatalog::get();
        let style = match catalog.find_by_id(spec.id) {
            Some(s) => s,
            None => {
                eprintln!("   ⚠  style '{}' not found in catalog — skipping", spec.id);
                continue;
            }
        };

        eprint!("   {:<22}", spec.id);

        // Apply baseline params to a fresh state so the model starts from a clean slate.
        let base_state = {
            let mut s = AppState::default();
            s.llm.active_style = Some(spec.id.to_string());
            if let Some(bp) = &style.baseline_params {
                s = apply_llm_update(s, bp, &[]);
            }
            s
        };

        let system = build_system_prompt(&base_state, &[]);

        // Override the inference timeout so a slow model doesn't hang forever.
        let _ = timeout_secs; // LlamaServerBackend uses its own INFER_TIMEOUT_SECS constant;
        // we just track elapsed time ourselves.

        let t_inf = Instant::now();
        let sampling = SamplingParams {
            heat: 0.1,
            ..SamplingParams::default()
        };
        let output = backend.infer(&system, spec.prompt, &sampling); // heat=0.1 → near-deterministic
        let inference_ms = t_inf.elapsed().as_millis() as u64;

        let (final_state, json_valid) = match output {
            Ok(out) => {
                if let Some(ref update) = out.param_update {
                    let s = apply_llm_update(base_state, update, &[]);
                    (s, true)
                } else {
                    (base_state, false)
                }
            }
            Err(e) => {
                eprintln!("  ERROR: {e}");
                (base_state, false)
            }
        };

        let check_results: Vec<(&'static str, CheckResult)> = spec
            .checks
            .iter()
            .map(|c| {
                let result = if json_valid {
                    (c.eval)(&final_state)
                } else {
                    CheckResult::Skip("inference failed".into())
                };
                (c.name, result)
            })
            .collect();

        let passes = check_results.iter().filter(|(_, r)| r.is_pass()).count();
        let total = check_results.len();
        let symbols: String = check_results
            .iter()
            .map(|(_, r)| r.symbol())
            .collect::<Vec<_>>()
            .join("");
        eprintln!("  {passes}/{total}  [{symbols}]  ({inference_ms}ms)");

        style_results.push(StyleResult {
            style_id: spec.id.to_string(),
            check_results,
            inference_ms,
            json_valid,
        });
    }

    let total_ms = t_start.elapsed().as_millis() as u64;
    backend.shutdown();

    ModelResult {
        model_name,
        style_results,
        total_ms,
    }
}

// ─── Report rendering ─────────────────────────────────────────────────────────

fn print_ascii_table(results: &[ModelResult], specs: &[&StyleSpec]) {
    let col_w = 12usize;
    let style_w = 22usize;

    // Header
    println!();
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  IMPULSE INSTRUCT — MODEL STYLE QUALITY EVALUATION               ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();

    // Column headers
    print!("{:<style_w$}", "Style");
    for r in results {
        let name = if r.model_name.len() > col_w {
            &r.model_name[..col_w]
        } else {
            &r.model_name
        };
        print!("  {name:<col_w$}");
    }
    println!();

    print!("{:<style_w$}", "─".repeat(style_w));
    for _ in results {
        print!("  {}", "─".repeat(col_w));
    }
    println!();

    // Per-style rows
    for spec in specs.iter() {
        print!("{:<style_w$}", spec.id);
        for model in results {
            let sr = model.style_results.iter().find(|r| r.style_id == spec.id);
            let cell = match sr {
                None => format!("{:>col_w$}", "–"),
                Some(sr) => {
                    let passes = sr.check_results.iter().filter(|(_, r)| r.is_pass()).count();
                    let total = sr.check_results.len();
                    let bar_filled = if total > 0 { (passes * 5) / total } else { 0 };
                    let bar: String = "█".repeat(bar_filled) + &"░".repeat(5 - bar_filled);
                    format!("{bar} {passes}/{total}")
                }
            };
            print!("  {cell:<col_w$}");
        }
        println!();
    }

    // Separator
    print!("{:<style_w$}", "─".repeat(style_w));
    for _ in results {
        print!("  {}", "─".repeat(col_w));
    }
    println!();

    // Totals row
    print!("{:<style_w$}", "TOTAL");
    for model in results {
        let p = model.passes();
        let t = model.total_checks();
        let pct = model.score_pct();
        let cell = format!("{p}/{t} ({pct:.0}%)");
        print!("  {cell:<col_w$}");
    }
    println!();

    // Time row
    print!("{:<style_w$}", "Time");
    for model in results {
        let secs = model.total_ms / 1000;
        let cell = format!("{secs}s");
        print!("  {cell:<col_w$}");
    }
    println!();

    println!();

    // Failures detail
    let mut any_failures = false;
    for model in results {
        for sr in &model.style_results {
            for (check_name, result) in &sr.check_results {
                if let Some(detail) = result.detail() {
                    if !any_failures {
                        println!("FAILURES:");
                        any_failures = true;
                    }
                    let symbol = result.symbol();
                    println!(
                        "  {symbol}  {}  /  {}  /  {}:  {}",
                        model.model_name, sr.style_id, check_name, detail
                    );
                }
            }
        }
    }
    if any_failures {
        println!();
    }
}

fn print_json(results: &[ModelResult]) {
    let out: Vec<serde_json::Value> = results
        .iter()
        .map(|m| {
            let styles: Vec<serde_json::Value> = m
                .style_results
                .iter()
                .map(|sr| {
                    let checks: Vec<serde_json::Value> = sr
                        .check_results
                        .iter()
                        .map(|(name, r)| {
                            serde_json::json!({
                                "name": name,
                                "pass": r.is_pass(),
                                "detail": r.detail().unwrap_or(""),
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "style": sr.style_id,
                        "json_valid": sr.json_valid,
                        "inference_ms": sr.inference_ms,
                        "passes": sr.check_results.iter().filter(|(_, r)| r.is_pass()).count(),
                        "total": sr.check_results.len(),
                        "checks": checks,
                    })
                })
                .collect();
            serde_json::json!({
                "model": m.model_name,
                "total_ms": m.total_ms,
                "passes": m.passes(),
                "total_checks": m.total_checks(),
                "score_pct": m.score_pct(),
                "styles": styles,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}

// ─── CLI ─────────────────────────────────────────────────────────────────────

struct Args {
    model_paths: Vec<String>,
    style_filter: Vec<String>,
    json_output: bool,
    timeout_secs: u64,
}

impl Args {
    fn parse() -> Self {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut model_paths: Vec<String> = Vec::new();
        let mut style_filter: Vec<String> = Vec::new();
        let mut json_output = false;
        let mut timeout_secs = 180u64;

        let mut i = 0;
        while i < raw.len() {
            match raw[i].as_str() {
                "--models" => {
                    i += 1;
                    if let Some(v) = raw.get(i) {
                        for p in v.split(',') {
                            model_paths.push(p.trim().to_string());
                        }
                    }
                }
                "--styles" => {
                    i += 1;
                    if let Some(v) = raw.get(i) {
                        style_filter = v.split(',').map(|s| s.trim().to_string()).collect();
                    }
                }
                "--json" => json_output = true,
                "--timeout" => {
                    i += 1;
                    if let Some(v) = raw.get(i) {
                        timeout_secs = v.parse().unwrap_or(180);
                    }
                }
                "--help" | "-h" => {
                    println!("model_eval — test GGUF models against style quality criteria\n");
                    println!("USAGE:");
                    println!("  cargo run --bin model_eval [-- OPTIONS]\n");
                    println!("OPTIONS:");
                    println!(
                        "  --models <p1,p2>   Comma-separated GGUF paths (default: scan models/)"
                    );
                    println!(
                        "  --styles <s1,s2>   Comma-separated style IDs to test (default: all)"
                    );
                    println!("  --json             Output machine-readable JSON");
                    println!(
                        "  --timeout <secs>   Per-inference timeout in seconds (default: 180)"
                    );
                    std::process::exit(0);
                }
                _ => {}
            }
            i += 1;
        }

        // Default: scan models/ directory for .gguf files
        if model_paths.is_empty()
            && let Ok(entries) = std::fs::read_dir("models")
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    model_paths.push(p.to_string_lossy().to_string());
                }
            }
            model_paths.sort();
        }

        Self {
            model_paths,
            style_filter,
            json_output,
            timeout_secs,
        }
    }
}

// ─── main ────────────────────────────────────────────────────────────────────

fn main() {
    // Suppress library log output — we control our own progress display.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();

    let args = Args::parse();

    if args.model_paths.is_empty() {
        eprintln!("No models found. Put .gguf files in models/ or pass --models <path,...>");
        eprintln!("Download with: ./download-models.sh");
        std::process::exit(1);
    }

    let all_specs = build_style_specs();
    let specs: Vec<_> = if args.style_filter.is_empty() {
        all_specs.iter().collect()
    } else {
        all_specs
            .iter()
            .filter(|s| args.style_filter.iter().any(|f| s.id.contains(f.as_str())))
            .collect()
    };

    if specs.is_empty() {
        eprintln!(
            "No matching styles found. Available: {}",
            all_specs
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::process::exit(1);
    }

    eprintln!("═══════════════════════════════════════════");
    eprintln!(
        "  model_eval  — {} models, {} styles",
        args.model_paths.len(),
        specs.len()
    );
    eprintln!("═══════════════════════════════════════════");
    eprintln!("  NOTE: ensure the main app is closed first (port 8766 conflict)");
    eprintln!();

    let mut results: Vec<ModelResult> = Vec::new();

    for model_path in &args.model_paths {
        if !std::path::Path::new(model_path).exists() {
            eprintln!("⚠  Model not found: {model_path} — skipping");
            continue;
        }
        let result = run_model(model_path, &specs, args.timeout_secs);
        results.push(result);
    }

    if results.is_empty() {
        eprintln!("No models could be loaded.");
        std::process::exit(1);
    }

    if args.json_output {
        print_json(&results);
    } else {
        print_ascii_table(&results, &specs);
    }
}
