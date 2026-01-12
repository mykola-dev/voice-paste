# Configuration Specification

## Config File Location

```
%AppData%/VoicePaste/config.json
```

Example: `C:\Users\<user>\AppData\Roaming\VoicePaste\config.json`

## Default Configuration

```json
{
  "hotkey": "ScrollLock",
  "pasteShortcut": "Ctrl+Shift+V",
  "model": "medium",
  "device": "cuda",
  "restoreClipboard": true,
  "clipboardRestoreDelayMs": 400
}
```

## Configuration Options

### hotkey

Global hotkey to toggle recording.

| Value | Description |
|-------|-------------|
| `"ScrollLock"` | Default, dedicated key |
| `"F8"` | Function key alternative |
| `"Ctrl+Alt+Space"` | Modifier combination |

**Format:** Single key or `Modifier+Key` combination

### pasteShortcut

Keystroke sent to paste transcript.

| Value | Description |
|-------|-------------|
| `"Ctrl+Shift+V"` | Default, Windows Terminal |
| `"Ctrl+V"` | Standard Windows paste |
| `"Shift+Insert"` | Universal alternative |

### model

Whisper model size.

| Value | Size | Quality | Speed |
|-------|------|---------|-------|
| `"small"` | 466MB | Good | Fast |
| `"medium"` | 1.5GB | Very Good | Medium |
| `"large-v3"` | 3GB | Best | Slow |

### device

Transcription device preference.

| Value | Behavior |
|-------|----------|
| `"cuda"` | GPU-first, fallback to CPU on error |
| `"cpu"` | Always use CPU |

### restoreClipboard

Whether to restore clipboard after paste.

| Value | Behavior |
|-------|----------|
| `true` | Save → Paste → Restore (default) |
| `false` | Overwrite clipboard |

### clipboardRestoreDelayMs

Delay before restoring clipboard (ms).

| Value | Use Case |
|-------|----------|
| `400` | Default, works for most apps |
| `200` | Faster, may fail on slow apps |
| `800` | Safer for slow applications |

## Environment Variables

Override config file settings:

```bash
# Override model path (use local model)
VOICEPASTE_MODEL_PATH=/path/to/whisper/models

# Force device
VOICEPASTE_DEVICE=cpu

# Enable debug logging
VOICEPASTE_DEBUG=1
```

## Settings Manager

### Loading

```csharp
public class SettingsManager
{
    private static readonly string ConfigPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "VoicePaste",
        "config.json"
    );
    
    public Settings Load()
    {
        if (!File.Exists(ConfigPath))
            return Settings.Default;
        
        var json = File.ReadAllText(ConfigPath);
        return JsonSerializer.Deserialize<Settings>(json) 
               ?? Settings.Default;
    }
}
```

### Saving

```csharp
public void Save(Settings settings)
{
    var dir = Path.GetDirectoryName(ConfigPath)!;
    Directory.CreateDirectory(dir);
    
    var json = JsonSerializer.Serialize(settings, new JsonSerializerOptions
    {
        WriteIndented = true
    });
    
    File.WriteAllText(ConfigPath, json);
}
```

## Validation

### On Load

```csharp
public Settings Validate(Settings settings)
{
    // Ensure valid hotkey
    if (!IsValidHotkey(settings.Hotkey))
        settings.Hotkey = "ScrollLock";
    
    // Ensure valid model
    if (!ValidModels.Contains(settings.Model))
        settings.Model = "medium";
    
    // Ensure valid device
    if (settings.Device != "cuda" && settings.Device != "cpu")
        settings.Device = "cuda";
    
    // Clamp delay
    settings.ClipboardRestoreDelayMs = Math.Clamp(
        settings.ClipboardRestoreDelayMs, 100, 2000);
    
    return settings;
}
```

## Migration

### Version Field

```json
{
  "version": 1,
  ...
}
```

### Future Migrations

When config schema changes:
1. Check version field
2. Apply migrations sequentially
3. Update version
4. Save migrated config
