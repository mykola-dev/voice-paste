# Running VoicePaste

> Archived legacy run instructions: referenced launch scripts were removed. Use the [Rust guide](../README.md), not these commands.

## Development

```batch
dev-debug.bat
```

The script sets up bundled Python if necessary, builds the Debug configuration, and runs the app
with a console. It does not stop other running instances. Quit an older instance from its tray
menu first if it holds the configured hotkey.

Alternatively:

```powershell
dotnet run --project src/app/VoicePaste.csproj
```

For the self-contained portable build, see [Portable build](09-portable-build.md).

## Dictation

1. Focus the destination window.
2. Press the configured hotkey (default **ScrollLock**) to start recording.
3. Press it again to transcribe and paste, or **Esc** to discard the recording.
4. Right-click the tray icon for Settings or Quit.

## Logging and troubleshooting

Startup, state changes, warnings and errors are written to `%TEMP%\VoicePaste\voicepaste.log`.
Detailed component diagnostics are off by default. Enable **Settings → Logging → Enable debug
logging**, or set `VOICEPASTE_DEBUG=1` before launching. Python worker verbosity is selected when
the worker starts; a resident worker picks up a changed setting after its next restart.

Neither the
normal operation log nor any diagnostics intentionally log dictated text or clipboard contents.
Exception messages and stack traces can still include local paths; inspect logs before sharing.

- **Hotkey unavailable:** quit the other instance or choose another hotkey in Settings.
- **Microphone unavailable:** check Windows microphone permissions and the default input device.
- **CUDA unavailable:** check NVIDIA drivers and dependencies in the Python environment used by
  the worker. CUDA DLLs installed under that interpreter's `site-packages/nvidia` are discovered
  automatically, with a fallback to other user Python installations for native CUDA DLLs, without
  a hardcoded username or Python version. CUDA auto-fallback can use CPU;
  CUDA-only mode reports a failure instead.
- **Paste fails:** try the shortcut appropriate for the destination (`Ctrl+V`, `Ctrl+Shift+V`, or
  `Shift+Insert`). The text may remain in the clipboard if simulated input fails.

Use the tray's **Quit** command for a normal shutdown so the worker stops cleanly.
See [Testing](10-testing.md) for automated checks that do not perform dictation or paste.
