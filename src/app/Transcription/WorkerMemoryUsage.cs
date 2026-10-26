namespace VoicePaste.Transcription;

/// <summary>Resident RAM and dedicated GPU memory of the worker, not just model weights.</summary>
public sealed record WorkerMemoryUsage(
    string? Model, string? Device, int? ProcessId, bool IsReady, long? RamBytes, long? VramBytes)
{
    public static WorkerMemoryUsage NotLoaded { get; } = new(null, null, null, false, null, null);
}
