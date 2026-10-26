using System.Windows.Media;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformRasterizerTests
{
    [Theory]
    [InlineData(0, 128)]
    [InlineData(0.25, 191)]
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
        for (var i = 0; i < 150; i++) history.Update(level, TimeSpan.FromSeconds(1d / 60));
        return history;
    }
}
