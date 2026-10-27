using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class WaveformEnvelopeTests
{
    private static TimeSpan Ms(double value) => TimeSpan.FromMilliseconds(value);

    [Fact]
    public void HeldInput_IsNotCommittedAgainEveryRenderFrame()
    {
        var envelope = new WaveformEnvelope();
        envelope.AddMeasurement(0.04f, Ms(0));
        envelope.Advance(TimeSpan.Zero, Ms(0));
        var count = envelope.History.Count;
        for (var i = 1; i <= 3; i++) envelope.Advance(Ms(8), Ms(i * 8));
        Assert.Equal(count, envelope.History.Count);
        Assert.Equal(0.024, envelope.History.GetSample(count - 1).AgeSeconds, 9);
        envelope.AddMeasurement(0.08f, Ms(30));
        envelope.Advance(Ms(8), Ms(32));
        Assert.Equal(count + 1, envelope.History.Count);
        Assert.Equal(0.002, envelope.History.GetSample(count).AgeSeconds, 9);
        Assert.Equal(0.4, envelope.History.GetSample(count).Level, 6);
    }

    [Fact]
    public void MultipleCallbacksBetweenFrames_KeepTheirTimingAndShortPeaks()
    {
        var envelope = new WaveformEnvelope();
        envelope.Advance(TimeSpan.Zero, Ms(0));
        envelope.AddMeasurement(0.02f, Ms(10));
        envelope.AddMeasurement(0.16f, Ms(40));
        envelope.AddMeasurement(0, Ms(70));
        envelope.Advance(Ms(90), Ms(90));
        var history = envelope.History;
        Assert.Equal(0.08, history.GetSample(history.Count - 3).AgeSeconds, 9);
        Assert.Equal(0.05, history.GetSample(history.Count - 2).AgeSeconds, 9);
        Assert.Equal(0.8, history.GetSample(history.Count - 2).Level, 6);
        Assert.Equal(0.02, history.GetSample(history.Count - 1).AgeSeconds, 9);
        Assert.Equal(0.8 * Math.Exp(-0.03 / 0.14), history.GetSample(history.Count - 1).Level, 6);
        Assert.Equal(0.8 * Math.Exp(-0.05 / 0.14), history.LiveLevel, 6);
    }

    [Fact]
    public void FirstMeasurementBeforeFirstFrame_IsVisibleImmediately()
    {
        var envelope = new WaveformEnvelope();
        envelope.AddMeasurement(0.1f, Ms(100));
        envelope.Advance(TimeSpan.Zero, Ms(105));
        Assert.Equal(0.005, envelope.History.GetSample(envelope.History.Count - 1).AgeSeconds, 9);
        Assert.Equal(0.5, envelope.History.LiveLevel, 6);
        Assert.Equal(0.5, envelope.History.GetSample(envelope.History.Count - 1).Level, 6);
    }

    [Fact]
    public void ReleaseAndCommittedMeasurements_DoNotDependOnFrameRate()
    {
        static WaveformEnvelope Run(int frameMilliseconds)
        {
            var envelope = new WaveformEnvelope();
            envelope.Advance(TimeSpan.Zero, Ms(0));
            var sampleTime = 0;
            for (var frameTime = frameMilliseconds; frameTime <= 240; frameTime += frameMilliseconds)
            {
                while (sampleTime <= frameTime)
                {
                    envelope.AddMeasurement(sampleTime < 60 ? 0.08f : 0, Ms(sampleTime));
                    sampleTime += 30;
                }
                envelope.Advance(Ms(frameMilliseconds), Ms(frameTime));
            }
            return envelope;
        }
        var slower = Run(20);
        var faster = Run(8);
        Assert.Equal(slower.History.Count, faster.History.Count);
        for (var i = 0; i < slower.History.Count; i++)
        {
            Assert.Equal(slower.History.GetSample(i).AgeSeconds, faster.History.GetSample(i).AgeSeconds, 9);
            Assert.Equal(slower.History.GetSample(i).Level, faster.History.GetSample(i).Level, 9);
        }
        Assert.Equal(slower.History.LiveLevel, faster.History.LiveLevel, 9);
    }

    [Fact]
    public void CallbackSchedulingJitter_DoesNotChangeMeasurementSpacing()
    {
        var envelope = new WaveformEnvelope();
        envelope.Advance(TimeSpan.Zero, Ms(100));
        envelope.AddMeasurement(0.02f, Ms(110));
        envelope.Advance(Ms(16), Ms(119));
        envelope.AddMeasurement(0.04f, Ms(140));
        envelope.Advance(Ms(32), Ms(150));
        var history = envelope.History;
        Assert.Equal(0.03, history.GetSample(history.Count - 2).AgeSeconds -
            history.GetSample(history.Count - 1).AgeSeconds, 9);
        Assert.Equal(0.008, history.GetSample(history.Count - 1).AgeSeconds, 9);
    }

    [Fact]
    public void OverflowStallAndReset_DoNotRetainStaleAudio()
    {
        var envelope = new WaveformEnvelope();
        for (var i = 0; i < 2000; i++) envelope.AddMeasurement(0.1f, Ms(i));
        envelope.Advance(Ms(2000), Ms(2000));
        Assert.InRange(envelope.History.Count, 2, WaveformHistory.Capacity);
        Assert.Equal(0.001, envelope.History.GetSample(envelope.History.Count - 1).AgeSeconds, 9);
        envelope.Advance(Ms(2000), Ms(4000));
        envelope.Advance(Ms(16), Ms(4016));
        Assert.Equal(0, envelope.History.LiveLevel);
        Assert.All(Enumerable.Range(0, envelope.History.Count), i => Assert.Equal(0, envelope.History.GetSample(i).Level));
        envelope.AddMeasurement(1, Ms(4020));
        envelope.Reset();
        envelope.Advance(Ms(16), Ms(4032));
        Assert.Equal(0, envelope.History.LiveLevel);
        Assert.Equal(3, envelope.History.Count);
    }
}
