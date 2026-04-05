# Audio Feedback - PULSE Listening to Itself

PULSE can analyse the audio it produces and feed that analysis back to
the LLM. This page documents what's built, what's experimental, and what
to check if you want to push this further.

---

## What's implemented (Phase 1)

A lightweight DSP analysis pipeline runs in the UI thread when the user
clicks **LISTEN** in the LLM strip. It produces a structured text snapshot
that is prepended to the inference prompt - no model changes required.

### Analysis pipeline (`src/audio/analysis.rs`)

Pure functions, never called from the audio callback.

| Metric | How |
|--------|-----|
| Sub RMS (<80 Hz) | One-pole IIR LP at 80 Hz → RMS |
| Low RMS (80–250 Hz) | LP(250) − LP(80) residual → RMS |
| Mid RMS (250 Hz–4 kHz) | LP(4000) − LP(250) residual → RMS |
| High RMS (>4 kHz) | Signal − LP(4000) residual → RMS |
| Peak | Max absolute sample value |
| Crest factor | Peak dBFS − overall RMS dBFS |
| Transient density | 50ms energy frames; count >6 dB jumps, normalise to /bar |

All dB values are dBFS. Results are displayed inline in the LLM strip and
formatted as a snapshot string prepended to the prompt:

```
[AUDIO SNAPSHOT - 8.3s captured]
Band RMS (dBFS):  sub -18  low -14  mid -22  high -28
Peak: -2.1 dBFS  |  Crest: 16.0 dB  |  Transients: ~8.0/bar
```

### Capture buffer

A 10-second ring buffer (`capture_rx`, 441 000 samples) runs alongside
the scope buffer in the audio callback. The LISTEN button drains it and
runs the analysis. No BPM sync - captures whatever was playing.

### LLM wiring

Clicking LISTEN:
1. Drains the capture buffer → `analyse_audio(samples, 44100.0)`
2. Formats the snapshot string
3. Sends `LlmInput::Infer` with the snapshot prepended to the prompt
4. Sets `listen_pending = true` so the response is logged as **LISTEN →**
   instead of the normal persona name

---

## Phase 2 - Real audio input to the model (experimental / future)

### The idea

Gemma 4 E4B (our default model) is multimodal and has an audio encoder.
In theory, we could capture a short WAV of the synth's output, base64-encode
it, and pass it directly in the inference request - letting the model hear
what it made rather than reading a text description.

### Current status (researched 2026-04-05)

| Question | Answer |
|----------|--------|
| Gemma 4 E4B has audio encoder? | Yes - USM conformer, same as Gemma-3n |
| llama.cpp supports it? | **No** - see below |
| mmproj quantised variants? | Only F16 (990 MB); no Q4 mmproj yet |
| Gemma 4 audio trained on music? | **No - speech only** (model card warning) |
| Other llama.cpp-compatible audio models? | Ultravox 0.5, Voxtral Mini, Qwen2.5-Omni |
| vLLM supports Gemma 4 audio? | Yes, day-one (April 2, 2026) |

### llama.cpp blocker

The Gemma 4 support PR (**ggml-org/llama.cpp #21309**, merged April 2, 2026)
explicitly ships *"vision + MoE, no audio"*. The PR author stated audio
is a follow-up. As of April 5 the server responds:

```
"audio input is not supported - hint: if this is unexpected,
 you may need to provide the mmproj"
```

even when the mmproj is loaded (the CLIP loader detects the audio encoder
but the eval path doesn't propagate it).

**Track these to know when it's ready:**
- PR to add audio: watch the llama.cpp repo for a follow-up to #21309 by `ngxson`
- Eval bug: **ggml-org/llama.cpp #21325** - "Eval bug: Gemma 4 audio support is missing"
- Fix attempt: **ggml-org/llama.cpp #21348** (status unclear as of April 5)

### The music problem

Even if llama.cpp support lands, Gemma 4's audio encoder was trained on
speech only. The model card explicitly warns that *"music and non-speech
sounds were not part of the training data."* Whether a kick drum pattern
or a TB-303 line produces useful output from the audio path is unknown
and worth testing once the API is available.

**Our guess:** for mix-level feedback (loudness, transient density,
frequency balance) the text descriptor in Phase 1 probably works better
than raw audio, because we control exactly what's measured. Raw audio
input might only add value for things we can't easily quantify - timbre,
character, "does this sound harsh" - but that requires music-trained audio
understanding that Gemma 4 doesn't have today.

### API format when llama.cpp ships it

The server uses the OpenAI-compatible `/v1/chat/completions` endpoint.
Based on working examples with Ultravox 0.5 and Voxtral Mini, the audio
content part looks like:

```json
{
  "role": "user",
  "content": [
    {
      "type": "input_audio",
      "input_audio": {
        "data": "<base64-encoded WAV>",
        "format": "wav"
      }
    },
    {
      "type": "text",
      "text": "You are listening to the audio you just produced. React - correct any mix or arrangement issues. Respond in JSON."
    }
  ]
}
```

Format notes: miniaudio is used for decoding, so PCM/WAV/MP3/FLAC should
work in theory. Confirmed working format is WAV. Recommended capture:
16kHz mono, as many seconds as needed.

Required launch flags once the model supports it:
```bash
llama-server \
  -hf ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M \
  --mmproj mmproj-gemma-4-e4b-it-f16.gguf \
  --jinja --flash-attn on -ngl 99 -c 24676
```

### Implementation sketch (do not start until llama.cpp PR lands)

1. Add `audio_feedback_enabled: bool` to `SamplingParams` / settings UI
2. In `LlamaServerBackend::infer`: when enabled, check for a pending
   `AudioCapture` payload in the request
3. Resample capture buffer → 16kHz mono (simple linear interp, we already
   have this in `load_wav_to_44100`)
4. Encode as WAV bytes → base64
5. Build `input_audio` content part instead of plain text
6. Note in settings: requires Gemma 4 E2B/E4B + mmproj-f16 loaded

### Alternative: Ultravox as a secondary listener

Ultravox 0.5 8B already works in llama.cpp with audio input today. It's
not tuned for music production JSON, but it can produce a text description
of what it hears. That description could be appended to the context sent
to the main model:

```
[ULTRAVOX LISTENER]: "Dense percussive pattern. Very loud low-frequency
 transients. Sparse high-frequency content. Possible clipping."
```

This is a two-model pipeline and requires significant extra VRAM, but
it's technically feasible without waiting for Gemma 4 audio support.

---

## Sources

- [model: support gemma 4 (vision + moe, no audio) - llama.cpp #21309](https://github.com/ggml-org/llama.cpp/pull/21309)
- [Eval bug: Gemma 4 audio support is missing - llama.cpp #21325](https://github.com/ggml-org/llama.cpp/issues/21325)
- [How to input audio to Gemma 4 E4B? - llama.cpp Discussion #21334](https://github.com/ggml-org/llama.cpp/discussions/21334)
- [ggml-org/gemma-4-E4B-it-GGUF on HuggingFace](https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF)
- [Welcome Gemma 4 - Hugging Face blog](https://huggingface.co/blog/gemma4)
- [Audio input support in llama.cpp - Discussion #13759](https://github.com/ggml-org/llama.cpp/discussions/13759)
- [llama.cpp multimodal docs](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md)
- [Gemma 4 Usage Guide - vLLM Recipes](https://docs.vllm.ai/projects/recipes/en/latest/Google/Gemma4.html)
