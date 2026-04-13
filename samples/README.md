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
