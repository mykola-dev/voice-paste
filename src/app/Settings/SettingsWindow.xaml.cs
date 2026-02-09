using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using System.Windows;

using VoicePaste.Overlay;
using VoicePaste.Transcription;

namespace VoicePaste.Settings;

public partial class SettingsWindow : Window
{
    private sealed record LanguageModeItem(LanguageMode Value, string Display, string Hint);

    private sealed record DeviceModeItem(TranscriptionDevice Value, string Display, string Hint);

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

    public SettingsWindow(SettingsManager settingsManager, AppSettings current)
    {
        _settingsManager = settingsManager;

        InitializeComponent();

        PopulateCombos();
        LoadSettings(current);
        _loadedSettings = current;

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

        LogPathText.Text = $"Log: {Path.Combine(Path.GetTempPath(), "VoicePaste", "voicepaste.log")}";
    }

    private void CenterWindowOnScreen()
    {
        // Center on the primary screen work area (accounting for taskbar)
        var workArea = SystemParameters.WorkArea;
        Left = workArea.Left + (workArea.Width - Width) / 2;
        Top = workArea.Top + (workArea.Height - Height) / 2;
    }

    public AppSettings? SavedSettings { get; private set; }

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

        CustomPromptText.Text = settings.CustomInitialPrompt;
        DebugLoggingCheck.IsChecked = settings.DebugLogging;
    }

    private sealed record UiState(bool Enabled, string Status);

    private async Task InstallVadDependenciesAsync()
    {
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var pythonExe = PythonFinder.Find();
        var requirementsPath = FindVadRequirementsPath(exeDir);

        var overlay = new RecordingOverlay();
        overlay.Show();
        overlay.ShowDownloading("Installing VAD dependencies...");

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

    private async void Save_Click(object sender, RoutedEventArgs e)
    {
        ErrorText.Text = string.Empty;

        var hotkey = (HotkeyCombo.SelectedItem as string) ?? "ScrollLock";
        if (!HotkeyParser.TryParse(hotkey, out _))
        {
            ErrorText.Text = "Invalid hotkey format.";
            return;
        }

        var selectedModel = (ModelCombo.SelectedItem as string) ?? "large-v3-turbo";

        var settings = new AppSettings
        {
            Version = 1,
            Hotkey = hotkey,
            PasteShortcut = (PasteShortcut)(PasteShortcutCombo.SelectedItem ?? PasteShortcut.CtrlShiftV),
            Model = selectedModel,
            Device = (TranscriptionDevice)(DeviceCombo.SelectedValue ?? TranscriptionDevice.CudaAuto),
            LanguageMode = (LanguageMode)(LanguageCombo.SelectedValue ?? LanguageMode.Auto),
            BeamSize = (int)BeamSizeSlider.Value,
            EnableVad = VadCheck.IsChecked == true,
            CustomInitialPrompt = CustomPromptText.Text,
            DebugLogging = DebugLoggingCheck.IsChecked == true,
            SettingsWindowWidth = Width,
            SettingsWindowHeight = Height,
            SettingsWindowLeft = Left,
            SettingsWindowTop = Top
        };

        settings = SettingsManager.ValidateAndMigrate(settings);

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
