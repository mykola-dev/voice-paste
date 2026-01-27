using System;
using System.Collections.Generic;

namespace VoicePaste.Settings;

public enum TranscriptionDevice
{
    CudaAuto,
    CudaOnly,
    Cpu
}

public enum LanguageMode
{
    Auto,
    En,
    Ua,
    Bilingual
}

public enum PasteShortcut
{
    CtrlShiftV,
    CtrlV,
    ShiftInsert
}

public sealed record AppSettings
{
    public int Version { get; init; } = 1;

    public string Hotkey { get; init; } = "ScrollLock";

    public PasteShortcut PasteShortcut { get; init; } = PasteShortcut.CtrlShiftV;

    public string Model { get; init; } = "medium";

    public TranscriptionDevice Device { get; init; } = TranscriptionDevice.CudaAuto;

    public LanguageMode LanguageMode { get; init; } = LanguageMode.Auto;

    public bool RestoreClipboard { get; init; } = true;

    public int ClipboardRestoreDelayMs { get; init; } = 400;

    public bool DebugLogging { get; init; } = false;
    
    public string CustomInitialPrompt { get; init; } = string.Empty;

    public int BeamSize { get; init; } = 5;

    public double SettingsWindowWidth { get; init; } = 560;

    public double SettingsWindowHeight { get; init; } = 520;

    public static readonly IReadOnlyCollection<string> ValidModels = new[]
    {
        "tiny",
        "base",
        "small",
        "medium",
        "large-v2",
        "large-v3",
        "large-v3-turbo",
        "turbo",
        "distil-large-v2",
        "distil-large-v3",
        "distil-large-v3.5"
    };

    public static AppSettings Default => new();
}
