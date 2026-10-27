using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformContourTests
{
    [Fact]
    public void RisingRamp_HasNoArtificialHorizontalTangentsAtMeasurementKnots()
    {
        double[] ages = [0.12, 0.09, 0.06, 0.03, 0];
        double[] levels = [0.1, 0.2, 0.3, 0.4, 0.5];
        var tangents = new double[ages.Length];
        WaveformContour.BuildTangents(ages, levels, tangents);
        Assert.All(tangents, tangent => Assert.Equal(10d / 3, tangent, 9));
        for (var i = 0; i < levels.Length - 1; i++)
        for (var step = 0; step <= 100; step++)
        {
            var t = step / 100d;
            Assert.Equal(levels[i] + 0.1 * t, WaveformContour.Evaluate(levels[i], levels[i + 1],
                tangents[i], tangents[i + 1], 0.03, t), 9);
        }
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void NonuniformMeasurements_AreContinuousMonotoneAndNeverOvershoot(bool descending)
    {
        double[] ages = [0.35, 0.2, 0.19, 0.13, 0.1, 0.07, 0.069, 0];
        double[] levels = [0, 0.1, 0.3, 0.3, 0.8, 1, 0.7, 0];
        if (descending) levels = levels.Select(level => 1 - level).ToArray();
        var tangents = new double[ages.Length];
        WaveformContour.BuildTangents(ages, levels, tangents);
        for (var i = 0; i < levels.Length - 1; i++)
        {
            var seconds = ages[i] - ages[i + 1];
            var previous = levels[i];
            for (var step = 0; step <= 1000; step++)
            {
                var level = WaveformContour.Evaluate(levels[i], levels[i + 1], tangents[i], tangents[i + 1],
                    seconds, step / 1000d);
                Assert.InRange(level, Math.Min(levels[i], levels[i + 1]), Math.Max(levels[i], levels[i + 1]));
                Assert.True(levels[i + 1] >= levels[i] ? level >= previous : level <= previous);
                previous = level;
            }
            Assert.Equal(levels[i + 1], previous, 12);
            if (i == levels.Length - 2) continue;
            const double epsilon = 0.000000001;
            var leftSlope = (levels[i + 1] - WaveformContour.Evaluate(levels[i], levels[i + 1],
                tangents[i], tangents[i + 1], seconds, 1 - epsilon / seconds)) / epsilon;
            var nextSeconds = ages[i + 1] - ages[i + 2];
            var rightSlope = (WaveformContour.Evaluate(levels[i + 1], levels[i + 2], tangents[i + 1],
                tangents[i + 2], nextSeconds, epsilon / nextSeconds) - levels[i + 1]) / epsilon;
            Assert.InRange(Math.Abs(leftSlope - rightSlope), 0, 0.01);
        }
        Assert.Equal(0, tangents[2]);
        Assert.Equal(0, tangents[3]);
        Assert.Equal(0, tangents[5]);
    }
}
