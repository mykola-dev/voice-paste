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
    [Fact]
    public void RenderCost_OptionalOffscreenDiagnostics()
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
                void Frame(int i)
                {
                    wave.Update(0.22 + 0.18 * Math.Sin(i * 0.15), TimeSpan.FromSeconds(1d / 60));
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
                output.WriteLine($"Offscreen 190x30: mean {total.Elapsed.TotalMilliseconds / timings.Length:F3} ms, " +
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
