# Style composition audit

Static review of all 29 catalog briefs, descriptions and rhythm/pitch seeds, plus the lane generation contract. This is not a listening-test report or a live-model benchmark.

## Shared findings and changes

- Fixed ignored lead patterns: Hoover and AN1X now apply the `sequencer.*` fields that their writers emit, with scope and lock checks. Previously only patterns nested inside synth objects were read.
- Dense integer index lists (16+ hits) no longer become inline on/off arrays; short boolean arrays are also recognised correctly.
- Amen, Hoover, AN1X and 909 schema requirements now prevent empty response objects or omitted pattern fields from counting as complete generation.
- Pitch scoring now rewards in-key agreement instead of the number of distinct pitches; a tonic drone is no longer penalised simply for repeating a note. Bass voice key overrides are included in prompts and scoring.

- Production lane schemas now require one pitch per playing step, including rests, rather than accepting only four pitches and leaving the tail at old/default values. Rhythm indices respect each voice’s actual loop length, including polymetric lanes.
- Empty rhythms and single-hit drones are permitted. Completeness means a fully specified intended arrangement, not mandatory activity in every bar or voice.
- Writers now see existing bass/Hoover/AN1X step:pitch pairs. Repeated motifs, chord-tone anchors, register separation and deliberate resolution are explicit defaults; chromatic styles retain their exceptions.
- Removed the fixed C-minor pitch example that conflicted with other active keys. Style instructions override generic acid rhythm/density examples. Removed the generic requirement for several distinct bass pitches.
- D&B seed corrected to two bars at four steps per beat. Dark Ambient kick seed cleared. Baroque now supplies D minor as its suggested key.

## Individual review and composition direction

Each row records the style-specific direction added to both its brief and full description.

| Style | Direction / conflict addressed |
|---|---|
| Classic Acid | Use one tonic pitch with an occasional octave; repeat a short rhythmic cell across the loop. Supporting voices stay sparse. Filter movement supplies variation. |
| Acid House | Repeat a syncopated tonic/fifth motif, with occasional octave answers. Keep passing notes brief and return to the tonic at the loop boundary. |
| Acid Techno | Keep one or two anchor pitches and repeat the same driving cell; vary accents and filter motion instead of randomising pitches. Dense hats are intentional. |
| Detroit Techno | Use a recurring minor-key bass hook with stepwise answers; land on tonic or fifth on strong beats. Upper voices answer in the gaps. |
| Berlin Techno | Keep bass on one low tonic with deliberate rests for the kick. Avoid extra melodic lines; atmosphere and subtle timbre changes carry development. |
| Chicago House | Build a two-bar major-key hook from tonic, third and fifth, with octave answers. Repeat the first bar and vary only its ending. |
| Deep House | Use a restrained Dorian motif with smooth voice leading; let the bass anchor tonic/fifth while higher parts sustain complementary chord tones. |
| UK Garage | Repeat a skippy two-bar phrase. Sub anchors the tonic; a higher answering motif fills rests rather than doubling every bass attack. |
| Dubstep | Use a sparse tonic/fifth sub motif under the half-time beat. Repeat the pitch contour while filter rhythm changes; avoid competing low-register voices. |
| Jungle | Keep sub pitches sparse and stable on tonic/fifth; break variation supplies the complexity. Repeat the sub motif through the full loop with room between attacks. |
| Drum and Bass | At the default 4 steps per beat, a 32-step loop is TWO bars: kicks 0,8,11,16,24,27; snares 4,12,20,28. Repeat a short minor-pentatonic reese hook with a small ending change. AN1X holds tonic/fifth above the sub, with only 1-2 attacks per bar. |
| Breakcore | Rhythmic fragmentation and chromatic tension are intentional. Reuse a recognisable pitch/rhythm cell between disruptions so chaos has structure; do not force tonal cadences. |
| Dub Techno | One tonic or tonic/fifth motif is enough. Empty hats and snares are valid; leave space for echoes. Sustained upper tones share the bass harmony. |
| Minimal Techno | One or two repeated pitches and tiny rhythmic changes are enough. Snare may be silent; do not add notes merely to fill space. |
| Trance | Repeat a driving tonic/fifth bass cell and a singable upper motif. Upper chord-tone stabs follow the same harmony; regular eighth-note bass grids are intentional. |
| Gabber / Hardcore | Repeat a single pitch aligned with the kick; silent hats/snare are valid. Distorted kick pitch supplies movement, so avoid independent wandering bass melodies. |
| Early Rave / Dominator | Hoover owns a short repeated hook; bass supports on tonic/fifth in a lower register. Keep their strong-beat pitches compatible rather than writing two competing leads. |
| Synthwave | Repeat a minor triad arpeggio with a small turnaround; bass and pad share its chord tones. Preserve contour across both halves of the loop. |
| Darksynth / Outrun | Use a recurring low tonic/fifth riff with octave accents. Keep the upper line sparse; distortion supplies aggression without unrelated pitch jumps. |
| Vaporwave | Use slow tonic/third/fifth movement and sustained upper tones. Repeat a gentle phrase; limit pitch drift so detuning does not obscure the harmonic centre. |
| IDM / Braindance | Chromatic movement and irregular rhythm are intentional, but derive variations from a short recurring motif. Keep one stable pitch or rhythmic reference amid timbral disruption. |
| Electro | Use a short angular tonic/fifth/octave riff with repeated syncopation. Staccato upper stabs answer the bass in its rests; keep the grid rigid. |
| Ambient House | Repeat a gentle Dorian motif with long rests; higher pads share tonic/third/fifth tones and move slowly. Keep percussion understated across the full loop. |
| Ambient Techno | One tonic drone or slow two-note motion is enough. Sparse drums and long held upper tones are intentional; vary timbre gradually instead of retriggering busy melodies. |
| Dark Ambient | All drum rhythms must be empty. Use sparse low drones with restrained upper texture; unresolved tension is intentional, but avoid arbitrary new notes at every step. |
| Space Ambient | Use sustained tonic/fifth/octave tones with rare stepwise movement. A sparse pulse is optional. Keep pitch drift modest enough to preserve a calm harmonic centre. |
| Lo-fi Hip-Hop | Repeat a relaxed Dorian bass phrase with chord-tone anchors and short stepwise passing notes. Upper tones answer or sustain; slight detuning should preserve pitch identity. |
| Baroque / Bach | Use the current key (default D minor), a recurring stepwise subject, and phrase-ending tonic resolution. Keep drums and bass silent; continue the subject across the entire loop. |
| Meditation / Healing | All drum rhythms must be empty. Sustain tonic/fifth/octave tones with minimal attacks and modulation; no competing melodic runs or surprise pitch leaps. |

