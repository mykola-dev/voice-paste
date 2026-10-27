using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using VoicePaste.Logging;
using VoicePaste.Overlay;
using VoicePaste.Settings;
using VoicePaste.Transcription;
using VoicePaste.TrayIcon;

namespace VoicePaste;

/// <summary>
/// Main application entry point.
/// </summary>
public partial class App : Application
{
    private VoicePasteController? _controller;
    private TrayIconManager? _trayIcon;
    private RecordingOverlay? _overlay;
    private SettingsManager? _settingsManager;
    private AppSettings _settings = AppSettings.Default;

    private static readonly string LogPath = Path.Combine(
        Path.GetTempPath(),
        "VoicePaste",
        "voicepaste.log"
    );

    protected override async void OnStartup(StartupEventArgs e)
    {
        try
        {
            // Fix console encoding for Cyrillic/Unicode characters
            // Published WPF apps may not have a valid console handle.
            try
            {
                Console.InputEncoding = System.Text.Encoding.UTF8;
                Console.OutputEncoding = System.Text.Encoding.UTF8;
            }
            catch
            {
                // Ignore when no console is attached.
            }
            
            // Setup logging
            Directory.CreateDirectory(Path.GetDirectoryName(LogPath)!);
            Log("=== VoicePaste Starting ===");
            
            base.OnStartup(e);
            
            Log("Creating main window...");
            // Create hidden main window (needed for hotkey registration)
            var mainWindow = new MainWindow();
            MainWindow = mainWindow;
            mainWindow.Show();  // Must be shown for hotkey registration to work
            mainWindow.Visibility = Visibility.Hidden;
            Log("Main window created");
            
            // Check and download models before initializing
            Log("Checking required models...");
            await CheckAndDownloadModelsAsync();
            
            _settingsManager = new SettingsManager();
            _settings = _settingsManager.Load();
            Logger.Configure(_settings.DebugLogging);

            Log("Initializing controller...");
            _controller = new VoicePasteController(mainWindow, _settings);
            Log("Controller initialized successfully");
            
            // Subscribe to events
            _controller.StateChanged += OnStateChanged;
            _controller.ErrorOccurred += OnErrorOccurred;
            _controller.AudioLevelChanged += OnAudioLevelChanged;
            
            Log("Application started successfully");
            Console.WriteLine($"VoicePaste is running. Press {_settings.Hotkey} to record.");
            Console.WriteLine($"Log file: {LogPath}");
            
            // Initialize tray icon
            Log("Creating tray icon...");
            _trayIcon = new TrayIconManager();
            _trayIcon.StartStopClicked += OnTrayStartStopClicked;
            _trayIcon.SettingsClicked += OnTraySettingsClicked;
            _trayIcon.QuitClicked += OnTrayQuitClicked;
            Log("Tray icon created");
            
            // Initialize overlay
            Log("Creating overlay...");
            _overlay = new RecordingOverlay();
            _overlay.SetRecordingAnimation(_settings.RecordingAnimation);
            _overlay.SetIndicatorColor(_settings.IndicatorColor);
            Log("Overlay created");
            _controller.Initialize();
        }
        catch (Exception ex)
        {
            var errorMsg = $"Startup failed: {ex.GetType().Name}: {ex.Message}\n{ex.StackTrace}";
            Log($"ERROR: {errorMsg}");
            MessageBox.Show(
                $"VoicePaste failed to start:\n\n{ex.Message}\n\nSee log: {LogPath}",
                "VoicePaste Startup Error",
                MessageBoxButton.OK,
                MessageBoxImage.Error
            );
            Shutdown(1);
        }
    }
    
    private void OnStateChanged(object? sender, AppState state)
    {
        // Update tray icon
        _trayIcon?.UpdateState(state);
        
        // Update overlay
        Dispatcher.Invoke(() =>
        {
            switch (state)
            {
                case AppState.Idle:
                    _overlay?.Hide();
                    break;
                    
                case AppState.Recording:
                    _overlay?.ShowRecording();
                    break;
                    
                case AppState.Transcribing:
                    _overlay?.ShowTranscribing();
                    break;
            }
        });
        
        var message = $"State changed: {state}";
        Log(message);
    }
    
    private void OnAudioLevelChanged(object? sender, float level)
    {
        _overlay?.UpdateAudioLevel(level);
    }
    
