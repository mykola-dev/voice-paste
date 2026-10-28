# Default raster lips

User-approved state mapping (2026-10-28):
- `idle`: broad toothy smile (upper right of supplied sheet).
- `recording`: open mouth (upper left); also the application icon.
- `transcribing`: bitten lip (bottom).

Transparent full-resolution cutouts: `masters/lips-<state>.png`.
`masters/lips-extraction.json` records the source SHA-256 and crop regions.
Original shapes, asymmetry, colors and proportions are retained, without tracing.
White exterior removed using a red-perimeter mask with enclosed holes filled,
preserving teeth/tongue/mouth; boundary alpha estimated from local chroma and
the white matte removed. The user-supplied source is not required at runtime.

`lips-<state>-<size>.bgra`: native 16/20/24/28/32/40/48px premultiplied BGRA,
embedded using `include_bytes!`, no runtime image decoder. At 175% scale the
28px asset is used. Regenerate from the transparent masters, not a small icon:
fit proportionally into `(size - 2)` pixels, center on a transparent square,
resize premultiplied RGBA with Lanczos, then export B,G,R,A bytes (RGB multiplied
by alpha, rounded to nearest). No recoloring, mirroring or aspect-ratio stretching.

`apps/voicepaste/assets/icon.ico` contains the recording artwork at
16/20/24/28/32/40/48/64/96/128/256px. `icon.png` is the 256px raster preview;
the redundant SVG wrapper around that same PNG was removed during cleanup.

## Provenance and rights

The user states that they generated this artwork with an AI image generator and
approved keeping it in the repository (2026-10-28). No specific generator/model,
third-party stock source or independent license verification is asserted here.
No Twemoji or Creative Commons attribution applies to the current lips.

Superseded Twemoji blobs, source masters and their unrelated license were removed
during pre-commit cleanup. Current transparent masters and extraction metadata
remain the editable source of the embedded raster assets.
