#!/usr/bin/env python3
"""NeuTTS Air synthesis server — persistent HTTP wrapper.

Exposes a local HTTP API on port 8770 for text-to-speech synthesis using
NeuTTS Air (GGUF).  Keeps the model loaded in memory between requests to
avoid the 5–10 s cold-start per utterance.

Usage:
    # Persistent server (for in-app TTS and demo recording)
    python3 scripts/neutts-server.py [--port 8770] [--model models/neutts-air-q4.gguf]

    # One-shot mode (generate a single clip and exit)
    python3 scripts/neutts-server.py --oneshot \
        --text "Hello world" \
        --out /tmp/output.wav \
        --ref-audio voices/default.wav \
        --ref-text voices/default.txt \
        [--temperature 0.8] [--top-k 30]

Endpoints:
    GET  /health     → {"ok": true}
    POST /synthesize → {"text", "ref_audio", "ref_text", "temperature",
                        "top_k", "top_p", "out_path"}
                     ← {"ok": true, "path": "...", "duration_ms": N}
"""

import argparse
import json
import os
import sys
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path

# ---------------------------------------------------------------------------
# Model loading (deferred until first use or server startup)
# ---------------------------------------------------------------------------

_tts_instance = None

def _get_tts(model_path: str | None = None):
    """Lazy-load the NeuTTS model.  Returns the NeuTTS instance."""
    global _tts_instance
    if _tts_instance is not None:
        return _tts_instance

    try:
        from neutts import NeuTTS
    except ImportError:
        print("ERROR: neutts not installed.  Run: scripts/setup-neutts.sh", file=sys.stderr)
        sys.exit(1)

    # NeuTTS hardcodes self.max_context = 2048 in its __init__, which also
    # becomes the llama.cpp `n_ctx` and the generation `max_tokens`. For
    # longer sentences this truncates the audio or returns garbled codes.
    # The underlying Qwen backbone is trained on 32768 tokens; match it
    # to use the full native capacity (~640 s of audio headroom). Override
    # with the NEUTTS_CTX env var if VRAM is tight.
    _neutts_ctx = int(os.environ.get("NEUTTS_CTX", "32768"))

    class NeuTTSWide(NeuTTS):
        def _load_backbone(self, backbone_repo, backbone_device):
            self.max_context = _neutts_ctx
            super()._load_backbone(backbone_repo, backbone_device)

    # Resolve model path
    if model_path is None:
        # Search common locations
        candidates = [
            "models/neutts-air-q4.gguf",
            "models/neutts-air-Q4_0.gguf",
        ]
        for c in candidates:
            if os.path.isfile(c):
                model_path = c
                break
        if model_path is None:
            print("ERROR: NeuTTS GGUF model not found.  Run: scripts/download-models.sh neutts",
                  file=sys.stderr)
            sys.exit(1)

    print(f"Loading NeuTTS model: {model_path} …", flush=True)
    t0 = time.time()
    # backbone_repo accepts a local .gguf path directly.
    # Try GPU first; fall back to CPU if CUDA unavailable.
    import torch
    device = "gpu" if torch.cuda.is_available() else "cpu"
    print(f"Device: {device}", flush=True)
    _tts_instance = NeuTTSWide(
        backbone_repo=model_path,
        backbone_device=device,
        language="en-us",
    )
    print(f"Model loaded in {time.time() - t0:.1f}s", flush=True)
    return _tts_instance


# Cache encoded reference audio to avoid re-encoding on every call
_ref_cache: dict[str, object] = {}


def _get_ref_codes(tts, ref_audio: str):
    """Encode reference audio (cached)."""
    if ref_audio not in _ref_cache:
        print(f"Encoding reference: {ref_audio} …", flush=True)
        _ref_cache[ref_audio] = tts.encode_reference(ref_audio)
    return _ref_cache[ref_audio]


def synthesize(text: str, ref_audio: str, ref_text: str, out_path: str,
               temperature: float = 0.8, top_k: int = 30, top_p: float = 0.9,
               model_path: str | None = None) -> dict:
    """Run synthesis and write PCM-16 WAV to *out_path*.  Returns metadata."""
    import numpy as np
    import soundfile as sf

    tts = _get_tts(model_path)

    # Read reference transcript
    if os.path.isfile(ref_text):
        with open(ref_text) as f:
            ref_transcript = f.read().strip()
    else:
        ref_transcript = ref_text  # allow inline text

    # Encode reference audio (cached across calls)
    ref_codes = _get_ref_codes(tts, ref_audio)

    t0 = time.time()
    wav = tts.infer(
        text=text,
        ref_codes=ref_codes,
        ref_text=ref_transcript,
    )
    duration_ms = int((time.time() - t0) * 1000)

    # NeuTTS outputs float32 numpy array at 24 kHz.
    # Convert to PCM-16 for the Rust read_wav_f32() parser.
    if isinstance(wav, np.ndarray):
        wav_16 = np.clip(wav, -1.0, 1.0)
        wav_16 = (wav_16 * 32767).astype(np.int16)
        sf.write(out_path, wav_16, 24000, subtype="PCM_16")
    else:
        # Fallback: assume wav is already writable
        sf.write(out_path, wav, 24000, subtype="PCM_16")

    return {"ok": True, "path": out_path, "duration_ms": duration_ms}


