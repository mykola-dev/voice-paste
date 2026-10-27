using System.Diagnostics;
using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using VoicePaste.Overlay;
using Xunit.Abstractions;

namespace VoicePaste.Tests;

public class WaveformPerformanceTests(ITestOutputHelper output)
{
    [Theory]
    [InlineData(1)]
    [InlineData(1.5)]
    [InlineData(2)]
    public void RasterCost_OptionalDpiDiagnostics(double dpiScale)
    {
        if (Environment.GetEnvironmentVariable("VOICEPASTE_BENCHMARK_WAVEFORM") != "1") return;
        var history = new WaveformHistory();
        var width = (int)(224 * dpiScale * WaveformMeter.RasterScale);
        var height = (int)(30 * dpiScale * WaveformMeter.RasterScale);
        var pixels = new byte[width * height * 4];
        var frame = 0;
        void Render()
        {
            history.Advance(TimeSpan.FromMilliseconds(10));
            if (frame++ % 3 == 0) history.AddMeasurement(0.24 + 0.2 * Math.Sin(frame * 0.09), TimeSpan.Zero);
            WaveformRasterizer.Render(history, Colors.DarkOrange, 30, width, height, pixels);
        }
        for (var i = 0; i < 300; i++) Render();
        var timings = new double[1200];
        var allocated = GC.GetAllocatedBytesForCurrentThread();
        var total = Stopwatch.StartNew();
        for (var i = 0; i < timings.Length; i++)
        {
            var start = Stopwatch.GetTimestamp();
            Render();
            timings[i] = Stopwatch.GetElapsedTime(start).TotalMilliseconds;
        }
        total.Stop();
        var bytes = GC.GetAllocatedBytesForCurrentThread() - allocated;
        Array.Sort(timings);
        output.WriteLine($"Raster {width}x{height}, DPI {dpiScale:F2}: mean {total.Elapsed.TotalMilliseconds / timings.Length:F3} ms, " +
            $"p50 {timings[timings.Length / 2]:F3} ms, p95 {timings[(int)(timings.Length * 0.95)]:F3} ms, " +
            $"allocation {bytes / timings.Length:N0} bytes/frame.");
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void RenderCost_OptionalOffscreenDiagnostics(bool audioMeasurements)
    {
        if (Environment.GetEnvironmentVariable("VOICEPASTE_BENCHMARK_WAVEFORM") != "1") return;
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try
            {
                var wave = new WaveformMeter { Width = 190, Height = 30 };
                wave.Measure(new Size(190, 30));
                wave.Arrange(new Rect(0, 0, 190, 30));
                var target = new RenderTargetBitmap(190, 30, 96, 96, PixelFormats.Pbgra32);
                var time = TimeSpan.Zero;
                var nextMeasurement = TimeSpan.Zero;
                var frameInterval = TimeSpan.FromSeconds(1d / 60);
                void Frame(int i)
                {
                    if (audioMeasurements)
                    {
                        time += frameInterval;
                        while (nextMeasurement <= time)
                        {
                            wave.AddAudioMeasurement((float)(0.044 + 0.036 * Math.Sin(nextMeasurement.TotalSeconds * 9)), nextMeasurement);
                            nextMeasurement += TimeSpan.FromMilliseconds(30);
                        }
                        wave.AdvanceAudio(frameInterval, time);
                    }
                    else wave.Update(0.22 + 0.18 * Math.Sin(i * 0.15), frameInterval);
                    wave.UpdateLayout();
                    target.Clear();
                    target.Render(wave);
                }
                for (var i = 0; i < 180; i++) Frame(i);
                var timings = new double[600];
                var allocated = GC.GetAllocatedBytesForCurrentThread();
                var total = Stopwatch.StartNew();
                for (var i = 0; i < timings.Length; i++)
                {
                    var start = Stopwatch.GetTimestamp();
                    Frame(i);
                    timings[i] = Stopwatch.GetElapsedTime(start).TotalMilliseconds;
                }
                total.Stop();
                var bytes = GC.GetAllocatedBytesForCurrentThread() - allocated;
                Array.Sort(timings);
                output.WriteLine($"Offscreen 190x30 ({(audioMeasurements ? "30 ms audio queue" : "direct levels")}): " +
                    $"mean {total.Elapsed.TotalMilliseconds / timings.Length:F3} ms, " +
                    $"p95 {timings[(int)(timings.Length * 0.95)]:F3} ms, p99 {timings[(int)(timings.Length * 0.99)]:F3} ms, " +
                    $"UI-thread allocation {bytes / timings.Length:N0} bytes/frame. Not a live FPS measurement.");
            }
            catch (Exception ex) { error = ex; }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(60)), "Waveform benchmark timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
