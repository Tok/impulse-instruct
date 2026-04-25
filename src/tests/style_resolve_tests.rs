// ─── tests/style_resolve_tests.rs ─────────────────────────────────────────────
// `StyleCatalog::resolve_style_id` — pulls the LlmAction::SetStyle
// resolver out of the impure UI dispatch shell so its lookup order
// (exact id → case-insensitive id → case-insensitive name) is locked
// down by tests.

#[cfg(test)]
mod resolve {
    use crate::llm::styles::StyleCatalog;

    /// Pick a known style from the live catalog so the tests don't
    /// depend on a specific id but DO use a real entry.  The catalog
    /// is non-empty in any normal build (it's loaded from
    /// `styles.json` baked into the binary), so the unwrap is safe.
    fn first_style() -> (String, String) {
        let cat = StyleCatalog::get();
        let s = cat
            .styles()
            .first()
            .expect("style catalog should be non-empty");
        (s.id.clone(), s.name.clone())
    }

    #[test]
    fn exact_id_match_returns_canonical_id() {
        let (id, _name) = first_style();
        let out = StyleCatalog::get().resolve_style_id(&id);
        assert_eq!(out.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn upper_case_id_resolves_to_canonical() {
        let (id, _name) = first_style();
        let out = StyleCatalog::get().resolve_style_id(&id.to_uppercase());
        assert_eq!(
            out.as_deref(),
            Some(id.as_str()),
            "upper-cased id should resolve back to the canonical lower-case id"
        );
    }

    #[test]
    fn display_name_resolves_to_canonical_id() {
        let (id, name) = first_style();
        let out = StyleCatalog::get().resolve_style_id(&name);
        assert_eq!(
            out.as_deref(),
            Some(id.as_str()),
            "display name should resolve to the canonical id"
        );
    }

    #[test]
    fn lower_cased_display_name_resolves_to_canonical_id() {
        let (id, name) = first_style();
        let out = StyleCatalog::get().resolve_style_id(&name.to_lowercase());
        assert_eq!(out.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn unknown_query_returns_none() {
        let out = StyleCatalog::get().resolve_style_id("definitely_not_a_style_id_xyz_zzz_42");
        assert!(out.is_none());
    }

    #[test]
    fn empty_query_returns_none() {
        // Defensive: an LLM that emits SetStyle("") shouldn't match
        // an empty-id style (none exist) and shouldn't crash.
        assert!(StyleCatalog::get().resolve_style_id("").is_none());
    }

    #[test]
    fn whitespace_pad_does_not_match() {
        // Documented contract: the resolver does NOT trim — callers
        // can pre-trim if they want loose matching.  Locks the
        // current behaviour so a future "loose match" change is
        // explicit, not accidental.
        let (id, _) = first_style();
        let padded = format!("  {}  ", id);
        assert!(StyleCatalog::get().resolve_style_id(&padded).is_none());
    }
}
