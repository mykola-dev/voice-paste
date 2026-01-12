using System;
using System.IO;
using NAudio.Wave;

namespace VoicePaste.Audio;

/// <summary>
/// Records audio from microphone using WASAPI.
/// Output format: 16kHz mono 16-bit PCM WAV (Whisper-compatible).
/// </summary>
public class AudioRecorder : IDisposable
{
    private const int SampleRate = 16000;
    private const int Channels = 1;
    private const int BitsPerSample = 16;
    
    private WaveInEvent? _waveIn;
    private WaveFileWriter? _writer;
    private string? _tempFilePath;
    private bool _isRecording;
    
    /// <summary>
    /// Fires when audio level changes (for visualization).
    /// Value is RMS level (0.0 to 1.0).
    /// </summary>
    public event EventHandler<float>? LevelChanged;
    
    public bool IsRecording => _isRecording;
    
    /// <summary>
    /// Start recording from default microphone.
    /// </summary>
    /// <exception cref="InvalidOperationException">Already recording.</exception>
    /// <exception cref="AudioRecordingException">No microphone available or access denied.</exception>
    public void StartRecording()
    {
        if (_isRecording)
            throw new InvalidOperationException("Already recording");
        
        // Check if microphone is available
        if (WaveInEvent.DeviceCount == 0)
        {
            throw new AudioRecordingException(
                "No microphone detected. Please connect a microphone and try again.",
                AudioErrorType.NoDeviceAvailable
            );
        }
        
        try
        {
            // Create temp file
            var tempDir = Path.Combine(Path.GetTempPath(), "VoicePaste");
            Directory.CreateDirectory(tempDir);
            _tempFilePath = Path.Combine(tempDir, $"recording_{DateTime.Now:yyyyMMdd_HHmmss}.wav");
            
            // Configure audio input
            _waveIn = new WaveInEvent
            {
                WaveFormat = new WaveFormat(SampleRate, BitsPerSample, Channels),
                BufferMilliseconds = 100  // 100ms buffer for responsive level meter
            };
            
            // Log microphone info
            string micName = "Default Microphone";
            try
            {
                if (_waveIn.DeviceNumber >= 0 && _waveIn.DeviceNumber < WaveInEvent.DeviceCount)
                {
                    var capabilities = WaveInEvent.GetCapabilities(_waveIn.DeviceNumber);
                    micName = capabilities.ProductName;
                }
            }
            catch { /* Ignore errors getting device name */ }
            
            Console.WriteLine($"[Audio] Using microphone: {micName}");
            Console.WriteLine($"[Audio] Format: {SampleRate}Hz, {Channels} channel(s), {BitsPerSample}-bit");
            
            _writer = new WaveFileWriter(_tempFilePath, _waveIn.WaveFormat);
            
            _waveIn.DataAvailable += OnDataAvailable;
            _waveIn.RecordingStopped += OnRecordingStopped;
            
            _waveIn.StartRecording();
            _isRecording = true;
        }
        catch (UnauthorizedAccessException ex)
        {
            CleanupResources();
            throw new AudioRecordingException(
                "Microphone access denied. Please grant microphone permission in Windows Settings (Privacy > Microphone).",
                AudioErrorType.AccessDenied,
                ex
            );
        }
        catch (NAudio.MmException ex)
        {
            CleanupResources();
            throw new AudioRecordingException(
                $"Failed to start recording: {ex.Message}. The microphone may be in use by another application.",
                AudioErrorType.DeviceInUse,
                ex
            );
        }
        catch (Exception ex) when (ex is not AudioRecordingException)
        {
            CleanupResources();
            throw new AudioRecordingException(
                $"Failed to start recording: {ex.Message}",
                AudioErrorType.Unknown,
                ex
            );
        }
    }
    
    /// <summary>
    /// Stop recording and return path to recorded WAV file.
    /// </summary>
    /// <returns>Path to WAV file.</returns>
    /// <exception cref="InvalidOperationException">Not recording.</exception>
    public string StopRecording()
    {
        if (!_isRecording)
            throw new InvalidOperationException("Not recording");
        
        _waveIn?.StopRecording();
        _waveIn?.Dispose();
        _waveIn = null;
        
        _writer?.Dispose();
        _writer = null;
        
        _isRecording = false;
        
        return _tempFilePath!;
    }
    
    private void OnDataAvailable(object? sender, WaveInEventArgs e)
    {
        // Write to WAV file
        _writer?.Write(e.Buffer, 0, e.BytesRecorded);
        
        // Calculate RMS level for visualization
        float level = CalculateRmsLevel(e.Buffer, e.BytesRecorded);
        LevelChanged?.Invoke(this, level);
    }
    
    private void OnRecordingStopped(object? sender, StoppedEventArgs e)
    {
        if (e.Exception != null)
        {
            Console.WriteLine($"[Audio] Recording stopped with error: {e.Exception.Message}");
            CleanupResources();
            throw new AudioRecordingException(
                "Recording stopped unexpectedly. The microphone may have been disconnected.",
                AudioErrorType.RecordingStopped,
                e.Exception
            );
        }
    }
    
    private void CleanupResources()
    {
        try
        {
            _waveIn?.Dispose();
            _waveIn = null;
            _writer?.Dispose();
            _writer = null;
            _isRecording = false;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Audio] Error during cleanup: {ex.Message}");
        }
    }
    
    /// <summary>
    /// Calculate RMS (Root Mean Square) level from PCM buffer.
    /// </summary>
    private static float CalculateRmsLevel(byte[] buffer, int count)
    {
        if (count == 0) return 0f;
        
        float sum = 0f;
        int sampleCount = count / 2; // 16-bit = 2 bytes per sample
        
        for (int i = 0; i < count - 1; i += 2)
        {
            short sample = BitConverter.ToInt16(buffer, i);
            float normalized = sample / 32768f; // Normalize to -1.0 to 1.0
            sum += normalized * normalized;
        }
        
        float rms = (float)Math.Sqrt(sum / sampleCount);
        
        // Amplify for better visualization (multiply by 5, clamp to 1.0)
        return Math.Min(rms * 5f, 1f);
    }
    
    public void Dispose()
    {
        if (_isRecording)
        {
            try { StopRecording(); }
            catch { /* Ignore cleanup errors */ }
        }
        
        _waveIn?.Dispose();
        _writer?.Dispose();
    }
}

/// <summary>
/// Type of audio recording error.
/// </summary>
public enum AudioErrorType
{
    NoDeviceAvailable,
    AccessDenied,
    DeviceInUse,
    RecordingStopped,
    Unknown
}

/// <summary>
/// Exception thrown when audio recording fails.
/// </summary>
public class AudioRecordingException : Exception
{
    public AudioErrorType ErrorType { get; }
    
    public AudioRecordingException(string message, AudioErrorType errorType) 
        : base(message)
    {
        ErrorType = errorType;
    }
    
    public AudioRecordingException(string message, AudioErrorType errorType, Exception inner) 
        : base(message, inner)
    {
        ErrorType = errorType;
    }
}
