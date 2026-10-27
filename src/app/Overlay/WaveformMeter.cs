using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace VoicePaste.Overlay;

/// <summary>Continuous volume envelope with amplitude-dependent opacity, scrolling right to left.</summary>
public sealed class WaveformMeter : FrameworkElement
{
    private readonly WaveformEnvelope _envelope = new();
    private WaveformHistory History => _envelope.History;
    internal const double HeightGain = 2;
    internal const int RasterScale = 2;
    private Color _waveColor = Color.FromRgb(255, 140, 0);
    private WriteableBitmap? _bitmap;
    private byte[] _pixels = [];
    private bool _pixelsDirty = true;

    public WaveformMeter()
    {
        IsHitTestVisible = false;
        ClipToBounds = true;
        RenderOptions.SetBitmapScalingMode(this, BitmapScalingMode.HighQuality);
    }

    public void Update(double level, TimeSpan elapsed)
    {
        History.Update(level, elapsed);
        Refresh(waitForCompositor: false);
    }

    internal void AddAudioMeasurement(float level, TimeSpan timestamp) => _envelope.AddMeasurement(level, timestamp);

    internal void AdvanceAudio(TimeSpan elapsed, TimeSpan captureTime)
    {
        _envelope.Advance(elapsed, captureTime);
        Refresh(waitForCompositor: false);
    }

    public void Reset()
    {
        _envelope.Reset();
        Refresh(waitForCompositor: true);
    }

    public void SetColor(Color color)
    {
        _waveColor = Color.FromRgb(color.R, color.G, color.B);
        Refresh(waitForCompositor: true);
    }

    protected override void OnRender(DrawingContext drawingContext)
    {
        base.OnRender(drawingContext);
        if (ActualWidth <= 0 || ActualHeight <= 0) return;
        EnsureSurface();
        if (_pixelsDirty) UploadPixels(waitForCompositor: true);
        drawingContext.DrawImage(_bitmap, new Rect(0, 0, ActualWidth, ActualHeight));
    }

    private void Refresh(bool waitForCompositor)
    {
        _pixelsDirty = true;
        if (ActualWidth <= 0 || ActualHeight <= 0)
        {
            InvalidateVisual();
            return;
        }
        // The retained ImageDrawing already references this mutable bitmap. Changing its pixels
        // notifies WPF directly; InvalidateVisual would also invalidate arrange on every frame.
        if (EnsureSurface()) InvalidateVisual();
        UploadPixels(waitForCompositor);
    }

    private void UploadPixels(bool waitForCompositor)
    {
        var bitmap = _bitmap!;
        WaveformRasterizer.Render(History, _waveColor, ActualHeight, bitmap.PixelWidth, bitmap.PixelHeight, _pixels);
        // Never stall a recording frame waiting for the composition thread's previous bitmap copy.
        // A busy bitmap keeps its previous image; the next frame retries at the current history time.
        var locked = true;
        if (waitForCompositor) bitmap.Lock();
        else locked = bitmap.TryLock(new Duration(TimeSpan.Zero));
        if (!locked) return;
        try
        {
            var stride = bitmap.PixelWidth * 4;
            if (bitmap.BackBufferStride == stride)
                Marshal.Copy(_pixels, 0, bitmap.BackBuffer, _pixels.Length);
            else
                for (var y = 0; y < bitmap.PixelHeight; y++)
                    Marshal.Copy(_pixels, y * stride, IntPtr.Add(bitmap.BackBuffer, y * bitmap.BackBufferStride), stride);
            bitmap.AddDirtyRect(new Int32Rect(0, 0, bitmap.PixelWidth, bitmap.PixelHeight));
            _pixelsDirty = false;
        }
        finally
        {
            bitmap.Unlock();
        }
    }

    protected override void OnRenderSizeChanged(SizeChangedInfo sizeInfo)
    {
        base.OnRenderSizeChanged(sizeInfo);
        _pixelsDirty = true;
        InvalidateVisual();
    }

    protected override void OnDpiChanged(DpiScale oldDpi, DpiScale newDpi)
    {
        base.OnDpiChanged(oldDpi, newDpi);
        InvalidateVisual();
    }

    private bool EnsureSurface()
    {
        var dpi = VisualTreeHelper.GetDpi(this);
        var width = Math.Max(1, (int)Math.Ceiling(ActualWidth * dpi.DpiScaleX * RasterScale));
        var height = Math.Max(1, (int)Math.Ceiling(ActualHeight * dpi.DpiScaleY * RasterScale));
        if (_bitmap?.PixelWidth == width && _bitmap.PixelHeight == height) return false;
        _pixels = new byte[checked(width * height * 4)];
        _bitmap = new WriteableBitmap(width, height, 96 * dpi.DpiScaleX * RasterScale,
            96 * dpi.DpiScaleY * RasterScale, PixelFormats.Pbgra32, null);
        _pixelsDirty = true;
        return true;
    }
}
