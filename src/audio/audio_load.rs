// ─── audio/audio_load.rs ─────────────────────────────────────────────────────
// Unified audio-file loader for SampleInstrument V2.  Dispatches by
// file extension to the appropriate decoder, downmixes to mono, and
// resamples to the engine rate.  V1 only handled `.wav` via the
// hand-rolled parser in `audio::mod`; V2 follow-up adds `.flac`
// (via the pure-Rust `claxon` crate) and `.aiff`/`.aif` (hand-rolled
// here — AIFF format is RIFF-shaped enough that a small parser is
// cheaper than another dep).
//
// All paths return `Option<Arc<Vec<f32>>>` of mono samples at the
// engine sample rate so the SampleInstrument's pitch-shift +
// resample stage doesn't need to know which decoder produced them.

use std::sync::Arc;

use super::{SAMPLE_RATE, SAMPLE_RATE_HZ, load_wav_to_44100};

/// Detect format from extension and dispatch to the matching loader.
/// Falls back to `None` for unknown formats.  Logs the loaded file's
/// metadata at info level so the user sees which path won.
pub fn load_audio_to_engine(path: &str) -> Option<Arc<Vec<f32>>> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "wav" => load_wav_to_44100(path),
        "flac" => load_flac_to_engine(path),
        "aif" | "aiff" | "aifc" => load_aiff_to_engine(path),
        _ => {
            log::warn!("load_audio_to_engine: unsupported extension '{ext}' for {path}");
            None
        }
    }
}

/// Decode a FLAC file (16/24/32-bit, any channel count, any rate)
/// via `claxon`, downmix to mono, resample to the engine rate.
/// Returns `None` on any decode error so the caller can fall back
/// gracefully — same shape as `load_wav_to_44100`.
fn load_flac_to_engine(path: &str) -> Option<Arc<Vec<f32>>> {
    let mut reader = claxon::FlacReader::open(path).ok()?;
    let info = reader.streaminfo();
    let channels = info.channels.max(1);
    let bits = info.bits_per_sample.clamp(1, 32);
    let src_rate = info.sample_rate;
    let scale = 1.0 / (1_u32 << (bits - 1)) as f32;

    // Walk samples — claxon yields interleaved per-channel samples
    // as i32, regardless of `bits_per_sample`.  Accumulate into
    // mono by averaging across channels per frame.
    let mut mono: Vec<f32> = Vec::new();
    let mut acc = 0.0_f32;
    let mut ch_count = 0u32;
    for sample in reader.samples() {
        let s = sample.ok()?;
        acc += (s as f32) * scale;
        ch_count += 1;
        if ch_count >= channels {
            mono.push(acc / channels as f32);
            acc = 0.0;
            ch_count = 0;
        }
    }
    let n_frames = mono.len();

    let out = resample_mono_linear(&mono, src_rate);
    log::info!(
        "Loaded FLAC: {} ({} Hz, {} ch, {} bits, {} frames → {} samples at {} Hz)",
        path,
        src_rate,
        channels,
        bits,
        n_frames,
        out.len(),
        SAMPLE_RATE_HZ
    );
    Some(Arc::new(out))
}

