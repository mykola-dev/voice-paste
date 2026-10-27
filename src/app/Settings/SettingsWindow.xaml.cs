using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Threading;
using System.Windows.Media;

using VoicePaste.Logging;
using VoicePaste.Overlay;
using VoicePaste.Transcription;

namespace VoicePaste.Settings;

public partial class SettingsWindow : Window
{
    private sealed record LanguageModeItem(LanguageMode Value, string Display, string Hint);

    private sealed record DeviceModeItem(TranscriptionDevice Value, string Display, string Hint);

    private sealed record RecordingAnimationItem(RecordingAnimation Value, string Display);

    private void UpdateLanguageHint()
    {
        var selected = LanguageCombo.SelectedItem as LanguageModeItem;
        LanguageHintText.Text = selected?.Hint ?? string.Empty;
    }

    private void UpdateDeviceHint()
    {
        var selected = DeviceCombo.SelectedItem as DeviceModeItem;
        DeviceHintText.Text = selected?.Hint ?? string.Empty;
    }

    private readonly SettingsManager _settingsManager;
    private AppSettings? _loadedSettings;
    private readonly Func<CancellationToken, Task<WorkerMemoryUsage>>? _memoryProvider;
    private readonly CancellationTokenSource _memoryCancellation = new();
    private readonly DispatcherTimer _memoryTimer = new() { Interval = TimeSpan.FromSeconds(2) };
    private bool _refreshingMemory;
    private string _indicatorColor = IndicatorColors.Default;

    public SettingsWindow(SettingsManager settingsManager, AppSettings current,
        Func<CancellationToken, Task<WorkerMemoryUsage>>? memoryProvider = null)
    {
        _settingsManager = settingsManager;
        _memoryProvider = memoryProvider;

        InitializeComponent();

        PopulateCombos();
        LoadSettings(current);
        _loadedSettings = current;
        _memoryTimer.Tick += OnMemoryTick;
        Loaded += (_, _) =>
        {
            _memoryTimer.Start();
            OnMemoryTick(this, EventArgs.Empty);
        };
        Closed += (_, _) =>
        {
            _memoryTimer.Stop();
            _memoryTimer.Tick -= OnMemoryTick;
            _memoryCancellation.Cancel();
        };

        Width = current.SettingsWindowWidth;
        Height = current.SettingsWindowHeight;

        // Apply saved position or center on screen
        if (double.IsNaN(current.SettingsWindowLeft) || double.IsNaN(current.SettingsWindowTop))
        {
            CenterWindowOnScreen();
        }
        else
        {
            Left = current.SettingsWindowLeft;
            Top = current.SettingsWindowTop;
        }

        LogPathText.Text = Logger.GetLogFilePath() is { } logPath ? $"Log: {logPath}" : "File logging unavailable";
    }

    private void CenterWindowOnScreen()
    {
        // Center on the primary screen work area (accounting for taskbar)
        var workArea = SystemParameters.WorkArea;
        Left = workArea.Left + (workArea.Width - Width) / 2;
        Top = workArea.Top + (workArea.Height - Height) / 2;
    }

    public AppSettings? SavedSettings { get; private set; }

    private async void OnMemoryTick(object? sender, EventArgs e)
    {
        if (_refreshingMemory || _memoryCancellation.IsCancellationRequested || _memoryProvider == null) return;
        _refreshingMemory = true;
        try
        {
            var usage = await _memoryProvider(_memoryCancellation.Token);
            if (!_memoryCancellation.IsCancellationRequested) MemoryUsageText.Text = FormatMemoryUsage(usage);
        }
        catch (OperationCanceledException) { }
        catch (Exception ex)
        {
            if (!_memoryCancellation.IsCancellationRequested) MemoryUsageText.Text = "Memory usage unavailable";
            Logger.Debug("Settings", $"Memory query failed: {ex.Message}");
        }
        finally { _refreshingMemory = false; }
    }

    internal static string FormatMemoryUsage(WorkerMemoryUsage usage)
    {
        if (usage.ProcessId == null) return "Model not loaded";
        static string Bytes(long? bytes) => bytes is long value ? $"{value / (1024d * 1024):N0} MiB" : "unavailable";
        var text = $"{usage.Model} ({usage.Device?.ToUpperInvariant()}{(usage.IsReady ? "" : ", loading")}): RAM {Bytes(usage.RamBytes)}";
        return usage.Device == "cuda" && usage.VramBytes.HasValue
            ? $"{text} · VRAM {Bytes(usage.VramBytes)}" : text;
    }

