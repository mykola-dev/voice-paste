using VoicePaste.Settings;
using VoicePaste.Transcription;

namespace VoicePaste.Tests;

public class MemoryUsageTests
{
    [Theory]
    [InlineData("123, 100\n456, 999", 100)]
    [InlineData("123, 100\n123, 200", 300)]
    [InlineData("123, 0", 0)]
    public void Vram_UsesOnlyExactPidAndSumsDevices(string csv, long expectedMiB)
        => Assert.Equal(expectedMiB * 1024 * 1024, GpuMemoryReader.Parse(csv, 123));

    [Theory]
    [InlineData("")]
    [InlineData("456, 100")]
    [InlineData("123, N/A")]
    [InlineData("123, [N/A]")]
    [InlineData("123, 100\n123, N/A")]
    [InlineData("123, -1")]
    [InlineData("123, 9223372036854775807")]
    public void UnknownVram_IsNotZeroOrWholeCardUsage(string csv)
        => Assert.Null(GpuMemoryReader.Parse(csv, 123));

    [Fact]
    public void Display_UnloadedAndCpu()
    {
        Assert.Equal("Model not loaded", SettingsWindow.FormatMemoryUsage(WorkerMemoryUsage.NotLoaded));
        var cpu = SettingsWindow.FormatMemoryUsage(new("large-v2", "cpu", 42, true, 850 * 1024 * 1024, null));
        Assert.Contains("large-v2 (CPU): RAM 850 MiB", cpu);
        Assert.DoesNotContain("VRAM", cpu);
    }

    [Fact]
    public void Display_LoadingAndUnknownGpuMemory()
    {
        var text = SettingsWindow.FormatMemoryUsage(new("large-v2", "cuda", 42, false, null, null));
        Assert.Contains("loading", text);
        Assert.Contains("RAM unavailable", text);
        Assert.DoesNotContain("VRAM", text);
    }

    [Fact]
    public void Display_ShowsVramOnlyWhenMeasured()
    {
        var usage = new WorkerMemoryUsage("large-v2", "cuda", 42, true, 850 * 1024 * 1024, null);
        Assert.DoesNotContain("VRAM", SettingsWindow.FormatMemoryUsage(usage));
        Assert.Contains("VRAM 512 MiB", SettingsWindow.FormatMemoryUsage(usage with { VramBytes = 512 * 1024 * 1024 }));
        Assert.Contains("VRAM 0 MiB", SettingsWindow.FormatMemoryUsage(usage with { VramBytes = 0 }));
    }

    [Fact]
    public void NonTranscriptionChanges_DoNotReplaceWorker()
    {
        var previous = AppSettings.Default;
        var next = previous with { KeepInMemory = true, Hotkey = "F8", PasteShortcut = PasteShortcut.CtrlV,
            DebugLogging = true, SettingsWindowWidth = 800, RecordingAnimation = RecordingAnimation.Waveform,
            IndicatorColor = "#4287F5" };
        Assert.False(VoicePasteController.RequiresWorkerReplacement(previous, next));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { Model = "large-v2" }));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { Device = TranscriptionDevice.Cpu }));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { LanguageMode = LanguageMode.Bilingual }));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { BeamSize = 2 }));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { CustomInitialPrompt = "prompt" }));
        Assert.True(VoicePasteController.RequiresWorkerReplacement(previous, next with { EnableVad = true }));
    }
}