# ---------------------------------------------------------------------------
# HTTP server
# ---------------------------------------------------------------------------

class NeuTTSHandler(BaseHTTPRequestHandler):
    model_path: str | None = None

    def log_message(self, fmt, *args):
        # Compact log format
        print(f"[neutts] {args[0]}", flush=True)

    def do_GET(self):
        if self.path == "/health":
            self._json_response({"ok": True})
        else:
            self._json_response({"error": "not found"}, 404)

    def do_POST(self):
        if self.path != "/synthesize":
            self._json_response({"error": "not found"}, 404)
            return

        try:
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length)) if length > 0 else {}
        except (json.JSONDecodeError, ValueError) as e:
            self._json_response({"ok": False, "error": str(e)}, 400)
            return

        text = body.get("text", "")
        # Accept voice_ref (stem) or explicit ref_audio/ref_text paths
        voice_ref = body.get("voice_ref", body.get("ref_audio", "default"))
        if not voice_ref.endswith(".wav"):
            ref_audio = f"voices/{voice_ref}.wav"
            ref_text = f"voices/{voice_ref}.txt"
        else:
            ref_audio = voice_ref
            ref_text = body.get("ref_text", voice_ref.replace(".wav", ".txt"))
        out_path = body.get("out_path", f"/tmp/neutts_{int(time.time()*1e9)}.wav")
        temperature = float(body.get("temperature", 0.8))
        top_k = int(body.get("top_k", 30))
        top_p = float(body.get("top_p", 0.9))

        if not text.strip():
            self._json_response({"ok": False, "error": "empty text"}, 400)
            return

        try:
            result = synthesize(
                text=text, ref_audio=ref_audio, ref_text=ref_text,
                out_path=out_path, temperature=temperature, top_k=top_k,
                top_p=top_p, model_path=self.model_path,
            )
            # Return WAV bytes directly for in-app use
            with open(out_path, "rb") as f:
                wav_data = f.read()
            self.send_response(200)
            self.send_header("Content-Type", "audio/wav")
            self.send_header("Content-Length", str(len(wav_data)))
            self.send_header("X-Duration-Ms", str(result.get("duration_ms", 0)))
            self.end_headers()
            self.wfile.write(wav_data)
        except Exception as e:
            self._json_response({"ok": False, "error": str(e)}, 500)

    def _json_response(self, data: dict, status: int = 200):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def run_server(port: int, model_path: str | None):
    """Start the persistent HTTP server."""
    # Pre-load model so /health is meaningful
    _get_tts(model_path)

    NeuTTSHandler.model_path = model_path
    server = HTTPServer(("127.0.0.1", port), NeuTTSHandler)
    print(f"NeuTTS server listening on http://127.0.0.1:{port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.", flush=True)
        server.server_close()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    p = argparse.ArgumentParser(description="NeuTTS Air synthesis server")
    p.add_argument("--port", type=int, default=8770)
    p.add_argument("--model", type=str, default=None,
                   help="Path to .gguf model file")

    # One-shot mode
    p.add_argument("--oneshot", action="store_true",
                   help="Generate a single clip and exit (no server)")
    p.add_argument("--text", type=str, default="")
    p.add_argument("--out", type=str, default="/tmp/neutts_output.wav")
    p.add_argument("--ref-audio", type=str, default="voices/default.wav")
    p.add_argument("--ref-text", type=str, default="voices/default.txt")
    p.add_argument("--temperature", type=float, default=0.8)
    p.add_argument("--top-k", type=int, default=30)

    args = p.parse_args()

    if args.oneshot:
        if not args.text:
            print("ERROR: --text is required in oneshot mode", file=sys.stderr)
            sys.exit(1)
        result = synthesize(
            text=args.text, ref_audio=args.ref_audio, ref_text=args.ref_text,
            out_path=args.out, temperature=args.temperature, top_k=args.top_k,
            model_path=args.model,
        )
        print(json.dumps(result))
    else:
        run_server(args.port, args.model)


if __name__ == "__main__":
    main()