## Remaining validation and limits

Auditory quality still needs a controlled before/after listening pass with the live model. In particular compare full-loop coverage, recurring motifs, simultaneous low-register clashes and intentional silence. Use equal prompts, rack configurations and sampling settings.

The sequential writer currently has dedicated melodic lanes for bass, Hoover and AN1X only; additional instrument types need separate pipeline support. This change does not generate a multi-slot song arrangement or guarantee a successful response from every lane. Rhythm coverage is guidance, not forced filling: deliberate rests must remain possible. Exact-length note arrays increase generated tokens, especially for 64-step loops.

## Verification

The library suite passes: 2,416 tests passed, two ignored. New regression coverage includes polymetric schema bounds, full note-array lengths, lead application, locked/scoped updates, intentional silence, dense index-list decoding, per-voice key context, harmonic scoring and all 29 composition briefs. Library Clippy passes with warnings denied. All-target Clippy is blocked by existing lint failures in unrelated test code (77 diagnostics); those were not changed as part of this work.

## Jungle follow-up

The first prompt-only pass did not establish reliable Jungle output. The next pass replaces acid/house lane tasks for Jungle, removes the startup demand for 3–5 bass pitches, honours the style's single-bass rack declaration, and makes the sub's low resonance and centred pitch explicit. Jungle bass changes now have a much lower scheduler weight; automatic settings changes and settings retries are excluded so a jam cannot casually reset tempo or key.

When scale snapping is enabled, Jungle generation uses a shared tonic/fifth/octave palette with separated registers and at most six bass attacks per 32 steps. Scale snapping can be disabled for deliberate chromatic writing. The apply layer now honours scale_snap updates, lead notes and independent bass-voice keys. These constraints prevent out-of-palette pitches; they do not prove that every phrase is musically good.

Amen generation requires typed per-step slice indices and preserves the user-controlled slice count and source BPM. Style-created samplers now select an available local break if none is configured. Loading BPM-labelled files updates the source tempo unless that parameter is locked; unknown filenames retain the configured tempo. Sampler baseline settings apply immediately instead of disappearing into unsupported ramps.

Musical direction: a recognisable sampled break, small turnaround edits and a restrained sub, consistent with [Ableton's Jungle production walkthrough](https://www.ableton.com/en/blog/how-to-create-a-jungle-inspired-track-on-ableton-move/). No claim of a listening-tested authentic result is made.

Validation includes deterministic regression tests and an opt-in real-model smoke test (`jungle_live_lane_smoke`, ignored by default, requires LLAMA_SERVER_URL). The first live Gemma E4B run emitted 32 bass attacks despite claiming a four-hit motif; after adding schema density limits, a subsequent run emitted four attacks with in-palette notes. This is a structural check, not an audio comparison or a statistical model benchmark.

The final run also used metadata from the local two-bar `cw_amen14_175.wav` fixture. Jungle now rejects duplicate/unsorted rhythm index lists (including disguised gate masks) and retries once with corrective feedback before failing without applying the malformed lane. The final live smoke test passed. Library verification: 2,428 passed, three ignored; library Clippy passed with warnings denied. Generated audio has not been auditioned.

## Bass coverage and audibility correction

The previous live smoke test was insufficient: it accepted six consecutive attacks at steps 0–5 because it checked density, not phrase coverage. Primary Jungle bass now selects among complete rhythm motifs spanning every quarter of the playing loop, scaled for shorter banks and repeated for longer banks. Explicit silence remains available. Runtime validation also rejects a clustered nonempty primary bass phrase. The final Gemma E4B run produced `[0,10,16,26]` across the 32-step loop; all pitches were MIDI 45.

The old patch increased decay but left amp release at 0.05 (100 ms time constant). With the actual default half-step gate at 165 BPM, gate-off happens after about 45 ms. The revised patch explicitly sets lowpass mode, 3 ms attack, full sustain and amp release 0.32 (640 ms time constant), with bass volume 0.8. Envelope baseline settings apply immediately rather than waiting for jam-cycle ramps. Automatic lane fades no longer dip Jungle bass to 15% volume for a bar on every rewrite.

Offline rendering through the real Bass303 voice at 48 kHz, using sequencer-equivalent gates and the same distributed rhythm, measured RMS 0.0414 → 0.0775 (+5.4 dB), with peak 0.2744. Quarter RMS values were 0.0790, 0.0766, 0.0779 and 0.0765. These are isolated voice measurements, not a full-mix listening test. Reproducible A/B WAVs can be written with `JUNGLE_RENDER_DIR=target/jungle-audio-check cargo test --lib jungle_audio_tests -- --nocapture`.

Final checks for this correction: 2,432 library tests passed, three ignored; library Clippy and the live Jungle smoke test passed.
