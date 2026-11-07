#!/usr/bin/env python3
"""Create an original neural MC reference; never clone a named performer.
Run with .tts-venv/bin/python scripts/design-mc-voice.py [--replace].
VoiceDesign is downloaded once; normal playback still uses the Base model.
"""
import argparse
from pathlib import Path
import shutil
import time

TEXT = "Selector! Big up the junglist massive! We have the bass, we have the breaks, we have the riddim. Pull up and rewind! This one goes out to all the ravers in the building tonight."
DIRECTION = (
    "An adult male British jungle sound-system MC with a natural London accent. "
    "A warm resonant chest voice with a slightly rough edge. Animated, confident, "
    "rhythmic crowd calls with varied pitch and crisp consonants. Project enthusiastically "
    "as if calling to a dance floor, with short pauses between phrases. "
    "Natural human speech, not singing or a robotic announcer. Dry clean voice, no music."
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--replace", action="store_true", help="back up and replace an existing MC reference")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    wav = root / "voices/mc.wav"
    txt = root / "voices/mc.txt"
    if wav.exists() and not args.replace:
        print("MC reference exists; keeping it. Use --replace to back it up and regenerate.")
        return
    import numpy as np
    import soundfile as sf
    import torch
    from qwen_tts import Qwen3TTSModel

    torch.manual_seed(42)
    model = Qwen3TTSModel.from_pretrained(
        "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign",
        device_map="cuda:0" if torch.cuda.is_available() else "cpu",
        dtype=torch.bfloat16 if torch.cuda.is_available() else torch.float32,
    )
    audio, sr = model.generate_voice_design(
        text=TEXT, language="English", instruct=DIRECTION,
        max_new_tokens=360, temperature=0.8,
    )
    samples = np.asarray(audio[0])
    duration = len(samples) / sr
    if not np.isfinite(samples).all() or not 3 <= duration <= 30 or np.sqrt(np.mean(samples**2)) < 0.001:
        raise RuntimeError("Invalid reference output; existing voice was kept")
    backup = root / "target/voice-backups" / str(time.time_ns())
    if wav.exists():
        backup.mkdir(parents=True)
        shutil.copy2(wav, backup / wav.name)
        if txt.exists():
            shutil.copy2(txt, backup / txt.name)
        print(f"Previous reference saved to {backup}")
    wav.parent.mkdir(exist_ok=True)
    temporary = wav.with_suffix(".new.wav")
    sf.write(temporary, samples, sr, subtype="PCM_16")
    txt.write_text(TEXT + "\n")
    temporary.replace(wav)
    print(f"Created original neural MC reference: {wav} ({duration:.1f}s)")


if __name__ == "__main__":
    main()
