using System.IO;
using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformRenderingTests
{
    [Fact]
    public void Waveform_RendersLivePeakAndReturnsToSilentBaseline()
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try
            {
                var wave = new WaveformMeter { Width = 280, Height = 20 };
                byte[] Render()
                {
                    wave.Measure(new Size(280, 20));
                    wave.Arrange(new Rect(0, 0, 280, 20));
                    wave.UpdateLayout();
                    var bitmap = new RenderTargetBitmap(280, 20, 96, 96, PixelFormats.Pbgra32);
                    bitmap.Render(wave);
                    var pixels = new byte[280 * 20 * 4];
                    bitmap.CopyPixels(pixels, 280 * 4, 0);
                    return pixels;
                }
                static byte Alpha(byte[] pixels, int x, int y) => pixels[(y * 280 + x) * 4 + 3];
                static int ColumnAlpha(byte[] pixels, int x)
                    => Enumerable.Range(0, 20).Sum(y => Alpha(pixels, x, y));

                var silent = Render();
                Assert.Equal(0, Alpha(silent, 275, 1));
                Assert.True(Alpha(silent, 275, 10) > 0);
                Assert.InRange(ColumnAlpha(silent, 275), 116, 140); // One DIP at 50%, allowing WPF resampling coverage.
                wave.Update(0.25, TimeSpan.Zero);
                var boosted = Render();
                Assert.True(Alpha(boosted, 279, 5) > 0); // 2x gain and immediate response at the live edge.
                Assert.Equal(0, Alpha(boosted, 279, 2));
                wave.Update(1, TimeSpan.Zero);
                var live = Render();
                Assert.True(Alpha(live, 279, 1) > 0); // Right edge rises without waiting for a history bin.
                Assert.Equal(0, Alpha(live, 5, 1)); // No invented peaks in the past.

                for (var i = 0; i < 150; i++)
                    wave.Update(0.15 + 0.8 * Math.Abs(Math.Sin(i * 0.12)) * Math.Abs(Math.Sin(i * 0.037)),
                        TimeSpan.FromSeconds(1d / 60));
                var history = Render();
                Assert.True(history.Count(value => value != 0) > silent.Count(value => value != 0));

                // Optional offscreen preview for visual review; never opens a window or microphone.
                var preview = Environment.GetEnvironmentVariable("VOICEPASTE_TEST_WAVEFORM_PREVIEW");
                if (!string.IsNullOrWhiteSpace(preview))
                {
                    var bitmap = BitmapSource.Create(280, 20, 96, 96, PixelFormats.Pbgra32, null, history, 280 * 4);
                    var encoder = new PngBitmapEncoder();
                    encoder.Frames.Add(BitmapFrame.Create(bitmap));
                    using var stream = File.Create(preview);
                    encoder.Save(stream);
                }

                wave.Reset();
                Assert.Equal(silent, Render());

                // A completed pulse moves by a fractional distance, retaining its contour/area.
                wave.Update(0.4, TimeSpan.Zero);
                wave.Update(0, TimeSpan.FromMilliseconds(60));
                wave.Update(0, TimeSpan.FromMilliseconds(60));
                var before = Render();
                Assert.InRange(Alpha(before, 259, 10), 238, 242); // 80% displayed level => ~94% local opacity.
                Assert.Equal(ColumnAlpha(silent, 200), ColumnAlpha(before, 200));
                wave.Update(0, TimeSpan.FromMilliseconds(16));
                var after = Render();
                Assert.InRange(Alpha(after, 256, 10), 238, 242); // Local opacity travels with the pulse.
                Assert.Equal(ColumnAlpha(silent, 279), ColumnAlpha(after, 279));
                static (double X, double Area) PulseCenter(byte[] pixels)
                {
                    double area = 0, moment = 0;
                    for (var y = 0; y < 8; y++)
                    for (var x = 0; x < 280; x++)
                    {
                        var alpha = Alpha(pixels, x, y);
                        area += alpha;
                        moment += alpha * (x + 0.5);
                    }
                    return (moment / area, area);
                }
                var first = PulseCenter(before);
                var second = PulseCenter(after);
                Assert.InRange(first.X - second.X, 280 * 0.016 / WaveformHistory.VisibleSeconds - 0.15,
                    280 * 0.016 / WaveformHistory.VisibleSeconds + 0.15);
                Assert.InRange(second.Area / first.Area, 0.97, 1.03);
                var filled = Enumerable.Range(0, 280).Where(x => Alpha(after, x, 7) > 0).ToArray();
                Assert.True(filled.Length > 3);
                Assert.Equal(Enumerable.Range(filled.First(), filled.Last() - filled.First() + 1), filled);

                // Live loudness must no longer change older fragments' opacity or the silent baseline.
                wave.Update(1, TimeSpan.Zero);
                var loudLiveEdge = Render();
                Assert.Equal(Alpha(after, 256, 10), Alpha(loudLiveEdge, 256, 10));
                Assert.Equal(ColumnAlpha(after, 200), ColumnAlpha(loudLiveEdge, 200));
                wave.Update(0, TimeSpan.Zero);
                Assert.Equal(after, Render());

                wave.Reset();
                wave.SetColor(Color.FromRgb(12, 34, 56));
                for (var i = 0; i < 150; i++) wave.Update(0.25, TimeSpan.FromSeconds(1d / 60));
                var medium = Render();
                for (var x = 0; x < 280; x++)
                {
                    Assert.InRange(Alpha(medium, x, 10), 197, 199); // Half height: local opacity is ~78%.
                    var pixel = (10 * 280 + x) * 4;
                    // Premultiplied RGB must retain the chosen color, not a second gradient color.
                    Assert.InRange(medium[pixel], 42, 44);
                    Assert.InRange(medium[pixel + 1], 25, 27);
                    Assert.Equal(9, medium[pixel + 2]);
                }
                for (var i = 0; i < 150; i++) wave.Update(1, TimeSpan.FromSeconds(1d / 60));
                var uniform = Render();
                for (var x = 0; x < 280; x++)
                {
                    var pixel = (10 * 280 + x) * 4;
                    Assert.Equal(new byte[] { 56, 34, 12, 255 }, uniform.Skip(pixel).Take(4).ToArray());
                    Assert.Equal(255, Alpha(uniform, x, 1)); // Continuous fill, no column gaps.
                }
                wave.Update(0, TimeSpan.FromSeconds(5));
                wave.SetColor(Color.FromRgb(255, 140, 0));
                Assert.Equal(silent, Render());
            }
            catch (Exception ex) { error = ex; }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Waveform rendering timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
