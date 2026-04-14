// ─── ui/style_rack.rs ──────────────────────────────────────────────────────
// Style → rack auto-setup helper.  When the user picks a style, walk its
// `rack_modules` list (declared in styles.json) and add any module whose kind
// isn't already present.  Existing modules are kept so switching styles only
// ever GROWS the rack — the operation is non-destructive.

use crate::ui::ImpulseApp;

/// Add any style-recommended modules that aren't already in the rack.
/// Logs the additions and recompiles the FX plan when at least one was added.
pub fn apply(app: &mut ImpulseApp, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let mut added: Vec<&str> = Vec::new();
    {
        let mut s = app.state.write();
        for name in names {
            let Some(kind) = crate::state::parse_module_kind(name) else {
                continue;
            };
            if !s.rack.modules.iter().any(|m| m.kind == kind) {
                s.rack.add_module(kind);
                added.push(name.as_str());
            }
        }
        if !added.is_empty() {
            s.rack.wire_default_cables();
        }
    }
    if !added.is_empty() {
        app.log_text
            .push_str(&format!("Style rack: + {}\n", added.join(", ")));
        app.push_fx_plan();
    }
}
