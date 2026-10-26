using System.Windows.Media;
using VoicePaste.Settings;

namespace VoicePaste.Tests;

public class IndicatorColorTests
{
    [Theory]
    [InlineData("#12ab9f", "#12AB9F")]
    [InlineData("  #123456  ", "#123456")]
    [InlineData("#000000", "#000000")]
    [InlineData("#FFFFFF", "#FFFFFF")]
    [InlineData(null, "#FF8C00")]
    [InlineData("red", "#FF8C00")]
    [InlineData("#FFF", "#FF8C00")]
    [InlineData("#GGGGGG", "#FF8C00")]
    [InlineData("#00112233", "#FF8C00")]
    public void Normalize_PreservesOpaqueRgbOrUsesDefault(string? input, string expected)
    {
        Assert.Equal(expected, IndicatorColors.Normalize(input));
        var settings = SettingsManager.ValidateAndMigrate(AppSettings.Default with { IndicatorColor = input! });
        Assert.Equal(expected, settings.IndicatorColor);
        Assert.Equal(expected, IndicatorColors.ToHex(IndicatorColors.Parse(input)));
    }

    [Fact]
    public void WindowsColorRef_UsesBgrByteOrder()
    {
        var color = Color.FromRgb(0x12, 0xAB, 0x9F);
        Assert.Equal(0x9FAB12u, NativeColorPicker.ToColorRef(color));
        Assert.Equal(color, NativeColorPicker.FromColorRef(0x9FAB12));
    }
}
