using System.IO;
using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using VoicePaste.Overlay;
using VoicePaste.Settings;

namespace VoicePaste.Tests;

public class RecordingOverlayLayoutTests
{
    [Theory]
    [InlineData(RecordingAnimation.LevelBar)]
    [InlineData(RecordingAnimation.Waveform)]
    public void NarrowOverlay_FitsRecordingHeaderAndTranscriptionAnimation(RecordingAnimation animation)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            RecordingOverlay? overlay = null;
            try
            {
                overlay = new RecordingOverlay();
                overlay.SetRecordingAnimation(animation);
                overlay.SetIndicatorColor("#1A9FE8");
                Assert.Equal(320 * 0.7, overlay.Width);
                Assert.Equal(100, overlay.Height);
                Assert.False(overlay.ShowActivated);
                Assert.Equal(0, Assert.IsType<ScaleTransform>(overlay.FindName("AudioLevelScale")).ScaleX);

                var border = Assert.IsType<Border>(overlay.Content);
                var grid = Assert.IsType<Grid>(border.Child);
                border.Measure(new Size(overlay.Width, overlay.Height));
                border.Arrange(new Rect(0, 0, overlay.Width, overlay.Height));
                border.UpdateLayout();
                var recording = Assert.IsType<Grid>(overlay.FindName("RecordingContent"));
                var header = Assert.IsType<StackPanel>(overlay.FindName("RecordingHeader"));
                Assert.InRange(header.DesiredSize.Width, 1, grid.ActualWidth);
                var visualizer = Assert.IsType<Grid>(overlay.FindName("RecordingVisualization"));
                Assert.InRange(header.DesiredSize.Height + visualizer.DesiredSize.Height, 1, grid.ActualHeight);
                var bar = Assert.IsType<Border>(overlay.FindName("AudioLevelMeter"));
                var wave = Assert.IsType<WaveformMeter>(overlay.FindName("VoiceWaveform"));
                Assert.InRange(visualizer.TranslatePoint(new Point(), border).Y -
                    header.TranslatePoint(new Point(0, header.ActualHeight), border).Y, 11.99, 12.01);
                Assert.True(visualizer.TranslatePoint(new Point(0, visualizer.ActualHeight), border).Y <= overlay.Height - 16);
                Assert.Equal(animation == RecordingAnimation.LevelBar ? Visibility.Visible : Visibility.Collapsed, bar.Visibility);
                Assert.Equal(animation == RecordingAnimation.Waveform ? Visibility.Visible : Visibility.Collapsed, wave.Visibility);
                if (animation == RecordingAnimation.Waveform)
                {
                    Assert.InRange(wave.ActualWidth, 1, grid.ActualWidth);
                    Assert.Equal(30, wave.ActualHeight);
                }

                var color = Color.FromRgb(26, 159, 232);
                foreach (var name in new[] { "AudioLevelBar", "SpinnerBar1", "SpinnerBar2", "SpinnerBar3", "SpinnerBar4", "SpinnerBar5" })
                    Assert.Equal(color, Assert.IsType<SolidColorBrush>(Assert.IsType<System.Windows.Shapes.Rectangle>(overlay.FindName(name)).Fill).Color);

                for (var i = 0; i < 150; i++)
                {
                    overlay.UpdateAudioLevel((float)(0.02 + 0.12 * Math.Abs(Math.Sin(i * 0.12)) * Math.Abs(Math.Sin(i * 0.037))));
                    overlay.RenderAudioFrame(TimeSpan.FromSeconds(1d / 60));
                }
                border.UpdateLayout();
                SavePreview(border, overlay.Width, overlay.Height, $"{animation}-recording");
                overlay.Hide();

                // Exercise actual rendering and mode switching without showing a window or recording audio.
                overlay.SetRecordingAnimation(RecordingAnimation.LevelBar);
                var scale = Assert.IsType<ScaleTransform>(overlay.FindName("AudioLevelScale"));
                overlay.UpdateAudioLevel(0.1f);
                overlay.UpdateAudioLevel(0); // A short peak between frames must not be lost.
                overlay.RenderAudioFrame(TimeSpan.Zero);
                Assert.Equal(0.5, scale.ScaleX, 6);
                overlay.RenderAudioFrame(TimeSpan.FromMilliseconds(140));
                Assert.Equal(0.5 * Math.Exp(-1), scale.ScaleX, 6);
                overlay.Hide();
                Assert.Equal(0, scale.ScaleX);
                overlay.SetRecordingAnimation(RecordingAnimation.Waveform);
                Assert.Equal(Visibility.Visible, wave.Visibility);
                Assert.Equal(Visibility.Collapsed, bar.Visibility);

                var transcribing = Assert.IsType<Grid>(overlay.FindName("TranscribingOverlay"));
                transcribing.Visibility = Visibility.Visible;
                recording.Visibility = Visibility.Collapsed;
                overlay.SetRecordingAnimation(RecordingAnimation.LevelBar);
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
                SavePreview(border, overlay.Width, overlay.Height, $"{animation}-transcribing");
            }
            catch (Exception ex) { error = ex; }
            finally { overlay?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Overlay layout timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
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
