using System.IO;
using System.Text.Json;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class OverlayFrameDiagnosticsTests
{
    private static TimeSpan Ms(double value) => TimeSpan.FromMilliseconds(value);
    private static OverlayFrameContext Context() => new("Waveform", 224, 100, 1000, 20, 1.5, 1.5,
        2, "Default", "Default", "123", 570, 90, 570 * 90 * 4, 1, 55, new(0, 1, 100, 0, 0, 0));

    [Theory]
    [InlineData(60, 50, 2)]
    [InlineData(120, 50, 5)]
    [InlineData(144, 100, 13)]
    public void MissedCallbacks_UseConfiguredCadenceRatherThanAssumingSixtyHz(double fps, double gapMs, long expected)
    {
        Assert.Equal(expected, OverlayFrameReport.EstimateMissedCallbacks([1000 / fps, gapMs], 1000 / fps));
        Assert.Equal(0, OverlayFrameReport.EstimateMissedCallbacks([gapMs], null));
    }

    [Fact]
    public void SessionReports_SeparateDuplicatesBusyUploadsStagesAudioAndSlowGaps()
    {
        var reports = new List<OverlayFrameReport>();
        var diagnostics = new OverlayFrameDiagnostics(reports.Add, Context, 120);
        diagnostics.Start(Ms(1000));
        for (var i = 0; i < 20; i++)
        {
            var time = Ms(1000 + i * (1000d / 120));
            diagnostics.BeginFrame(time, Ms(1000d / 120));
            diagnostics.RecordEnvelope(0.01);
            diagnostics.RecordRaster(0.1);
            diagnostics.RecordLock(0.01, false, false);
            diagnostics.RecordUpload(0.02);
            diagnostics.EndFrame(time + Ms(0.2), 0.2);
        }
        diagnostics.DuplicateFrame();
        diagnostics.DuplicateFrame();
        diagnostics.RecordAudio(Ms(1200), Ms(0.01));
        diagnostics.RecordAudio(Ms(1230), Ms(0.02));
        diagnostics.BeginFrame(Ms(1000 + 19 * (1000d / 120) + 50), Ms(50));
        diagnostics.RecordLock(0.001, true, false);
        diagnostics.EndFrame(Ms(1210), 0.15);
        diagnostics.Stop(Ms(1300));
        var report = reports[^1];
        Assert.Equal("stop", report.Event);
        Assert.Equal(21, report.Frames);
        Assert.Equal(2, report.DuplicateCallbacks);
        Assert.Equal(5, OverlayFrameReport.EstimateMissedCallbacks(report.IntervalsMs, report.EstimateTargetInterval()));
        Assert.Equal(20, report.BitmapUploads);
        Assert.Equal(1, report.BitmapBusySkips);
        Assert.Equal(0, report.BlockingLocks);
        Assert.Equal(0.1, report.Raster.MeanMs, 8);
        Assert.Equal(2, report.AudioCallbacks);
        Assert.Equal(30, report.AudioGapMaxMs, 5);
        Assert.Equal(70, report.LastAudioAgeMs!.Value, 5);
        Assert.Single(report.SlowFrames);
        Assert.False(diagnostics.IsActive);
        diagnostics.Heartbeat(Ms(5000));
        diagnostics.RecordAudio(Ms(5000), Ms(1));
        Assert.Equal(2, reports.Count);
    }

    [Fact]
    public void StartupCadence_DoesNotAdaptDownwardToLaterLag_AndHeartbeatReportsNoFrames()
    {
        var reports = new List<OverlayFrameReport>();
        var diagnostics = new OverlayFrameDiagnostics(reports.Add, Context);
        diagnostics.Start(Ms(1000));
        for (var i = 0; i <= 130; i++)
        {
            var time = Ms(1000 + i * (1000d / 120));
            diagnostics.BeginFrame(time, Ms(1000d / 120));
            diagnostics.EndFrame(time, 0.1);
        }
        diagnostics.Heartbeat(Ms(3000));
        var calibrated = reports[^1].EstimateTargetInterval();
        Assert.InRange(calibrated!.Value, 8.32, 8.34);
        diagnostics.Heartbeat(Ms(5000));
        Assert.Equal(0, reports[^1].Frames);
        Assert.True(reports[^1].SinceLastFrameMs > 2000);
        Assert.Equal(calibrated.Value, reports[^1].EstimateTargetInterval()!.Value, 5);
        diagnostics.BeginFrame(Ms(5100), Ms(3016));
        diagnostics.EndFrame(Ms(5101), 1);
        diagnostics.Stop(Ms(5200));
        Assert.True(OverlayFrameReport.EstimateMissedCallbacks(reports[^1].IntervalsMs,
            reports[^1].EstimateTargetInterval()) > 300);
    }

    [Fact]
    public void WindowBuffersAndSlowFrameExamples_AreBounded_AndSessionsReset()
    {
        var reports = new List<OverlayFrameReport>();
        var diagnostics = new OverlayFrameDiagnostics(reports.Add, Context, 120);
        diagnostics.Start(Ms(0));
        for (var i = 0; i < 5000; i++)
        {
            diagnostics.BeginFrame(TimeSpan.Zero, TimeSpan.Zero);
            diagnostics.EndFrame(TimeSpan.Zero, 20);
        }
        diagnostics.Stop(Ms(1));
        var report = reports[^1];
        Assert.Equal(2048, report.IntervalsMs.Length);
        Assert.Equal(2951, report.IntervalSamplesOmitted);
        Assert.Equal(8, report.SlowFrames.Length);
        var session = report.Session;
        diagnostics.Start(Ms(10000));
        diagnostics.Stop(Ms(10001));
        Assert.NotEqual(session, reports[^1].Session);
        Assert.Empty(reports[^1].IntervalsMs);
        Assert.Equal(0, reports[^1].Frames);
    }

    [Fact]
    public void RecordingCounters_DoNotAllocateOrPublishPerFrame()
    {
        var reports = new List<OverlayFrameReport>();
        var diagnostics = new OverlayFrameDiagnostics(reports.Add, Context, 120);
        diagnostics.Start(Ms(1000));
        void Frame()
        {
            diagnostics.BeginFrame(Ms(1001), Ms(1000d / 120));
            diagnostics.RecordEnvelope(0.01);
            diagnostics.RecordRaster(0.1);
            diagnostics.RecordLock(0.01, false, false);
            diagnostics.RecordUpload(0.02);
            diagnostics.RecordAudio(Ms(1001), Ms(0.001));
            diagnostics.EndFrame(Ms(1001), 0.2);
        }
        for (var i = 0; i < 100; i++) Frame();
        var before = GC.GetAllocatedBytesForCurrentThread();
        for (var i = 0; i < 1000; i++) Frame();
        Assert.Equal(0, GC.GetAllocatedBytesForCurrentThread() - before);
        Assert.Single(reports);
    }

    [Fact]
    public async Task Writer_EmitsProductionIdentityAndMetrics_AndRotatesBoundedly()
    {
        var root = Path.Combine(Path.GetTempPath(), "opencode", "voicepaste-frame-log-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        var path = Path.Combine(root, "frames.jsonl");
        try
        {
            using var writer = new FrameDiagnosticsWriter(path, maxBytes: 1024);
            var diagnostics = new OverlayFrameDiagnostics(writer.Publish, Context, 120);
            diagnostics.Start(Ms(1000));
            diagnostics.BeginFrame(Ms(1000), Ms(0));
            diagnostics.EndFrame(Ms(1001), 0.1);
            diagnostics.BeginFrame(Ms(1100), Ms(100));
            diagnostics.RecordLock(0.01, true, false);
            diagnostics.EndFrame(Ms(1101), 0.1);
            diagnostics.Stop(Ms(1200));
            writer.Dispose();
            await writer.Completion.WaitAsync(TimeSpan.FromSeconds(5));
            Assert.True(File.Exists(path + ".previous"));
            var lines = await File.ReadAllLinesAsync(path);
            using var last = JsonDocument.Parse(lines[^1]);
            Assert.Equal("stop", last.RootElement.GetProperty("Event").GetString());
            Assert.Equal(11, last.RootElement.GetProperty("EstimatedMissedCallbacks").GetInt64());
            Assert.Equal(1, last.RootElement.GetProperty("BitmapBusySkips").GetInt32());
            Assert.Equal(570, last.RootElement.GetProperty("Context").GetProperty("BitmapWidth").GetInt32());
            Assert.False(last.RootElement.TryGetProperty("IntervalsMs", out _));
        }
        finally { Directory.Delete(root, recursive: true); }
    }

    [Fact]
    public async Task Writer_InaccessibleDestination_DoesNotThrowIntoRecording()
    {
        var root = Path.Combine(Path.GetTempPath(), "opencode", "voicepaste-frame-log-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            // A directory cannot be opened as an append-only log file.
            using var writer = new FrameDiagnosticsWriter(root);
            var diagnostics = new OverlayFrameDiagnostics(writer.Publish, Context);
            diagnostics.Start(Ms(0));
            diagnostics.Stop(Ms(1000));
            writer.Dispose();
            await writer.Completion.WaitAsync(TimeSpan.FromSeconds(5));
        }
        finally { Directory.Delete(root, recursive: true); }
    }
}
