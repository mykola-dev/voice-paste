using System.IO;
using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class RecordingOverlayLayoutTests
{
    [Fact]
    public void NarrowOverlay_AlwaysShowsWaveformAndFitsRecordingHeaderAndTranscriptionAnimation()
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            RecordingOverlay? overlay = null;
            try
            {
                overlay = new RecordingOverlay();
                overlay.SetIndicatorColor("#1A9FE8");
                Assert.Equal(320 * 0.7, overlay.Width);
                Assert.Equal(100, overlay.Height);
                Assert.False(overlay.ShowActivated);
                Assert.Null(overlay.FindName("AudioLevelMeter"));
                Assert.Null(overlay.FindName("AudioLevelBar"));
                Assert.Null(overlay.FindName("AudioLevelScale"));

                var border = Assert.IsType<Border>(overlay.Content);
                var grid = Assert.IsType<Grid>(border.Child);
                border.Measure(new Size(overlay.Width, overlay.Height));
                border.Arrange(new Rect(0, 0, overlay.Width, overlay.Height));
                border.UpdateLayout();
                var recording = Assert.IsType<Grid>(overlay.FindName("RecordingContent"));
                var header = Assert.IsType<StackPanel>(overlay.FindName("RecordingHeader"));
                Assert.Equal(2, header.Children.Count);
                var dot = Assert.IsType<System.Windows.Shapes.Ellipse>(header.Children[0]);
                Assert.Equal(Colors.Red, Assert.IsType<SolidColorBrush>(dot.Fill).Color);
                var timer = Assert.IsType<TextBlock>(header.Children[1]);
                Assert.Equal("00:00", timer.Text);
                Assert.Equal(new Thickness(0), timer.Margin);
                Assert.Null(overlay.FindName("StateText"));
                Assert.InRange(header.DesiredSize.Width, 1, grid.ActualWidth);
                var wave = Assert.IsType<WaveformMeter>(overlay.FindName("VoiceWaveform"));
                Assert.Same(recording, wave.Parent);
                Assert.InRange(header.DesiredSize.Height + wave.DesiredSize.Height, 1, grid.ActualHeight);
                Assert.Null(wave.OpacityMask);
                Assert.InRange(wave.TranslatePoint(new Point(), border).Y -
                    header.TranslatePoint(new Point(0, header.ActualHeight), border).Y, 11.99, 12.01);
                Assert.True(wave.TranslatePoint(new Point(0, wave.ActualHeight), border).Y <= overlay.Height - 16);
                Assert.Equal(Visibility.Visible, wave.Visibility);
                Assert.Equal(overlay.Width, wave.ActualWidth, 5);
                Assert.Equal(0, wave.TranslatePoint(new Point(), border).X, 5);
                Assert.Equal(30, wave.ActualHeight);

                var color = Color.FromRgb(26, 159, 232);
                foreach (var name in new[] { "SpinnerBar1", "SpinnerBar2", "SpinnerBar3", "SpinnerBar4", "SpinnerBar5" })
                    Assert.Equal(color, Assert.IsType<SolidColorBrush>(Assert.IsType<System.Windows.Shapes.Rectangle>(overlay.FindName(name)).Fill).Color);

                var silent = RenderPixels(border);
                for (var i = 0; i < 150; i++)
                {
                    var timestamp = TimeSpan.FromSeconds((i + 1) / 60d);
                    overlay.UpdateAudioLevel((float)(0.02 + 0.12 * Math.Abs(Math.Sin(i * 0.12)) * Math.Abs(Math.Sin(i * 0.037))), timestamp);
                    overlay.RenderAudioFrame(TimeSpan.FromSeconds(1d / 60), timestamp);
                }
                border.UpdateLayout();
                SavePreview(border, overlay.Width, overlay.Height, "Waveform-recording");
                Assert.False(silent.SequenceEqual(RenderPixels(border)));
                overlay.Hide();
                Assert.Equal(silent, RenderPixels(border));

                // A short audio peak between frames must survive the waveform handoff.
                overlay.UpdateAudioLevel(0.1f, TimeSpan.Zero);
                overlay.UpdateAudioLevel(0, TimeSpan.FromMilliseconds(30));
                overlay.RenderAudioFrame(TimeSpan.Zero, TimeSpan.FromMilliseconds(32));
                Assert.False(silent.SequenceEqual(RenderPixels(border)));
                overlay.Hide();
                Assert.Equal(silent, RenderPixels(border));
                Assert.Equal(Visibility.Visible, wave.Visibility);
                border.UpdateLayout();
                Assert.Equal(0, wave.TranslatePoint(new Point(), border).X, 5);
                Assert.Equal(overlay.Width, wave.ActualWidth, 5);

                var transcribing = Assert.IsType<Grid>(overlay.FindName("TranscribingOverlay"));
                transcribing.Visibility = Visibility.Visible;
                recording.Visibility = Visibility.Collapsed;
                Assert.Equal(Visibility.Visible, transcribing.Visibility);
                Assert.Equal(Visibility.Collapsed, recording.Visibility);
                border.Measure(new Size(overlay.Width, overlay.Height));
                border.Arrange(new Rect(0, 0, overlay.Width, overlay.Height));
                border.UpdateLayout();
                Assert.True(double.IsNaN(transcribing.Width));
                Assert.Equal(grid.ActualWidth, transcribing.ActualWidth);
                Assert.InRange(transcribing.DesiredSize.Height, 1, grid.ActualHeight);
                var spinner = Assert.IsType<StackPanel>(overlay.FindName("TranscriptionSpinner"));
                var center = spinner.TranslatePoint(new Point(spinner.ActualWidth / 2, spinner.ActualHeight / 2), border);
                Assert.Equal(overlay.Width / 2, center.X, 5);
                Assert.Equal(overlay.Height / 2, center.Y, 5);
                SavePreview(border, overlay.Width, overlay.Height, "Waveform-transcribing");
            }
            catch (Exception ex) { error = ex; }
            finally { overlay?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Overlay layout timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }

    private static byte[] RenderPixels(FrameworkElement element)
    {
        var width = (int)element.ActualWidth;
        var height = (int)element.ActualHeight;
        var bitmap = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(element);
        var pixels = new byte[width * height * 4];
        bitmap.CopyPixels(pixels, width * 4, 0);
        return pixels;
    }

    private static void SavePreview(Visual visual, double width, double height, string name)
    {
        var prefix = Environment.GetEnvironmentVariable("VOICEPASTE_TEST_OVERLAY_PREVIEW");
        if (string.IsNullOrWhiteSpace(prefix)) return;
        var bitmap = new RenderTargetBitmap((int)width, (int)height, 96, 96, PixelFormats.Pbgra32);
        bitmap.Render(visual);
        var encoder = new PngBitmapEncoder();
        encoder.Frames.Add(BitmapFrame.Create(bitmap));
        using var stream = File.Create($"{prefix}-{name}.png");
        encoder.Save(stream);
    }
}
