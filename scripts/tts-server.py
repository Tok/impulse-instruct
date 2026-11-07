#!/usr/bin/env python3
"""Qwen3-TTS voice-cloning server — persistent HTTP wrapper.

Serves text-to-speech on 127.0.0.1:8770 using Qwen3-TTS (Base model,
voice cloning from a short reference clip).  The model stays loaded between
requests, and each voice's encoded reference prompt is cached, so only the
first utterance per voice pays the encode cost.

Usage:
    # Persistent server (in-app TTS + demo recording)
    .tts-venv/bin/python scripts/tts-server.py [--port 8770] [--model ID_OR_PATH]

    # One-shot mode (generate a single clip and exit)
    .tts-venv/bin/python scripts/tts-server.py --oneshot \\
        --text "Hello world" --out /tmp/out.wav \\
        --ref-audio voices/default.wav --ref-text voices/default.txt

Model: Qwen/Qwen3-TTS-12Hz-1.7B-Base by default (~4.5 GB VRAM in bf16).
Use --model Qwen/Qwen3-TTS-12Hz-0.6B-Base (or TTS_MODEL=…) on tighter GPUs.
Both are Apache-2.0 and ungated on Hugging Face.

Endpoints:
    GET  /health     → {"ok": true}   (only after the model has loaded)
    POST /synthesize → JSON {"text",
                             "voice_ref"            stem under voices/, or
                             "ref_audio","ref_text" explicit paths,
                             "temperature","top_k","top_p","seed","language",
                             "out_path"             optional: also write here}
                     ← 200 audio/wav (PCM-16 mono, model rate — 24 kHz)
                       header X-Duration-Ms = generation time
"""

import argparse
import io
import json
import os
import sys
import time
import warnings
from http.server import BaseHTTPRequestHandler, HTTPServer

warnings.filterwarnings("ignore", category=FutureWarning)
warnings.filterwarnings("ignore", category=UserWarning)
warnings.filterwarnings("ignore", category=DeprecationWarning)

DEFAULT_MODEL = os.environ.get("TTS_MODEL", "Qwen/Qwen3-TTS-12Hz-1.7B-Base")

# The 12 Hz codec emits one generation step per 1/12 s audio frame
# (measured: 60 new tokens → 4.6 s of audio).  Used for the runaway guard.
TOKENS_PER_SECOND = 12

_model = None
_prompt_cache: dict = {}


def _load_model(model_id: str | None):
    """Load Qwen3-TTS once (GPU bf16 if available, else CPU fp32)."""
    global _model
    if _model is not None:
        return _model
    try:
        import torch
        from qwen_tts import Qwen3TTSModel
    except ImportError:
        print("ERROR: qwen-tts not installed.  Run: ./scripts/download-models.sh tts",
              file=sys.stderr)
        sys.exit(1)

    model_id = model_id or DEFAULT_MODEL
    cuda = torch.cuda.is_available()
    print(f"Loading {model_id} on {'cuda' if cuda else 'cpu'} …", flush=True)
    t0 = time.time()
    _model = Qwen3TTSModel.from_pretrained(
        model_id,
        device_map="cuda:0" if cuda else "cpu",
        dtype=torch.bfloat16 if cuda else torch.float32,
    )
    print(f"Model loaded in {time.time() - t0:.1f}s", flush=True)
    return _model


def _read_ref_text(ref_text: str) -> str:
    """`ref_text` may be a transcript file path or the transcript itself."""
    if os.path.isfile(ref_text):
        with open(ref_text, encoding="utf-8") as f:
            return f.read().strip()
    return ref_text.strip()


def _voice_prompt(model, ref_audio: str, ref_text: str):
    """Encode (and cache) the voice-clone prompt for a reference clip.

    Keyed on path + mtime so re-recording a voice file is picked up without
    restarting the server."""
    if not os.path.isfile(ref_audio):
        raise FileNotFoundError(f"reference audio not found: {ref_audio}")
    transcript = _read_ref_text(ref_text)
    key = (os.path.abspath(ref_audio), os.path.getmtime(ref_audio), transcript)
    if key not in _prompt_cache:
        print(f"Encoding voice reference {ref_audio} …", flush=True)
        _prompt_cache[key] = model.create_voice_clone_prompt(
            ref_audio=ref_audio, ref_text=transcript
        )
    return _prompt_cache[key]


def max_new_tokens_for(text: str) -> int:
    """Runaway guard: ~1% of takes otherwise babble for minutes.  Allows a
    slow 6 chars/s plus 3 s headroom, clamped to 4–60 s of audio."""
    seconds = min(max(len(text) / 6.0 + 3.0, 4.0), 60.0)
    return int(seconds * TOKENS_PER_SECOND)


