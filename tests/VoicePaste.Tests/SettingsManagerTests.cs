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
        Assert.False(settings.KeepInMemory);
        Assert.Equal("#FF8C00", settings.IndicatorColor);
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
            Model = "large-v3-turbo",
            Device = TranscriptionDevice.Cpu,
            PasteShortcut = PasteShortcut.CtrlV,
            LanguageMode = LanguageMode.Bilingual,
            DebugLogging = true,
            KeepInMemory = true,
            IndicatorColor = "#4287F5"
        };

        manager.Save(input);
        var loaded = manager.Load();

        Assert.Equal("Ctrl+Alt+Space", loaded.Hotkey);
        Assert.Equal("large-v3-turbo", loaded.Model);
        Assert.Equal(TranscriptionDevice.Cpu, loaded.Device);
        Assert.Equal(PasteShortcut.CtrlV, loaded.PasteShortcut);
        Assert.Equal(LanguageMode.Bilingual, loaded.LanguageMode);
        Assert.True(loaded.DebugLogging);
        Assert.True(loaded.KeepInMemory);
        Assert.Equal("#4287F5", loaded.IndicatorColor);
        Assert.DoesNotContain("recordingAnimation", File.ReadAllText(path));
        Assert.Contains("\"keepInMemory\": true", File.ReadAllText(path));
    }

    [Fact]
    public void Load_OldConfig_LeavesResidencyDisabled()
    {
        var path = Path.Combine(Path.GetTempPath(), "VoicePaste.Tests", Guid.NewGuid().ToString("N"), "config.json");
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        try
        {
            File.WriteAllText(path, """{"model":"large-v2","hotkey":"F8"}""");
            var settings = new SettingsManager(path).Load();
            Assert.False(settings.KeepInMemory);
            Assert.Equal("#FF8C00", settings.IndicatorColor);
            Assert.Equal("large-v2", settings.Model);
            Assert.Equal("F8", settings.Hotkey);
        }
        finally { Directory.Delete(Path.GetDirectoryName(path)!, true); }
    }

    [Theory]
    [InlineData("\"levelBar\"")]
    [InlineData("\"waveform\"")]
    [InlineData("0")]
    [InlineData("1")]
    [InlineData("\"futureAnimation\"")]
    [InlineData("999")]
    [InlineData("null")]
    [InlineData("{}")]
    public void Load_LegacyAnimation_IsIgnoredAndRemovedOnSaveWithoutLosingOtherSettings(string jsonValue)
    {
        var directory = Path.Combine(Path.GetTempPath(), "VoicePaste.Tests", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var path = Path.Combine(directory, "config.json");
            File.WriteAllText(path, "{\"hotkey\":\"F8\",\"model\":\"large-v2\",\"keepInMemory\":true,\"indicatorColor\":\"#1A9FE8\",\"recordingAnimation\":" + jsonValue + "}");
            var manager = new SettingsManager(path);
            var settings = manager.Load();
            Assert.Equal("F8", settings.Hotkey);
            Assert.Equal("large-v2", settings.Model);
            Assert.True(settings.KeepInMemory);
            Assert.Equal("#1A9FE8", settings.IndicatorColor);
            manager.Save(settings);
            Assert.DoesNotContain("recordingAnimation", File.ReadAllText(path));
            Assert.Equal(settings, manager.Load());
        }
        finally { Directory.Delete(directory, true); }
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
