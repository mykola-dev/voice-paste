using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using VoicePaste.Settings;

namespace VoicePaste.Overlay;

/// <summary>
/// Recording overlay window that shows recording status, timer, and audio level.
/// </summary>
public partial class RecordingOverlay : Window
{
    private readonly DispatcherTimer _timer;
    private bool _renderingAudio;
    private TimeSpan? _lastRenderingTime;
    private readonly LevelMeterSmoother _levelSmoother = new();
    private float _latestAudioLevel;
    private float _pendingAudioPeak;
    private RecordingAnimation _recordingAnimation;
    private DateTime _recordingStartTime;
    private bool _isRecording;
    private OverlayFrameDiagnostics? _frameDiagnostics;
    private FrameDiagnosticsWriter? _frameDiagnosticsWriter;
    private DispatcherFrameObserver? _dispatcherObserver;


    public RecordingOverlay() : this(SelectRenderMode(
        Environment.GetEnvironmentVariable("VOICEPASTE_OVERLAY_SOFTWARE_RENDERING"))) { }

    internal static RenderMode SelectRenderMode(string? softwareRendering)
        => softwareRendering == "0" ? RenderMode.Default : RenderMode.SoftwareOnly;

    internal RecordingOverlay(RenderMode renderMode)
    {
        InitializeComponent();

        // Ensure overlay never steals focus and is click-through.
        SourceInitialized += (_, _) =>
        {
            ApplyNonActivatingClickThroughStyle();
            // Layered hardware targets read back through D3D/GDI before presentation. For this
            // small transparent overlay, software rendering avoids that round trip. Keep the
            // override local to this window, not the app or CUDA worker; "0" opts out for A/B.
            if (renderMode == RenderMode.SoftwareOnly &&
                HwndSource.FromHwnd(new WindowInteropHelper(this).Handle) is HwndSource source)
                source.CompositionTarget.RenderMode = renderMode;
        };
        
        // Position at top-right corner
        PositionWindow();
        
        // Setup timer for recording duration
        _timer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(100)
        };
        _timer.Tick += Timer_Tick;
    }

    internal string EnableFrameDiagnostics(string? path = null)
    {
        if (_frameDiagnosticsWriter != null) return _frameDiagnosticsWriter.Path;
        path ??= Path.Combine(Path.GetTempPath(), "VoicePaste", $"overlay-frames-{Environment.ProcessId}.jsonl");
        _frameDiagnosticsWriter = new FrameDiagnosticsWriter(path);
        _dispatcherObserver = new DispatcherFrameObserver(Dispatcher);
        double? targetFps = double.TryParse(Environment.GetEnvironmentVariable("VOICEPASTE_FRAME_TARGET_FPS"),
            NumberStyles.Float, CultureInfo.InvariantCulture, out var fps) && fps is > 0 and <= 1000 ? fps : null;
        _frameDiagnostics = new OverlayFrameDiagnostics(_frameDiagnosticsWriter.Publish, GetFrameContext, targetFps);
        VoiceWaveform.Diagnostics = _frameDiagnostics;
        if (_isRecording)
        {
            _dispatcherObserver.Start();
            _frameDiagnostics.Start(Stopwatch.GetElapsedTime(0));
        }
        return path;
    }

    internal Task CompleteFrameDiagnostics()
    {
        _frameDiagnostics?.Stop(Stopwatch.GetElapsedTime(0));
        _dispatcherObserver?.Dispose();
        _frameDiagnosticsWriter?.Dispose();
        return _frameDiagnosticsWriter?.Completion ?? Task.CompletedTask;
    }

    private OverlayFrameContext GetFrameContext()
    {
        var dpi = VisualTreeHelper.GetDpi(VoiceWaveform);
        var surface = VoiceWaveform.GetDiagnostics();
        var source = PresentationSource.FromVisual(this) as HwndSource;
        return new(_recordingAnimation.ToString(), ActualWidth, ActualHeight, Left, Top,
            dpi.DpiScaleX, dpi.DpiScaleY, RenderCapability.Tier >> 16,
            RenderOptions.ProcessRenderMode.ToString(), source?.CompositionTarget?.RenderMode.ToString() ?? "unknown",
            source?.Handle.ToString("X") ?? "0", surface.Width, surface.Height, surface.Bytes, surface.Generation,
            surface.HistoryCount, surface.Queue, _dispatcherObserver?.Snapshot());
    }

    public void SetRecordingAnimation(RecordingAnimation animation)
    {
        animation = Enum.IsDefined(animation) ? animation : RecordingAnimation.LevelBar;
        if (_recordingAnimation == animation) return;
        _recordingAnimation = animation;
        AudioLevelMeter.Visibility = animation == RecordingAnimation.LevelBar ? Visibility.Visible : Visibility.Collapsed;
        VoiceWaveform.Visibility = animation == RecordingAnimation.Waveform ? Visibility.Visible : Visibility.Collapsed;
        _levelSmoother.Reset();
        AudioLevelScale.ScaleX = 0;
        VoiceWaveform.Reset();
    }

    public void SetIndicatorColor(string color)
    {
        var selected = IndicatorColors.Parse(color);
        var brush = new SolidColorBrush(selected);
        brush.Freeze();
        AudioLevelBar.Fill = brush;
        VoiceWaveform.SetColor(selected);
        foreach (var bar in new[] { SpinnerBar1, SpinnerBar2, SpinnerBar3, SpinnerBar4, SpinnerBar5 })
            bar.Fill = brush;
    }

    private void PositionWindow()
    {
        // Position at top-right corner with some margin
        var workArea = SystemParameters.WorkArea;
        Left = workArea.Right - Width - 20;
        Top = workArea.Top + 20;
    }

    /// <summary>
    /// Show overlay in recording state.
    /// </summary>
    public void ShowRecording()
    {
        _isRecording = true;
        _recordingStartTime = DateTime.Now;
        
        // Hide transcribing UI and show recording UI
        TranscribingOverlay.Visibility = Visibility.Collapsed;
        RecordingContent.Visibility = Visibility.Visible;
        TimerText.Text = "00:00";

        ResetAudioLevel();
        _dispatcherObserver?.Start();
        _frameDiagnostics?.Start(Stopwatch.GetElapsedTime(0));
        _lastRenderingTime = null;
        _renderingAudio = true;
        CompositionTarget.Rendering += RenderAudio;

        _timer.Start();
        Show();
    }

    /// <summary>
    /// Show overlay in transcribing state.
    /// </summary>
    public void ShowTranscribing()
    {
        _isRecording = false;
        _timer.Stop();
        ResetAudioLevel();
        
        // Hide recording UI
        RecordingContent.Visibility = Visibility.Collapsed;
        
        // Show transcribing UI
        TranscribingOverlay.Visibility = Visibility.Visible;
        
        if (!IsVisible)
        {
            Show();
        }
    }

    /// <summary>
    /// Show overlay in downloading state.
    /// </summary>
    public void ShowDownloading(string message = "Downloading models...")
    {
        _isRecording = false;
        _timer.Stop();
        ResetAudioLevel();
        
        // Hide recording UI
        RecordingContent.Visibility = Visibility.Collapsed;
        
        // Show downloading UI (reuse transcribing overlay)
        TranscribingOverlay.Visibility = Visibility.Visible;
        
        if (!IsVisible)
        {
            Show();
        }
    }

    /// <summary>
    /// Hide the overlay.
    /// </summary>
    public new void Hide()
    {
        _isRecording = false;
        _timer.Stop();
        ResetAudioLevel();
        base.Hide();
    }

    private void ApplyNonActivatingClickThroughStyle()
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        if (hwnd == IntPtr.Zero)
        {
            return;
        }

        const int GWL_EXSTYLE = -20;
        const int WS_EX_TOOLWINDOW = 0x00000080;
        const int WS_EX_NOACTIVATE = 0x08000000;
        const int WS_EX_TRANSPARENT = 0x00000020;

        var exStyle = GetWindowLongPtr(hwnd, GWL_EXSTYLE);
        if (exStyle == IntPtr.Zero)
        {
            return;
        }

        var newExStyle = new IntPtr(exStyle.ToInt64() | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT);
        _ = SetWindowLongPtr(hwnd, GWL_EXSTYLE, newExStyle);
    }

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtr", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtr", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    /// <summary>
    /// Update the audio level meter (0.0 to 1.0).
    /// </summary>
    public void UpdateAudioLevel(float level)
        => UpdateAudioLevel(level, Stopwatch.GetElapsedTime(0));

    internal void UpdateAudioLevel(float level, TimeSpan timestamp)
    {
        var diagnostics = _frameDiagnostics;
        var measure = diagnostics?.IsActive == true;
        var start = measure ? Stopwatch.GetTimestamp() : 0;
        // Capture callbacks must not wait for the UI thread or queue a render for every sample.
        level = float.IsFinite(level) ? Math.Clamp(level, 0, 1) : 0;
        VoiceWaveform.AddAudioMeasurement(level, timestamp);
        Volatile.Write(ref _latestAudioLevel, level);
        // Preserve short peaks between render frames, including when the UI briefly stalls.
        var peak = Volatile.Read(ref _pendingAudioPeak);
        while (level > peak)
        {
            var previous = Interlocked.CompareExchange(ref _pendingAudioPeak, level, peak);
            if (previous == peak) break;
            peak = previous;
        }
        if (measure) diagnostics!.RecordAudio(timestamp, Stopwatch.GetElapsedTime(start));
    }

    private void RenderAudio(object? sender, EventArgs e)
    {
        if (!_isRecording) return;
        // WPF can raise Rendering more than once for the same frame during layout updates.
        var renderingTime = ((RenderingEventArgs)e).RenderingTime;
        if (_lastRenderingTime == renderingTime)
        {
            _frameDiagnostics?.DuplicateFrame();
            return;
        }
        // Advance by the presentation clock, not by jitter in when the UI callback happens to run.
        var elapsed = _lastRenderingTime.HasValue ? renderingTime - _lastRenderingTime.Value : TimeSpan.Zero;
        _lastRenderingTime = renderingTime;
        var diagnostics = _frameDiagnostics;
        var measure = diagnostics?.IsActive == true;
        var now = Stopwatch.GetElapsedTime(0);
        var start = measure ? Stopwatch.GetTimestamp() : 0;
        if (measure) diagnostics!.BeginFrame(now, elapsed);
        RenderAudioFrame(elapsed);
        if (measure) diagnostics!.EndFrame(Stopwatch.GetElapsedTime(0), Stopwatch.GetElapsedTime(start).TotalMilliseconds);
    }

    internal void RenderAudioFrame(TimeSpan elapsed, TimeSpan? captureTime = null)
    {
        var input = Math.Max(Volatile.Read(ref _latestAudioLevel), Interlocked.Exchange(ref _pendingAudioPeak, 0));
        var level = _levelSmoother.Update(input, elapsed);
        if (_recordingAnimation == RecordingAnimation.Waveform)
            VoiceWaveform.AdvanceAudio(elapsed, captureTime ?? Stopwatch.GetElapsedTime(0));
        else
            AudioLevelScale.ScaleX = level;
    }

    private void ResetAudioLevel()
    {
        _frameDiagnostics?.Stop(Stopwatch.GetElapsedTime(0));
        _dispatcherObserver?.Dispose();
        if (_renderingAudio)
        {
            CompositionTarget.Rendering -= RenderAudio;
            _renderingAudio = false;
        }
        _lastRenderingTime = null;
        Volatile.Write(ref _latestAudioLevel, 0);
        Interlocked.Exchange(ref _pendingAudioPeak, 0);
        _levelSmoother.Reset();
        AudioLevelScale.ScaleX = 0;
        VoiceWaveform.Reset();
    }

    private void Timer_Tick(object? sender, EventArgs e)
    {
        if (!_isRecording) return;
        _frameDiagnostics?.Heartbeat(Stopwatch.GetElapsedTime(0));

        var elapsed = DateTime.Now - _recordingStartTime;
        TimerText.Text = $"{elapsed:mm\\:ss}";
    }



    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        // Don't allow closing the window, just hide it
        e.Cancel = true;
        Hide();
    }
}
