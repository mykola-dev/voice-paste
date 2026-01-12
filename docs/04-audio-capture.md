# Audio Capture Specification

## Requirements

- **No silence detection** - Record until manual stop
- **Format compatible with Whisper** - 16kHz mono preferred
- **Low latency** - Responsive to hotkey
- **Level metering** - For overlay visualization

## Audio Format

| Parameter | Value | Reason |
|-----------|-------|--------|
| Sample Rate | 16000 Hz | Whisper native rate |
| Channels | 1 (Mono) | Whisper expectation |
| Bit Depth | 16-bit | Standard PCM |
| Format | WAV | Simple, no encoding needed |

## Implementation (NAudio + WASAPI)

### Recording Flow

```csharp
public class AudioRecorder : IDisposable
{
    private WaveInEvent _waveIn;
    private WaveFileWriter _writer;
    private string _tempPath;
    
    public void StartRecording()
    {
        _tempPath = Path.GetTempFileName() + ".wav";
        
        _waveIn = new WaveInEvent
        {
            WaveFormat = new WaveFormat(16000, 16, 1)
        };
        
        _writer = new WaveFileWriter(_tempPath, _waveIn.WaveFormat);
        
        _waveIn.DataAvailable += OnDataAvailable;
        _waveIn.StartRecording();
    }
    
    public string StopRecording()
    {
        _waveIn.StopRecording();
        _writer.Dispose();
        return _tempPath;
    }
    
    private void OnDataAvailable(object sender, WaveInEventArgs e)
    {
        _writer.Write(e.Buffer, 0, e.BytesRecorded);
        
        // Calculate level for meter
        float level = CalculateRmsLevel(e.Buffer, e.BytesRecorded);
        LevelChanged?.Invoke(this, level);
    }
}
```

### Level Metering

Calculate RMS for visualization:

```csharp
private float CalculateRmsLevel(byte[] buffer, int count)
{
    float sum = 0;
    for (int i = 0; i < count; i += 2)
    {
        short sample = BitConverter.ToInt16(buffer, i);
        float normalized = sample / 32768f;
        sum += normalized * normalized;
    }
    return (float)Math.Sqrt(sum / (count / 2));
}
```

## Device Selection

### Default Behavior
- Use system default input device
- No device selection in MVP

### Future Enhancement
- Add device dropdown in settings
- Remember last used device

## Error Handling

| Error | Handling |
|-------|----------|
| No input device | Show error in settings, prevent recording |
| Device in use | Retry once, then show error |
| Permission denied | Show Windows privacy settings link |

## Temp File Management

- Create in `%TEMP%/VoicePaste/`
- Delete after successful transcription
- Clean up orphaned files on startup

## Performance Considerations

- Buffer size: 100ms default (responsive level meter)
- Don't process audio during recording (just save)
- Async file writes to prevent blocking
