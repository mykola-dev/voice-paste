using System.Windows.Media;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformRasterizerTests
{
    [Theory]
    [InlineData(0, 128)]
    [InlineData(0.25, 198)]
    [InlineData(0.4, 241)]
    [InlineData(0.45, 255)]
    [InlineData(0.475, 255)]
    [InlineData(0.5, 255)]
    [InlineData(1, 255)]
    public void UniformAmplitude_PreservesExactLocalOpacityAndPremultipliedColor(double level, byte alpha)
    {
        var history = UniformHistory(level);
        var color = Color.FromRgb(12, 34, 56);
        foreach (var dpiScale in new[] { 1, 1.25, 1.5, 2 })
        {
            var width = (int)Math.Ceiling(190 * dpiScale * WaveformMeter.RasterScale);
            var height = (int)Math.Ceiling(30 * dpiScale * WaveformMeter.RasterScale);
            var pixels = new byte[width * height * 4];
            WaveformRasterizer.Render(history, color, 30, width, height, pixels);
            for (var x = 0; x < width; x++)
            {
                var p = ((height / 2) * width + x) * 4;
                Assert.Equal(alpha, pixels[p + 3]);
                Assert.Equal((56 * alpha + 127) / 255, pixels[p]);
                Assert.Equal((34 * alpha + 127) / 255, pixels[p + 1]);
                Assert.Equal((12 * alpha + 127) / 255, pixels[p + 2]);
            }
        }
    }

    [Fact]
    public void CurvedEdges_HaveSubpixelCoverageAndVerticalSymmetry()
    {
        var history = new WaveformHistory();
        history.Update(0.4, TimeSpan.Zero);
        history.Update(0, TimeSpan.FromMilliseconds(60));
        history.Update(0, TimeSpan.FromMilliseconds(60));
        const int width = 560, height = 60;
        var pixels = new byte[width * height * 4];
        WaveformRasterizer.Render(history, Colors.DarkOrange, 30, width, height, pixels);
        var partialEdges = 0;
        for (var y = 0; y < height; y++)
        for (var x = 0; x < width; x++)
        {
            var p = (y * width + x) * 4;
            var alpha = pixels[p + 3];
            if (alpha is > 0 and < 120) partialEdges++;
            Assert.InRange((int)alpha - pixels[((height - 1 - y) * width + x) * 4 + 3], -1, 1);
            Assert.True(pixels[p] <= alpha && pixels[p + 1] <= alpha && pixels[p + 2] <= alpha);
        }
        Assert.True(partialEdges > 20, "Curved edges should be coverage-antialiased, not binary pixels.");
    }

    [Theory]
    [InlineData(1)]
    [InlineData(1.25)]
    [InlineData(1.5)]
    [InlineData(2)]
    public void Silence_HasOneDipOfCoverageAtEveryDpi(double dpiScale)
    {
        var width = (int)Math.Ceiling(190 * dpiScale * WaveformMeter.RasterScale);
        var height = (int)Math.Ceiling(30 * dpiScale * WaveformMeter.RasterScale);
        var pixels = new byte[width * height * 4];
        WaveformRasterizer.Render(new WaveformHistory(), Colors.DarkOrange, 30, width, height, pixels);
        var coverage = Enumerable.Range(0, height).Sum(y => pixels[(y * width + width / 2) * 4 + 3]);
        Assert.InRange(coverage / (128d * height / 30), 0.99, 1.01);
    }

    [Theory]
    [InlineData(0, 0)]
    [InlineData(0.01, 0.01028)]
    [InlineData(0.025, 0.02890625)]
    [InlineData(0.05, 0.075)]
    [InlineData(0.075, 0.13828125)]
    [InlineData(0.1, 0.2)]
    [InlineData(0.25, 0.5)]
    [InlineData(0.45, 0.9)]
    [InlineData(1, 1)]
    [InlineData(-1, 0)]
    [InlineData(double.NaN, 0)]
    public void ExtraGain_RampsOnlyInTheLowestTenPercent(double input, double expected)
        => Assert.Equal(expected, WaveformRasterizer.GetDisplayLevel(input), 9);

    [Fact]
    public void GainRamp_IsMonotonicAndHasNoKinkAtTenPercent()
    {
        double previous = 0;
        for (var i = 0; i <= 1000; i++)
        {
            var input = i / 1000d;
            var output = WaveformRasterizer.GetDisplayLevel(input);
            Assert.True(output >= previous);
            Assert.InRange(output, input, Math.Min(1, input * 2));
            if (input >= 0.1) Assert.Equal(Math.Min(1, input * 2), output, 9);
            previous = output;
        }
        const double delta = 0.000001;
        var threshold = WaveformRasterizer.GetDisplayLevel(0.1);
        Assert.InRange((threshold - WaveformRasterizer.GetDisplayLevel(0.1 - delta)) / delta, 1.999, 2.001);
        Assert.InRange((WaveformRasterizer.GetDisplayLevel(0.1 + delta) - threshold) / delta, 1.999, 2.001);
    }

    [Theory]
    [InlineData(0, 0.5)]
    [InlineData(0.45, 0.75)]
    [InlineData(0.9, 1)]
    [InlineData(0.95, 1)]
    [InlineData(1, 1)]
    public void LocalOpacity_ReachesFullAtNinetyPercent(double displayedLevel, double expected)
        => Assert.Equal(expected, WaveformRasterizer.GetLocalOpacity(displayedLevel), 9);

    [Fact]
    public void AudioRateRamp_ContourAndOpacityShareOneCurveWithoutRenderFramePlateaus()
    {
        var envelope = new WaveformEnvelope();
        for (var milliseconds = 0; milliseconds <= 300; milliseconds += 10)
        {
            var timestamp = TimeSpan.FromMilliseconds(milliseconds);
            if (milliseconds % 30 == 0)
                envelope.AddMeasurement((float)(0.02 + milliseconds / 30 * 0.006), timestamp);
            envelope.Advance(TimeSpan.FromMilliseconds(milliseconds == 0 ? 0 : 10), timestamp);
        }
        const int width = 3240, height = 600;
        var pixels = new byte[width * height * 4];
        WaveformRasterizer.Render(envelope.History, Colors.DarkOrange, 30, width, height, pixels);
        for (var x = 3000; x < 3220; x++)
        {
            var level = 0.2 + 0.002 * (x + 0.5 - 2940);
            var opacity = WaveformRasterizer.GetLocalOpacity(level);
            var alpha = pixels[((height / 2) * width + x) * 4 + 3];
            Assert.InRange(Math.Abs(alpha - 255 * opacity), 0, 0.51);
            var coverage = Enumerable.Range(0, height).Sum(y => pixels[(y * width + x) * 4 + 3]) / 255d;
            var expectedThickness = 20 + level * (height - 20); // 1-DIP baseline at this resolution.
            Assert.InRange(Math.Abs(coverage - opacity * expectedThickness), 0, 1.5);
        }
    }

    [Fact]
    public void ReusedBuffer_ClearsPreviousPeaksAfterReset()
    {
        var history = UniformHistory(1);
        var pixels = new byte[380 * 60 * 4];
        WaveformRasterizer.Render(history, Colors.DarkOrange, 30, 380, 60, pixels);
        Assert.Equal(255, pixels[3]);
        history.Reset();
        WaveformRasterizer.Render(history, Colors.DarkOrange, 30, 380, 60, pixels);
        Assert.All(pixels.Take(380 * 4), value => Assert.Equal(0, value));
        Assert.Equal(128, pixels[(30 * 380) * 4 + 3]);
    }

    [Fact]
    public void PackedPixels_PreserveExactBgraAndPremultiplicationIncludingAntialiasedEdges()
    {
        var history = new WaveformHistory();
        for (var i = 0; i < 80; i++)
            history.Update(0.21 + 0.19 * Math.Sin(i * 0.37), TimeSpan.FromMilliseconds(30));
        const int width = 570, height = 90;
        var pixels = new byte[width * height * 4];
        foreach (var color in new[] { Colors.Black, Colors.White, Color.FromRgb(17, 131, 249), Colors.DarkOrange })
        {
            WaveformRasterizer.Render(history, color, 30, width, height, pixels);
            for (var p = 0; p < pixels.Length; p += 4)
            {
                var alpha = pixels[p + 3];
                Assert.Equal((color.B * alpha + 127) / 255, pixels[p]);
                Assert.Equal((color.G * alpha + 127) / 255, pixels[p + 1]);
                Assert.Equal((color.R * alpha + 127) / 255, pixels[p + 2]);
            }
        }
    }

    [Fact]
    public void Rasterization_DoesNotAllocateManagedObjectsPerFrame()
    {
        var history = UniformHistory(0.25);
        var pixels = new byte[380 * 60 * 4];
        for (var i = 0; i < 20; i++) WaveformRasterizer.Render(history, Colors.DarkOrange, 30, 380, 60, pixels);
        var before = GC.GetAllocatedBytesForCurrentThread();
        for (var i = 0; i < 100; i++) WaveformRasterizer.Render(history, Colors.DarkOrange, 30, 380, 60, pixels);
        Assert.Equal(0, GC.GetAllocatedBytesForCurrentThread() - before);
    }

    private static WaveformHistory UniformHistory(double level)
    {
        var history = new WaveformHistory();
        for (var i = 0; i < Math.Ceiling((WaveformHistory.VisibleSeconds + 0.5) * 60); i++)
            history.Update(level, TimeSpan.FromSeconds(1d / 60));
        return history;
    }
}
