# Voice references for NeuTTS Air

NeuTTS Air clones the timbre from a short reference clip per voice.
Each voice here is a `.wav` + a matching `.txt` transcript:

```
voices/
  default.wav   default.txt   ← reference audio + exact transcript
  dj.wav        dj.txt
  mc.wav        mc.txt
  ...
```

## What NeuTTS wants

- **Mono WAV**, 16–48 kHz
- **3–15 seconds** of one speaker talking
- **Clean** — no music, no other speakers, minimal room/echo
- **A matching `.txt`** with the exact transcript (word-for-word)

## Where to get clean samples (recommended)

- **LibriVox on Internet Archive** — public-domain audiobook readings,
  one speaker at a time, clean studio-style recordings. Ideal source.
  <https://archive.org/details/librivoxaudio>
- **Mozilla Common Voice** — short CC0 clips, many languages and
  accents. <https://commonvoice.mozilla.org/>
- **Internet Archive open-source audio** — broader collection,
  quality varies. <https://archive.org/details/opensource_audio>

Grab a 10-second clip of someone talking cleanly, trim it in Audacity
or ffmpeg (`ffmpeg -ss 12 -t 10 -ac 1 -ar 44100 in.mp3 voices/my.wav`),
and paste the exact transcript into `voices/my.txt`.

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
