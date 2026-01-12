using System;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Hardcodet.Wpf.TaskbarNotification;

namespace VoicePaste.TrayIcon;

/// <summary>
/// Manages the system tray icon and context menu.
/// </summary>
public class TrayIconManager : IDisposable
{
    private readonly TaskbarIcon _taskbarIcon;
    private AppState _currentState = AppState.Idle;

    public event EventHandler? StartStopClicked;
    public event EventHandler? SettingsClicked;
    public event EventHandler? QuitClicked;

    public TrayIconManager()
    {
        _taskbarIcon = new TaskbarIcon
        {
            ToolTipText = "VoicePaste - Idle",
            Visibility = Visibility.Visible
        };

        // Set initial icon
        UpdateIcon(AppState.Idle);

        // Create context menu
        _taskbarIcon.ContextMenu = CreateContextMenu();
    }

    /// <summary>
    /// Updates the tray icon based on application state.
    /// </summary>
    public void UpdateState(AppState state)
    {
        _currentState = state;
        UpdateIcon(state);
        UpdateTooltip(state);
        
        // Update Start/Stop menu item text
        if (_taskbarIcon.ContextMenu?.Items[0] is System.Windows.Controls.MenuItem startStopItem)
        {
            startStopItem.Header = state == AppState.Recording ? "Stop Recording" : "Start Recording";
            startStopItem.IsEnabled = state != AppState.Transcribing;
        }
    }

    private void UpdateIcon(AppState state)
    {
        // Create a simple icon based on state
        // Gray = Idle, Red = Recording, Yellow = Transcribing
        Color iconColor = state switch
        {
            AppState.Idle => Colors.Gray,
            AppState.Recording => Colors.Red,
            AppState.Transcribing => Colors.Orange,
            _ => Colors.Gray
        };

        _taskbarIcon.Icon = CreateIcon(iconColor);
    }

    private void UpdateTooltip(AppState state)
    {
        _taskbarIcon.ToolTipText = state switch
        {
            AppState.Idle => "VoicePaste - Ready (ScrollLock to record)",
            AppState.Recording => "VoicePaste - Recording... (ScrollLock to stop)",
            AppState.Transcribing => "VoicePaste - Transcribing...",
            _ => "VoicePaste"
        };
    }

    private System.Drawing.Icon CreateIcon(Color color)
    {
        // Create a simple circular icon with the specified color
        const int size = 32;
        var drawingVisual = new DrawingVisual();
        
        using (var context = drawingVisual.RenderOpen())
        {
            // Background
            context.DrawRectangle(Brushes.Transparent, null, new Rect(0, 0, size, size));
            
            // Microphone icon (simple circle with a line)
            var brush = new SolidColorBrush(color);
            var pen = new Pen(brush, 2);
            
            // Microphone body (circle)
            context.DrawEllipse(null, pen, new Point(size / 2.0, size / 2.0 - 4), 6, 8);
            
            // Microphone stand (line)
            context.DrawLine(pen, new Point(size / 2.0, size / 2.0 + 6), new Point(size / 2.0, size / 2.0 + 14));
            
            // Base
            context.DrawLine(pen, new Point(size / 2.0 - 4, size / 2.0 + 14), new Point(size / 2.0 + 4, size / 2.0 + 14));
        }

        var renderBitmap = new RenderTargetBitmap(size, size, 96, 96, PixelFormats.Pbgra32);
        renderBitmap.Render(drawingVisual);

        // Convert to System.Drawing.Icon
        using var stream = new System.IO.MemoryStream();
        var encoder = new PngBitmapEncoder();
        encoder.Frames.Add(BitmapFrame.Create(renderBitmap));
        encoder.Save(stream);
        stream.Seek(0, System.IO.SeekOrigin.Begin);

        using var bitmap = new System.Drawing.Bitmap(stream);
        return System.Drawing.Icon.FromHandle(bitmap.GetHicon());
    }

    private System.Windows.Controls.ContextMenu CreateContextMenu()
    {
        var contextMenu = new System.Windows.Controls.ContextMenu();

        // Start/Stop Recording
        var startStopItem = new System.Windows.Controls.MenuItem
        {
            Header = "Start Recording"
        };
        startStopItem.Click += (s, e) => StartStopClicked?.Invoke(this, EventArgs.Empty);
        contextMenu.Items.Add(startStopItem);

        // Separator
        contextMenu.Items.Add(new System.Windows.Controls.Separator());

        // Settings
        var settingsItem = new System.Windows.Controls.MenuItem
        {
            Header = "Settings..."
        };
        settingsItem.Click += (s, e) => SettingsClicked?.Invoke(this, EventArgs.Empty);
        contextMenu.Items.Add(settingsItem);

        // Separator
        contextMenu.Items.Add(new System.Windows.Controls.Separator());

        // Quit
        var quitItem = new System.Windows.Controls.MenuItem
        {
            Header = "Quit"
        };
        quitItem.Click += (s, e) => QuitClicked?.Invoke(this, EventArgs.Empty);
        contextMenu.Items.Add(quitItem);

        return contextMenu;
    }

    public void Dispose()
    {
        _taskbarIcon?.Dispose();
    }
}
