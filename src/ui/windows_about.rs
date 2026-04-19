// ─── ui/windows_about.rs ──────────────────────────────────────────────────────
// "About" overlay window — static product + build info.  Split out of
// `windows.rs` so each window owns its own file.
use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    pub(super) fn draw_about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        egui::Window::new("About Impulse Instruct")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("IMPULSE • INSTRUCT")
                            .monospace()
                            .size(16.0)
                            .color(theme::CHALK),
                    );
                    let version = env!("CARGO_PKG_VERSION");
                    ui.label(
                        egui::RichText::new(format!("v{version}"))
                            .monospace()
                            .size(10.0)
                            .color(theme::SMOKE),
                    );
                    ui.label(
                        egui::RichText::new("A synthesizer with AI agents inside it")
                            .monospace()
                            .size(9.0)
                            .color(theme::ASH),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "303 Bass · 808 Kit · 909 Kit · AN1X · Hoover · Noise · Granular",
                        )
                        .monospace()
                        .size(8.0)
                        .color(theme::ASH),
                    );
                    ui.label(
                        egui::RichText::new("LLM: llama.cpp · Gemma 4")
                            .monospace()
                            .size(8.0)
                            .color(theme::ASH),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Rust · egui · cpal · rtrb")
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("↗ github.com/Tok/impulse-instruct")
                                    .monospace()
                                    .size(9.0)
                                    .color(theme::FOG)
                                    .underline(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Open in browser")
                        .clicked()
                    {
                        let _ = super::webbrowser_open("https://github.com/Tok/impulse-instruct");
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Contributions welcome — issues, PRs, feedback")
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                    );
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                });
            });
    }
}
