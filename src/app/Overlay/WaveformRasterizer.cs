using System.Windows.Media;

namespace VoicePaste.Overlay;

/// <summary>Allocation-free coverage rendering of the same smoothstep volume contour and local alpha.</summary>
internal static class WaveformRasterizer
{
    private const int SubpixelSamples = 4;

    public static void Render(WaveformHistory history, Color color, double heightDips,
        int width, int height, byte[] pixels)
    {
        Array.Clear(pixels);
        Span<double> tops = stackalloc double[SubpixelSamples];
        Span<double> bottoms = stackalloc double[SubpixelSamples];
        Span<double> opacities = stackalloc double[SubpixelSamples];
        var nextIndex = 1;
        var previous = history.GetSample(0);
        var next = history.GetSample(nextIndex);
        var center = height / 2d;
        var baselineHalfHeight = Math.Min(center, height / heightDips);

        // Scan the history once from left to right, retaining fractional time positions.
        for (var x = 0; x < width; x++)
        {
            double minTop = height, maxTop = 0, minBottom = height, maxBottom = 0, opacitySum = 0;
            for (var sample = 0; sample < SubpixelSamples; sample++)
            {
                var age = WaveformHistory.VisibleSeconds * (1 - (x + (sample + 0.5) / SubpixelSamples) / width);
                while (nextIndex < history.Count - 1 && age < next.AgeSeconds)
                {
                    previous = next;
                    next = history.GetSample(++nextIndex);
                }
                var t = Math.Clamp((previous.AgeSeconds - age) / (previous.AgeSeconds - next.AgeSeconds), 0, 1);
                var eased = t * t * (3 - 2 * t);
                var fromLevel = Math.Clamp(previous.Level * WaveformMeter.HeightGain, 0, 1);
                var toLevel = Math.Clamp(next.Level * WaveformMeter.HeightGain, 0, 1);
                var level = fromLevel + (toLevel - fromLevel) * eased;
                var halfHeight = baselineHalfHeight + level * (center - baselineHalfHeight);
                tops[sample] = center - halfHeight;
                bottoms[sample] = center + halfHeight;
                opacities[sample] = 0.5 + 0.5 * level;
                opacitySum += opacities[sample];
                minTop = Math.Min(minTop, tops[sample]);
                maxTop = Math.Max(maxTop, tops[sample]);
                minBottom = Math.Min(minBottom, bottoms[sample]);
                maxBottom = Math.Max(maxBottom, bottoms[sample]);
            }

            var interiorAlpha = (byte)Math.Round(255 * opacitySum / SubpixelSamples);
            var interiorB = Premultiply(color.B, interiorAlpha);
            var interiorG = Premultiply(color.G, interiorAlpha);
            var interiorR = Premultiply(color.R, interiorAlpha);
            var firstFullRow = (int)Math.Ceiling(maxTop);
            var endFullRow = (int)Math.Floor(minBottom);
            var lastRow = Math.Min(height, (int)Math.Ceiling(maxBottom));
            for (var y = Math.Max(0, (int)Math.Floor(minTop)); y < lastRow; y++)
            {
                var pixel = (y * width + x) * 4;
                if (y >= firstFullRow && y < endFullRow)
                {
                    pixels[pixel] = interiorB;
                    pixels[pixel + 1] = interiorG;
                    pixels[pixel + 2] = interiorR;
                    pixels[pixel + 3] = interiorAlpha;
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
                pixels[pixel] = Premultiply(color.B, a);
                pixels[pixel + 1] = Premultiply(color.G, a);
                pixels[pixel + 2] = Premultiply(color.R, a);
                pixels[pixel + 3] = a;
            }
        }
    }

    private static byte Premultiply(byte channel, byte alpha) => (byte)((channel * alpha + 127) / 255);
}
