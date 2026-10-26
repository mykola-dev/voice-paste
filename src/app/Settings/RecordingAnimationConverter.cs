using System.Text.Json;
using System.Text.Json.Serialization;

namespace VoicePaste.Settings;

/// <summary>An unrecognized animation must not discard the rest of a user's configuration.</summary>
internal sealed class RecordingAnimationConverter : JsonConverter<RecordingAnimation>
{
    public override RecordingAnimation Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String &&
            Enum.TryParse<RecordingAnimation>(reader.GetString(), true, out var animation) && Enum.IsDefined(animation))
            return animation;
        if (reader.TokenType == JsonTokenType.Number && reader.TryGetInt32(out var value) &&
            Enum.IsDefined(typeof(RecordingAnimation), value))
            return (RecordingAnimation)value;
        reader.Skip();
        return RecordingAnimation.LevelBar;
    }

    public override void Write(Utf8JsonWriter writer, RecordingAnimation value, JsonSerializerOptions options)
        => writer.WriteStringValue(value == RecordingAnimation.Waveform ? "waveform" : "levelBar");
}
