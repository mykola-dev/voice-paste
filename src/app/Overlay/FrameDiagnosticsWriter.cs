using System.Diagnostics;
using System.IO;
using System.Text.Json;
using System.Threading.Channels;

namespace VoicePaste.Overlay;

/// <summary>Bounded, best-effort diagnostic output. Never performs disk I/O on the recording UI thread.</summary>
internal sealed class FrameDiagnosticsWriter : IDisposable
{
    private readonly Channel<OverlayFrameReport> _reports = Channel.CreateBounded<OverlayFrameReport>(
        new BoundedChannelOptions(16) { SingleReader = true, SingleWriter = true, FullMode = BoundedChannelFullMode.Wait });
    private readonly long _maxBytes;
    private long _droppedReports;
    public string Path { get; }
    public Task Completion { get; }

    public FrameDiagnosticsWriter(string path, long maxBytes = 8 * 1024 * 1024)
    {
        Path = path;
        _maxBytes = maxBytes;
        Completion = Task.Run(WriteReportsAsync);
    }

    public void Publish(OverlayFrameReport report)
    {
        if (!_reports.Writer.TryWrite(report)) Interlocked.Increment(ref _droppedReports);
    }

    public void Dispose() => _reports.Writer.TryComplete();

    private async Task WriteReportsAsync()
    {
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(Path))!);
            var assembly = typeof(RecordingOverlay).Assembly;
            using var process = Process.GetCurrentProcess();
            await AppendAsync(JsonSerializer.Serialize(new
            {
                Event = "process", Utc = DateTimeOffset.UtcNow, Pid = Environment.ProcessId,
                Executable = Environment.ProcessPath, Assembly = assembly.Location,
                BuildId = assembly.ManifestModule.ModuleVersionId, Version = assembly.GetName().Version?.ToString(),
                Runtime = Environment.Version.ToString(), Is64Bit = Environment.Is64BitProcess,
                Note = "Estimated missed WPF callbacks are not measured GPU/DWM presentation drops. No audio or transcripts are logged."
            }));
            await foreach (var report in _reports.Reader.ReadAllAsync())
            {
                var intervals = report.IntervalsMs;
                var target = report.EstimateTargetInterval();
                var missed = OverlayFrameReport.EstimateMissedCallbacks(intervals, target);
                Array.Sort(intervals);
                process.Refresh();
                await AppendAsync(JsonSerializer.Serialize(new
                {
                    report.Event, report.Session, report.Utc, report.SessionSeconds, report.WindowSeconds,
                    report.Context, report.Frames, CallbackFps = report.WindowSeconds > 0 ? report.Frames / report.WindowSeconds : 0,
                    report.DuplicateCallbacks, TargetIntervalMs = target, report.CadenceSource,
                    CadenceCalibrated = report.TargetIntervalMs.HasValue || report.CalibrationIntervalsMs.Length >= 120,
                    CalibrationSamples = report.CalibrationIntervalsMs.Length,
                    EstimatedMissedCallbacks = target.HasValue ? (long?)missed : null,
                    OngoingEstimatedMissedCallbacks = target is > 0 && report.SinceLastFrameMs is > 0
                        ? (long?)Math.Max(0, (long)Math.Floor(report.SinceLastFrameMs.Value / target.Value) - 1) : null,
                    GapP50Ms = Percentile(intervals, 0.5), GapP95Ms = Percentile(intervals, 0.95),
                    GapMaxMs = Percentile(intervals, 1), GapsOver50Ms = intervals.Count(ms => ms > 50),
                    GapsOver100Ms = intervals.Count(ms => ms > 100), report.IntervalSamplesOmitted,
                    report.PresentationGapMaxMs, report.HeartbeatGapMaxMs, report.SinceLastFrameMs,
                    report.Handler, report.Envelope, report.Raster, report.BitmapLock, report.Upload, report.OnRender,
                    report.BitmapUploads, report.BitmapBusySkips, report.BlockingLocks, report.BitmapRebuilds,
                    report.OnRenderCalls, report.AudioCallbacks, report.LastAudioAgeMs,
                    report.AudioGapMaxMs, report.AudioCallbackWorkMaxMs, report.SlowFrames,
                    WorkingSetBytes = process.WorkingSet64, PrivateBytes = process.PrivateMemorySize64,
                    ProcessCpuMs = process.TotalProcessorTime.TotalMilliseconds,
                    Gc0 = GC.CollectionCount(0), Gc1 = GC.CollectionCount(1), Gc2 = GC.CollectionCount(2),
                    ManagedBytes = GC.GetTotalMemory(false), LogReportsDropped = Interlocked.Read(ref _droppedReports)
                }));
            }
        }
        catch (Exception ex)
        {
            // Diagnostics must not terminate recording or raise UI errors, even on a full/read-only disk.
            Trace.WriteLine($"Frame diagnostics unavailable: {ex.Message}");
            _reports.Writer.TryComplete();
        }
    }

    private async Task AppendAsync(string line)
    {
        if (File.Exists(Path) && new FileInfo(Path).Length >= _maxBytes)
            File.Move(Path, Path + ".previous", overwrite: true);
        await File.AppendAllTextAsync(Path, line + Environment.NewLine);
    }

    private static double Percentile(double[] sorted, double fraction)
        => sorted.Length == 0 ? 0 : sorted[Math.Min(sorted.Length - 1, (int)((sorted.Length - 1) * fraction))];
}