/// Hand-rolled AIFF / AIFC decoder.  Limited to 16-bit big-endian
/// PCM — same constraint as the WAV loader (16-bit only).  The
/// codebase's existing samples stay in this range and supporting
/// 24/32-bit AIFF would mean implementing the AIFC compression
/// schemes, which is well outside scope.
fn load_aiff_to_engine(path: &str) -> Option<Arc<Vec<f32>>> {
    let bytes = std::fs::read(path).ok()?;
    // AIFF: "FORM" <size> "AIFF"|"AIFC" + chunks
    if bytes.len() < 12 || &bytes[0..4] != b"FORM" {
        return None;
    }
    let form_kind = &bytes[8..12];
    if form_kind != b"AIFF" && form_kind != b"AIFC" {
        return None;
    }

    let mut pos = 12usize;
    let mut channels = 1u16;
    let mut src_rate = SAMPLE_RATE_HZ;
    let mut bits = 16u16;
    let mut data_start = 0usize;
    let mut n_frames = 0usize;
    // AIFC adds a "compression type" 4cc field.  We only accept
    // "NONE" (uncompressed big-endian PCM).
    let mut is_compressed = false;

    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let chunk_len = u32::from_be_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        pos += 8;
        if tag == b"COMM" && chunk_len >= 18 {
            channels = u16::from_be_bytes(bytes[pos..pos + 2].try_into().ok()?);
            n_frames = u32::from_be_bytes(bytes[pos + 2..pos + 6].try_into().ok()?) as usize;
            bits = u16::from_be_bytes(bytes[pos + 6..pos + 8].try_into().ok()?);
            // AIFF stores sample rate as an 80-bit IEEE 754
            // extended-precision float at offset +8.  Decode the
            // common case: positive integer rates that fit cleanly
            // — the format is rarely seen with fractional rates.
            let sr_be = &bytes[pos + 8..pos + 18];
            src_rate = aiff_extended_to_u32(sr_be).unwrap_or(SAMPLE_RATE_HZ);
            // AIFC carries an extra 4cc (compression) at offset +18.
            if form_kind == b"AIFC" && chunk_len >= 22 {
                let comp = &bytes[pos + 18..pos + 22];
                if comp != b"NONE" && comp != b"sowt" {
                    is_compressed = true;
                }
            }
        } else if tag == b"SSND" && chunk_len >= 8 {
            // SSND header: offset (u32 BE) + blockSize (u32 BE) +
            // sample bytes.  `offset` is bytes from the end of
            // the SSND header where the actual frames begin.
            let offset = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
            data_start = pos + 8 + offset;
            break;
        }
        pos += chunk_len + (chunk_len & 1);
    }

    if data_start == 0 || channels == 0 || bits != 16 || is_compressed {
        log::warn!(
            "AIFF unsupported: {} (channels={}, bits={}, compressed={})",
            path,
            channels,
            bits,
            is_compressed
        );
        return None;
    }

    let frame_bytes = channels as usize * 2;
    let mut mono = Vec::with_capacity(n_frames);
    for i in 0..n_frames {
        let base = data_start + i * frame_bytes;
        let mut sum = 0.0f32;
        for ch in 0..channels as usize {
            let off = base + ch * 2;
            if off + 2 > bytes.len() {
                break;
            }
            // AIFF is big-endian; that's the only difference from
            // the WAV PCM path.
            let raw = i16::from_be_bytes(bytes[off..off + 2].try_into().ok()?);
            sum += raw as f32 / 32768.0;
        }
        mono.push(sum / channels as f32);
    }

    let out = resample_mono_linear(&mono, src_rate);
    log::info!(
        "Loaded AIFF: {} ({} Hz, {} ch, {} frames → {} samples at {} Hz)",
        path,
        src_rate,
        channels,
        n_frames,
        out.len(),
        SAMPLE_RATE_HZ
    );
    Some(Arc::new(out))
}

/// Linear-interp resample a mono buffer from `src_rate` to the
/// engine rate.  Pure helper — shared between the FLAC / AIFF / WAV
/// paths in this module and the SF2 sample decoder in
/// `sf2_loader.rs` (re-imported as `crate::audio::audio_load::resample_mono_linear`).
/// Slice input + owned `Vec` output so the SF2 caller (which works
/// off a borrowed cache slice) doesn't have to clone first.
pub(crate) fn resample_mono_linear(mono: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == SAMPLE_RATE_HZ {
        return mono.to_vec();
    }
    let ratio = src_rate as f32 / SAMPLE_RATE;
    let new_len = (mono.len() as f32 / ratio) as usize;
    (0..new_len)
        .map(|i| {
            let src = i as f32 * ratio;
            let idx = src as usize;
            let frac = src - idx as f32;
            let a = mono.get(idx).copied().unwrap_or(0.0);
            let b = mono.get(idx + 1).copied().unwrap_or(0.0);
            a + (b - a) * frac
        })
        .collect()
}

