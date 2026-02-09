using System;
using VoicePaste.Settings;
using Xunit;

namespace VoicePaste.Tests;

public class SettingsWindowSizeTests
{
    [Fact]
    public void ValidateAndMigrate_ClampsWindowSize()
    {
        var settings = new AppSettings
        {
            SettingsWindowWidth = 99999,
            SettingsWindowHeight = 1
        };

        var validated = SettingsManager.ValidateAndMigrate(settings);

        Assert.Equal(1400, validated.SettingsWindowWidth);
        Assert.Equal(420, validated.SettingsWindowHeight);
    }

    [Fact]
    public void ValidateAndMigrate_NanPosition_StaysNan()
    {
        var settings = new AppSettings
        {
            SettingsWindowLeft = double.NaN,
            SettingsWindowTop = double.NaN
        };

        var validated = SettingsManager.ValidateAndMigrate(settings);

        Assert.True(double.IsNaN(validated.SettingsWindowLeft));
        Assert.True(double.IsNaN(validated.SettingsWindowTop));
    }

    [Fact]
    public void ValidateAndMigrate_OffScreenPosition_ResetsToNan()
    {
        // Position way off screen should be reset
        var settings = new AppSettings
        {
            SettingsWindowLeft = 50000,
            SettingsWindowTop = 50000,
            SettingsWindowWidth = 560,
            SettingsWindowHeight = 520
        };

        var validated = SettingsManager.ValidateAndMigrate(settings);

        Assert.True(double.IsNaN(validated.SettingsWindowLeft));
        Assert.True(double.IsNaN(validated.SettingsWindowTop));
    }

    [Fact]
    public void ValidateAndMigrate_NegativeInfinityPosition_ResetsToNan()
    {
        var settings = new AppSettings
        {
            SettingsWindowLeft = double.NegativeInfinity,
            SettingsWindowTop = double.NegativeInfinity
        };

        var validated = SettingsManager.ValidateAndMigrate(settings);

        Assert.True(double.IsNaN(validated.SettingsWindowLeft));
        Assert.True(double.IsNaN(validated.SettingsWindowTop));
    }
}
