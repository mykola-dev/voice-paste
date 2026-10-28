# Embedded Settings assets

Source: Zed `37faef81f74fc1bb08be9939c4cffc8ede6ced2f` (2026-10-27).

- `one/one.json`: unmodified `assets/themes/one/one.json`, MIT (`one/LICENSE`).
- `fonts/IBMPlexSans-{Regular,SemiBold}.ttf`: unmodified Zed UI fonts,
  SIL Open Font License (`fonts/OFL.txt`).
- Component icons: GPUI Kit Assets 0.8.1, embedded SVG, MIT/ISC notices shipped
  alongside this directory by the packaging script.
- `tray/`: user-selected raster lips as native-size premultiplied BGRA blobs,
  compiled in with `include_bytes!`. The user states that they generated the
  artwork with AI and approved retaining it; provenance is in `tray/README.md`.

These files are compiled into the executable. No font/theme downloads at runtime.
Editor application source is not incorporated. Palette tokens are mapped onto
the compatible GPUI Kit component states; VoicePaste owns its Settings layout.