    private void PopulateCombos()
    {
        HotkeyCombo.ItemsSource = new List<string>
        {
            "ScrollLock",
            "F8",
            "F9",
            "F10",
            "Ctrl+Alt+Space"
        };

        PasteShortcutCombo.ItemsSource = Enum.GetValues(typeof(PasteShortcut)).Cast<PasteShortcut>();
        RecordingAnimationCombo.ItemsSource = new[]
        {
            new RecordingAnimationItem(RecordingAnimation.LevelBar, "Level bar"),
            new RecordingAnimationItem(RecordingAnimation.Waveform, "Waveform")
        };
        RecordingAnimationCombo.DisplayMemberPath = nameof(RecordingAnimationItem.Display);
        RecordingAnimationCombo.SelectedValuePath = nameof(RecordingAnimationItem.Value);
        ModelCombo.ItemsSource = AppSettings.ValidModels;
        DeviceCombo.ItemsSource = new List<DeviceModeItem>
        {
            new(TranscriptionDevice.CudaAuto, "CUDA (auto-fallback)", "Try GPU first; fallback to CPU if CUDA fails."),
            new(TranscriptionDevice.CudaOnly, "CUDA (GPU only)", "Use GPU only; transcription fails if CUDA is unavailable."),
            new(TranscriptionDevice.Cpu, "CPU", "Always use CPU (slowest, most compatible).")
        };

        DeviceCombo.DisplayMemberPath = nameof(DeviceModeItem.Display);
        DeviceCombo.SelectedValuePath = nameof(DeviceModeItem.Value);
        DeviceCombo.SelectionChanged += (_, _) => UpdateDeviceHint();

        LanguageCombo.ItemsSource = new List<LanguageModeItem>
        {
            new(LanguageMode.Auto, "Automatic (detect per recording)", "Detect language automatically shows best single language result."),
            new(LanguageMode.En, "English (force)", "Always transcribe as English."),
            new(LanguageMode.Ua, "Ukrainian (force)", "Always transcribe as Ukrainian (uk)."),
            new(LanguageMode.Bilingual, "Bilingual (English + Ukrainian)", "Auto-detect with context for English and Ukrainian. Prevents misidentification as Russian. Best for mixed-language speech.")
        };

        LanguageCombo.DisplayMemberPath = nameof(LanguageModeItem.Display);
        LanguageCombo.SelectedValuePath = nameof(LanguageModeItem.Value);
        LanguageCombo.SelectionChanged += (_, _) => UpdateLanguageHint();

        UpdateDeviceHint();
        UpdateLanguageHint();
    }

    private void LoadSettings(AppSettings settings)
    {
        HotkeyCombo.SelectedItem = settings.Hotkey;
        PasteShortcutCombo.SelectedItem = settings.PasteShortcut;
        ModelCombo.SelectedItem = settings.Model;
        DeviceCombo.SelectedValue = settings.Device;
        LanguageCombo.SelectedValue = settings.LanguageMode;
        BeamSizeSlider.Value = settings.BeamSize;
        VadCheck.IsChecked = settings.EnableVad;
        KeepInMemoryCheck.IsChecked = settings.KeepInMemory;
        RecordingAnimationCombo.SelectedValue = settings.RecordingAnimation;
        SetIndicatorColorSelection(settings.IndicatorColor);

        CustomPromptText.Text = settings.CustomInitialPrompt;
        DebugLoggingCheck.IsChecked = settings.DebugLogging;
    }

    private sealed record UiState(bool Enabled, string Status);

    internal void SetIndicatorColorSelection(string color)
    {
        _indicatorColor = IndicatorColors.Normalize(color);
        IndicatorColorSwatch.Background = new SolidColorBrush(IndicatorColors.Parse(_indicatorColor));
        IndicatorColorText.Text = _indicatorColor;
    }

