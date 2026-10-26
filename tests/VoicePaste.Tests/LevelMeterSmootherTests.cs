using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class LevelMeterSmootherTests
{
    [Fact]
    public void Gain_IsFiveTimesExistingMeterInput()
    {
        var smoother = new LevelMeterSmoother();
        Assert.Equal(0.25, smoother.Update(0.05f, TimeSpan.FromSeconds(5)), 6);
    }

    [Fact]
    public void LoudInput_ClampsWithoutOverflow()
    {
        var smoother = new LevelMeterSmoother();
        Assert.Equal(1, smoother.Update(10, TimeSpan.FromSeconds(5)), 6);
    }

    [Theory]
    [InlineData(float.NaN)]
    [InlineData(float.PositiveInfinity)]
    [InlineData(float.NegativeInfinity)]
    [InlineData(-1)]
    [InlineData(0)]
    public void InvalidOrSilentInput_DoesNotCreateActivity(float input)
        => Assert.Equal(0, new LevelMeterSmoother().Update(input, TimeSpan.FromSeconds(1)));

    [Fact]
    public void Rise_IsImmediateEvenWithoutElapsedTime()
    {
        var smoother = new LevelMeterSmoother();
        Assert.Equal(0.5, smoother.Update(0.1f, TimeSpan.Zero), 6);
        Assert.Equal(1, smoother.Update(0.2f, TimeSpan.Zero), 6);
    }

    [Fact]
    public void Release_Uses140MillisecondTimeConstant()
    {
        var smoother = new LevelMeterSmoother();
        smoother.Update(0.2f, TimeSpan.Zero);
        Assert.Equal(Math.Exp(-1), smoother.Update(0, TimeSpan.FromMilliseconds(140)), 6);
        Assert.InRange(smoother.Update(0, TimeSpan.FromSeconds(3)), 0, 0.00001);
    }

    [Fact]
    public void Smoothing_IsIndependentOfRenderRate()
    {
        var slower = new LevelMeterSmoother();
        var faster = new LevelMeterSmoother();
        slower.Update(0.1f, TimeSpan.Zero);
        faster.Update(0.1f, TimeSpan.Zero);
        double slowLevel = 0, fastLevel = 0;
        for (var i = 0; i < 10; i++) slowLevel = slower.Update(0, TimeSpan.FromMilliseconds(20));
        for (var i = 0; i < 20; i++) fastLevel = faster.Update(0, TimeSpan.FromMilliseconds(10));
        Assert.Equal(slowLevel, fastLevel, 10);
    }

    [Fact]
    public void Reset_RemovesPreviousRecordingLevel()
    {
        var smoother = new LevelMeterSmoother();
        smoother.Update(0.2f, TimeSpan.FromSeconds(1));
        smoother.Reset();
        Assert.Equal(0, smoother.Update(0, TimeSpan.FromMilliseconds(33)));
    }
}
