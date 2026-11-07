//! Compact musical families; only the selected family's variations are shown.
use super::{ImpulseApp, theme};

const FAMILIES: &[(&str, &[&str])] = &[
    ("Acid", &["acid_classic", "acid_house", "acid_techno"]),
    (
        "Techno",
        &[
            "detroit_techno",
            "berlin_techno",
            "dub_techno",
            "minimal_techno",
        ],
    ),
    ("House", &["chicago_house", "deep_house", "ambient_house"]),
    ("Breaks", &["jungle", "drum_and_bass", "breakcore"]),
    ("Bass & electro", &["uk_garage", "dubstep", "electro"]),
    ("Rave", &["trance", "gabber", "early_rave"]),
    ("Retro", &["synthwave", "darksynth", "vaporwave"]),
    (
        "Ambient",
        &[
            "ambient_techno",
            "dark_ambient",
            "space_ambient",
            "meditation",
        ],
    ),
    ("Hip-hop", &["lo_fi_hip_hop"]),
    ("Experimental", &["idm"]),
    ("Classical", &["baroque_bach"]),
];

fn choice(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.0).color(if selected {
            theme::VOID
        } else {
            theme::CHALK
        }))
        .fill(if selected { theme::CHALK } else { theme::PIT })
        .stroke(egui::Stroke::new(1.0, theme::SLATE))
        .min_size(egui::vec2(70.0, 22.0)),
    )
}

impl ImpulseApp {
    pub(super) fn draw_wizard_style_picker(&mut self, ui: &mut egui::Ui) {
        if draw_choices(ui, &mut self.wizard_style_id)
            && self.wizard_selected == Self::WIZARD_RESUME
        {
            self.wizard_selected = 0;
        }
    }
}

fn draw_choices(ui: &mut egui::Ui, selection: &mut Option<String>) -> bool {
    let mut changed = false;
    ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
    ui.spacing_mut().interact_size.y = 22.0;
    ui.label(egui::RichText::new("Musical style").size(16.0).strong());
    ui.label(
        egui::RichText::new("Choose a family, then a variation.")
            .size(11.0)
            .color(theme::FOG),
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for (name, ids) in FAMILIES {
            let selected = selection.as_deref().is_some_and(|id| ids.contains(&id));
            if choice(ui, name, selected).clicked() && !selected {
                (*selection) = Some(ids[0].to_string());
                changed = true;
            }
        }
    });
    let catalog = crate::llm::styles::StyleCatalog::get();
    let family = FAMILIES
        .iter()
        .find(|(_, ids)| (*selection).as_deref().is_some_and(|id| ids.contains(&id)));
    ui.add_space(6.0);
    if let Some((_, ids)) = family {
        ui.label(
            egui::RichText::new("Variation")
                .size(11.0)
                .color(theme::FOG),
        );
        ui.horizontal_wrapped(|ui| {
            for id in *ids {
                if let Some(style) = catalog.find_by_id(id) {
                    let selected = (*selection).as_deref() == Some(*id);
                    if choice(ui, &style.name, selected)
                        .on_hover_text(&style.brief)
                        .clicked()
                    {
                        (*selection) = Some((*id).into());
                        changed = true;
                    }
                }
            }
        });
    }
    ui.add_space(6.0);
    if let Some(style) = selection.as_deref().and_then(|id| catalog.find_by_id(id)) {
        ui.label(
            egui::RichText::new(format!("{} · {} BPM", style.name, style.bpm_range))
                .size(12.0)
                .strong(),
        )
        .on_hover_text(style.rack_modules.join(" · "));
    } else {
        ui.label(
            egui::RichText::new("Or build a general rack below.")
                .size(11.0)
                .color(theme::SMOKE),
        );
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_catalog_style_has_exactly_one_family() {
        let catalog = crate::llm::styles::StyleCatalog::get();
        let ids: Vec<_> = FAMILIES
            .iter()
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        for style in catalog.styles() {
            assert_eq!(
                ids.iter().filter(|id| **id == style.id).count(),
                1,
                "{}",
                style.id
            );
        }
        assert!(ids.iter().all(|id| catalog.find_by_id(id).is_some()));
    }
    #[test]
    fn style_families_fit_compact_setup_columns() {
        for width in [300.0, 420.0, 500.0] {
            for (_, ids) in FAMILIES {
                for id in *ids {
                    let ctx = egui::Context::default();
                    theme::apply(&ctx);
                    let mut selection = Some((*id).to_string());
                    let mut height = 0.0;
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width + 16.0, 600.0),
                        )),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 5.0);
                        let response = ui.scope(|ui| {
                            draw_choices(ui, &mut selection);
                        });
                        height = response.response.rect.height();
                    });
                    output.textures_delta.clear();
                    assert!(height <= 280.0, "{id} at width {width}: height {height}");
                }
            }
        }
    }
}
