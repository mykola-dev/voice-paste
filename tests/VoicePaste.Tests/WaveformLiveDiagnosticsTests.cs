using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Runtime.ExceptionServices;
using System.Text.Json;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using VoicePaste.Overlay;
using VoicePaste.Settings;
using Xunit.Abstractions;

namespace VoicePaste.Tests;

public class WaveformLiveDiagnosticsTests(ITestOutputHelper output)
{
    [Fact]
    public async Task Composition_LongRunningSyntheticAudio_OptionalDiagnostics()
    {
        if (!int.TryParse(Environment.GetEnvironmentVariable("VOICEPASTE_LIVE_WAVEFORM_SECONDS"), out var seconds)) return;
        seconds = Math.Clamp(seconds, 10, 120);
        Exception? error = null;
        var report = new List<string>();
        var surfaces = new List<(int Width, int Height, int HistoryCount, bool Reused)>();
        var productionLogTask = Task.CompletedTask;
        var thread = new Thread(() =>
        {
            RecordingOverlay? overlay = null;
            System.Threading.Timer? audio = null;
            EventHandler? before = null, after = null;
            try
            {
                overlay = new RecordingOverlay();
                var productionLog = Environment.GetEnvironmentVariable("VOICEPASTE_LIVE_FRAME_LOG");
                if (!string.IsNullOrWhiteSpace(productionLog))
                    report.Add($"Production instrumentation: {overlay.EnableFrameDiagnostics(productionLog)}");
                overlay.SetRecordingAnimation(RecordingAnimation.Waveform);
                overlay.Left = SystemParameters.WorkArea.Left + 20;
                overlay.Top = SystemParameters.WorkArea.Bottom - overlay.Height - 20;
                var wave = (WaveformMeter)overlay.FindName("VoiceWaveform");
                var envelopeField = typeof(WaveformMeter).GetField("_envelope", BindingFlags.NonPublic | BindingFlags.Instance)!;
                var bitmapField = typeof(WaveformMeter).GetField("_bitmap", BindingFlags.NonPublic | BindingFlags.Instance)!;
                var envelope = (WaveformEnvelope)envelopeField.GetValue(wave)!;
                report.Add($"WPF rendering tier={RenderCapability.Tier >> 16}; synthetic 30 ms input; no microphone.");
                var clock = Stopwatch.StartNew();
                var intervals = new List<double>();
                var costs = new List<double>();
                var previous = 0d;
                TimeSpan? previousRenderingTime = null;
                var uniqueFrame = false;
                long start = 0;
                before = (_, e) =>
                {
                    var renderingTime = ((RenderingEventArgs)e).RenderingTime;
                    uniqueFrame = renderingTime != previousRenderingTime;
                    if (!uniqueFrame) return;
                    previousRenderingTime = renderingTime;
                    var now = clock.Elapsed.TotalMilliseconds;
                    if (previous > 0) intervals.Add(now - previous);
                    previous = now;
                    start = Stopwatch.GetTimestamp();
                };
                after = (_, _) =>
                {
                    if (uniqueFrame) costs.Add(Stopwatch.GetElapsedTime(start).TotalMilliseconds);
                };
                CompositionTarget.Rendering += before;
                overlay.ShowRecording();
                CompositionTarget.Rendering += after;
                audio = new System.Threading.Timer(_ => overlay.UpdateAudioLevel(
                    (float)(0.044 + 0.036 * Math.Sin(clock.Elapsed.TotalSeconds * 9))), null, 0, 30);
                WriteableBitmap? firstBitmap = null;
                var injectedStall = false;
                var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
                timer.Tick += (_, _) =>
                {
                    var bitmap = (WriteableBitmap?)bitmapField.GetValue(wave);
                    firstBitmap ??= bitmap;
                    if (bitmap != null) surfaces.Add((bitmap.PixelWidth, bitmap.PixelHeight,
                        envelope.History.Count, ReferenceEquals(firstBitmap, bitmap)));
                    using var process = Process.GetCurrentProcess();
                    intervals.Sort();
                    costs.Sort();
                    var elapsed = clock.Elapsed.TotalSeconds;
                    report.Add($"t={elapsed:F1}s frames={costs.Count} " +
                        $"interval p50/p95/max={Percentile(intervals, 0.5):F2}/{Percentile(intervals, 0.95):F2}/{Percentile(intervals, 1):F2}ms " +
                        $"update p50/p95/max={Percentile(costs, 0.5):F3}/{Percentile(costs, 0.95):F3}/{Percentile(costs, 1):F3}ms " +
                        $"bitmap={bitmap?.PixelWidth}x{bitmap?.PixelHeight} reused={ReferenceEquals(firstBitmap, bitmap)} " +
                        $"history={envelope.History.Count} WS={process.WorkingSet64 / 1048576d:F1}MiB " +
                        $"private={process.PrivateMemorySize64 / 1048576d:F1}MiB " +
                        $"GC={GC.CollectionCount(0)}/{GC.CollectionCount(1)}/{GC.CollectionCount(2)}");
                    costs.Clear();
                    intervals.Clear();
                    if (!injectedStall && int.TryParse(Environment.GetEnvironmentVariable("VOICEPASTE_LIVE_STALL_MS"), out var stallMs))
                    {
                        injectedStall = true;
                        Thread.Sleep(Math.Clamp(stallMs, 0, 1000)); // Opt-in, test-only UI stall for telemetry verification.
                    }
                    if (elapsed < seconds) return;
                    timer.Stop();
                    Dispatcher.CurrentDispatcher.BeginInvokeShutdown(DispatcherPriority.Background);
                };
                timer.Start();
                Dispatcher.Run();
            }
            catch (Exception ex) { error = ex; }
            finally
            {
                audio?.Dispose();
                if (before != null) CompositionTarget.Rendering -= before;
                if (after != null) CompositionTarget.Rendering -= after;
                overlay?.Hide();
                productionLogTask = overlay?.CompleteFrameDiagnostics() ?? Task.CompletedTask;
                if (overlay != null) (PresentationSource.FromVisual(overlay) as HwndSource)?.Dispose();
            }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(seconds + 20)), "Live diagnostics timed out.");
        await productionLogTask.WaitAsync(TimeSpan.FromSeconds(5));
        foreach (var line in report) output.WriteLine(line);
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
        var logPath = Environment.GetEnvironmentVariable("VOICEPASTE_LIVE_FRAME_LOG");
        if (!string.IsNullOrWhiteSpace(logPath))
        {
            var lines = await File.ReadAllLinesAsync(logPath);
            Assert.Contains(lines, line =>
            {
                using var json = JsonDocument.Parse(line);
                return json.RootElement.GetProperty("Event").GetString() == "process" &&
                    json.RootElement.GetProperty("Pid").GetInt32() == Environment.ProcessId &&
                    json.RootElement.GetProperty("BuildId").GetGuid() == typeof(RecordingOverlay).Assembly.ManifestModule.ModuleVersionId;
            });
            if (int.TryParse(Environment.GetEnvironmentVariable("VOICEPASTE_LIVE_STALL_MS"), out var stallMs) && stallMs >= 100)
                Assert.Contains(lines, line =>
                {
                    using var json = JsonDocument.Parse(line);
                    return json.RootElement.GetProperty("Event").GetString() == "window" &&
                        json.RootElement.GetProperty("GapMaxMs").GetDouble() >= stallMs * 0.8 &&
                        json.RootElement.GetProperty("EstimatedMissedCallbacks").GetInt64() > 0 &&
                        json.RootElement.GetProperty("Context").GetProperty("Dispatcher")
                            .GetProperty("MaxOperationMs").GetDouble() >= stallMs * 0.8;
                });
        }
        Assert.NotEmpty(surfaces);
        Assert.All(surfaces, surface =>
        {
            Assert.Equal(surfaces[0].Width, surface.Width);
            Assert.Equal(surfaces[0].Height, surface.Height);
            Assert.True(surface.Reused, "Recording must reuse the same fixed-size bitmap.");
            Assert.InRange(surface.HistoryCount, 2, WaveformHistory.Capacity);
        });
    }

    private static double Percentile(List<double> values, double quantile)
        => values.Count == 0 ? 0 : values[Math.Min(values.Count - 1, (int)((values.Count - 1) * quantile))];
}
