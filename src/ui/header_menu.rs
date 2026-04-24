// ─── ui/header_menu.rs ───────────────────────────────────────────────────────
// File / Edit / View / Help menu bar + its `load_project_from_path`
// helper.  Extracted from `header.rs` (which was at the 1000-line cap)
// so the main header module can stay focused on the transport strip
// and log/scope panel.

use egui::{Frame, TopBottomPanel};
use std::path::{Path, PathBuf};

use crate::export::{export_mp3, export_stems, export_wav};
use crate::midi::{MidiImport, import_midi_file, save_midi_export_to};
use crate::state::{load_project, save_project};
use crate::ui::header::list_recent_projects_in;
use crate::ui::{ImpulseApp, theme};

fn list_recent_projects() -> Vec<PathBuf> {
    list_recent_projects_in(Path::new("."))
}

/// Process-global tokio runtime dedicated to `rfd` file-picker calls.
/// Kept alive for the lifetime of the app so ashpd's cached D-Bus
/// connection (and zbus's executor state) survives across successive
/// picker invocations — a fresh per-call runtime hangs the second Import
/// MIDI click because the zbus proxy established on the first call is
/// tied to a runtime that's already been dropped.
///
/// Multi-thread runtime (not current-thread) because ashpd internally
/// issues `tokio::spawn_blocking`, which needs a real blocking-pool,
/// not just a single-threaded event loop.
fn picker_runtime() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("rfd-picker")
            .build()
            .expect("failed to build rfd picker tokio runtime")
    })
}

/// Open a native file-picker using `rfd`'s xdg-portal backend.
/// `rfd`'s sync `.pick_file()` panics on the egui UI thread because the
/// portal backend calls `tokio::spawn_blocking` via ashpd and the UI
/// thread has no tokio runtime.  We drive the async API on the shared
/// picker runtime (see `picker_runtime` above) instead.
///
/// `block_on` here blocks the UI until the user picks a file or cancels —
/// matches the behaviour of `rfd::FileDialog::pick_file()` that the rest
/// of the menu already depends on.  A picker handle on the worker
/// thread closes the dialog automatically if the future is dropped.
pub(crate) fn pick_file_via_portal(filter_name: &str, extensions: &[&str]) -> Option<PathBuf> {
    let filter_name = filter_name.to_string();
    let extensions: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
    picker_runtime().block_on(async move {
        let exts: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        rfd::AsyncFileDialog::new()
            .add_filter(&filter_name, &exts)
            .set_directory(".")
            .pick_file()
            .await
            .map(|h| h.path().to_path_buf())
    })
}

/// Save-file counterpart to `pick_file_via_portal`.  Same rationale —
/// the portal backend needs a long-lived tokio reactor.
fn save_file_via_portal(
    filter_name: &str,
    extensions: &[&str],
    default_name: &str,
) -> Option<PathBuf> {
    let filter_name = filter_name.to_string();
    let extensions: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
    let default_name = default_name.to_string();
    picker_runtime().block_on(async move {
        let exts: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        rfd::AsyncFileDialog::new()
            .add_filter(&filter_name, &exts)
            .set_directory(".")
            .set_file_name(&default_name)
            .save_file()
            .await
            .map(|h| h.path().to_path_buf())
    })
}

impl ImpulseApp {
    /// Shared helper: load a project JSON from `path`, swap it into the
    /// live state, push audio params, and log success/failure.  Called
    /// from "Load latest", "Recent projects → <name>", and "Open…".
    pub(super) fn load_project_from_path(&mut self, path: &Path) {
        match load_project(path) {
            Ok(loaded) => {
                *self.state.write() = loaded;
                self.push_audio_params();
                let msg = format!("[ loaded ← {} ]", path.display());
                log::info!("{}", msg);
                self.log_text.push_str(&format!("{}\n", msg));
            }
            Err(e) => {
                let msg = format!("[ load failed: {} ]", e);
                log::error!("{}", msg);
                self.log_text.push_str(&format!("{}\n", msg));
            }
        }
    }