/// Decode an AIFF 80-bit IEEE 754 extended-precision float into a
/// `u32` sample rate.  AIFF only ever stores positive integer-ish
/// rates here (44100, 48000, 96000, …), so we extract the integer
/// portion and bail to `None` on negatives / pathological encodings.
///
/// Layout: 1-bit sign + 15-bit biased exponent (bias 16383) + 64-bit
/// mantissa with explicit leading bit.  Result =
/// `(-1)^sign · mantissa · 2^(exp - 16383 - 63)`.
fn aiff_extended_to_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 10 {
        return None;
    }
    let sign = bytes[0] >> 7;
    if sign != 0 {
        return None;
    }
    let exponent = u16::from_be_bytes([bytes[0] & 0x7F, bytes[1]]);
    let mut mantissa = u64::from_be_bytes(bytes[2..10].try_into().ok()?);
    if exponent == 0 && mantissa == 0 {
        return Some(0);
    }
    // Bias 16383, mantissa has explicit leading 1 (bit 63).
    let unbiased = exponent as i32 - 16383 - 63;
    if unbiased > 0 {
        mantissa <<= unbiased;
    } else if unbiased < 0 {
        let shift = (-unbiased) as u32;
        if shift >= 64 {
            return Some(0);
        }
        mantissa >>= shift;
    }
    if mantissa > u32::MAX as u64 {
        return None;
    }
    Some(mantissa as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aiff_extended_decodes_44100() {
        // value = mantissa · 2^(exp - 16383 - 63).  44100 fits in
        // 16 bits with msb at bit 15; shifting to bit 63 leaves
        // mantissa = 0xAC44 << 48 and exp - 16446 = 0 → exp =
        // 16398 (0x400E).  Same exponent as 48000 below — both
        // are 16-bit-msb integers.
        let bytes = [0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(aiff_extended_to_u32(&bytes), Some(44100));
    }

    #[test]
    fn aiff_extended_decodes_48000() {
        // 48000 = 0xBB80, msb at bit 15; same exponent shape as
        // 44100 above (mantissa shift = 48).
        let bytes = [0x40, 0x0E, 0xBB, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(aiff_extended_to_u32(&bytes), Some(48000));
    }

    #[test]
    fn aiff_extended_rejects_negative() {
        // Sign bit set → bail.
        let bytes = [0xC0, 0x0E, 0xBB, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(aiff_extended_to_u32(&bytes), None);
    }

    #[test]
    fn aiff_extended_zero_returns_zero() {
        let bytes = [0u8; 10];
        assert_eq!(aiff_extended_to_u32(&bytes), Some(0));
    }

    /// `resample_mono_linear` short-circuits when the source rate
    /// already matches the engine rate — the output should be a
    /// bit-equal copy of the input.
    #[test]
    fn resample_mono_linear_passthrough_at_engine_rate() {
        let input: Vec<f32> = (0..32).map(|i| i as f32 * 0.01).collect();
        let out = resample_mono_linear(&input, SAMPLE_RATE_HZ);
        assert_eq!(out, input);
    }

    /// Halving the source rate (relative to the engine rate) should
    /// roughly double the output length — linear interp halves the
    /// stride per output sample.  Exact length depends on the
    /// floor() in the new_len calc; assert the length is in the
    /// expected ballpark and the endpoints land near the input.
    #[test]
    fn resample_mono_linear_doubles_length_when_src_is_half_engine() {
        let input: Vec<f32> = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let half = SAMPLE_RATE_HZ / 2;
        let out = resample_mono_linear(&input, half);
        assert_eq!(out.len(), input.len() * 2);
        // First sample lines up with input[0].
        assert!((out[0] - input[0]).abs() < 1e-5);
        // Mid-points fall between input samples (linear interp).
        assert!((out[1] - 0.5).abs() < 1e-5);
        assert!((out[3] - 1.5).abs() < 1e-5);
    }

    /// Doubling the source rate (relative to the engine rate) should
    /// roughly halve the output length — linear interp doubles the
    /// stride.
    #[test]
    fn resample_mono_linear_halves_length_when_src_is_double_engine() {
        let input: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let double = SAMPLE_RATE_HZ * 2;
        let out = resample_mono_linear(&input, double);
        assert_eq!(out.len(), input.len() / 2);
        // Stride 2 → out[0] = input[0], out[1] = input[2], etc.
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], 2.0);
        assert_eq!(out[3], 6.0);
    }

    #[test]
    fn load_audio_to_engine_unknown_extension_returns_none() {
        // Doesn't even have to exist — extension-dispatch fails first.
        assert!(load_audio_to_engine("nope.xyz").is_none());
    }

    #[test]
    fn load_audio_to_engine_routes_wav_to_existing_loader() {
        // Synthesise a tiny mono 16-bit PCM WAV at engine rate +
        // round-trip through the loader.  We use `hound` directly
        // for the encode side so the round-trip doesn't reach into
        // our hand-rolled parser's quirks.
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ii_audio_load_test_{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE_HZ,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).expect("wav create");
        for i in 0..256 {
            let v = ((i as f32 / 256.0) * 32_000.0) as i16;
            writer.write_sample(v).expect("wav write");
        }
        writer.finalize().expect("wav finalize");
        let _ = std::io::stdout().flush();
        let out = load_audio_to_engine(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let arc = out.expect("WAV load via dispatch");
        assert_eq!(
            arc.len(),
            256,
            "256 frames in, 256 frames out (engine rate)"
        );
    }

    /// Build a minimal AIFF (FORM/COMM/SSND) header + body in
    /// memory so the AIFF decoder can be exercised without a
    /// fixture file.
    fn write_aiff_mono_16(samples: &[i16], sr: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let n_frames = samples.len() as u32;
        let ssnd_data_bytes = (n_frames * 2) as u32;
        let ssnd_chunk_bytes = ssnd_data_bytes + 8; // offset + blockSize headers
        let comm_chunk_bytes: u32 = 18;
        let total = 4 + (8 + comm_chunk_bytes) + (8 + ssnd_chunk_bytes);
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&total.to_be_bytes());
        out.extend_from_slice(b"AIFF");
        // COMM chunk
        out.extend_from_slice(b"COMM");
        out.extend_from_slice(&comm_chunk_bytes.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes()); // channels
        out.extend_from_slice(&n_frames.to_be_bytes()); // sample frames
        out.extend_from_slice(&16u16.to_be_bytes()); // bits
        // 80-bit IEEE extended sample rate.  Mantissa bit 63 holds
        // the leading bit of the integer.
        let mantissa = (sr as u64) << 48;
        let exp = 0x400E_u16; // 16-bit msb integers (44100/48000 etc.)
        let sign_exp = exp;
        out.extend_from_slice(&sign_exp.to_be_bytes());
        out.extend_from_slice(&mantissa.to_be_bytes());
        // SSND chunk
        out.extend_from_slice(b"SSND");
        out.extend_from_slice(&ssnd_chunk_bytes.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes()); // offset
        out.extend_from_slice(&0u32.to_be_bytes()); // blockSize
        for s in samples {
            out.extend_from_slice(&s.to_be_bytes());
        }
        out
    }

    #[test]
    fn load_audio_to_engine_decodes_aiff() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ii_audio_load_aiff_{}.aiff", std::process::id()));
        let samples: Vec<i16> = (0..256_i16).collect();
        let bytes = write_aiff_mono_16(&samples, SAMPLE_RATE_HZ);
        std::fs::write(&path, &bytes).expect("write aiff");
        let out = load_audio_to_engine(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        let arc = out.expect("AIFF load via dispatch");
        assert_eq!(arc.len(), 256, "AIFF round-trip preserves frame count");
        // Sample[0] = 0 → 0.0; sample[100] = 100 / 32768.
        assert!((arc[0] - 0.0).abs() < 1e-6);
        assert!((arc[100] - (100.0 / 32768.0)).abs() < 1e-5);
    }

    #[test]
    fn load_audio_to_engine_rejects_non_aiff_form() {
        // Wrong FORM kind → bail.
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(b"FORM");
        bytes[4..8].copy_from_slice(&100u32.to_be_bytes());
        bytes[8..12].copy_from_slice(b"WAVE"); // not AIFF
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ii_audio_load_bad_{}.aiff", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let out = load_audio_to_engine(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(out.is_none(), "non-AIFF FORM should fail to decode");
    }
}
