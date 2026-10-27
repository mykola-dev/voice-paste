using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using VoicePaste.Audio;
using VoicePaste.Hotkey;
using VoicePaste.Logging;
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
    private readonly RecordingCancelHotkey _cancelHotkey;

    private AppSettings _settings;
    private AppSettings _transcriptionSettings;
    private bool _disposed;

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
                _cancelHotkey.SetRecording(value == AppState.Recording);
                _state = value;
                StateChanged?.Invoke(this, _state);
            }
        }
    }
    
    public VoicePasteController(Window window, AppSettings settings)
    {
        _settings = SettingsManager.ValidateAndMigrate(settings);
        _transcriptionSettings = _settings;

        _audioRecorder = new AudioRecorder();
        _audioRecorder.LevelChanged += (s, level) => AudioLevelChanged?.Invoke(this, level);

        _transcriptionService = CreateTranscriptionService(_settings);
        SubscribeToTranscriptionEvents();
        
        _clipboardPaster = CreateClipboardPaster(_settings);

        _hotkeyManager = new GlobalHotkeyManager(window);
        _hotkeyManager.HotkeyPressed += OnHotkeyPressed;

        _cancelHotkey = new RecordingCancelHotkey(window.Dispatcher);
        _cancelHotkey.CancelRequested += OnCancelRequested;

        RegisterConfiguredHotkey(_settings);
    }

    // Called after application UI and event subscriptions have been initialized.
    public void Initialize()
    {
        if (_settings.KeepInMemory) _transcriptionService.StartPreloading();
    }

    public Task<WorkerMemoryUsage> GetMemoryUsageAsync(System.Threading.CancellationToken cancellationToken = default)
        => _transcriptionService.GetMemoryUsageAsync(cancellationToken);

    private void SubscribeToTranscriptionEvents()
    {
        _transcriptionService.CudaFallbackOccurred += (_, message) => ErrorOccurred?.Invoke(this, message);
        _transcriptionService.PreloadingFailed += (_, message) => ErrorOccurred?.Invoke(this, $"Model loading failed: {message}");
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
    /// Handle Escape cancellation outside the low-level keyboard callback.
    /// </summary>
    private void OnCancelRequested(object? sender, EventArgs e)
    {
        if (_disposed || State != AppState.Recording) return;
        try
        {
            Console.WriteLine("[Controller] Escape pressed - discarding recording");
            CancelRecording();
        }
        catch (Exception ex)
        {
            Logger.Error("Controller", "Failed to cancel recording", ex);
            ErrorOccurred?.Invoke(this, $"Failed to cancel recording: {ex.Message}");
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
        
        try
        {
            // Start preloading the transcription model while we record (once per session)
            _transcriptionService.StartPreloading();
            
            _audioRecorder.StartRecording();
            // Publish the recording state only once cancellation can actually stop the mic.
            // Showing the overlay can pump messages, including a queued Escape cancellation.
            State = AppState.Recording;
            Console.WriteLine("[Controller] Recording started");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Controller] Failed to start recording: {ex.Message}");
            try { DiscardAudioRecording(); }
            catch (Exception cleanupError) { Logger.Warning("Controller", $"Recording cleanup failed: {cleanupError.Message}"); }
            _focusedWindowBeforeRecording = IntPtr.Zero;
            try { ApplyIdleTranscriptionSettings(); }
            finally { State = AppState.Idle; }
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
            Console.WriteLine($"[Controller] Transcription complete. Text length: {transcript.Length}");
            
            // Paste result
            if (!string.IsNullOrWhiteSpace(transcript))
            {
                // Restore focus to the window that was active before recording
                if (_focusedWindowBeforeRecording != IntPtr.Zero)
                {
                    Console.WriteLine($"[Controller] Restoring focus to window: {_focusedWindowBeforeRecording}");
                    bool focusRestored = SetForegroundWindow(_focusedWindowBeforeRecording);
                    if (!focusRestored)
                    {
                        Logger.Warning("Controller", $"SetForegroundWindow({_focusedWindowBeforeRecording}) returned false");
                    }
                    
                    // Give Windows a short message-pump turn without adding a visible pre-paste stall.
                    await Task.Delay(50);
                }
                
                Console.WriteLine("[Controller] Pasting text...");
                try
                {
                    bool pasted = await _clipboardPaster.PasteTextAsync(transcript);
                    if (pasted)
                    {
                        Console.WriteLine("[Controller] Paste complete");
                    }
                    else
                    {
                        Console.WriteLine("[Controller] WARNING: Paste did not complete. Transcript may still be available in clipboard.");
                        Logger.Warning("Controller", "Paste did not complete. Suppressing user-facing error dialog.");
                    }
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[Controller] WARNING: Unexpected paste error suppressed: {ex.Message}");
                    Logger.Warning("Controller", $"Unexpected paste error suppressed: {ex}");
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
            await DeleteRecordingFileAsync(_currentRecordingPath);

            ApplyIdleTranscriptionSettings();
            State = AppState.Idle;
        }
    }

    private static async Task DeleteRecordingFileAsync(string? recordingPath)
    {
        if (recordingPath == null)
        {
            return;
        }

        try
        {
            await Task.Run(() =>
            {
                if (System.IO.File.Exists(recordingPath))
                {
                    System.IO.File.Delete(recordingPath);
                }
            });
        }
        catch
        {
            // Ignore cleanup errors
        }
    }
    
    /// <summary>
    /// Cancel current recording without transcribing.
    /// </summary>
    public void CancelRecording()
    {
        if (State != AppState.Recording)
            return;

        try
        {
            // Stop and discard without ever invoking transcription or clipboard/paste.
            DiscardAudioRecording();
        }
        finally
        {
            _currentRecordingPath = null;
            _focusedWindowBeforeRecording = IntPtr.Zero;
            try { ApplyIdleTranscriptionSettings(); }
            finally { State = AppState.Idle; }
        }
    }

    private void DiscardAudioRecording()
    {
        try
        {
            if (_audioRecorder.IsRecording) _currentRecordingPath = _audioRecorder.StopRecording();
        }
        catch
        {
            _audioRecorder.Dispose();
            throw;
        }
        finally
        {
            if (_currentRecordingPath != null && System.IO.File.Exists(_currentRecordingPath))
            {
                try { System.IO.File.Delete(_currentRecordingPath); }
                catch { /* Ignore cleanup errors. */ }
            }
            _currentRecordingPath = null;
        }
    }
    
    public void UpdateSettings(AppSettings settings)
    {
        var validated = SettingsManager.ValidateAndMigrate(settings);

        // Safe apply: changes will affect next transcription/paste.
        var old = _settings;
        _settings = validated;

        // Update paste behavior immediately (affects next paste);
        _clipboardPaster = CreateClipboardPaster(_settings);

        // Never replace a worker in the middle of recording/transcription.
        if (State == AppState.Idle) ApplyIdleTranscriptionSettings();

        // Hotkey can be safely re-registered anytime.
        if (!string.Equals(old.Hotkey, _settings.Hotkey, StringComparison.OrdinalIgnoreCase))
        {
            RegisterConfiguredHotkey(_settings);
        }
    }

    private void ApplyIdleTranscriptionSettings()
    {
        if (_disposed) return;
        if (RequiresWorkerReplacement(_transcriptionSettings, _settings))
        {
            _transcriptionService.Dispose();
            _transcriptionService = CreateTranscriptionService(_settings);
            SubscribeToTranscriptionEvents();
        }
        else
        {
            // Also releases a preloaded worker after a cancelled/failed recording.
            _transcriptionService.SetKeepInMemory(_settings.KeepInMemory);
        }
        _transcriptionSettings = _settings;
        if (_settings.KeepInMemory) _transcriptionService.StartPreloading();
    }

    internal static bool RequiresWorkerReplacement(AppSettings previous, AppSettings next) =>
        previous.Model != next.Model || previous.Device != next.Device ||
        previous.LanguageMode != next.LanguageMode || previous.BeamSize != next.BeamSize ||
        previous.CustomInitialPrompt != next.CustomInitialPrompt || previous.EnableVad != next.EnableVad;

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
            enableVad: settings.EnableVad,
            keepInMemory: settings.KeepInMemory);
    }

    private static ClipboardPaster CreateClipboardPaster(AppSettings settings)
    {
        return new ClipboardPaster(
            pasteShortcut: settings.PasteShortcut);
    }

    public void Dispose()
    {
        _disposed = true;
        _cancelHotkey.Dispose();
        _transcriptionService.Dispose();
        _audioRecorder?.Dispose();
        _hotkeyManager?.Dispose();
    }
}
