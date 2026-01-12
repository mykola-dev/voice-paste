using System;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace VoicePaste.Settings;

public sealed class SettingsManager
{
    private readonly string _configPath;

    public SettingsManager(string? configPath = null)
    {
        _configPath = configPath ?? GetDefaultConfigPath();
    }

    public string ConfigPath => _configPath;

    public AppSettings Load()
    {
        try
        {
            if (!File.Exists(_configPath))
            {
                return AppSettings.Default;
            }

            var json = File.ReadAllText(_configPath);
            var settings = JsonSerializer.Deserialize<AppSettings>(json, CreateJsonOptions());
            return ValidateAndMigrate(settings);
        }
        catch
        {
            return AppSettings.Default;
        }
    }

    public void Save(AppSettings settings)
    {
        var validated = ValidateAndMigrate(settings);

        var dir = Path.GetDirectoryName(_configPath);
        if (!string.IsNullOrWhiteSpace(dir))
        {
            Directory.CreateDirectory(dir);
        }

        var json = JsonSerializer.Serialize(validated, CreateJsonOptions());
        File.WriteAllText(_configPath, json);
    }

    public static string GetDefaultConfigPath()
    {
        return Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "VoicePaste",
            "config.json");
    }

    private static JsonSerializerOptions CreateJsonOptions()
    {
        return new JsonSerializerOptions
        {
            WriteIndented = true,
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
            Converters =
            {
                new JsonStringEnumConverter(JsonNamingPolicy.CamelCase)
            }
        };
    }

    public static AppSettings ValidateAndMigrate(AppSettings? settings)
    {
        var current = settings ?? AppSettings.Default;

        // Migration hook.
        // For now we only support version 1; if missing/0, treat as v1.
        var version = current.Version;
        if (version <= 0)
        {
            current = current with { Version = 1 };
        }

        // Validation.
        var model = current.Model?.Trim().ToLowerInvariant() ?? "medium";
        if (!AppSettings.ValidModels.Contains(model))
        {
            model = "medium";
        }

        var hotkey = (current.Hotkey ?? "ScrollLock").Trim();
        if (!HotkeyParser.TryParse(hotkey, out _))
        {
            hotkey = "ScrollLock";
        }

        var delay = Math.Clamp(current.ClipboardRestoreDelayMs, 100, 2000);

        var width = double.IsFinite(current.SettingsWindowWidth) ? current.SettingsWindowWidth : AppSettings.Default.SettingsWindowWidth;
        var height = double.IsFinite(current.SettingsWindowHeight) ? current.SettingsWindowHeight : AppSettings.Default.SettingsWindowHeight;

        width = Math.Clamp(width, 480, 1400);
        height = Math.Clamp(height, 420, 1200);

        return current with
        {
            Version = 1,
            Model = model,
            Hotkey = hotkey,
            ClipboardRestoreDelayMs = delay,
            SettingsWindowWidth = width,
            SettingsWindowHeight = height
        };
    }
}
