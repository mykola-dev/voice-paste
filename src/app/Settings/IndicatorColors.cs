using System.Globalization;
using System.Windows.Media;

namespace VoicePaste.Settings;

internal static class IndicatorColors
{
    public const string Default = "#FF8C00";

    public static string Normalize(string? value)
    {
        var text = value?.Trim();
        return text is { Length: 7 } && text[0] == '#' &&
            uint.TryParse(text.AsSpan(1), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var rgb)
                ? $"#{rgb:X6}" : Default;
    }

    public static Color Parse(string? value)
    {
        var rgb = uint.Parse(Normalize(value).AsSpan(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture);
        return Color.FromRgb((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
    }

    public static string ToHex(Color color) => $"#{color.R:X2}{color.G:X2}{color.B:X2}";
}
