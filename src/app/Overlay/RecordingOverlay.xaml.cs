using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Threading;

namespace VoicePaste.Overlay;

/// <summary>
/// Recording overlay window that shows recording status, timer, and audio level.
/// </summary>
public partial class RecordingOverlay : Window
{
    private readonly DispatcherTimer _timer;
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
        
        // Show recording UI
        TranscribingOverlay.Visibility = Visibility.Collapsed;
        RecordingIndicator.Visibility = Visibility.Visible;
        StateText.Text = "Recording";

        
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
        
        // Show transcribing UI
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
        if (!_isRecording) return;

        Dispatcher.Invoke(() =>
        {
            // Clamp level between 0 and 1
            level = Math.Clamp(level, 0f, 1f);
            
            // Update progress bar width
            if (AudioLevelBar.Parent is System.Windows.Controls.Border border)
            {
                AudioLevelBar.Width = level * (border.ActualWidth - 2);
            }
        });
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
