# Samples

Drop-in directory for user-provided audio samples (WAV, 44.1 kHz).

The app scans sub-folders at runtime — nothing is bundled with the
binary, so pull whichever packs you like and unzip them here.

## Amen breaks

The `AMEN` sampler module reads `.wav` files from `samples/amen/`.
Good free sources on the Internet Archive:

- https://archive.org/details/amen-breaks
- https://archive.org/details/amen-breaks-compilation

Download a `.zip`, extract the WAVs into `samples/amen/`, and the
module's file picker will list them automatically.  The module plays
one sample at a time — pitch-shift and loop mode are controlled from
its panel.

## Textures (granular voice)

The `GRAN` granular texture module reads `.wav` files from
`samples/textures/`.  Unlike the amen sampler, the granular voice is
happier with longer, slower material — pads, field recordings, drones,
vowel tones, ambient loops.  Anything with slowly-evolving spectral
content grains well.

Archive.org collections worth poking at:

- https://archive.org/details/opensource_audio — huge mixed-bag of
  speech, field recordings, radio drama, music (varying licenses;
  check each item).
- https://archive.org/details/audio_ambient — ambient and drone
  uploads.

Outside of archive.org:

- https://freesound.org — the canonical source for short one-shots
  and field recordings.  CC-licensed; filter for CC0 if you want the
  least friction.  Grain-friendly search terms: *drone*, *pad*,
  *texture*, *field*, *reverb tail*.

Same workflow as amen: drop `.wav` files into `samples/textures/`
and the module's picker will list them.  Size doesn't matter much —
the voice only reads small random windows at a time.
