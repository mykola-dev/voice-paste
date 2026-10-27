using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformSurfaceTests
{
    [Fact]
    public void Surface_IsDpiSupersampledAndReusedUntilSizeChanges()
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try
            {
                var wave = new WaveformMeter { Width = 190, Height = 30 };
                var target = new RenderTargetBitmap(300, 60, 96, 96, PixelFormats.Pbgra32);
                WriteableBitmap Render()
                {
                    wave.Measure(new Size(wave.Width, wave.Height));
                    wave.Arrange(new Rect(0, 0, wave.Width, wave.Height));
                    wave.UpdateLayout();
                    target.Clear();
                    target.Render(wave);
                    var drawing = VisualTreeHelper.GetDrawing(wave);
                    return Assert.IsType<WriteableBitmap>(Assert.Single(drawing.Children.OfType<ImageDrawing>()).ImageSource);
                }
                var first = Render();
                var dpi = VisualTreeHelper.GetDpi(wave);
                Assert.Equal(Math.Ceiling(190 * dpi.DpiScaleX * 2), first.PixelWidth);
                Assert.Equal(Math.Ceiling(30 * dpi.DpiScaleY * 2), first.PixelHeight);
                Assert.Equal(BitmapScalingMode.HighQuality, RenderOptions.GetBitmapScalingMode(wave));
                var history = new WaveformHistory();
                var expected = new byte[first.PixelWidth * first.PixelHeight * 4];
                var actual = new byte[expected.Length];
                for (var i = 0; i < 10; i++)
                {
                    wave.Update(0.1 * i, TimeSpan.FromSeconds(1d / 60));
                    Assert.True(wave.IsMeasureValid);
                    Assert.True(wave.IsArrangeValid, "A recording frame must not invalidate WPF layout.");
                    // Pixel publication must not depend on reopening the retained ImageDrawing
                    // through OnRender. Compare before calling Render()/UpdateLayout() again.
                    history.Update(0.1 * i, TimeSpan.FromSeconds(1d / 60));
                    WaveformRasterizer.Render(history, Colors.DarkOrange, 30,
                        first.PixelWidth, first.PixelHeight, expected);
                    first.CopyPixels(actual, first.PixelWidth * 4, 0);
                    Assert.Equal(expected, actual);
                    Assert.Same(first, Render());
                }
                wave.SetColor(Colors.SteelBlue);
                Assert.True(wave.IsArrangeValid);
                Assert.Same(first, Render());
                wave.Reset();
                Assert.True(wave.IsArrangeValid);
                Assert.Same(first, Render());
                WaveformRasterizer.Render(new WaveformHistory(), Colors.SteelBlue, 30,
                    first.PixelWidth, first.PixelHeight, expected);
                first.CopyPixels(actual, first.PixelWidth * 4, 0);
                Assert.Equal(expected, actual);
                wave.Width = 270;
                wave.Height = 40;
                var resized = Render();
                Assert.NotSame(first, resized);
                Assert.Equal(Math.Ceiling(270 * dpi.DpiScaleX * 2), resized.PixelWidth);
                Assert.Equal(Math.Ceiling(40 * dpi.DpiScaleY * 2), resized.PixelHeight);
                expected = new byte[resized.PixelWidth * resized.PixelHeight * 4];
                actual = new byte[expected.Length];
                WaveformRasterizer.Render(new WaveformHistory(), Colors.SteelBlue, 40,
                    resized.PixelWidth, resized.PixelHeight, expected);
                resized.CopyPixels(actual, resized.PixelWidth * 4, 0);
                Assert.Equal(expected, actual);
            }
            catch (Exception ex) { error = ex; }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Waveform surface tests timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
