using System;
using System.IO;
using VoicePaste.Settings;
using Xunit;

namespace VoicePaste.Tests;

public class SettingsManagerTests
{
    [Fact]
    public void Load_WhenMissingFile_ReturnsDefaults()
    {
        var path = Path.Combine(Path.GetTempPath(), "VoicePaste.Tests", Guid.NewGuid().ToString("N"), "config.json");
        var manager = new SettingsManager(path);

        var settings = manager.Load();

        Assert.Equal("ScrollLock", settings.Hotkey);
        Assert.Equal(PasteShortcut.CtrlShiftV, settings.PasteShortcut);
    }

    [Fact]
    public void Save_ThenLoad_RoundTrips()
    {
        var dir = Path.Combine(Path.GetTempPath(), "VoicePaste.Tests", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);
        var path = Path.Combine(dir, "config.json");

        var manager = new SettingsManager(path);
        var input = new AppSettings
        {
            Hotkey = "Ctrl+Alt+Space",
            Model = "small",
            Device = TranscriptionDevice.Cpu,
            PasteShortcut = PasteShortcut.CtrlV,
            LanguageMode = LanguageMode.Bilingual,
            DebugLogging = true
        };

        manager.Save(input);
        var loaded = manager.Load();

        Assert.Equal("Ctrl+Alt+Space", loaded.Hotkey);
        Assert.Equal("small", loaded.Model);
        Assert.Equal(TranscriptionDevice.Cpu, loaded.Device);
        Assert.Equal(PasteShortcut.CtrlV, loaded.PasteShortcut);
        Assert.Equal(LanguageMode.Bilingual, loaded.LanguageMode);
        Assert.True(loaded.DebugLogging);
    }

    [Fact]
    public void ValidateAndMigrate_FixesInvalidSettings()
    {
        var settings = new AppSettings
        {
            Hotkey = "NotAKey",
            Model = "not-a-model"
        };

        var validated = SettingsManager.ValidateAndMigrate(settings);

        Assert.Equal("ScrollLock", validated.Hotkey);
        Assert.Equal("medium", validated.Model);
        Assert.Equal(1, validated.Version);
    }
}