    private void OnErrorOccurred(object? sender, string message)
    {
        Log($"ERROR: {message}");
        Dispatcher.BeginInvoke(new Action(() =>
            MessageBox.Show(message, "VoicePaste Error", MessageBoxButton.OK, MessageBoxImage.Error)));
    }
    
    
    private void OnTrayStartStopClicked(object? sender, EventArgs e)
    {
        Log("Tray Start/Stop clicked");
        try
        {
            if (_controller?.State == AppState.Idle)
            {
                _controller.StartRecording();
            }
            else if (_controller?.State == AppState.Recording)
            {
                _ = _controller.StopRecordingAndTranscribeAsync();
            }
        }
        catch (Exception ex)
        {
            Log($"ERROR in tray Start/Stop: {ex.Message}");
            MessageBox.Show($"Error: {ex.Message}", "VoicePaste Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private Window? _settingsWindow;

    private void OnTraySettingsClicked(object? sender, EventArgs e)
    {
        Log("Tray Settings clicked");

        Dispatcher.Invoke(() =>
        {
            if (_settingsManager is null)
            {
                _settingsManager = new SettingsManager();
            }

            if (_settingsWindow is { IsVisible: true })
            {
                _settingsWindow.Activate();
                return;
            }

            var dialog = new SettingsWindow(_settingsManager, _settings,
                token => _controller?.GetMemoryUsageAsync(token) ?? Task.FromResult(WorkerMemoryUsage.NotLoaded))
            {
                Owner = MainWindow
            };

            _settingsWindow = dialog;

            var result = dialog.ShowDialog();
            if (result == true && dialog.SavedSettings is not null)
            {
                ApplySettings(dialog.SavedSettings);
            }
        });
    }
    
    private void OnTrayQuitClicked(object? sender, EventArgs e)
    {
        Log("Tray Quit clicked");
        Shutdown();
    }
    
    protected override void OnExit(ExitEventArgs e)
    {
        Log("Application exiting...");
        _trayIcon?.Dispose();
        _controller?.Dispose();
        Log("=== VoicePaste Stopped ===");
        base.OnExit(e);
    }

    private void ApplySettings(AppSettings settings)
    {
        _settings = settings;
        Logger.Configure(_settings.DebugLogging);

        try
        {
            _overlay?.SetRecordingAnimation(_settings.RecordingAnimation);
            _overlay?.SetIndicatorColor(_settings.IndicatorColor);
            _controller?.UpdateSettings(_settings);
        }
        catch (Exception ex)
        {
            Log($"ERROR applying settings: {ex.Message}");
            MessageBox.Show($"Failed to apply settings: {ex.Message}", "VoicePaste", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private static void Log(string message)
        => Logger.Info("App", message);

    private async Task CheckAndDownloadModelsAsync()
    {
        var downloadService = new ModelDownloadService();

        if (downloadService.AllModelsPresent())
        {
            Log("All required models are already cached.");
            Console.WriteLine("[Models] All models present in cache.");
            return;
        }

        var missing = downloadService.GetMissingModels();
        Log($"Missing models: {string.Join(", ", missing)}");
        Console.WriteLine($"[Models] Missing: {string.Join(", ", missing)}");
        Console.WriteLine("[Models] Downloading... This may take a few minutes on first launch.");

        // Create overlay and show download UI
        var overlay = new RecordingOverlay();
        overlay.Show();
        overlay.ShowDownloading();

        using var cts = new CancellationTokenSource();

        // Handle progress updates
        downloadService.ProgressChanged += (sender, progress) =>
        {
            Dispatcher.Invoke(() =>
            {
                var message = $"{progress.Status} ({progress.Percentage}%)";
                Log($"[ModelDownload] {message}");
            });
        };

        try
        {
            // Run download asynchronously to keep UI responsive
            await downloadService.DownloadMissingModelsAsync(cts.Token);

            Log("All models downloaded successfully.");
            Console.WriteLine("[Models] Download complete.");
        }
        catch (Exception ex)
        {
            Log($"ERROR downloading models: {ex.Message}");
            overlay.Hide();

            var result = MessageBox.Show(
                $"Failed to download required models:\n\n{ex.Message}\n\nVoicePaste requires internet connection on first launch.\n\nRetry download?",
                "Model Download Failed",
                MessageBoxButton.YesNo,
                MessageBoxImage.Error
            );

            if (result == MessageBoxResult.Yes)
            {
                await CheckAndDownloadModelsAsync(); // Retry
            }
            else
            {
                throw new ApplicationException("Required models are not available.");
            }
        }
        finally
        {
            overlay.Hide();
        }
    }
}
