namespace VoicePaste.Overlay;

/// <summary>Shape-preserving cubic Hermite interpolation on nonuniform measurement times.</summary>
internal static class WaveformContour
{
    public static void BuildTangents(ReadOnlySpan<double> ages, ReadOnlySpan<double> levels, Span<double> tangents)
    {
        var count = levels.Length;
        tangents[0] = (levels[1] - levels[0]) / (ages[0] - ages[1]);
        tangents[count - 1] = (levels[count - 1] - levels[count - 2]) / (ages[count - 2] - ages[count - 1]);
        for (var i = 1; i < count - 1; i++)
        {
            var before = ages[i - 1] - ages[i];
            var after = ages[i] - ages[i + 1];
            var left = (levels[i] - levels[i - 1]) / before;
            var right = (levels[i + 1] - levels[i]) / after;
            if (left == 0 || right == 0 || Math.Sign(left) != Math.Sign(right))
            {
                tangents[i] = 0; // Real plateaus and extrema, not every measurement knot.
                continue;
            }
            var w1 = 2 * after + before;
            var w2 = after + 2 * before;
            tangents[i] = (w1 + w2) / (w1 / left + w2 / right);
        }
    }

    public static double Evaluate(double from, double to, double fromTangent, double toTangent,
        double seconds, double t)
    {
        t = Math.Clamp(t, 0, 1);
        var t2 = t * t;
        var t3 = t2 * t;
        var level = (2 * t3 - 3 * t2 + 1) * from + (t3 - 2 * t2 + t) * seconds * fromTangent
            + (-2 * t3 + 3 * t2) * to + (t3 - t2) * seconds * toTangent;
        return Math.Clamp(level, Math.Min(from, to), Math.Max(from, to));
    }
}
