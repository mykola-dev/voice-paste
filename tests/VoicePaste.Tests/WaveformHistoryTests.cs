using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformHistoryTests
{
    [Fact]
    public void LiveRightEdge_RisesWithoutWaitingForHistorySample()
    {
        var history = new WaveformHistory();
        var count = history.Count;
        history.Update(0.8, TimeSpan.Zero);
        Assert.Equal(count, history.Count);
        Assert.Equal((0d, 0.8), history.GetSample(history.Count - 1));
        Assert.All(Enumerable.Range(0, history.Count - 1), i => Assert.Equal(0, history.GetSample(i).Level));
    }

    [Fact]
    public void Pulse_TranslatesRightToLeftEveryFrameWithoutChangingHeight()
    {
        var history = new WaveformHistory();
        history.Update(1, TimeSpan.Zero);
        history.Update(0, TimeSpan.FromMilliseconds(60));
        var peakIndex = history.Count - 2;
        var peak = history.GetSample(peakIndex);
        Assert.Equal(1, peak.Level);
        Assert.Equal(0.06, peak.AgeSeconds, 9);
        var x = 280 * (1 - peak.AgeSeconds / WaveformHistory.VisibleSeconds);
        foreach (var milliseconds in new[] { 7, 11, 16, 19 })
        {
            history.Update(0, TimeSpan.FromMilliseconds(milliseconds));
            var moved = history.GetSample(peakIndex);
            Assert.Equal(1, moved.Level);
            var nextX = 280 * (1 - moved.AgeSeconds / WaveformHistory.VisibleSeconds);
            Assert.Equal(280 * milliseconds / 1000d / WaveformHistory.VisibleSeconds, x - nextX, 9);
            x = nextX;
        }
        history.Update(0, TimeSpan.FromSeconds(5));
        Assert.All(Enumerable.Range(0, history.Count), i => Assert.Equal(0, history.GetSample(i).Level));
    }

    [Theory]
    [InlineData(double.NaN)]
    [InlineData(double.PositiveInfinity)]
    [InlineData(-1)]
    [InlineData(0)]
    public void SilenceAndInvalidInput_DoNotInventActivity(double level)
    {
        var history = new WaveformHistory();
        history.Update(level, TimeSpan.FromSeconds(2));
        Assert.All(Enumerable.Range(0, history.Count), i => Assert.Equal(0, history.GetSample(i).Level));
    }

    [Fact]
    public void ClampsAndResetsEntireHistory()
    {
        var history = new WaveformHistory();
        history.Update(100, TimeSpan.FromSeconds(10));
        Assert.All(Enumerable.Range(0, history.Count), i => Assert.InRange(history.GetSample(i).Level, 0, 1));
        Assert.Equal(1, history.GetSample(history.Count - 1).Level);
        history.Reset();
        Assert.All(Enumerable.Range(0, history.Count), i => Assert.Equal(0, history.GetSample(i).Level));
        Assert.Equal(0, history.GetSample(history.Count - 1).AgeSeconds);
    }

    [Fact]
    public void History_RetainsOneOffscreenPredecessorAndExpiresOldPeaks()
    {
        var history = new WaveformHistory();
        history.Update(1, TimeSpan.Zero);
        for (var i = 0; i < 1000; i++) history.Update(0, TimeSpan.FromSeconds(1d / 60));
        Assert.InRange(history.Count, 97, 101);
        Assert.True(history.GetSample(0).AgeSeconds >= WaveformHistory.VisibleSeconds);
        Assert.True(history.GetSample(1).AgeSeconds < WaveformHistory.VisibleSeconds);
        Assert.All(Enumerable.Range(0, history.Count), i => Assert.Equal(0, history.GetSample(i).Level));
    }

    [Fact]
    public void RingBuffer_StaysBoundedAndChronologicalEvenAtExtremeUpdateRates()
    {
        var history = new WaveformHistory();
        for (var i = 0; i < 2000; i++) history.Update(i % 2, TimeSpan.FromTicks(10));
        Assert.Equal(WaveformHistory.Capacity, history.Count);
        for (var i = 1; i < history.Count; i++)
            Assert.True(history.GetSample(i - 1).AgeSeconds > history.GetSample(i).AgeSeconds);
        Assert.Equal((0d, 1d), history.GetSample(history.Count - 1));
    }

    [Fact]
    public void ZeroOrNegativeElapsed_OnlyChangesLiveEdge()
    {
        var history = new WaveformHistory();
        history.Update(0.5, TimeSpan.FromMilliseconds(16));
        var committed = Enumerable.Range(0, history.Count - 1).Select(history.GetSample).ToArray();
        history.Update(1, TimeSpan.Zero);
        history.Update(0.7, TimeSpan.FromSeconds(-1));
        Assert.Equal(committed, Enumerable.Range(0, history.Count - 1).Select(history.GetSample));
        Assert.Equal((0d, 0.7), history.GetSample(history.Count - 1));
    }
}
