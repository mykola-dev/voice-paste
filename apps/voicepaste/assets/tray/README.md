# Default raster lips

User-approved state mapping (2026-10-28, revised):
- Idle: replacement artwork from `build/lips-source2.png`; the internal `bitten`
  asset name is retained. Its source hash and crop are recorded in extraction metadata.
- Recording and transcribing (also model preparation): `open` mouth (upper left).
- Outside the tray: the stable open-mouth application icon, regardless of state.
- The broad toothy smile was removed at the user's request.

Transparent full-resolution cutouts: `masters/lips-<face>.png`.
`masters/lips-extraction.json` records the source SHA-256 and crop regions.
Original shapes, asymmetry, colors and proportions are retained, without tracing.
White exterior removed using a red-perimeter mask with enclosed holes filled,
preserving teeth/tongue/mouth; boundary alpha estimated from local chroma and
the white matte removed. The user-supplied source is not required at runtime.

`lips-<face>-<size>.bgra`: native 16/20/24/28/32/40/48px premultiplied BGRA,
embedded using `include_bytes!`, no runtime image decoder. At 175% scale the
28px asset is used. Regenerate from the transparent masters, not a small icon:
trim only fully transparent exterior, fit proportionally into `size` pixels,
center on a transparent square with no extra inset,
resize premultiplied RGBA with Lanczos, then export B,G,R,A bytes (RGB multiplied
by alpha, rounded to nearest). No recoloring, mirroring, clipping of mouth corners
or aspect-ratio stretching.

`apps/voicepaste/assets/icon.ico` contains the recording artwork at
16/20/24/28/32/40/48/64/96/128/256px. `icon.png` is the 256px raster preview;
the redundant SVG wrapper around that same PNG was removed during cleanup.

Correctness fix (2026-10-28): CreateIcon monochrome AND-mask rows must be
WORD-aligned. The former packed-byte stride was wrong at 20/24/40px (including
an undersized input buffer); regression tests now cover every shipped size.
The 28px stride at 175% was already aligned, so this is not claimed as the
cause of the reported 175% appearance. Window classes now use resource 1,
separate from the stateful notification-area handles. Live shell acceptance
(Task Manager Processes/Details and tray settings) remains a user visual check.

## Provenance and rights

The user states that they generated this artwork with an AI image generator and
approved keeping it in the repository (2026-10-28). No specific generator/model,
third-party stock source or independent license verification is asserted here.
No Twemoji or Creative Commons attribution applies to the current lips.

Superseded Twemoji blobs, source masters and their unrelated license were removed
during pre-commit cleanup. Current transparent masters and extraction metadata
remain the editable source of the embedded raster assets.
