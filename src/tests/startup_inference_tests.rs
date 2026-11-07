use crate::ui::util::background_inference_ready;

#[test]
fn rack_setup_blocks_background_generation_even_with_a_ready_restored_session() {
    for startup_done in [false, true] {
        for initializing in [false, true] {
            assert!(!background_inference_ready(
                true,
                startup_done,
                initializing
            ));
        }
    }
}

#[test]
fn closing_setup_waits_for_the_startup_hook_and_model() {
    assert!(!background_inference_ready(false, false, false));
    assert!(!background_inference_ready(false, false, true));
    assert!(!background_inference_ready(false, true, true));
    assert!(background_inference_ready(false, true, false));
}

#[test]
fn reopening_setup_pauses_generation_and_completion_reenables_it() {
    assert!(background_inference_ready(false, true, false));
    assert!(!background_inference_ready(true, true, false));
    assert!(background_inference_ready(false, true, false));
}
