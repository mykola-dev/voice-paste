using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using System.Windows;

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

    public SettingsWindow(SettingsManager settingsManager, AppSettings current)
    {
        _settingsManager = settingsManager;

        InitializeComponent();

        PopulateCombos();
        LoadSettings(current);

        Width = current.SettingsWindowWidth;
        Height = current.SettingsWindowHeight;

        LogPathText.Text = $"Log: {Path.Combine(Path.GetTempPath(), "VoicePaste", "voicepaste.log")}";
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

        CustomPromptText.Text = settings.CustomInitialPrompt;
        DebugLoggingCheck.IsChecked = settings.DebugLogging;
    }

    private static bool IsPortableModelsCachePresent(string model)
    {
        try
        {
            var exeDir = AppDomain.CurrentDomain.BaseDirectory;
            var localModelsDir = Path.GetFullPath(Path.Combine(exeDir, "models"));
            
            // 1. Check local models folder (portable)
            if (Directory.Exists(localModelsDir))
            {
                if (CheckDirForModel(localModelsDir, model))
                    return true;
            }

            // 2. Check global Hugging Face cache
            var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            var globalModelsDir = Path.Combine(userProfile, ".cache", "huggingface", "hub");
            
            if (Directory.Exists(globalModelsDir))
            {
                if (CheckDirForModel(globalModelsDir, model))
                    return true;
            }

            return false;
        }
        catch
        {
            return false;
        }
    }

    private static bool CheckDirForModel(string rootDir, string model)
    {
        try
        {
            // Heuristic: any folder containing the model name and "snapshots".
            // Works for typical hub layouts like: models\hub\models--*--faster-whisper-<model>\snapshots\...
            return Directory.EnumerateDirectories(rootDir, "*", SearchOption.AllDirectories)
                .Any(d => d.Contains(model, StringComparison.OrdinalIgnoreCase) && d.Contains("snapshots", StringComparison.OrdinalIgnoreCase));
        }
        catch
        {
            return false;
        }
    }

    private async Task CacheModelAsync(string model)
    {
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var pythonExe = PythonFinder.Find();
        var modelsDir = Path.GetFullPath(Path.Combine(exeDir, "models"));

        Directory.CreateDirectory(modelsDir);

        var scriptPath = Path.Combine(Path.GetTempPath(), $"voicepaste-cache-model-{Guid.NewGuid():N}.py");

        try
        {
            var script = "import os\n" +
                         "import sys\n" +
                         "models_dir = sys.argv[1]\n" +
                         "model = sys.argv[2]\n" +
                         "os.environ['HF_HOME'] = models_dir\n" +
                         "os.environ['HF_HUB_DISABLE_TELEMETRY'] = '1'\n" +
                         "os.environ['HF_HUB_DISABLE_SYMLINKS_WARNING'] = '1'\n" +
                         "print(f'Caching model: {model}', flush=True)\n" +
                         "from faster_whisper import WhisperModel\n" +
                         "WhisperModel(model, device='cpu')\n" +
                         "print('Done', flush=True)\n";

            await File.WriteAllTextAsync(scriptPath, script, Encoding.UTF8);

            var psi = new ProcessStartInfo
            {
                FileName = pythonExe,
                ArgumentList = { scriptPath, modelsDir, model },
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            };

            psi.Environment["HF_HOME"] = modelsDir;
            psi.Environment["PYTHONUTF8"] = "1";
            psi.Environment["PYTHONIOENCODING"] = "utf-8";
            psi.Environment["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1";

            using var process = new Process { StartInfo = psi };

            var error = new StringBuilder();

            process.OutputDataReceived += (_, args) =>
            {
                if (args.Data is null)
                    return;

                Dispatcher.Invoke(() => ErrorText.Text = args.Data);
            };

            process.ErrorDataReceived += (_, args) =>
            {
                if (args.Data is null)
                    return;

                error.AppendLine(args.Data);
                Dispatcher.Invoke(() => ErrorText.Text = args.Data);
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            await process.WaitForExitAsync();

            if (process.ExitCode != 0)
                throw new InvalidOperationException($"Model download failed: {error}".Trim());
        }
        finally
        {
            try
            {
                if (File.Exists(scriptPath))
                    File.Delete(scriptPath);
            }
            catch
            {
                // Ignore temp cleanup errors
            }
        }
    }

    private sealed record UiState(bool Enabled, string Status);

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

        var selectedModel = (ModelCombo.SelectedItem as string) ?? "medium";

        var settings = new AppSettings
        {
            Version = 1,
            Hotkey = hotkey,
            PasteShortcut = (PasteShortcut)(PasteShortcutCombo.SelectedItem ?? PasteShortcut.CtrlShiftV),
            Model = selectedModel,
            Device = (TranscriptionDevice)(DeviceCombo.SelectedValue ?? TranscriptionDevice.CudaAuto),
            LanguageMode = (LanguageMode)(LanguageCombo.SelectedValue ?? LanguageMode.Auto),
            BeamSize = (int)BeamSizeSlider.Value,
            CustomInitialPrompt = CustomPromptText.Text,
            DebugLogging = DebugLoggingCheck.IsChecked == true,
            SettingsWindowWidth = Width,
            SettingsWindowHeight = Height
        };

        settings = SettingsManager.ValidateAndMigrate(settings);

        var prior = SetUiBusy("Saving...");

        try
        {
            if (!IsPortableModelsCachePresent(settings.Model))
            {
                ErrorText.Text = $"Downloading model '{settings.Model}'...";
                await CacheModelAsync(settings.Model);
                ErrorText.Text = $"Model '{settings.Model}' downloaded.";
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