    private void ChooseIndicatorColor_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            if (NativeColorPicker.TryChoose(this, IndicatorColors.Parse(_indicatorColor), out var selected))
                SetIndicatorColorSelection(IndicatorColors.ToHex(selected));
        }
        catch (Exception ex) { ErrorText.Text = ex.Message; }
    }

    private void ResetIndicatorColor_Click(object sender, RoutedEventArgs e)
        => SetIndicatorColorSelection(IndicatorColors.Default);

    private async Task InstallVadDependenciesAsync()
    {
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var pythonExe = PythonFinder.Find();
        var requirementsPath = FindVadRequirementsPath(exeDir);

        var overlay = new RecordingOverlay();
        overlay.Show();
        overlay.ShowDownloading();

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = pythonExe,
                ArgumentList = { "-m", "pip", "install", "-r", requirementsPath },
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            };

            psi.Environment["PYTHONUTF8"] = "1";
            psi.Environment["PYTHONIOENCODING"] = "utf-8";

            using var process = new Process { StartInfo = psi };
            var error = new StringBuilder();

            process.OutputDataReceived += (_, args) =>
            {
                if (args.Data == null) return;
                Dispatcher.Invoke(() => ErrorText.Text = args.Data);
            };
            process.ErrorDataReceived += (_, args) =>
            {
                if (args.Data == null) return;
                error.AppendLine(args.Data);
                Dispatcher.Invoke(() => ErrorText.Text = args.Data);
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            await process.WaitForExitAsync();

            if (process.ExitCode != 0)
            {
                throw new InvalidOperationException($"VAD dependency install failed: {error}".Trim());
            }
        }
        finally
        {
            overlay.Hide();
        }
    }

    private static string FindVadRequirementsPath(string exeDir)
    {
        var candidates = new[]
        {
            Path.Combine(exeDir, "transcribe", "requirements-vad.txt"),
            Path.GetFullPath(Path.Combine(exeDir, "..", "..", "..", "..", "src", "transcribe", "requirements-vad.txt")),
            Path.GetFullPath(Path.Combine(exeDir, "..", "..", "..", "..", "..", "src", "transcribe", "requirements-vad.txt"))
        };

        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate))
                return candidate;
        }

        throw new InvalidOperationException("VAD requirements file not found.");
    }

    private UiState SetUiBusy(string status)
    {
        var prior = new UiState(IsEnabled, ErrorText.Text);
        IsEnabled = false;
        ErrorText.Text = status;
        return prior;
    }

    private void RestoreUi(UiState prior)
    {
        IsEnabled = prior.Enabled;
        if (!string.IsNullOrWhiteSpace(prior.Status))
            ErrorText.Text = prior.Status;
    }

    internal AppSettings GetSettingsFromUi()
    {
        return new AppSettings
        {
            Version = 1,
            Hotkey = (HotkeyCombo.SelectedItem as string) ?? "ScrollLock",
            PasteShortcut = (PasteShortcut)(PasteShortcutCombo.SelectedItem ?? PasteShortcut.CtrlShiftV),
            Model = (ModelCombo.SelectedItem as string) ?? "large-v3-turbo",
            Device = (TranscriptionDevice)(DeviceCombo.SelectedValue ?? TranscriptionDevice.CudaAuto),
            LanguageMode = (LanguageMode)(LanguageCombo.SelectedValue ?? LanguageMode.Auto),
            BeamSize = (int)BeamSizeSlider.Value,
            EnableVad = VadCheck.IsChecked == true,
            KeepInMemory = KeepInMemoryCheck.IsChecked == true,
            RecordingAnimation = (RecordingAnimation)(RecordingAnimationCombo.SelectedValue ?? RecordingAnimation.LevelBar),
            IndicatorColor = _indicatorColor,
            CustomInitialPrompt = CustomPromptText.Text,
            DebugLogging = DebugLoggingCheck.IsChecked == true,
            SettingsWindowWidth = Width,
            SettingsWindowHeight = Height,
            SettingsWindowLeft = Left,
            SettingsWindowTop = Top
        };
    }

    private async void Save_Click(object sender, RoutedEventArgs e)
    {
        ErrorText.Text = string.Empty;

        var hotkey = (HotkeyCombo.SelectedItem as string) ?? "ScrollLock";
        if (!HotkeyParser.TryParse(hotkey, out _))
        {
            ErrorText.Text = "Invalid hotkey format.";
            return;
        }

        var settings = SettingsManager.ValidateAndMigrate(GetSettingsFromUi());

        var prior = SetUiBusy("Saving...");

        try
        {
            if (settings.EnableVad && !(_loadedSettings?.EnableVad ?? false))
            {
                ErrorText.Text = "Installing VAD dependencies...";
                await InstallVadDependenciesAsync();
            }

            ErrorText.Text = "Saving settings...";
            _settingsManager.Save(settings);
        }
        catch (Exception ex)
        {
            ErrorText.Text = $"Failed to save settings: {ex.Message}";
            RestoreUi(prior);
            return;
        }

        RestoreUi(prior);

        SavedSettings = settings;
        DialogResult = true;
        Close();
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        DialogResult = false;
        Close();
    }
    
    private void ResizeHandle_DragDelta(object sender, System.Windows.Controls.Primitives.DragDeltaEventArgs e)
    {
        if (double.IsNaN(CustomPromptText.Height))
            CustomPromptText.Height = CustomPromptText.ActualHeight;

        var newHeight = CustomPromptText.Height + e.VerticalChange;
        if (newHeight >= CustomPromptText.MinHeight && newHeight <= CustomPromptText.MaxHeight)
        {
            CustomPromptText.Height = newHeight;
        }
    }
}
