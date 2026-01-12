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
}
