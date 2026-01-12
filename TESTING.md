# VoicePaste - Testing Guide

## Application Status: ✅ Running Successfully

The application has been successfully built and is ready for manual testing.

## Setup Complete

- ✅ .NET SDK 8.0.416 installed
- ✅ Python 3.12.10 installed  
- ✅ faster-whisper 1.2.1 installed with CUDA support
- ✅ Application builds with 0 warnings/errors
- ✅ Application starts successfully

## How to Run

```bash
cd D:\dev\ai\voice-agent
dotnet run --project src/app/VoicePaste.csproj
```

The application will:
1. Start in the background (no visible window)
2. Register ScrollLock as the global hotkey
3. Print state changes to console

## Testing Workflow

### 1. Start Recording
- **Action**: Press **ScrollLock** key
- **Expected**: Console output: "State changed: Recording"
- **Note**: Make sure your microphone is connected and permissions are granted

### 2. Speak into Microphone
- Speak clearly for a few seconds (e.g., "Hello, this is a test")
- There's NO auto-stop on silence - you control when to stop

### 3. Stop Recording & Transcribe
- **Action**: Press **ScrollLock** again
- **Expected**: 
  - Console output: "State changed: Transcribing"
  - Python worker starts (first run downloads ~1.5GB Whisper model)
  - After transcription: "State changed: Idle"

### 4. Check Paste
- Open any text editor (Notepad, VS Code, etc.)
- Click inside to focus
- The transcribed text should automatically paste via Ctrl+Shift+V

## Expected First Run

On the **first transcription**, faster-whisper will:
- Download the "medium" model (~1.5GB)
- May take 2-5 minutes depending on internet speed
- Progress shown in console stderr
- Subsequent runs will be instant (model cached)

## Troubleshooting

### Microphone Not Working
1. Check Windows Settings → Privacy → Microphone access
2. Verify default recording device in Sound settings
3. Test with Sound Recorder app first

### ScrollLock Not Working
- Try pressing it a few times
- Check if ScrollLock LED on keyboard lights up
- Some keyboards don't have ScrollLock - we'll add configuration later

### Transcription Fails
- First run: Wait for model download to complete
- Check console for error messages
- Verify Python 3.12 is being used: `py -3.12 --version`

### Paste Not Working
- Paste shortcut: Ctrl+Shift+V (not regular Ctrl+V)
- Some terminals use different paste shortcuts
- Try pasting in Notepad first to verify

## GPU Acceleration

If you have an NVIDIA GPU with CUDA:
- faster-whisper will automatically use GPU
- Much faster transcription (~2-5x speedup)
- Falls back to CPU if CUDA unavailable

Check GPU usage: Task Manager → Performance → GPU

## Console Output Example

```
State changed: Recording
State changed: Transcribing
State changed: Idle
```

## Stopping the Application

Press Ctrl+C in the console window

## Next Steps After Testing

Once basic functionality works:
- [ ] Add tray icon
- [ ] Add recording overlay with timer and audio levels
- [ ] Add configuration UI
- [ ] Add error notifications
- [ ] Support configurable hotkeys
