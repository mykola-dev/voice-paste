using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace VoicePaste.Overlay;

/// <summary>Continuous volume envelope with amplitude-dependent opacity, scrolling right to left.</summary>
public sealed class WaveformMeter : FrameworkElement
{
    private readonly WaveformHistory _history = new();
    internal const double HeightGain = 2;
    internal const int RasterScale = 2;
    private Color _waveColor = Color.FromRgb(255, 140, 0);
    private WriteableBitmap? _bitmap;
    private byte[] _pixels = [];

    public WaveformMeter()
    {
        IsHitTestVisible = false;
        ClipToBounds = true;
        RenderOptions.SetBitmapScalingMode(this, BitmapScalingMode.HighQuality);
    }

    public void Update(double level, TimeSpan elapsed)
    {
        _history.Update(level, elapsed);
        InvalidateVisual();
    }

    public void Reset()
    {
        _history.Reset();
        InvalidateVisual();
    }

    public void SetColor(Color color)
    {
        _waveColor = Color.FromRgb(color.R, color.G, color.B);
        InvalidateVisual();
    }

    protected override void OnRender(DrawingContext drawingContext)
    {
        base.OnRender(drawingContext);
        if (ActualWidth <= 0 || ActualHeight <= 0) return;
        EnsureSurface();
        var bitmap = _bitmap!;
        WaveformRasterizer.Render(_history, _waveColor, ActualHeight, bitmap.PixelWidth, bitmap.PixelHeight, _pixels);
        bitmap.WritePixels(new Int32Rect(0, 0, bitmap.PixelWidth, bitmap.PixelHeight), _pixels, bitmap.PixelWidth * 4, 0);
        drawingContext.DrawImage(bitmap, new Rect(0, 0, ActualWidth, ActualHeight));
    }

    protected override void OnDpiChanged(DpiScale oldDpi, DpiScale newDpi)
    {
        base.OnDpiChanged(oldDpi, newDpi);
        InvalidateVisual();
    }

    private void EnsureSurface()
    {
        var dpi = VisualTreeHelper.GetDpi(this);
        var width = Math.Max(1, (int)Math.Ceiling(ActualWidth * dpi.DpiScaleX * RasterScale));
        var height = Math.Max(1, (int)Math.Ceiling(ActualHeight * dpi.DpiScaleY * RasterScale));
        if (_bitmap?.PixelWidth == width && _bitmap.PixelHeight == height) return;
        _pixels = new byte[checked(width * height * 4)];
        _bitmap = new WriteableBitmap(width, height, 96 * dpi.DpiScaleX * RasterScale,
            96 * dpi.DpiScaleY * RasterScale, PixelFormats.Pbgra32, null);
    }
}
