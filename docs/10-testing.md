# Testing

> Archived legacy tests/commands: these projects and scripts were removed. Use the [Rust checks](build-windows.md#build-and-checks-windows-x64).

## Automated .NET tests

Run on Windows with the .NET 8 SDK and bundled Python or a supported installed Python (3.10–3.12):

```powershell
dotnet test tests/VoicePaste.Tests/VoicePaste.Tests.csproj
```

The suite covers settings, hotkeys, worker lifecycle/protocol, RAM/VRAM parsing, waveform history,
rendering, layout and optional diagnostics. Worker lifecycle tests use a fake Python worker, not
Whisper or CUDA. The valid-text clipboard integration test is skipped by default.

`AudioRecorderTests` includes real microphone capture. To avoid activating a microphone:

```powershell
dotnet test tests/VoicePaste.Tests/VoicePaste.Tests.csproj --filter 'FullyQualifiedName!~AudioRecorderTests'
```

For one test class, use `--filter 'FullyQualifiedName~SettingsManagerTests'`.

## Automated Python tests

```powershell
python -m unittest discover -s tests -p 'test_*.py' -v
```

These tests use the standard library and an in-process Whisper stub. They require Python 3.10+
but no third-party packages, GPU, model downloads, audio input or clipboard access.

- `tests/test_transcribe.py`: current `transcribe_audio(Path, model, ...)` API, language/decoding
  options, empty/error handling, quiet defaults, import safety, local-only model checks, and
  CUDA-library discovery for the active Python interpreter.
- `tests/test_worker_protocol.py`: real CLI/server JSON framing, Unicode, empty results, multiple
  requests, CUDA fallback, CUDA-only failures, and debug output kept separate from transcripts.

The obsolete exploratory streaming/CUDA/sample scripts and import-time paste probe were removed.
Use the worker CLI for manual transcription, or the C# paste harness below for a deliberate paste.

## Portable-build regression checks

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable_processes.ps1
```

These use mock deployment commands or isolated fixture processes, not the running VoicePaste app.
See [Portable build](09-portable-build.md#script-regression-checks).

## Explicit manual checks

- Microphone/transcription: run the app, dictate non-sensitive sample text, and verify cancellation,
  transcription and paste. This uses real audio and may download missing models.
- CLI transcription: `python src/transcribe/transcribe.py --input sample.wav --device cpu`.
  The result is intentionally printed to stdout; do not publish real dictation as test output.
- Paste: `dotnet run --project tests/PasteTest/PasteTest.csproj`. **This overwrites the clipboard
  and sends a real paste shortcut after a countdown.** Run only with a safe destination focused.
- Frame/cost investigations: the opt-in synthetic tests are documented under
  [Waveform diagnostics](07-configuration.md#waveform-performance-diagnostics-development).

## Before publishing

Keep recordings, real transcripts, generated configuration, secrets and system traces out of Git.
Ignore rules cover common recordings, logs, environment files, private keys and trace artifacts;
they do not remove files already tracked by Git. Review the staged diff before committing.

Error messages, dumps, screenshots and videos can contain local paths or other personal data.
Review media manually before publishing. Git author/committer email is also public commit metadata;
use GitHub's `noreply` address if you do not want to expose a personal email. Changing configuration
or deleting a file does not remove it from existing commits.
