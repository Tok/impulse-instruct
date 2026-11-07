//! Offline speech treatment. Allocation is confined to the synthesis worker.
/// Saturated, band-limited mic into a short diffuse room. Drive adds harmonics
/// rather than pushing the master into clipping; wet tails fit the speech buffer.
pub fn process_mic(input: &[f32], sr: u32, drive: f32, room: f32) -> Vec<f32> {
    if input.is_empty() || sr == 0 {
        return input.to_vec();
    }
    let drive = drive.clamp(0.0, 1.0);
    let room = room.clamp(0.0, 1.0);
    if drive == 0.0 && room == 0.0 {
        return input.to_vec();
    }
    let tail = if room > 0.0 {
        (sr as f32 * 0.8) as usize
    } else {
        0
    };
    let mut dry = Vec::with_capacity(input.len() + tail);
    let mut low = 0.0;
    let mut high = 0.0;
    let hp = 1.0 - (-std::f32::consts::TAU * 140.0 / sr as f32).exp();
    let lp = 1.0 - (-std::f32::consts::TAU * 4200.0 / sr as f32).exp();
    for &sample in input {
        low += hp * (sample - low);
        high += lp * (sample - low - high);
        let crushed = (high * (1.0 + drive * 24.0)).tanh() * 0.28;
        dry.push(sample * (1.0 - drive) + crushed * drive);
    }
    dry.resize(input.len() + tail, 0.0);
    let mut output = dry.clone();
    // Parallel feedback combs with unrelated lengths: a small rough room,
    // not a rhythmic echo competing with the break.
    for seconds in [0.037, 0.053, 0.079, 0.097] {
        let delay = ((seconds * sr as f32) as usize).max(1);
        let mut line = vec![0.0; delay];
        let mut damping = 0.0;
        for (i, &sample) in dry.iter().enumerate() {
            let delayed = line[i % delay];
            damping += 0.35 * (delayed - damping);
            line[i % delay] = sample + damping * 0.62;
            output[i] += delayed * room * 0.25;
        }
    }
    output.iter_mut().for_each(|s| *s = s.clamp(-0.9, 0.9));
    output
}
