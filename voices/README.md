# Voice references for Qwen3-TTS

Qwen3-TTS clones the timbre from a short reference clip per voice.
Each voice here is a `.wav` + a matching `.txt` transcript:

```
voices/
  default.wav   default.txt   ← reference audio + exact transcript
  dj.wav        dj.txt
  mc.wav        mc.txt
  ...
```

Impulse Instruct doesn't ship voice recordings — only these transcripts.
The `.wav` files are created on your machine: either robotic placeholders
from `./scripts/generate-voices.sh`, or (much better) real recordings you
pick yourself, as below.

## What Qwen3-TTS wants

- **Mono WAV**, 16–48 kHz (24 kHz is the model's native rate)
- **3–15 seconds** of one speaker talking — ~10 s is the sweet spot
- **Clean** — no music, no other speakers, minimal room/echo
- **A matching `.txt`** with the exact transcript, word for word.  Qwen
  uses the transcript to line the voice up with the audio; a mismatch
  shows up as slurred or garbled starts on every line it speaks.

## Where to get clean samples (recommended)

- **LibriVox on Internet Archive** — public-domain audiobook readings,
  one speaker at a time, clean studio-style recordings. Ideal source.
  <https://archive.org/details/librivoxaudio>
- **Mozilla Common Voice** — short CC0 clips, many languages and
  accents. <https://commonvoice.mozilla.org/>
- **Internet Archive open-source audio** — broader collection,
  quality varies; check each item's licence.
  <https://archive.org/details/opensource_audio>

### Step by step (LibriVox)

1. On <https://librivox.org>, pick a **solo** recording (one reader for
   the whole book — the catalogue lists the reader per section).  Listen
   to a minute first: you want a steady, dry voice with no background hum.
2. Download a chapter MP3 from the book's archive.org page.
3. Find 8–12 s of continuous speech without long pauses and note the
   start time.  Cut it to mono 24 kHz:

   ```
   ffmpeg -ss 00:01:12 -t 10 -i chapter01.mp3 -ac 1 -ar 24000 voices/mc.wav
   ```

4. Write the exact words of that clip into `voices/mc.txt`.  LibriVox
   reads public-domain texts, so the book's "Online text" link (usually
   Project Gutenberg) has the wording to copy from — then match it to what
   the reader actually says, including any slips.

Using an existing name (`default`, `dj`, `mc`, `narrator`, `robot`)
swaps that voice everywhere it's used, with no settings change; a new
name (e.g. `voices/selecta.wav` + `selecta.txt`) becomes selectable in a
TTS module's voice field.  The TTS server notices an overwritten file
without a restart.

**Licence and consent:** LibriVox recordings are public domain and Common
Voice clips are CC0, so both are safe to clone.  Don't clone a real
person's voice without their permission, and check the licence of
anything else you pull from archive.org.

## Character voices (MC/DJ vibe, with caveats)

Search archive.org for `"rave MC"`, `"jungle MC"`, `"pirate radio"`,
`"hip hop radio"` etc. for character clips. Catch: most have music
underneath and will clone poorly. You'll need to strip the music
first (Audacity "Vocal Reduction and Isolation", or a demixer like
Demucs). For a quick demo, LibriVox voices clone much better.

## Regenerate the default espeak-ng clips

If you just want placeholder voices to develop against:

```
./scripts/generate-voices.sh
```

This synthesises `default / dj / mc / narrator / robot` via espeak-ng
— robotic but functional.
