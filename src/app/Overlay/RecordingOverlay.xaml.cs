using System;
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


    public RecordingOverlay()
    {
        InitializeComponent();

        // Ensure overlay never steals focus and is click-through.
        SourceInitialized += (_, _) => ApplyNonActivatingClickThroughStyle();
        
        // Position at top-right corner
        PositionWindow();
        
        // Setup timer for recording duration
        _timer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(100)
        };
        _timer.Tick += Timer_Tick;
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
        StateText.Text = "Recording";

        ResetAudioLevel();
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
    {
        // Capture callbacks must not wait for the UI thread or queue a render for every sample.
        level = float.IsFinite(level) ? Math.Clamp(level, 0, 1) : 0;
        Volatile.Write(ref _latestAudioLevel, level);
        // Preserve short peaks between render frames, including when the UI briefly stalls.
        var peak = Volatile.Read(ref _pendingAudioPeak);
        while (level > peak)
        {
            var previous = Interlocked.CompareExchange(ref _pendingAudioPeak, level, peak);
            if (previous == peak) break;
            peak = previous;
        }
    }

    private void RenderAudio(object? sender, EventArgs e)
    {
        if (!_isRecording) return;
        // WPF can raise Rendering more than once for the same frame during layout updates.
        var renderingTime = ((RenderingEventArgs)e).RenderingTime;
        if (_lastRenderingTime == renderingTime) return;
        // Advance by the presentation clock, not by jitter in when the UI callback happens to run.
        var elapsed = _lastRenderingTime.HasValue ? renderingTime - _lastRenderingTime.Value : TimeSpan.Zero;
        _lastRenderingTime = renderingTime;
        RenderAudioFrame(elapsed);
    }

    internal void RenderAudioFrame(TimeSpan elapsed)
    {
        var input = Math.Max(Volatile.Read(ref _latestAudioLevel), Interlocked.Exchange(ref _pendingAudioPeak, 0));
        var level = _levelSmoother.Update(input, elapsed);
        if (_recordingAnimation == RecordingAnimation.Waveform)
            VoiceWaveform.Update(level, elapsed);
        else
            AudioLevelScale.ScaleX = level;
    }

    private void ResetAudioLevel()
    {
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
