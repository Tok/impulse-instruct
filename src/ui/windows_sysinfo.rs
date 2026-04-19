// ─── ui/windows_sysinfo.rs ────────────────────────────────────────────────────
// System-info overlay window — live VRAM / RAM / CPU readouts.  Split out
// of `windows.rs` so each window owns its own file.
use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    pub(super) fn draw_sysinfo_window(&mut self, ctx: &egui::Context) {
        if !self.show_sysinfo {
            return;
        }
        let si = self
            .sys_info
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default();
        egui::Window::new("System Info")
            .collapsible(false)
            .resizable(false)
            .min_width(320.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let row = |ui: &mut egui::Ui, label: &str, val: &str| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.label(
                            egui::RichText::new(val)
                                .color(theme::FOG)
                                .monospace()
                                .size(9.5),
                        );
                    });
                };

                ui.label(
                    egui::RichText::new("GPU")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();
                if si.gpu_name.is_empty() {
                    row(ui, "GPU:    ", "nvidia-smi not found or no NVIDIA GPU");
                } else {
                    row(ui, "Name:   ", &si.gpu_name);
                    row(ui, "Driver: ", &si.driver_version);
                    if !si.cuda_version.is_empty() {
                        row(ui, "CUDA:   ", &si.cuda_version);
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let frac = if si.vram_total_mb > 0 {
                            si.vram_used_mb as f32 / si.vram_total_mb as f32
                        } else {
                            0.0
                        };
                        ui.label(
                            egui::RichText::new("VRAM:   ")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac));
                        ui.label(
                            egui::RichText::new(format!(
                                "  {} / {}  ({:.0}%)",
                                crate::sysinfo::fmt_mb(si.vram_used_mb),
                                crate::sysinfo::fmt_mb(si.vram_total_mb),
                                frac * 100.0
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("System Memory")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();
                if si.ram_total_mb > 0 {
                    ui.horizontal(|ui| {
                        let frac = si.ram_used_mb as f32 / si.ram_total_mb as f32;
                        ui.label(
                            egui::RichText::new("RAM:    ")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac));
                        ui.label(
                            egui::RichText::new(format!(
                                "  {} / {}  ({:.0}%)",
                                crate::sysinfo::fmt_mb(si.ram_used_mb),
                                crate::sysinfo::fmt_mb(si.ram_total_mb),
                                frac * 100.0
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                } else {
                    row(ui, "RAM:    ", "/proc/meminfo not available");
                }

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Updated every 3 seconds")
                        .color(theme::IRON)
                        .monospace()
                        .size(8.5),
                );
                ui.add_space(4.0);
                if ui.button("Close").clicked() {
                    self.show_sysinfo = false;
                }
            });
    }
}