    pub(super) fn draw_menu_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("menu_bar")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(4.0, 2.0)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(egui::RichText::new("File").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("New project").monospace().size(10.0))
                            .clicked()
                        {
                            // Re-open the wizard and forget the wizard_done
                            // flag so the user picks a fresh preset.
                            self.show_wizard = true;
                            // Snapshot keeps current rack/style intact until
                            // the user actually applies a preset, mirroring
                            // wizard "resume" behaviour.
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Open project…").monospace().size(10.0))
                            .clicked()
                        {
                            // Native file dialog via rfd.  Blocks the UI
                            // thread briefly while the picker is open —
                            // fine for a file-menu action; matches the
                            // pattern used for Save elsewhere.
                            let picked = rfd::FileDialog::new()
                                .add_filter("Project", &["json"])
                                .set_directory(".")
                                .pick_file();
                            if let Some(path) = picked {
                                self.load_project_from_path(&path);
                            }
                            ui.close_menu();
                        }
                        if ui
                            .button(
                                egui::RichText::new("Load latest project")
                                    .monospace()
                                    .size(10.0),
                            )
                            .clicked()
                        {
                            // Quick-load shortcut: newest project-*.json in cwd.
                            match list_recent_projects().into_iter().next() {
                                Some(path) => {
                                    self.load_project_from_path(&path);
                                }
                                None => {
                                    let msg = "[ no project-*.json found in working dir ]";
                                    log::warn!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }
                        // Recent projects sub-menu — populated from the cwd
                        // scan so each entry round-trips to the same load
                        // helper.  Caps at 10 so a cwd full of saves
                        // doesn't blow out the menu height.
                        let recent = list_recent_projects();
                        ui.menu_button(
                            egui::RichText::new(format!(
                                "Recent projects ({})",
                                recent.len().min(10)
                            ))
                            .monospace()
                            .size(10.0),
                            |ui| {
                                if recent.is_empty() {
                                    ui.label(
                                        egui::RichText::new("— none —")
                                            .monospace()
                                            .size(10.0)
                                            .color(theme::SMOKE),
                                    );
                                    return;
                                }
                                let mut pick: Option<PathBuf> = None;
                                for path in recent.iter().take(10) {
                                    let label = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| path.display().to_string());
                                    if ui
                                        .button(egui::RichText::new(label).monospace().size(10.0))
                                        .clicked()
                                    {
                                        pick = Some(path.clone());
                                    }
                                }
                                if let Some(p) = pick {
                                    self.load_project_from_path(&p);
                                    ui.close_menu();
                                }
                            },
                        );
                        if ui
                            .button(egui::RichText::new("Save project").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            match save_project(&snapshot) {
                                Ok(path) => {
                                    let msg = format!("[ saved → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ save failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        if ui
                            .button(egui::RichText::new("Export WAV").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_wav(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        if ui
                            .button(egui::RichText::new("Export MP3").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_mp3(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export: {} ]", e);
                                    log::warn!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        if ui
                            .button(egui::RichText::new("Export Stems").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            let ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let prefix = format!("stems-{}", ts);
                            match export_stems(&snapshot, bars, &prefix) {
                                Ok(paths) => {
                                    let msg = format!(
                                        "[ stems → {} files ({}) ]",
                                        paths.len(),
                                        paths
                                            .iter()
                                            .filter_map(|p| p.file_name())
                                            .map(|n| n.to_string_lossy())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    );
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ stems failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        if ui
                            .button(egui::RichText::new("Import MIDI…").monospace().size(10.0))
                            .clicked()
                        {
                            let picked = pick_file_via_portal("MIDI", &["mid", "midi"]);
                            if let Some(path) = picked {
                                let current = self.state.read().clone();
                                match import_midi_file(current, &path, &MidiImport::default()) {
                                    Ok((new_state, summary)) => {
                                        *self.state.write() = new_state;
                                        self.push_audio_params();
                                        let trunc = if summary.was_truncated { " (truncated)" } else { "" };
                                        let msg = format!(
                                            "[ imported ← {} — {} BPM, /{} grid, {} banks, v0={} v1={}{} ]",
                                            path.display(),
                                            summary.bpm.round() as i32,
                                            summary.step_division,
                                            summary.banks_used,
                                            summary.notes_voice_0,
                                            summary.notes_voice_1,
                                            trunc,
                                        );
                                        log::info!("{}", msg);
                                        self.log_text.push_str(&format!("{}\n", msg));
                                    }
                                    Err(e) => {
                                        let msg = format!("[ MIDI import failed: {} ]", e);
                                        log::error!("{}", msg);
                                        self.log_text.push_str(&format!("{}\n", msg));
                                    }
                                }
                            }
                            ui.close_menu();
                        }

                        if ui
                            .button(egui::RichText::new("Export MIDI…").monospace().size(10.0))
                            .clicked()
                        {
                            let picked = save_file_via_portal("MIDI", &["mid"], "pattern.mid");
                            if let Some(path) = picked {
                                let snapshot = self.state.read().clone();
                                match save_midi_export_to(&snapshot.sequencer, Some(&path)) {
                                    Ok(p) => {
                                        let msg = format!("[ midi → {} ]", p.display());
                                        log::info!("{}", msg);
                                        self.log_text.push_str(&format!("{}\n", msg));
                                    }
                                    Err(e) => {
                                        let msg = format!("[ midi export failed: {} ]", e);
                                        log::error!("{}", msg);
                                        self.log_text.push_str(&format!("{}\n", msg));
                                    }
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        if ui
                            .button(egui::RichText::new("Quit").monospace().size(10.0))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(egui::RichText::new("Edit").monospace().size(10.0), |ui| {
                        let can_undo = self.history.can_undo();
                        let can_redo = self.history.can_redo();
                        if ui
                            .add_enabled(
                                can_undo,
                                egui::Button::new(
                                    egui::RichText::new("Undo  Ctrl+Z").monospace().size(10.0),
                                ),
                            )
                            .clicked()
                        {
                            let current = self.state.read().clone();
                            if let Some(prev) = self.history.undo(current) {
                                *self.state.write() = prev;
                                self.push_audio_params();
                            }
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                can_redo,
                                egui::Button::new(
                                    egui::RichText::new("Redo  Ctrl+Y").monospace().size(10.0),
                                ),
                            )
                            .clicked()
                        {
                            let current = self.state.read().clone();
                            if let Some(next) = self.history.redo(current) {
                                *self.state.write() = next;
                                self.push_audio_params();
                            }
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(egui::RichText::new("View").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("Compact All").monospace().size(10.0))
                            .clicked()
                        {
                            for m in &self.state.read().rack.modules {
                                self.module_scales.insert(m.kind, 0.6);
                            }
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Expand All").monospace().size(10.0))
                            .clicked()
                        {
                            self.module_scales.clear();
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Arrange").monospace().size(10.0))
                            .clicked()
                        {
                            self.state.write().rack.arrange_canonical();
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Reset Layout").monospace().size(10.0))
                            .clicked()
                        {
                            self.module_scales.clear();
                            self.state.write().rack.arrange_canonical();
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(egui::RichText::new("Help").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("Preferences…").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_prefs = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("System…").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_sysinfo = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new("About").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_about = true;
                            ui.close_menu();
                        }
                    });
                });
            });
    }
}
