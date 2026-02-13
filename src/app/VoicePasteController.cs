using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using VoicePaste.Audio;
using VoicePaste.Hotkey;
using VoicePaste.Paste;
using VoicePaste.Settings;
using VoicePaste.Transcription;

namespace VoicePaste;

/// <summary>
/// Application state.
/// </summary>
public enum AppState
{
    Idle,
    Recording,
    Transcribing
}

/// <summary>
/// Main controller that orchestrates the voice paste workflow.
/// Manages state machine: idle → recording → transcribing → idle.
/// </summary>
public class VoicePasteController : IDisposable
{
    private readonly AudioRecorder _audioRecorder;
    private TranscriptionService _transcriptionService;
    private ClipboardPaster _clipboardPaster;
    private readonly GlobalHotkeyManager _hotkeyManager;

    private AppSettings _settings;

    private AppState _state = AppState.Idle;
    private string? _currentRecordingPath;
    private IntPtr _focusedWindowBeforeRecording = IntPtr.Zero;
    private DateTime _lastHotkeyPress = DateTime.MinValue;
    private const int HotkeyDebounceMs = 300; // Debounce time in milliseconds

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);
    
    /// <summary>
    /// Fires when application state changes.
    /// </summary>
    public event EventHandler<AppState>? StateChanged;
    
    /// <summary>
    /// Fires when audio level changes during recording.
    /// </summary>
    public event EventHandler<float>? AudioLevelChanged;
    
    /// <summary>
    /// Fires when an error occurs.
    /// </summary>
    public event EventHandler<string>? ErrorOccurred;
    
    public AppState State
    {
        get => _state;
        private set
        {
            if (_state != value)
            {
                _state = value;
                StateChanged?.Invoke(this, _state);
            }
        }
    }
    
    public VoicePasteController(Window window, AppSettings settings)
    {
        _settings = SettingsManager.ValidateAndMigrate(settings);

        _audioRecorder = new AudioRecorder();
        _audioRecorder.LevelChanged += (s, level) => AudioLevelChanged?.Invoke(this, level);

        _transcriptionService = CreateTranscriptionService(_settings);
        _transcriptionService.CudaFallbackOccurred += (s, message) => 
        {
            Console.WriteLine($"[Controller] CUDA Fallback: {message}");
            ErrorOccurred?.Invoke(this, message);
        };
        _transcriptionService.RestartWorker();
        
        _clipboardPaster = CreateClipboardPaster(_settings);

        _hotkeyManager = new GlobalHotkeyManager(window);
        _hotkeyManager.HotkeyPressed += OnHotkeyPressed;

        RegisterConfiguredHotkey(_settings);
    }
    
    /// <summary>
    /// Handle hotkey press based on current state.
    /// </summary>
    private async void OnHotkeyPressed(object? sender, EventArgs e)
    {
        Console.WriteLine($"[Controller] Hotkey pressed! Current state: {State}");
        
        // Debounce: ignore rapid successive presses
        var now = DateTime.Now;
        var timeSinceLastPress = (now - _lastHotkeyPress).TotalMilliseconds;
        
        if (timeSinceLastPress < HotkeyDebounceMs)
        {
            Console.WriteLine($"[Controller] Hotkey ignored - debouncing ({timeSinceLastPress:F0}ms since last press)");
            return;
        }
        
        _lastHotkeyPress = now;
        
        try
        {
            switch (State)
            {
                case AppState.Idle:
                    StartRecording();
                    break;
                    
                case AppState.Recording:
                    await StopRecordingAndTranscribeAsync();
                    break;
                    
                case AppState.Transcribing:
                    // Ignore hotkey during transcription (non-interruptible)
                    Console.WriteLine("[Controller] Hotkey ignored - currently transcribing");
                    break;
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Controller] ERROR: {ex.Message}");
            ErrorOccurred?.Invoke(this, $"Error: {ex.Message}");
            State = AppState.Idle;
        }
    }
    
    /// <summary>
    /// Start audio recording.
    /// </summary>
    public void StartRecording()
    {
        if (State != AppState.Idle)
        {
            Console.WriteLine($"[Controller] Cannot start recording - current state is {State}");
            ErrorOccurred?.Invoke(this, $"Cannot start recording while {State.ToString().ToLower()}");
            return;
        }
        
        // Capture the currently focused window BEFORE we start recording
        _focusedWindowBeforeRecording = GetForegroundWindow();
        Console.WriteLine($"[Controller] Captured focus window: {_focusedWindowBeforeRecording}");
        
        Console.WriteLine("[Controller] Starting recording...");
        State = AppState.Recording;
        
        try
        {
            // Start preloading the transcription model while we record (once per session)
            _transcriptionService.StartPreloading();
            
            _audioRecorder.StartRecording();
            Console.WriteLine("[Controller] Recording started");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Controller] Failed to start recording: {ex.Message}");
            State = AppState.Idle;
            ErrorOccurred?.Invoke(this, ex.Message);
        }
    }
    
    /// <summary>
    /// Stop recording, transcribe audio, and paste result.
    /// </summary>
    public async Task StopRecordingAndTranscribeAsync()
    {
        if (State != AppState.Recording)
            throw new InvalidOperationException($"Cannot stop recording in state: {State}");
        
        // Stop recording
        Console.WriteLine("[Controller] Stopping recording...");
        _currentRecordingPath = _audioRecorder.StopRecording();
        Console.WriteLine($"[Controller] Recording saved to: {_currentRecordingPath}");
        
        State = AppState.Transcribing;
        
        try
        {
            // Transcribe audio
            Console.WriteLine("[Controller] Starting transcription...");
            string transcript = await _transcriptionService.TranscribeAsync(_currentRecordingPath);
            Console.WriteLine($"[Controller] Transcription complete. Text: '{transcript}'");
            
            // Paste result
            if (!string.IsNullOrWhiteSpace(transcript))
            {
                // Restore focus to the window that was active before recording
                if (_focusedWindowBeforeRecording != IntPtr.Zero)
                {
                    Console.WriteLine($"[Controller] Restoring focus to window: {_focusedWindowBeforeRecording}");
                    SetForegroundWindow(_focusedWindowBeforeRecording);
                    
                    // Wait for focus to actually switch
                    await Task.Delay(300);
                }
                
                Console.WriteLine("[Controller] Pasting text...");
                try
                {
                    await _clipboardPaster.PasteTextAsync(transcript);
                    Console.WriteLine("[Controller] Paste complete");
                }
                catch (Exception ex)
                {
                    var errorMsg = $"Failed to paste text: {ex.Message}";
                    Console.WriteLine($"[Controller] ERROR: {errorMsg}");
                    ErrorOccurred?.Invoke(this, errorMsg);
                }
            }
            else
            {
                Console.WriteLine("[Controller] No speech detected in recording");
                ErrorOccurred?.Invoke(this, "No speech detected. Please try again and speak clearly into the microphone.");
            }
        }
        finally
        {
            // Clean up temp file
            if (_currentRecordingPath != null && System.IO.File.Exists(_currentRecordingPath))
            {
                try
                {
                    System.IO.File.Delete(_currentRecordingPath);
                }
                catch
                {
                    // Ignore cleanup errors
                }
            }
            
            State = AppState.Idle;
        }
    }
    
    /// <summary>
    /// Cancel current recording without transcribing.
    /// </summary>
    public void CancelRecording()
    {
        if (State != AppState.Recording)
            return;
        
        // Stop recording
        _currentRecordingPath = _audioRecorder.StopRecording();
        
        // Clean up temp file
        if (_currentRecordingPath != null && System.IO.File.Exists(_currentRecordingPath))
        {
            try
            {
                System.IO.File.Delete(_currentRecordingPath);
            }
            catch
            {
                // Ignore cleanup errors
            }
        }
        
        State = AppState.Idle;
    }
    
    public void UpdateSettings(AppSettings settings)
    {
        var validated = SettingsManager.ValidateAndMigrate(settings);

        // Safe apply: changes will affect next transcription/paste.
        var old = _settings;
        _settings = validated;

        // Update paste behavior immediately (affects next paste);
        _clipboardPaster = CreateClipboardPaster(_settings);

        // Transcription changes apply for next transcription.
        _transcriptionService = CreateTranscriptionService(_settings);
        _transcriptionService.CudaFallbackOccurred += (s, message) => 
        {
            Console.WriteLine($"[Controller] CUDA Fallback: {message}");
            ErrorOccurred?.Invoke(this, message);
        };

        // Hotkey can be safely re-registered anytime.
        if (!string.Equals(old.Hotkey, _settings.Hotkey, StringComparison.OrdinalIgnoreCase))
        {
            RegisterConfiguredHotkey(_settings);
        }
    }

    private void RegisterConfiguredHotkey(AppSettings settings)
    {
        _hotkeyManager.UnregisterHotkey();

        if (!HotkeyParser.TryParse(settings.Hotkey, out var parsed))
        {
            // Fall back to ScrollLock.
            parsed = new ParsedHotkey(VirtualKeys.VK_SCROLL, 0);
        }

        _hotkeyManager.RegisterHotkey(parsed.Key, parsed.Modifiers);
    }

    private static TranscriptionService CreateTranscriptionService(AppSettings settings)
    {
        var device = settings.Device switch
        {
            TranscriptionDevice.Cpu => "cpu",
            TranscriptionDevice.CudaOnly => "cuda",
            TranscriptionDevice.CudaAuto => "cuda",
            _ => "cuda"
        };

        return new TranscriptionService(
            modelSize: settings.Model,
            device: device,
            languageMode: settings.LanguageMode,
            beamSize: settings.BeamSize,
            cudaAutoFallback: settings.Device == TranscriptionDevice.CudaAuto,
            initialPrompt: settings.CustomInitialPrompt,
            enableVad: settings.EnableVad);
    }

    private static ClipboardPaster CreateClipboardPaster(AppSettings settings)
    {
        return new ClipboardPaster(
            pasteShortcut: settings.PasteShortcut);
    }

    public void Dispose()
    {
        _audioRecorder?.Dispose();
        _hotkeyManager?.Dispose();
    }
}
