using System.Runtime.InteropServices;
using System.Windows.Media;

namespace VoicePaste.Overlay;

/// <summary>Allocation-free coverage rendering; contour height and local alpha share one smooth curve.</summary>
internal static class WaveformRasterizer
{
    private const int SubpixelSamples = 4;
    private const double BaselineHeightDips = 1;
    private const double GainRampEnd = 0.1;
    private const double FullOpacityLevel = 0.9;

    public static void Render(WaveformHistory history, Color color, double heightDips,
        int width, int height, byte[] pixels)
    {
        Array.Clear(pixels);
        var packedPixels = MemoryMarshal.Cast<byte, uint>(pixels);
        Span<double> tops = stackalloc double[SubpixelSamples];
        Span<double> bottoms = stackalloc double[SubpixelSamples];
        Span<double> opacities = stackalloc double[SubpixelSamples];
        var count = history.Count;
        Span<double> ages = stackalloc double[count];
        Span<double> levels = stackalloc double[count];
        Span<double> tangents = stackalloc double[count];
        for (var i = 0; i < count; i++)
        {
            var point = history.GetSample(i);
            ages[i] = point.AgeSeconds;
            levels[i] = GetDisplayLevel(point.Level);
        }
        WaveformContour.BuildTangents(ages, levels, tangents);
        var liveLevel = GetDisplayLevel(history.LiveLevel);
        var nextIndex = 1;
        var center = height / 2d;
        var baselineHalfHeight = Math.Min(center, BaselineHeightDips * height / heightDips / 2);

        // Scan the history once from left to right, retaining fractional time positions.
        for (var x = 0; x < width; x++)
        {
            double minTop = height, maxTop = 0, minBottom = height, maxBottom = 0, opacitySum = 0;
            for (var sample = 0; sample < SubpixelSamples; sample++)
            {
                var age = WaveformHistory.VisibleSeconds * (1 - (x + (sample + 0.5) / SubpixelSamples) / width);
                while (nextIndex < count - 1 && age < ages[nextIndex]) nextIndex++;
                var previousIndex = nextIndex - 1;
                var seconds = ages[previousIndex] - ages[nextIndex];
                var t = (ages[previousIndex] - age) / seconds;
                var level = age < ages[count - 1]
                    ? liveLevel + (levels[count - 1] - liveLevel) * age / ages[count - 1]
                    : WaveformContour.Evaluate(levels[previousIndex], levels[nextIndex],
                        tangents[previousIndex], tangents[nextIndex], seconds, t);
                var halfHeight = baselineHalfHeight + level * (center - baselineHalfHeight);
                tops[sample] = center - halfHeight;
                bottoms[sample] = center + halfHeight;
                opacities[sample] = GetLocalOpacity(level);
                opacitySum += opacities[sample];
                minTop = Math.Min(minTop, tops[sample]);
                maxTop = Math.Max(maxTop, tops[sample]);
                minBottom = Math.Min(minBottom, bottoms[sample]);
                maxBottom = Math.Max(maxBottom, bottoms[sample]);
            }

            var interiorAlpha = (byte)Math.Round(255 * opacitySum / SubpixelSamples);
            var interiorColor = PackColor(color, interiorAlpha);
            var firstFullRow = (int)Math.Ceiling(maxTop);
            var endFullRow = (int)Math.Floor(minBottom);
            var lastRow = Math.Min(height, (int)Math.Ceiling(maxBottom));
            for (var y = Math.Max(0, (int)Math.Floor(minTop)); y < lastRow; y++)
            {
                var pixel = y * width + x;
                if (y >= firstFullRow && y < endFullRow)
                {
                    packedPixels[pixel] = interiorColor;
                    continue;
                }

                // Analytic vertical coverage + horizontal supersampling soften only the edge,
                // not the whole wave. Opacity is folded into premultiplied pixels, not WPF gradients.
                double alpha = 0;
                for (var sample = 0; sample < SubpixelSamples; sample++)
                {
                    var coverage = Math.Max(0, Math.Min(y + 1, bottoms[sample]) - Math.Max(y, tops[sample]));
                    alpha += coverage * opacities[sample];
                }
                var a = (byte)Math.Round(255 * alpha / SubpixelSamples);
                packedPixels[pixel] = PackColor(color, a);
            }
        }
    }

    internal static double GetDisplayLevel(double level)
    {
        level = double.IsFinite(level) ? Math.Clamp(level, 0, 1) : 0;
        var t = Math.Clamp(level / GainRampEnd, 0, 1);
        // Ease only the extra gain, not the full amplitude curve. Above 10% this is the same
        // linear x2 response as before; near zero it tends smoothly to x1, without a noise gate.
        var gain = 1 + (WaveformMeter.HeightGain - 1) * t * t * (3 - 2 * t);
        return Math.Clamp(level * gain, 0, 1);
    }

    internal static double GetLocalOpacity(double displayedLevel)
        => 0.5 + 0.5 * Math.Clamp(displayedLevel / FullOpacityLevel, 0, 1);

    private static byte Premultiply(byte channel, byte alpha) => (byte)((channel * alpha + 127) / 255);

    // Windows BGRA is little-endian: publish all four channels with one store/bounds check.
    private static uint PackColor(Color color, byte alpha)
        => Premultiply(color.B, alpha) | ((uint)Premultiply(color.G, alpha) << 8)
            | ((uint)Premultiply(color.R, alpha) << 16) | ((uint)alpha << 24);
}