def synthesize(text: str, ref_audio: str, ref_text: str,
               temperature: float = 0.8, top_k: int = 30, top_p: float = 0.9,
               seed: int | None = None, language: str = "Auto",
               model_id: str | None = None) -> tuple[bytes, int]:
    """Return (PCM-16 WAV bytes, generation time in ms)."""
    import numpy as np
    import soundfile as sf
    import torch

    model = _load_model(model_id)
    prompt = _voice_prompt(model, ref_audio, ref_text)
    if seed is not None:
        torch.manual_seed(seed)
        torch.cuda.manual_seed_all(seed)
    # NB: never enable torch.use_deterministic_algorithms() — Qwen3-TTS's
    # generate path then fails with `_unsafe_index … Float`.
    t0 = time.time()
    wavs, sr = model.generate_voice_clone(
        text=text,
        language=language,
        voice_clone_prompt=prompt,
        do_sample=True,
        temperature=temperature,
        top_k=top_k,
        top_p=top_p,
        max_new_tokens=max_new_tokens_for(text),
    )
    gen_ms = int((time.time() - t0) * 1000)
    pcm = (np.clip(wavs[0], -1.0, 1.0) * 32767).astype(np.int16)
    buf = io.BytesIO()
    sf.write(buf, pcm, sr, format="WAV", subtype="PCM_16")
    return buf.getvalue(), gen_ms


def _resolve_voice(body: dict) -> tuple[str, str]:
    """Map a request to (ref_audio, ref_text) paths."""
    ref_audio = body.get("ref_audio")
    if ref_audio:
        return ref_audio, body.get("ref_text", os.path.splitext(ref_audio)[0] + ".txt")
    stem = body.get("voice_ref") or "default"
    if stem.endswith(".wav"):
        return stem, body.get("ref_text", os.path.splitext(stem)[0] + ".txt")
    return f"voices/{stem}.wav", f"voices/{stem}.txt"


class TTSHandler(BaseHTTPRequestHandler):
    model_id: str | None = None

    def log_message(self, fmt, *args):
        print(f"[tts] {args[0]}", flush=True)

    def do_GET(self):
        if self.path == "/health":
            self._json({"ok": True})
        else:
            self._json({"error": "not found"}, 404)

    def do_POST(self):
        if self.path != "/synthesize":
            self._json({"error": "not found"}, 404)
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length)) if length > 0 else {}
        except (json.JSONDecodeError, ValueError) as e:
            self._json({"ok": False, "error": str(e)}, 400)
            return

        text = str(body.get("text", "")).strip()
        if not text:
            self._json({"ok": False, "error": "empty text"}, 400)
            return
        ref_audio, ref_text = _resolve_voice(body)
        seed = body.get("seed")
        try:
            wav, gen_ms = synthesize(
                text=text, ref_audio=ref_audio, ref_text=ref_text,
                temperature=float(body.get("temperature", 0.8)),
                top_k=int(body.get("top_k", 30)),
                top_p=float(body.get("top_p", 0.9)),
                seed=int(seed) if seed is not None else None,
                language=str(body.get("language", "Auto")),
                model_id=self.model_id,
            )
        except FileNotFoundError as e:
            self._json({"ok": False, "error": str(e)}, 404)
            return
        except Exception as e:  # noqa: BLE001 — report any model failure to the client
            self._json({"ok": False, "error": f"{type(e).__name__}: {e}"}, 500)
            return

        out_path = body.get("out_path")
        if out_path:
            with open(out_path, "wb") as f:
                f.write(wav)
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.send_header("X-Duration-Ms", str(gen_ms))
        self.end_headers()
        self.wfile.write(wav)

    def _json(self, data: dict, status: int = 200):
        payload = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def run_server(port: int, model_id: str | None):
    _load_model(model_id)  # load before listening so /health means "ready"
    TTSHandler.model_id = model_id
    server = HTTPServer(("127.0.0.1", port), TTSHandler)
    print(f"Qwen3-TTS server listening on http://127.0.0.1:{port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.", flush=True)
        server.server_close()


def main():
    p = argparse.ArgumentParser(description="Qwen3-TTS voice-cloning server")
    p.add_argument("--port", type=int, default=8770)
    p.add_argument("--model", type=str, default=None,
                   help=f"HF model id or local path (default {DEFAULT_MODEL})")
    p.add_argument("--oneshot", action="store_true",
                   help="Generate a single clip and exit (no server)")
    p.add_argument("--text", type=str, default="")
    p.add_argument("--out", type=str, default="tts_output.wav")
    p.add_argument("--ref-audio", type=str, default="voices/default.wav")
    p.add_argument("--ref-text", type=str, default="voices/default.txt")
    p.add_argument("--temperature", type=float, default=0.8)
    p.add_argument("--top-k", type=int, default=30)
    p.add_argument("--top-p", type=float, default=0.9)
    p.add_argument("--seed", type=int, default=None)
    args = p.parse_args()

    if args.oneshot:
        if not args.text:
            print("ERROR: --text is required in oneshot mode", file=sys.stderr)
            sys.exit(1)
        wav, gen_ms = synthesize(
            text=args.text, ref_audio=args.ref_audio, ref_text=args.ref_text,
            temperature=args.temperature, top_k=args.top_k, top_p=args.top_p,
            seed=args.seed, model_id=args.model,
        )
        with open(args.out, "wb") as f:
            f.write(wav)
        print(json.dumps({"ok": True, "path": args.out, "duration_ms": gen_ms}))
    else:
        run_server(args.port, args.model)


if __name__ == "__main__":
    main()
