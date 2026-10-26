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
                for (var i = 0; i < 10; i++)
                {
                    wave.Update(0.1 * i, TimeSpan.FromSeconds(1d / 60));
                    Assert.Same(first, Render());
                }
                wave.SetColor(Colors.SteelBlue);
                Assert.Same(first, Render());
                wave.Reset();
                Assert.Same(first, Render());
                wave.Width = 270;
                wave.Height = 40;
                var resized = Render();
                Assert.NotSame(first, resized);
                Assert.Equal(Math.Ceiling(270 * dpi.DpiScaleX * 2), resized.PixelWidth);
                Assert.Equal(Math.Ceiling(40 * dpi.DpiScaleY * 2), resized.PixelHeight);
            }
            catch (Exception ex) { error = ex; }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Waveform surface tests timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
